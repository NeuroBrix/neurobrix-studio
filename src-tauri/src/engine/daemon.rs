//! The daemon manager: one owner of the daemon lifecycle for the whole
//! session. A daemon is Studio's only while the child handle from THIS
//! session exists; anything else that answers on the instance is
//! external — reported with its identity, never stopped.
//!
//! Locks are short and never held across an await: flows snapshot the
//! core state, run their probes and deadlines outside the lock, then
//! reconcile. A slow read can never block another command.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use super::client::{self, DaemonCallError, StatusRecord};
use super::discovery::EXECUTABLE_NAME;
use super::endpoint::{EndpointRecord, ResolveError, StudioInstance};
use super::info::{parse_info, InfoError};
use super::lifecycle::{
    self, classify_death, refused_because_already_running, DaemonSpawner, DeathRecord, OwnedChild,
    ReadinessConfig, ReadinessOutcome, ServeInvocation, StopConfig, StopOutcome,
    SystemDaemonSpawner,
};
use super::process::{EngineLocator, ProcessFailure, ProcessRunner, SystemProcessRunner};
use super::transport::{TransportError, TransportTimeouts};
use super::version::{check_supported, VersionCheck, SUPPORTED_ENGINE_RANGE};
use super::{excerpt, non_empty_excerpt, DISCOVERY_ARGS, DISCOVERY_TIMEOUT};

/// The manager's tunables. Defaults are Studio's own choices, documented
/// beside each field; the engine's contracts set the scale.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub transport: TransportTimeouts,
    /// The model-load window. The engine's own background start waits up
    /// to 300 s for its ready signal, so Studio's budget matches that
    /// scale instead of assuming faster hardware than the user has.
    pub readiness: ReadinessConfig,
    pub stop: StopConfig,
    /// The idle timeout Studio passes to `serve --timeout`. Recorded in
    /// the invocation and read back from it — never hardcoded at read.
    pub idle_timeout_secs: u64,
    /// The `info --json` probe budget — the same probe Stage 1 discovery
    /// runs, here with the instance env so it reports THIS instance's
    /// endpoint.
    pub info_timeout: Duration,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            transport: TransportTimeouts {
                connect: Duration::from_secs(2),
                read: Duration::from_secs(5),
                write: Duration::from_secs(5),
            },
            readiness: ReadinessConfig {
                deadline: Duration::from_secs(300),
                pause_between_attempts: Duration::from_millis(500),
            },
            stop: StopConfig {
                exit_deadline: Duration::from_secs(10),
                cleanup_grace: Duration::from_secs(5),
            },
            idle_timeout_secs: 1800,
            info_timeout: DISCOVERY_TIMEOUT,
        }
    }
}

/// The state of the daemon on Studio's instance, as one explicit value —
/// never a crash, never a silent fallback.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DaemonStatus {
    /// Nothing answers on the instance. `stale_files`: files a dead
    /// daemon left behind — reported; the engine cleans them on the next
    /// start.
    Stopped {
        stale_files: bool,
    },
    Starting {
        model: String,
    },
    Stopping,
    /// Owned by this session and answering a validated handshake.
    Ready {
        model: String,
        hardware: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        family: Option<String>,
        loaded: bool,
        mode: String,
        warm_serving: bool,
        engine_version: String,
        protocol_version: u64,
        operations: Vec<String>,
        /// From the serve invocation Studio issued.
        idle_timeout_secs: u64,
    },
    /// A daemon answers, but this session owns no handle for it. Never
    /// stopped by Studio.
    ExternalDaemon {
        model: String,
        hardware: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        family: Option<String>,
        loaded: bool,
        mode: String,
        warm_serving: bool,
        engine_version: String,
        protocol_version: u64,
        operations: Vec<String>,
    },
    /// The owned daemon is gone. Expected (the idle timeout, with the
    /// invocation's timeout, or an external stop) and unexpected deaths
    /// are distinct, explicit states.
    Died {
        expected: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        idle_timeout_secs: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostics: Option<String>,
    },
    Unavailable {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostics: Option<String>,
    },
    ContractFailure {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostics: Option<String>,
    },
}

/// Structured failures for the mutation commands — validated by the
/// frontend too, never swallowed, never silently retried.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MutationFailure {
    /// A daemon this session did not start is already on the instance —
    /// reported with its identity, never killed or adopted.
    Conflict {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        engine_version: Option<String>,
    },
    DuplicateSubmission {
        message: String,
    },
    /// Nothing this session owns — an external daemon is never Studio's
    /// to stop.
    NoOwnedDaemon {
        message: String,
    },
    NotReady {
        message: String,
    },
    Spawn {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostics: Option<String>,
    },
    LoadFailed {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostics: Option<String>,
    },
    ReadinessTimeout {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stdout_excerpt: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stderr_excerpt: Option<String>,
    },
    ContractFailure {
        message: String,
    },
    /// The shutdown never produced a confirmed stop; the daemon is still
    /// running.
    StopFailed {
        message: String,
    },
    EngineUnavailable {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostics: Option<String>,
    },
}

/// Every way resolving the instance's endpoint can fail.
#[derive(Debug, PartialEq)]
enum ResolutionError {
    EngineMissing,
    Unavailable {
        reason: String,
        diagnostics: Option<String>,
    },
    ContractFailure {
        reason: String,
        diagnostics: Option<String>,
    },
    Incompatible {
        engine: String,
    },
    /// The engine installation cannot serve daemons at all.
    NoServingModule,
}

/// The outcome of one status probe, with "nothing to dial or nothing
/// answering" kept apart from real failures: a named Windows instance
/// that exited cleanly removes its port file, and that is the normal
/// stopped state — never a contract failure.
#[derive(Debug, PartialEq)]
enum ProbeError {
    NotListening,
    Failed(DaemonCallError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Starting,
    Ready,
    Stopping,
}

/// One owned daemon's session state. Not `Debug`: the child handle behind
/// the trait object is an implementation detail the state never prints.
struct Core {
    /// The owned child while one exists. During an active start's
    /// readiness poll the child is held by that flow, not here.
    child: Option<Box<dyn OwnedChild>>,
    phase: Phase,
    /// The model of the daemon this session started.
    model: Option<String>,
    /// Recorded from the serve invocation when it started.
    idle_timeout_secs: Option<u64>,
    /// The last death of an owned daemon; shown until superseded by a
    /// new daemon or a fresh start.
    death: Option<DeathRecord>,
}

/// A consistent view of the core, taken under one short lock.
#[derive(Debug)]
struct Snapshot {
    has_child: bool,
    phase: Phase,
    model: Option<String>,
    idle_timeout_secs: Option<u64>,
    death: Option<DeathRecord>,
}

pub struct DaemonManager {
    core: Mutex<Core>,
    /// The instance's endpoint, resolved once per session through the
    /// env'd `info --json` probe and resolved to an address on every
    /// connection.
    endpoint: Mutex<Option<EndpointRecord>>,
    instance: StudioInstance,
    config: DaemonConfig,
    locator: Arc<dyn EngineLocator>,
    runner: Arc<dyn ProcessRunner>,
    spawner: Arc<dyn DaemonSpawner>,
}

impl DaemonManager {
    /// Real wiring: PATH locator, system runner, real spawner, the
    /// instance under the application cache directory.
    pub fn new(cache_dir: &Path) -> Self {
        Self::with_dependencies(
            cache_dir,
            Arc::new(super::process::PathEngineLocator),
            Arc::new(SystemProcessRunner),
            Arc::new(SystemDaemonSpawner),
            DaemonConfig::default(),
        )
    }

    fn with_dependencies(
        cache_dir: &Path,
        locator: Arc<dyn EngineLocator>,
        runner: Arc<dyn ProcessRunner>,
        spawner: Arc<dyn DaemonSpawner>,
        config: DaemonConfig,
    ) -> Self {
        Self {
            core: Mutex::new(Core {
                child: None,
                phase: Phase::Idle,
                model: None,
                idle_timeout_secs: None,
                death: None,
            }),
            endpoint: Mutex::new(None),
            instance: StudioInstance::under(cache_dir),
            config,
            locator,
            runner,
            spawner,
        }
    }

    fn lock_core(&self) -> std::sync::MutexGuard<'_, Core> {
        self.core
            .lock()
            .expect("the manager lock is never held across a panic")
    }

    // -----------------------------------------------------------------
    // Reading status: reads only; a read never starts or stops anything.

    pub async fn read_status(&self) -> DaemonStatus {
        let endpoint = match self.resolve_endpoint().await {
            Ok(endpoint) => endpoint,
            Err(error) => return resolution_to_status(&error),
        };
        let snapshot = self.reap_and_snapshot();

        match snapshot.phase {
            // An active start owns its transition and holds its child; a
            // start that handed the child back (its window passed, or the
            // daemon answered something refused) gets one bounded probe
            // per read — if it came up, the read finds it ready.
            Phase::Starting => {
                if !snapshot.has_child {
                    return starting_status(&snapshot);
                }
                match self.probe_status(&endpoint).await {
                    Ok(record) => {
                        self.flip_starting_to_ready();
                        self.ready_status(
                            &record,
                            snapshot
                                .idle_timeout_secs
                                .unwrap_or(self.config.idle_timeout_secs),
                        )
                    }
                    Err(_) => starting_status(&snapshot),
                }
            }
            Phase::Ready => match self.probe_status(&endpoint).await {
                Ok(record) => self.ready_status(
                    &record,
                    snapshot
                        .idle_timeout_secs
                        .unwrap_or(self.config.idle_timeout_secs),
                ),
                Err(ProbeError::NotListening) => {
                    // It answered readiness before; if it died between the
                    // reap and the dial, the next reap says so.
                    match self.reap_death() {
                        Some(death) => died_status(&death),
                        None => DaemonStatus::Unavailable {
                            reason: "The owned daemon is no longer accepting connections."
                                .to_owned(),
                            diagnostics: None,
                        },
                    }
                }
                Err(ProbeError::Failed(error)) => DaemonStatus::ContractFailure {
                    reason: error.to_string(),
                    diagnostics: None,
                },
            },
            Phase::Stopping => DaemonStatus::Stopping,
            Phase::Idle => match self.probe_status(&endpoint).await {
                Ok(record) => {
                    // A new reality supersedes the last death.
                    self.clear_death();
                    external_status(&record)
                }
                Err(ProbeError::NotListening) => {
                    let stale_files = self
                        .instance
                        .instance_files()
                        .iter()
                        .any(|file| file.exists());
                    match snapshot.death {
                        Some(death) => died_status(&death),
                        None => DaemonStatus::Stopped { stale_files },
                    }
                }
                Err(ProbeError::Failed(error)) => DaemonStatus::ContractFailure {
                    reason: error.to_string(),
                    diagnostics: None,
                },
            },
        }
    }

    /// Reap the owned child if it exited (classifying the death), then
    /// take a consistent snapshot — one short lock.
    fn reap_and_snapshot(&self) -> Snapshot {
        let mut core = self.lock_core();
        if let Some(child) = core.child.as_mut() {
            if let Some(exit) = child.try_wait() {
                let death = classify_death(exit, core.idle_timeout_secs);
                core.child = None;
                core.phase = Phase::Idle;
                core.death = Some(death);
            }
        }
        Snapshot {
            has_child: core.child.is_some(),
            phase: core.phase,
            model: core.model.clone(),
            idle_timeout_secs: core.idle_timeout_secs,
            death: core.death.clone(),
        }
    }

    /// Reap and classify only; `None` while the child is alive.
    fn reap_death(&self) -> Option<DeathRecord> {
        let mut core = self.lock_core();
        let exit = core.child.as_mut()?.try_wait()?;
        let death = classify_death(exit, core.idle_timeout_secs);
        core.child = None;
        core.phase = Phase::Idle;
        core.death = Some(death.clone());
        Some(death)
    }

    fn flip_starting_to_ready(&self) {
        let mut core = self.lock_core();
        if core.phase == Phase::Starting && core.child.is_some() {
            core.phase = Phase::Ready;
        }
    }

    fn clear_death(&self) {
        self.lock_core().death = None;
    }

    async fn probe_status(&self, endpoint: &EndpointRecord) -> Result<StatusRecord, ProbeError> {
        let address = endpoint.resolve().map_err(|error| match error {
            ResolveError::NoDaemon => ProbeError::NotListening,
            ResolveError::UnusablePortFile(detail) => {
                ProbeError::Failed(DaemonCallError::Contract(detail))
            }
        })?;
        client::connect_and_read_status(&address, &self.config.transport)
            .await
            .map_err(|error| match error {
                DaemonCallError::Transport(TransportError::NotListening) => {
                    ProbeError::NotListening
                }
                other => ProbeError::Failed(other),
            })
    }

    fn ready_status(&self, record: &StatusRecord, idle_timeout_secs: u64) -> DaemonStatus {
        DaemonStatus::Ready {
            model: record.model.clone(),
            hardware: record.hardware.clone(),
            family: record.family.clone(),
            loaded: record.loaded,
            mode: record.mode.clone(),
            warm_serving: record.warm_serving,
            engine_version: record.engine.clone(),
            protocol_version: record.protocol,
            operations: record.operations.clone(),
            idle_timeout_secs,
        }
    }

    // -----------------------------------------------------------------
    // Starting: explicit user action only, duplicates rejected

    pub async fn start(&self, model_id: String) -> Result<DaemonStatus, MutationFailure> {
        let model_id = model_id.trim().to_owned();
        if model_id.is_empty() {
            return Err(MutationFailure::Spawn {
                message: "A model id is required to start the engine daemon.".to_owned(),
                diagnostics: None,
            });
        }
        // Claim the start slot first: duplicate submissions are rejected,
        // never queued.
        {
            let mut core = self.lock_core();
            if core.phase != Phase::Idle {
                return Err(MutationFailure::DuplicateSubmission {
                    message: "A daemon start or stop is already in progress on this instance."
                        .to_owned(),
                });
            }
            core.phase = Phase::Starting;
            core.model = Some(model_id.clone());
            core.idle_timeout_secs = Some(self.config.idle_timeout_secs);
            core.death = None;
        }
        self.start_claimed(model_id).await
    }

    async fn start_claimed(&self, model_id: String) -> Result<DaemonStatus, MutationFailure> {
        let Some(executable) = self.locator.locate(EXECUTABLE_NAME) else {
            return Err(self.abandon_start(MutationFailure::EngineUnavailable {
                message: "No NeuroBrix engine was found on this computer.".to_owned(),
                diagnostics: None,
            }));
        };
        let endpoint = match self.resolve_endpoint().await {
            Ok(endpoint) => endpoint,
            Err(error) => return Err(self.abandon_start(resolution_to_failure(error))),
        };
        if let Err(error) = self.instance.ensure_directory() {
            return Err(self.abandon_start(MutationFailure::Spawn {
                message: "The engine instance directory could not be created.".to_owned(),
                diagnostics: Some(error.to_string()),
            }));
        }
        // A pre-existing daemon on the instance is a conflict — reported
        // with its identity, never killed or adopted. Only a validated
        // answer counts; anything else proceeds to the spawn, whose
        // already-running refusal is the engine's own diagnosis.
        if let Ok(address) = endpoint.resolve() {
            if let Ok(record) =
                client::connect_and_read_status(&address, &self.config.transport).await
            {
                return Err(self.abandon_start(MutationFailure::Conflict {
                    message: format!(
                        "A daemon this Studio session did not start is already serving \
                         '{}' on this instance.",
                        record.model
                    ),
                    engine_version: Some(record.engine),
                }));
            }
        }
        let invocation = ServeInvocation {
            model_id,
            idle_timeout_secs: self.config.idle_timeout_secs,
        };
        let args = lifecycle::serve_argument_array(&invocation);
        let env = self.instance.env();
        let mut child = match self.spawner.spawn(&executable, &args, &env) {
            Ok(child) => child,
            Err(message) => {
                return Err(self.abandon_start(MutationFailure::Spawn {
                    message: "The engine daemon could not be started.".to_owned(),
                    diagnostics: Some(excerpt(&message)),
                }));
            }
        };
        match lifecycle::await_readiness(
            &endpoint,
            child.as_mut(),
            &self.config.transport,
            &self.config.readiness,
        )
        .await
        {
            ReadinessOutcome::Ready(record) => {
                let mut core = self.lock_core();
                core.child = Some(child);
                core.phase = Phase::Ready;
                core.death = None;
                Ok(self.ready_status(&record, self.config.idle_timeout_secs))
            }
            ReadinessOutcome::ChildExited(exit) => {
                if refused_because_already_running(&exit) {
                    return Err(self.abandon_start(MutationFailure::Conflict {
                        message: "A daemon this Studio session did not start is already \
                                  running on this instance."
                            .to_owned(),
                        engine_version: None,
                    }));
                }
                Err(self.abandon_start(MutationFailure::LoadFailed {
                    message: "The engine daemon stopped before it became ready.".to_owned(),
                    exit_code: exit.code,
                    diagnostics: non_empty_excerpt(&exit.stderr_excerpt)
                        .or_else(|| non_empty_excerpt(&exit.stdout_excerpt)),
                }))
            }
            ReadinessOutcome::TimedOut {
                stdout_excerpt,
                stderr_excerpt,
            } => {
                // The window passed, but the child is alive and still
                // loading — it stays owned: a later read finds it if it
                // comes up, and its death is classified if it does not.
                let mut core = self.lock_core();
                core.child = Some(child);
                core.phase = Phase::Starting;
                Err(MutationFailure::ReadinessTimeout {
                    message: format!(
                        "The daemon did not become ready within {} seconds.",
                        self.config.readiness.deadline.as_secs()
                    ),
                    stdout_excerpt: non_empty_excerpt(&stdout_excerpt),
                    stderr_excerpt: non_empty_excerpt(&stderr_excerpt),
                })
            }
            ReadinessOutcome::Refused(reason) => {
                // The daemon is answering something Studio must refuse; it
                // stays owned and starting, and reads keep saying so.
                let mut core = self.lock_core();
                core.child = Some(child);
                core.phase = Phase::Starting;
                Err(MutationFailure::ContractFailure { message: reason })
            }
        }
    }

    /// Return the claimed start slot and hand back the failure.
    fn abandon_start(&self, failure: MutationFailure) -> MutationFailure {
        let mut core = self.lock_core();
        core.phase = Phase::Idle;
        core.model = None;
        core.idle_timeout_secs = None;
        failure
    }

    // -----------------------------------------------------------------
    // Stopping: only a daemon this session owns

    pub async fn stop(&self) -> Result<DaemonStatus, MutationFailure> {
        let mut child = {
            let mut core = self.lock_core();
            match core.phase {
                Phase::Idle => {
                    return Err(MutationFailure::NoOwnedDaemon {
                        message: "No daemon is running that this Studio session owns.".to_owned(),
                    });
                }
                Phase::Stopping => {
                    return Err(MutationFailure::DuplicateSubmission {
                        message: "A daemon stop is already in progress on this instance."
                            .to_owned(),
                    });
                }
                Phase::Starting => {
                    // If it already died, say so; otherwise it is not
                    // ready to stop.
                    if let Some(child) = core.child.as_mut() {
                        if let Some(exit) = child.try_wait() {
                            let death = classify_death(exit, core.idle_timeout_secs);
                            core.child = None;
                            core.phase = Phase::Idle;
                            core.death = Some(death.clone());
                            return Ok(died_status(&death));
                        }
                    }
                    return Err(MutationFailure::NotReady {
                        message: "The daemon is still starting and cannot be stopped yet."
                            .to_owned(),
                    });
                }
                Phase::Ready => {
                    core.phase = Phase::Stopping;
                    core.child
                        .take()
                        .expect("a ready daemon always has its child handle")
                }
            }
        };
        let endpoint = match self.resolve_endpoint().await {
            Ok(endpoint) => endpoint,
            Err(error) => {
                self.restore_child(child, Phase::Ready);
                return Err(resolution_to_failure(error));
            }
        };
        let files = self.instance.instance_files();
        let outcome = lifecycle::stop_owned(
            &endpoint,
            child.as_mut(),
            &files,
            &self.config.transport,
            &self.config.stop,
        )
        .await;
        match outcome {
            StopOutcome::Confirmed => {
                self.finish_stop();
                Ok(DaemonStatus::Stopped { stale_files: false })
            }
            StopOutcome::ConfirmedWithStaleFiles => {
                self.finish_stop();
                Ok(DaemonStatus::Stopped { stale_files: true })
            }
            StopOutcome::Failed { reason } => {
                // Not stopped: the daemon is still running and still
                // owned; a stop timeout is never "stopped".
                self.restore_child(child, Phase::Ready);
                Err(MutationFailure::StopFailed { message: reason })
            }
        }
    }

    fn finish_stop(&self) {
        let mut core = self.lock_core();
        core.phase = Phase::Idle;
        core.model = None;
        core.idle_timeout_secs = None;
        core.death = None;
    }

    /// The daemon outlived the stop attempt: it goes back to its phase,
    /// still owned.
    fn restore_child(&self, child: Box<dyn OwnedChild>, phase: Phase) {
        let mut core = self.lock_core();
        core.child = Some(child);
        core.phase = phase;
    }

    // -----------------------------------------------------------------
    // Endpoint resolution: the engine's own record, cached per session

    async fn resolve_endpoint(&self) -> Result<EndpointRecord, ResolutionError> {
        {
            let cached = self.endpoint.lock().expect("the endpoint lock");
            if let Some(endpoint) = cached.as_ref() {
                return Ok(endpoint.clone());
            }
        }
        let Some(executable) = self.locator.locate(EXECUTABLE_NAME) else {
            return Err(ResolutionError::EngineMissing);
        };
        // The same probe discovery runs, with the instance env so the
        // record reports THIS instance's endpoint. Blocking I/O off the
        // async runtime: `info --json` imports the compute stack.
        let runner = Arc::clone(&self.runner);
        let args: Vec<&str> = DISCOVERY_ARGS.to_vec();
        let env = self.instance.env();
        let timeout = self.config.info_timeout;
        let outcome =
            tokio::task::spawn_blocking(move || runner.run(&executable, &args, &env, timeout))
                .await
                .map_err(|_| ResolutionError::Unavailable {
                    reason: "The engine endpoint check did not complete.".to_owned(),
                    diagnostics: None,
                })?
                .map_err(|failure| match failure {
                    ProcessFailure::Spawn(message) => ResolutionError::Unavailable {
                        reason: "The engine was found, but it could not be started.".to_owned(),
                        diagnostics: Some(excerpt(&message)),
                    },
                    ProcessFailure::Timeout { stderr } => ResolutionError::Unavailable {
                        reason: format!(
                            "The engine did not respond within {} seconds.",
                            timeout.as_secs()
                        ),
                        diagnostics: non_empty_excerpt(&stderr),
                    },
                })?;
        if outcome.exit_code != Some(0) {
            return Err(ResolutionError::Unavailable {
                reason: "The engine's endpoint check failed.".to_owned(),
                diagnostics: non_empty_excerpt(&outcome.stderr),
            });
        }
        let record = match parse_info(&outcome.stdout) {
            Ok(record) => record,
            Err(InfoError::InvalidJson(detail)) => {
                return Err(ResolutionError::ContractFailure {
                    reason: "The engine's info response was not valid JSON.".to_owned(),
                    diagnostics: Some(detail),
                });
            }
            Err(InfoError::UnsupportedSchema(schema)) => {
                return Err(ResolutionError::ContractFailure {
                    reason: format!(
                        "The engine reported info schema '{}', which this version of \
                         Studio does not understand.",
                        excerpt(&schema)
                    ),
                    diagnostics: None,
                });
            }
        };
        match check_supported(&record.engine) {
            Ok(VersionCheck::Compatible) => {}
            Ok(VersionCheck::Incompatible) => {
                return Err(ResolutionError::Incompatible {
                    engine: record.engine,
                });
            }
            Err(_) => {
                return Err(ResolutionError::ContractFailure {
                    reason: format!(
                        "The engine reported a version that is not valid SemVer: '{}'.",
                        excerpt(&record.engine)
                    ),
                    diagnostics: None,
                });
            }
        }
        let Some(endpoint) = record.endpoint else {
            return Err(ResolutionError::NoServingModule);
        };
        *self.endpoint.lock().expect("the endpoint lock") = Some(endpoint.clone());
        Ok(endpoint)
    }
}

fn starting_status(snapshot: &Snapshot) -> DaemonStatus {
    DaemonStatus::Starting {
        model: snapshot.model.clone().unwrap_or_default(),
    }
}

fn external_status(record: &StatusRecord) -> DaemonStatus {
    DaemonStatus::ExternalDaemon {
        model: record.model.clone(),
        hardware: record.hardware.clone(),
        family: record.family.clone(),
        loaded: record.loaded,
        mode: record.mode.clone(),
        warm_serving: record.warm_serving,
        engine_version: record.engine.clone(),
        protocol_version: record.protocol,
        operations: record.operations.clone(),
    }
}

fn died_status(death: &DeathRecord) -> DaemonStatus {
    DaemonStatus::Died {
        expected: death.expected,
        idle_timeout_secs: death.idle_timeout_secs,
        exit_code: death.exit.code,
        diagnostics: non_empty_excerpt(&death.exit.stderr_excerpt),
    }
}

fn resolution_to_status(error: &ResolutionError) -> DaemonStatus {
    match error {
        ResolutionError::EngineMissing => DaemonStatus::Unavailable {
            reason: "No NeuroBrix engine was found on this computer.".to_owned(),
            diagnostics: None,
        },
        ResolutionError::Unavailable {
            reason,
            diagnostics,
        } => DaemonStatus::Unavailable {
            reason: reason.clone(),
            diagnostics: diagnostics.clone(),
        },
        ResolutionError::ContractFailure {
            reason,
            diagnostics,
        } => DaemonStatus::ContractFailure {
            reason: reason.clone(),
            diagnostics: diagnostics.clone(),
        },
        ResolutionError::Incompatible { engine } => DaemonStatus::Unavailable {
            reason: format!(
                "Engine {engine} is outside the tested range {SUPPORTED_ENGINE_RANGE}."
            ),
            diagnostics: None,
        },
        ResolutionError::NoServingModule => DaemonStatus::Unavailable {
            reason: "The engine installation cannot serve daemons; its serving module \
                     is unavailable."
                .to_owned(),
            diagnostics: None,
        },
    }
}

fn resolution_to_failure(error: ResolutionError) -> MutationFailure {
    match error {
        ResolutionError::EngineMissing => MutationFailure::EngineUnavailable {
            message: "No NeuroBrix engine was found on this computer.".to_owned(),
            diagnostics: None,
        },
        ResolutionError::Unavailable {
            reason,
            diagnostics,
        } => MutationFailure::EngineUnavailable {
            message: reason,
            diagnostics,
        },
        ResolutionError::ContractFailure {
            reason,
            diagnostics,
        } => MutationFailure::ContractFailure {
            message: diagnostics
                .map(|detail| format!("{reason} {detail}"))
                .unwrap_or(reason),
        },
        ResolutionError::Incompatible { engine } => MutationFailure::EngineUnavailable {
            message: format!(
                "Engine {engine} is outside the tested range {SUPPORTED_ENGINE_RANGE}."
            ),
            diagnostics: None,
        },
        ResolutionError::NoServingModule => MutationFailure::EngineUnavailable {
            message: "The engine installation cannot serve daemons; its serving module \
                     is unavailable."
                .to_owned(),
            diagnostics: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::process::ProcessOutcome;
    use crate::engine::testing::{
        dead_endpoint, envelope_with, persistent_fake_daemon, status_body, test_timeouts,
    };
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    const EXECUTABLE: &str = "/usr/local/bin/neurobrix";
    const MODEL: &str = "TinyLlama-1.1B-Chat-v1.0";

    /// The info record the fake engine serves, naming the endpoint the
    /// tests control — the same record shape `info --json` produces.
    fn info_stdout(endpoint: &EndpointRecord) -> String {
        serde_json::to_string(&json!({
            "schema": "neurobrix.info/1",
            "engine": "0.5.4",
            "protocol": 1,
            "operations": [
                "generate", "chat", "complete", "template",
                "new_chat", "status", "shutdown"
            ],
            "endpoint": serde_json::to_value(endpoint).expect("the endpoint serializes"),
        }))
        .expect("the info record serializes")
    }

    /// A scripted child: alive while the flag is set, exiting with the
    /// given summary when the test clears the flag.
    struct ScriptedChild {
        alive: Arc<AtomicBool>,
        exit: lifecycle::ExitSummary,
    }

    impl OwnedChild for ScriptedChild {
        fn try_wait(&mut self) -> Option<lifecycle::ExitSummary> {
            if self.alive.load(Ordering::Relaxed) {
                None
            } else {
                Some(self.exit.clone())
            }
        }
        fn excerpts(&self) -> (String, String) {
            (
                self.exit.stdout_excerpt.clone(),
                self.exit.stderr_excerpt.clone(),
            )
        }
    }

    /// One recorded spawn: the executable, its arguments, and the env pairs
    /// it carried.
    type RecordedSpawn = (PathBuf, Vec<String>, Vec<(String, String)>);

    /// The spawner the tests drive: records the invocation, hands out
    /// children whose death the test scripts through the shared flag.
    struct ScriptedSpawner {
        alive: Arc<AtomicBool>,
        exit: lifecycle::ExitSummary,
        recorded: Mutex<Vec<RecordedSpawn>>,
    }

    impl ScriptedSpawner {
        fn alive_exiting_with(exit: lifecycle::ExitSummary) -> (Arc<Self>, Arc<AtomicBool>) {
            let alive = Arc::new(AtomicBool::new(true));
            (
                Arc::new(Self {
                    alive: Arc::clone(&alive),
                    exit,
                    recorded: Mutex::new(Vec::new()),
                }),
                alive,
            )
        }

        fn always_alive() -> (Arc<Self>, Arc<AtomicBool>) {
            Self::alive_exiting_with(lifecycle::ExitSummary {
                code: Some(0),
                stdout_excerpt: String::new(),
                stderr_excerpt: String::new(),
            })
        }

        fn dead_on_arrival(exit: lifecycle::ExitSummary) -> Arc<dyn DaemonSpawner> {
            Arc::new(Self {
                alive: Arc::new(AtomicBool::new(false)),
                exit,
                recorded: Mutex::new(Vec::new()),
            })
        }

        fn recorded(&self) -> Vec<RecordedSpawn> {
            self.recorded.lock().expect("spawner lock").clone()
        }
    }

    impl DaemonSpawner for ScriptedSpawner {
        fn spawn(
            &self,
            program: &Path,
            args: &[String],
            env: &[(String, String)],
        ) -> Result<Box<dyn OwnedChild>, String> {
            self.recorded.lock().expect("spawner lock").push((
                program.to_owned(),
                args.to_vec(),
                env.to_vec(),
            ));
            Ok(Box::new(ScriptedChild {
                alive: Arc::clone(&self.alive),
                exit: self.exit.clone(),
            }))
        }
    }

    /// A locator answering a fixed path (or none at all).
    struct FixedLocator(Option<PathBuf>);
    impl EngineLocator for FixedLocator {
        fn locate(&self, _name: &str) -> Option<PathBuf> {
            self.0.clone()
        }
    }

    /// The runner serving the fixed info record, recording the env each
    /// probe carried.
    struct InfoRunner {
        stdout: String,
        recorded_env: Mutex<Vec<Vec<(String, String)>>>,
    }

    impl ProcessRunner for InfoRunner {
        fn run(
            &self,
            _program: &Path,
            _args: &[&str],
            env: &[(String, String)],
            _timeout: Duration,
        ) -> Result<ProcessOutcome, ProcessFailure> {
            self.recorded_env
                .lock()
                .expect("runner lock")
                .push(env.to_vec());
            Ok(ProcessOutcome {
                exit_code: Some(0),
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }
    }

    /// A runner that cannot reach the engine at all.
    struct FailingRunner(ProcessFailure);
    impl ProcessRunner for FailingRunner {
        fn run(
            &self,
            _program: &Path,
            _args: &[&str],
            _env: &[(String, String)],
            _timeout: Duration,
        ) -> Result<ProcessOutcome, ProcessFailure> {
            Err(self.0.clone())
        }
    }

    fn fast_config() -> DaemonConfig {
        DaemonConfig {
            transport: test_timeouts(),
            readiness: ReadinessConfig {
                deadline: Duration::from_millis(300),
                pause_between_attempts: Duration::from_millis(20),
            },
            stop: StopConfig {
                exit_deadline: Duration::from_millis(200),
                cleanup_grace: Duration::from_millis(200),
            },
            idle_timeout_secs: 1800,
            info_timeout: Duration::from_secs(5),
        }
    }

    fn manager_with(
        cache_dir: &Path,
        endpoint: &EndpointRecord,
        spawner: Arc<dyn DaemonSpawner>,
    ) -> DaemonManager {
        DaemonManager::with_dependencies(
            cache_dir,
            Arc::new(FixedLocator(Some(PathBuf::from(EXECUTABLE)))),
            Arc::new(InfoRunner {
                stdout: info_stdout(endpoint),
                recorded_env: Mutex::new(Vec::new()),
            }),
            spawner,
            fast_config(),
        )
    }

    /// A fresh instance directory per test: unique per call, so parallel
    /// tests never wipe each other's instance files.
    fn temp_dir() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir()
            .join("nbx-studio-daemon-tests")
            .join(format!("instance-{}-{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    /// A named-instance endpoint whose port file the tests control: the
    /// real Windows flow, where the port file only appears when the
    /// daemon binds — and where a start can succeed because the pre-spawn
    /// probe finds nothing to dial.
    fn port_file_endpoint(dir: &Path) -> (EndpointRecord, PathBuf) {
        let port_file = dir.join("engine").join("daemon.port");
        (
            EndpointRecord::Tcp {
                address: "127.0.0.1".parse().expect("loopback"),
                port: None,
                port_file: Some(port_file.clone()),
            },
            port_file,
        )
    }

    /// A start whose daemon comes up DURING the readiness window: the
    /// pre-spawn probe finds nothing (the port file does not exist yet),
    /// a child is spawned, and the daemon records its port while the
    /// readiness poll runs. The only honest way a start can succeed
    /// against a live fixture — a daemon answering at probe time is a
    /// conflict, by design.
    async fn start_with_daemon_coming_up(
        manager: Arc<DaemonManager>,
        spawner: &Arc<ScriptedSpawner>,
        port_file: &Path,
    ) -> (DaemonStatus, Arc<Mutex<Vec<serde_json::Value>>>) {
        let start = tokio::spawn(async move { manager.start(MODEL.to_owned()).await });
        // The probe has passed once the spawn is recorded.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while spawner.recorded().is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "the start never spawned a child"
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let (daemon_endpoint, received) =
            persistent_fake_daemon(envelope_with(status_body())).await;
        let EndpointRecord::Tcp {
            port: Some(bound_port),
            ..
        } = daemon_endpoint
        else {
            panic!("the fake daemon reports its bound port");
        };
        std::fs::write(port_file, bound_port.to_string()).expect("write the port file");
        let status = start
            .await
            .expect("the start task completes")
            .expect("the daemon came up and answered the handshake");
        (status, received)
    }

    #[tokio::test]
    async fn status_reports_no_daemon_when_nothing_answers() {
        let endpoint = dead_endpoint().await;
        let manager = manager_with(&temp_dir(), &endpoint, always_alive_spawner());
        assert_eq!(
            manager.read_status().await,
            DaemonStatus::Stopped { stale_files: false }
        );
    }

    #[tokio::test]
    async fn status_reports_files_a_dead_daemon_left_behind() {
        let dir = temp_dir();
        // A socket file from a daemon that crashed without cleanup.
        let socket = dir.join("engine").join("daemon.sock");
        std::fs::create_dir_all(socket.parent().expect("a parent")).expect("dir");
        std::fs::write(&socket, b"").expect("write the stale socket");
        let endpoint = dead_endpoint().await;
        let manager = manager_with(&dir, &endpoint, always_alive_spawner());
        assert_eq!(
            manager.read_status().await,
            DaemonStatus::Stopped { stale_files: true }
        );
        let _ = std::fs::remove_file(&socket);
    }

    #[tokio::test]
    async fn an_external_daemon_is_reported_with_its_identity_and_never_stopped() {
        let (endpoint, received) = persistent_fake_daemon(envelope_with(status_body())).await;
        let manager = manager_with(&temp_dir(), &endpoint, always_alive_spawner());
        match manager.read_status().await {
            DaemonStatus::ExternalDaemon {
                model,
                engine_version,
                loaded,
                ..
            } => {
                assert_eq!(model, MODEL);
                assert_eq!(engine_version, "0.5.4");
                assert!(loaded);
            }
            other => panic!("expected an external daemon, got {other:?}"),
        }
        // Studio never stops a daemon it does not own.
        assert!(matches!(
            manager.stop().await,
            Err(MutationFailure::NoOwnedDaemon { .. })
        ));
        let requests = received.lock().expect("record lock").clone();
        assert!(
            requests.iter().all(|request| request["method"] == "status"),
            "no shutdown request may ever reach an external daemon"
        );
    }

    #[tokio::test]
    async fn status_is_unavailable_when_the_engine_is_absent() {
        let manager = DaemonManager::with_dependencies(
            &temp_dir(),
            Arc::new(FixedLocator(None)),
            Arc::new(FailingRunner(ProcessFailure::Spawn(String::new()))),
            always_alive_spawner(),
            fast_config(),
        );
        match manager.read_status().await {
            DaemonStatus::Unavailable { reason, .. } => {
                assert!(reason.contains("No NeuroBrix engine"), "reason: {reason}")
            }
            other => panic!("expected unavailable, got {other:?}"),
        }
        assert!(matches!(
            manager.start(MODEL.to_owned()).await,
            Err(MutationFailure::EngineUnavailable { .. })
        ));
    }

    #[tokio::test]
    async fn status_is_unavailable_when_the_engine_does_not_respond() {
        let manager = DaemonManager::with_dependencies(
            &temp_dir(),
            Arc::new(FixedLocator(Some(PathBuf::from(EXECUTABLE)))),
            Arc::new(FailingRunner(ProcessFailure::Timeout {
                stderr: String::new(),
            })),
            always_alive_spawner(),
            fast_config(),
        );
        match manager.read_status().await {
            DaemonStatus::Unavailable { reason, .. } => {
                assert!(reason.contains("did not respond"), "reason: {reason}")
            }
            other => panic!("expected unavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn start_serves_the_argument_array_and_names_the_instance() {
        let dir = temp_dir();
        let (endpoint, port_file) = port_file_endpoint(&dir);
        let (spawner, _) = ScriptedSpawner::always_alive();
        let runner = Arc::new(InfoRunner {
            stdout: info_stdout(&endpoint),
            recorded_env: Mutex::new(Vec::new()),
        });
        let manager = Arc::new(DaemonManager::with_dependencies(
            &dir,
            Arc::new(FixedLocator(Some(PathBuf::from(EXECUTABLE)))),
            Arc::clone(&runner) as Arc<dyn ProcessRunner>,
            managed(&spawner),
            fast_config(),
        ));
        let (status, _) =
            start_with_daemon_coming_up(Arc::clone(&manager), &spawner, &port_file).await;
        match status {
            DaemonStatus::Ready {
                model,
                engine_version,
                protocol_version,
                idle_timeout_secs,
                ..
            } => {
                assert_eq!(model, MODEL);
                assert_eq!(engine_version, "0.5.4");
                assert_eq!(protocol_version, 1);
                assert_eq!(idle_timeout_secs, 1800);
            }
            other => panic!("expected a ready daemon, got {other:?}"),
        }
        let recorded = spawner.recorded();
        assert_eq!(recorded.len(), 1);
        let (program, args, env) = &recorded[0];
        assert_eq!(program, Path::new(EXECUTABLE));
        assert_eq!(
            args,
            &vec![
                "serve".to_owned(),
                "--model".to_owned(),
                MODEL.to_owned(),
                "--foreground".to_owned(),
                "--timeout".to_owned(),
                "1800".to_owned(),
            ]
        );
        let socket = dir.join("engine").join("daemon.sock");
        assert_eq!(
            env,
            &vec![(
                "NBX_SOCKET_PATH".to_owned(),
                socket.to_string_lossy().into_owned()
            )]
        );
        // The endpoint probe carried the same instance env.
        let probes = runner.recorded_env.lock().expect("runner lock").clone();
        assert!(
            probes.iter().any(|env| env.iter().any(
                |(key, value)| key == "NBX_SOCKET_PATH" && Path::new(value) == socket.as_path()
            )),
            "the info probe names the Studio instance"
        );
    }

    #[tokio::test]
    async fn start_refuses_a_daemon_someone_else_left_running() {
        let (endpoint, _) = persistent_fake_daemon(envelope_with(status_body())).await;
        let (spawner, _) = ScriptedSpawner::always_alive();
        let manager = manager_with(&temp_dir(), &endpoint, managed(&spawner));
        match manager.start(MODEL.to_owned()).await {
            Err(MutationFailure::Conflict { engine_version, .. }) => {
                assert_eq!(engine_version.as_deref(), Some("0.5.4"));
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
        assert!(
            spawner.recorded().is_empty(),
            "a conflict never spawns a second daemon"
        );
    }

    #[tokio::test]
    async fn start_rejects_duplicate_submissions() {
        let endpoint = dead_endpoint().await;
        let (spawner, _) = ScriptedSpawner::always_alive();
        let manager = manager_with(&temp_dir(), &endpoint, managed(&spawner));
        assert!(matches!(
            manager.start(MODEL.to_owned()).await,
            Err(MutationFailure::ReadinessTimeout { .. })
        ));
        assert!(matches!(
            manager.start(MODEL.to_owned()).await,
            Err(MutationFailure::DuplicateSubmission { .. })
        ));
        assert_eq!(spawner.recorded().len(), 1, "only one child was spawned");
    }

    #[tokio::test]
    async fn start_surfaces_a_load_failure_with_the_engines_diagnostics() {
        let endpoint = dead_endpoint().await;
        let spawner = ScriptedSpawner::dead_on_arrival(lifecycle::ExitSummary {
            code: Some(1),
            stdout_excerpt: "[Serve] Loading model...".to_owned(),
            stderr_excerpt: "RuntimeError: CUDA out of memory".to_owned(),
        });
        let manager = manager_with(&temp_dir(), &endpoint, spawner);
        match manager.start(MODEL.to_owned()).await {
            Err(MutationFailure::LoadFailed {
                exit_code,
                diagnostics,
                ..
            }) => {
                assert_eq!(exit_code, Some(1));
                assert!(
                    diagnostics.as_deref().unwrap_or_default().contains("CUDA"),
                    "the engine's own diagnostics are surfaced"
                );
            }
            other => panic!("expected a load failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_readiness_timeout_keeps_the_owned_daemon_and_a_later_read_finds_it_ready() {
        let dir = temp_dir();
        // A named instance whose port file does not exist yet: nothing to
        // dial, like a Windows instance whose daemon has not bound.
        let port_file = dir.join("engine").join("daemon.port");
        let endpoint = EndpointRecord::Tcp {
            address: "127.0.0.1".parse().expect("loopback"),
            port: None,
            port_file: Some(port_file.clone()),
        };
        let (spawner, _) = ScriptedSpawner::always_alive();
        let manager = manager_with(&dir, &endpoint, managed(&spawner));
        assert!(matches!(
            manager.start(MODEL.to_owned()).await,
            Err(MutationFailure::ReadinessTimeout { .. })
        ));
        // The daemon comes up and records its port where its clients read
        // it — the read resolves the endpoint fresh and finds it ready.
        let (daemon_endpoint, _) = persistent_fake_daemon(envelope_with(status_body())).await;
        let EndpointRecord::Tcp {
            port: Some(bound_port),
            ..
        } = daemon_endpoint
        else {
            panic!("the fake daemon reports its bound port");
        };
        std::fs::write(&port_file, bound_port.to_string()).expect("write the port file");
        match manager.read_status().await {
            DaemonStatus::Ready { model, .. } => assert_eq!(model, MODEL),
            other => panic!("the read finds the daemon ready, got {other:?}"),
        }
        assert_eq!(spawner.recorded().len(), 1, "no second child was spawned");
    }

    #[tokio::test]
    async fn a_daemon_answering_what_studio_refuses_stays_owned_and_starting() {
        let mut body = status_body();
        body["protocol"] = json!(2);
        let (endpoint, _) = persistent_fake_daemon(envelope_with(body)).await;
        let (spawner, _) = ScriptedSpawner::always_alive();
        let manager = manager_with(&temp_dir(), &endpoint, managed(&spawner));
        assert!(matches!(
            manager.start(MODEL.to_owned()).await,
            Err(MutationFailure::ContractFailure { .. })
        ));
        // The refused daemon is still owned, so reads say it is starting
        // rather than inventing a ready state.
        match manager.read_status().await {
            DaemonStatus::Starting { model } => assert_eq!(model, MODEL),
            other => panic!("expected the refused daemon to stay starting, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_unexpected_death_is_an_explicit_state_until_the_next_start() {
        let (spawner, alive) = ScriptedSpawner::alive_exiting_with(lifecycle::ExitSummary {
            code: Some(1),
            stdout_excerpt: String::new(),
            stderr_excerpt: "Traceback (most recent call last):".to_owned(),
        });
        let dir = temp_dir();
        let (endpoint, port_file) = port_file_endpoint(&dir);
        let manager = Arc::new(manager_with(&dir, &endpoint, managed(&spawner)));
        let (status, _) =
            start_with_daemon_coming_up(Arc::clone(&manager), &spawner, &port_file).await;
        assert!(matches!(status, DaemonStatus::Ready { .. }));
        alive.store(false, Ordering::Relaxed); // the daemon crashes
                                               // and its instance files go with it, as the engine's cleanup does
        let _ = std::fs::remove_file(&port_file);
        match manager.read_status().await {
            DaemonStatus::Died {
                expected,
                exit_code,
                diagnostics,
                ..
            } => {
                assert!(!expected, "a crash is never an expected death");
                assert_eq!(exit_code, Some(1));
                assert!(diagnostics.is_some(), "the stderr excerpt is reported");
            }
            other => panic!("expected the death to be reported, got {other:?}"),
        }
        // A start after the confirmed death proceeds — the record does not
        // block it. (This fixture has no daemon to answer, so the next
        // start's own window passes; the scripted child is made alive
        // again first.)
        alive.store(true, Ordering::Relaxed);
        assert!(matches!(
            manager.start(MODEL.to_owned()).await,
            Err(MutationFailure::ReadinessTimeout { .. })
        ));
        assert_eq!(spawner.recorded().len(), 2, "a fresh start was spawned");
    }

    #[tokio::test]
    async fn an_idle_timeout_death_is_expected_with_the_invocations_timeout() {
        let dir = temp_dir();
        let (endpoint, port_file) = port_file_endpoint(&dir);
        let (spawner, alive) = ScriptedSpawner::alive_exiting_with(lifecycle::ExitSummary {
            code: Some(0),
            stdout_excerpt: "[Daemon] Idle timeout (1800s) — shutting down".to_owned(),
            stderr_excerpt: String::new(),
        });
        let manager = Arc::new(manager_with(&dir, &endpoint, managed(&spawner)));
        let (status, _) =
            start_with_daemon_coming_up(Arc::clone(&manager), &spawner, &port_file).await;
        assert!(matches!(status, DaemonStatus::Ready { .. }));
        alive.store(false, Ordering::Relaxed); // the daemon idles out
        let _ = std::fs::remove_file(&port_file);
        match manager.read_status().await {
            DaemonStatus::Died {
                expected,
                idle_timeout_secs,
                ..
            } => {
                assert!(expected, "an idle timeout is an expected death");
                assert_eq!(
                    idle_timeout_secs,
                    Some(1800),
                    "the timeout comes from the invocation, not a constant"
                );
            }
            other => panic!("expected the idle death, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stop_confirms_an_owned_daemon() {
        let dir = temp_dir();
        let (endpoint, port_file) = port_file_endpoint(&dir);
        let (spawner, alive) = ScriptedSpawner::always_alive();
        let manager = Arc::new(manager_with(&dir, &endpoint, managed(&spawner)));
        let (status, _) =
            start_with_daemon_coming_up(Arc::clone(&manager), &spawner, &port_file).await;
        assert!(matches!(status, DaemonStatus::Ready { .. }));
        alive.store(false, Ordering::Relaxed); // it exits when Studio stops it
                                               // and cleans its instance files, as a confirmed stop requires
        let _ = std::fs::remove_file(&port_file);
        assert_eq!(
            manager.stop().await,
            Ok(DaemonStatus::Stopped { stale_files: false })
        );
        assert_eq!(
            manager.read_status().await,
            DaemonStatus::Stopped { stale_files: false }
        );
    }

    #[tokio::test]
    async fn a_stop_timeout_is_not_stopped_and_the_daemon_stays_ready() {
        let dir = temp_dir();
        let (endpoint, port_file) = port_file_endpoint(&dir);
        let (spawner, alive) = ScriptedSpawner::always_alive();
        let manager = Arc::new(manager_with(&dir, &endpoint, managed(&spawner)));
        let (status, received) =
            start_with_daemon_coming_up(Arc::clone(&manager), &spawner, &port_file).await;
        assert!(matches!(status, DaemonStatus::Ready { .. }));
        // The child never exits, however long the stop waits.
        match manager.stop().await {
            Err(MutationFailure::StopFailed { message }) => {
                assert!(message.contains("did not stop"), "message: {message}")
            }
            other => panic!("a stop timeout is never 'stopped', got {other:?}"),
        }
        assert!(alive.load(Ordering::Relaxed));
        match manager.read_status().await {
            DaemonStatus::Ready { model, .. } => assert_eq!(model, MODEL),
            other => panic!("the daemon stays owned and ready, got {other:?}"),
        }
        // The shutdown request did reach the engine method.
        let requests = received.lock().expect("record lock").clone();
        assert!(
            requests
                .iter()
                .any(|request| request["method"] == "shutdown"),
            "the stop used the engine's shutdown method"
        );
    }

    #[tokio::test]
    async fn stopping_a_still_starting_daemon_is_refused() {
        let endpoint = dead_endpoint().await;
        let (spawner, _) = ScriptedSpawner::always_alive();
        let manager = manager_with(&temp_dir(), &endpoint, managed(&spawner));
        assert!(matches!(
            manager.start(MODEL.to_owned()).await,
            Err(MutationFailure::ReadinessTimeout { .. })
        ));
        assert!(matches!(
            manager.stop().await,
            Err(MutationFailure::NotReady { .. })
        ));
    }

    #[tokio::test]
    async fn stopping_a_starting_daemon_that_already_died_reports_the_death() {
        let endpoint = dead_endpoint().await;
        let (spawner, alive) = ScriptedSpawner::always_alive();
        let manager = manager_with(&temp_dir(), &endpoint, managed(&spawner));
        assert!(matches!(
            manager.start(MODEL.to_owned()).await,
            Err(MutationFailure::ReadinessTimeout { .. })
        ));
        alive.store(false, Ordering::Relaxed); // it died before coming up
        match manager.stop().await {
            Ok(DaemonStatus::Died { .. }) => {
                // its own classified state — not a stop, not a guess
            }
            other => panic!("expected the death, got {other:?}"),
        }
    }

    /// The spawner as its trait object, for manager construction; the
    /// concrete arc stays with the test for its recorded calls.
    fn managed(spawner: &Arc<ScriptedSpawner>) -> Arc<dyn DaemonSpawner> {
        spawner.clone()
    }

    fn always_alive_spawner() -> Arc<dyn DaemonSpawner> {
        let (spawner, _) = ScriptedSpawner::always_alive();
        spawner
    }
}
