//! The owned daemon's lifecycle: how Studio starts it, knows it is ready,
//! stops it cleanly, and survives it dying.
//!
//! Start goes ONLY through the public CLI with an argument array — never
//! a shell string. `serve --model <id> --foreground` makes the spawned
//! child the daemon itself: no fork, no detach; the handle Studio holds
//! IS the daemon's PID. Readiness is a validated `status` handshake
//! within a bounded window — never process spawn, never PID-file
//! presence (the engine writes the PID before the model loads and binds
//! the socket after; server.py `_start_serving`). A stop is confirmed by
//! process exit AND instance-file cleanup; anything less is reported as
//! not stopped, never assumed.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};

use super::client::{self, DaemonCallError};
use super::endpoint::{EndpointRecord, ResolveError};
use super::transport::{TransportError, TransportTimeouts};

/// The pause between reaps and file checks in the bounded stop loop.
const POLL_PAUSE: Duration = Duration::from_millis(100);

/// How much of a child's output pipe Studio keeps: the last few
/// kilobytes for diagnostics, never an unbounded dump.
const TAIL_LIMIT: usize = 8 * 1024;
const DRAIN_CHUNK: usize = 1024;

/// A bounded tail of a child's output pipe: drained off-thread so a
/// chatty daemon can never block on a full pipe.
fn spawn_drain(mut pipe: impl AsyncRead + Unpin + Send + 'static) -> Arc<Mutex<String>> {
    let shared = Arc::new(Mutex::new(String::new()));
    let buffer = Arc::clone(&shared);
    tokio::spawn(async move {
        let mut chunk = [0u8; DRAIN_CHUNK];
        loop {
            match pipe.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let mut tail = buffer.lock().expect("tail lock");
                    tail.push_str(&String::from_utf8_lossy(&chunk[..read]));
                    if tail.len() > TAIL_LIMIT {
                        // Keep the last TAIL_LIMIT bytes, cut at a char
                        // boundary so multibyte output is not mangled.
                        let mut cut = tail.len() - TAIL_LIMIT;
                        while cut < tail.len() && !tail.is_char_boundary(cut) {
                            cut += 1;
                        }
                        tail.drain(..cut);
                    }
                }
            }
        }
    });
    shared
}

/// What a reaped child looked like. `None` exit code = terminated by a
/// signal; reported, never guessed.
#[derive(Debug, Clone, PartialEq)]
pub struct ExitSummary {
    pub code: Option<i32>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
}

/// A daemon Studio started and still owns in this session. Sync on
/// purpose: the flows poll with async sleeps, so no method ever blocks
/// and the trait stays object-safe for the fakes.
pub trait OwnedChild: Send {
    fn pid(&self) -> u32;
    /// Non-blocking reap; `None` while the child is alive.
    fn try_wait(&mut self) -> Option<ExitSummary>;
    /// The bounded output tails so far, `(stdout, stderr)`, without reaping.
    fn excerpts(&self) -> (String, String);
}

/// The real child behind `OwnedChild`: a tokio process with both pipes
/// drained into bounded tails. Dropping the handle does NOT kill the
/// daemon — an owner that walks away leaves it running, and app-quit
/// behaviour is a documented follow-up of its own.
struct TokioOwnedChild {
    child: Child,
    pid: u32,
    stdout_tail: Arc<Mutex<String>>,
    stderr_tail: Arc<Mutex<String>>,
}

impl OwnedChild for TokioOwnedChild {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn try_wait(&mut self) -> Option<ExitSummary> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(ExitSummary {
                code: status.code(),
                stdout_excerpt: self.stdout_tail.lock().expect("stdout tail").clone(),
                stderr_excerpt: self.stderr_tail.lock().expect("stderr tail").clone(),
            }),
            Ok(None) => None,
            // An error asking the OS is treated as still alive: the
            // bounded deadlines, not a guess, end the loops.
            Err(_) => None,
        }
    }

    fn excerpts(&self) -> (String, String) {
        (
            self.stdout_tail.lock().expect("stdout tail").clone(),
            self.stderr_tail.lock().expect("stderr tail").clone(),
        )
    }
}

/// Start only through the public CLI, argument array only — there is no
/// shell anywhere, so no interpolation is possible by construction.
pub trait DaemonSpawner: Send + Sync {
    fn spawn(
        &self,
        executable: &Path,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<Box<dyn OwnedChild>, String>;
}

pub struct SystemDaemonSpawner;

impl DaemonSpawner for SystemDaemonSpawner {
    fn spawn(
        &self,
        executable: &Path,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<Box<dyn OwnedChild>, String> {
        let mut child = Command::new(executable)
            .args(args)
            // The parent environment is inherited, never cleared — the
            // engine resolves PATH and home the way an interactive call
            // would; explicit pairs name the instance (NBX_SOCKET_PATH).
            .envs(env.iter().cloned())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        let pid = child.id().expect("a spawned child has a pid");
        let stdout_tail = spawn_drain(child.stdout.take().expect("stdout piped"));
        let stderr_tail = spawn_drain(child.stderr.take().expect("stderr piped"));
        Ok(Box::new(TokioOwnedChild {
            child,
            pid,
            stdout_tail,
            stderr_tail,
        }))
    }
}

/// The exact invocation Studio issues. The model id is an explicit input —
/// Studio never lists, picks, or validates models (Stage 4); the engine
/// is the authority. The idle timeout is passed explicitly so the value is
/// recorded in this invocation and read back from it — never hardcoded.
#[derive(Debug, Clone, PartialEq)]
pub struct ServeInvocation {
    pub model_id: String,
    pub idle_timeout_secs: u64,
}

/// `neurobrix serve --model <id> --foreground [--timeout N]`: the argument
/// array, in the engine's documented order.
pub fn serve_argument_array(invocation: &ServeInvocation) -> Vec<String> {
    vec![
        "serve".to_owned(),
        "--model".to_owned(),
        invocation.model_id.clone(),
        "--foreground".to_owned(),
        "--timeout".to_owned(),
        invocation.idle_timeout_secs.to_string(),
    ]
}

/// The engine's already-running refusal: cmd_serve prints
/// "[Serve] Daemon already running (PID …)" and exits 1 — the race where
/// a daemon started between Studio's pre-check and its spawn. Recognized
/// for what it is, never treated as a crash.
pub fn refused_because_already_running(exit: &ExitSummary) -> bool {
    exit.code == Some(1)
        && exit
            .stdout_excerpt
            .contains("[Serve] Daemon already running")
}

// ---------------------------------------------------------------------------
// Readiness: a validated status, never a spawn or a PID file

/// Bounded readiness. The deadline covers the model load — the engine's
/// own background start waits up to 300 s for its ready signal, so
/// Studio's budget matches that scale; the pause spaces the status
/// attempts while the daemon loads.
#[derive(Debug, Clone, Copy)]
pub struct ReadinessConfig {
    pub deadline: Duration,
    pub pause_between_attempts: Duration,
}

#[derive(Debug, PartialEq)]
pub enum ReadinessOutcome {
    /// The daemon answered a validated handshake; ready, and owned.
    Ready(client::StatusRecord),
    /// The child died before ever answering: a load failure, or the
    /// instance's already-running refusal (cmd_serve exits 1).
    ChildExited(ExitSummary),
    /// The deadline passed with the child alive and silent; the output so
    /// far carries what the engine was doing.
    TimedOut {
        stdout_excerpt: String,
        stderr_excerpt: String,
    },
    /// The daemon answered something Studio must refuse — surfaced
    /// immediately, never retried into success.
    Refused(String),
}

/// Readiness = a validated `status` within the window. The endpoint is
/// resolved fresh on every attempt: on a named Windows instance the port
/// file only appears when the daemon binds, so an address resolved once
/// before the loop would never see it.
pub async fn await_readiness(
    endpoint: &EndpointRecord,
    child: &mut dyn OwnedChild,
    timeouts: &TransportTimeouts,
    config: &ReadinessConfig,
) -> ReadinessOutcome {
    let deadline = Instant::now() + config.deadline;
    loop {
        // The child's early death carries the engine's own diagnostics.
        if let Some(exit) = child.try_wait() {
            return ReadinessOutcome::ChildExited(exit);
        }
        match endpoint.resolve() {
            Ok(address) => match client::connect_and_read_status(&address, timeouts).await {
                Ok(record) => return ReadinessOutcome::Ready(record),
                // Nothing listening yet is the daemon still loading.
                Err(DaemonCallError::Transport(TransportError::NotListening)) => {}
                Err(other) => {
                    // The daemon answered something Studio must refuse, or
                    // the transport failed in a way polling cannot fix; a
                    // death right after says more than the deadline ever
                    // will.
                    if let Some(exit) = child.try_wait() {
                        return ReadinessOutcome::ChildExited(exit);
                    }
                    return ReadinessOutcome::Refused(other.to_string());
                }
            },
            // A named Windows instance names no port until it binds.
            Err(ResolveError::NoDaemon) => {}
            Err(error) => return ReadinessOutcome::Refused(error.to_string()),
        }
        if Instant::now() >= deadline {
            let (stdout_excerpt, stderr_excerpt) = child.excerpts();
            return ReadinessOutcome::TimedOut {
                stdout_excerpt,
                stderr_excerpt,
            };
        }
        tokio::time::sleep(config.pause_between_attempts).await;
    }
}

// ---------------------------------------------------------------------------
// Death: expected and unexpected are different states

/// What the owned daemon's death looked like. Exit code 0 is the serve
/// loop breaking cleanly — the idle timeout (server.py prints
/// `[Daemon] Idle timeout (Ns) — shutting down`) or an external stop;
/// never zero, or a signal, is unexpected.
#[derive(Debug, Clone, PartialEq)]
pub struct DeathRecord {
    pub expected: bool,
    /// Some when this was the idle timeout of the serve invocation
    /// Studio issued — read from that invocation, never hardcoded.
    pub idle_timeout_secs: Option<u64>,
    pub exit: ExitSummary,
}

pub fn classify_death(exit: ExitSummary, invocation_idle_timeout_secs: Option<u64>) -> DeathRecord {
    let idle_timeout_secs = if exit.code == Some(0) && exit.stdout_excerpt.contains("Idle timeout")
    {
        invocation_idle_timeout_secs
    } else {
        None
    };
    DeathRecord {
        expected: exit.code == Some(0),
        idle_timeout_secs,
        exit,
    }
}

// ---------------------------------------------------------------------------
// Stop: confirmed by exit and cleanup, never by the response alone

#[derive(Debug, Clone, Copy)]
pub struct StopConfig {
    /// How long the child has to exit after the shutdown request.
    pub exit_deadline: Duration,
    /// How long the instance's files have to disappear after exit.
    pub cleanup_grace: Duration,
}

#[derive(Debug, PartialEq)]
pub enum StopOutcome {
    /// The child exited and the instance's files are gone — the only
    /// confirmed stop.
    Confirmed,
    /// The child exited but files from the dead daemon remain; the caller
    /// reports them as stale.
    ConfirmedWithStaleFiles,
    /// Not stopped: the shutdown request never landed, or the child
    /// outlived its exit deadline. The daemon is still running.
    Failed { reason: String },
}

/// Stop an owned daemon: the shutdown request on Studio's own connection,
/// then a confirmed exit and instance-file cleanup. A stop that times out
/// is not "stopped" — the outcome says which it is.
pub async fn stop_owned(
    endpoint: &EndpointRecord,
    child: &mut dyn OwnedChild,
    instance_files: &[PathBuf],
    timeouts: &TransportTimeouts,
    config: &StopConfig,
) -> StopOutcome {
    // It may have stopped on its own just before this call — an expected
    // death with its own reason, not a failure.
    if child.try_wait().is_some() {
        return confirm_files(instance_files, config.cleanup_grace).await;
    }
    // The stop dials what the record says right now, not an address
    // resolved before the daemon could have moved.
    let request = match endpoint.resolve() {
        Ok(address) => client::request_shutdown(&address, timeouts).await,
        Err(error) => {
            if child.try_wait().is_some() {
                return confirm_files(instance_files, config.cleanup_grace).await;
            }
            return StopOutcome::Failed {
                reason: format!("the instance's endpoint could not be resolved: {error}"),
            };
        }
    };
    if let Err(error) = request {
        // A refusal between the reap and the dial can mean it died in
        // between; that is stopped, with its own reason.
        if child.try_wait().is_some() {
            return confirm_files(instance_files, config.cleanup_grace).await;
        }
        return StopOutcome::Failed {
            reason: format!("the shutdown request failed: {error}"),
        };
    }
    let exit_deadline = Instant::now() + config.exit_deadline;
    loop {
        if child.try_wait().is_some() {
            return confirm_files(instance_files, config.cleanup_grace).await;
        }
        if Instant::now() >= exit_deadline {
            return StopOutcome::Failed {
                reason: format!(
                    "the daemon did not stop within {} seconds",
                    config.exit_deadline.as_secs()
                ),
            };
        }
        tokio::time::sleep(POLL_PAUSE).await;
    }
}

/// The daemon removes its instance files on the way out (server.py
/// `_cleanup`); cleanup is confirmed, not assumed. The engine cleans the
/// socket, the pid, and a named Windows instance's port file — the log
/// persists by its own design.
async fn confirm_files(files: &[PathBuf], grace: Duration) -> StopOutcome {
    let deadline = Instant::now() + grace;
    loop {
        if files.iter().all(|file| !file.exists()) {
            return StopOutcome::Confirmed;
        }
        if Instant::now() >= deadline {
            return StopOutcome::ConfirmedWithStaleFiles;
        }
        tokio::time::sleep(POLL_PAUSE).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testing::{
        dead_endpoint, envelope_with, fake_daemon, status_body, test_timeouts,
    };
    use serde_json::json;

    fn invocation() -> ServeInvocation {
        ServeInvocation {
            model_id: "TinyLlama-1.1B-Chat-v1.0".to_owned(),
            idle_timeout_secs: 1800,
        }
    }

    /// A test child: dies when scripted, alive until then, with the
    /// output tails the tests gave it.
    struct FakeChild {
        exit: Option<ExitSummary>,
        /// How many reaps report "still alive" before the exit appears —
        /// the stop path must poll.
        exit_after_polls: u32,
        stdout: String,
        stderr: String,
    }

    impl OwnedChild for FakeChild {
        fn pid(&self) -> u32 {
            4242
        }
        fn try_wait(&mut self) -> Option<ExitSummary> {
            if self.exit_after_polls > 0 {
                self.exit_after_polls -= 1;
                return None;
            }
            self.exit.clone()
        }
        fn excerpts(&self) -> (String, String) {
            (self.stdout.clone(), self.stderr.clone())
        }
    }

    fn alive_child() -> FakeChild {
        FakeChild {
            exit: None,
            exit_after_polls: 0,
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    fn fast_readiness() -> ReadinessConfig {
        ReadinessConfig {
            deadline: Duration::from_secs(2),
            pause_between_attempts: Duration::from_millis(20),
        }
    }

    fn fast_stop() -> StopConfig {
        StopConfig {
            exit_deadline: Duration::from_secs(2),
            cleanup_grace: Duration::from_millis(300),
        }
    }

    fn shutdown_answer() -> serde_json::Value {
        json!({"protocol": 1, "engine": "0.5.4", "result": {"status": "ok"}})
    }

    #[test]
    fn the_serve_invocation_is_an_exact_argument_array() {
        assert_eq!(
            serve_argument_array(&invocation()),
            vec![
                "serve".to_owned(),
                "--model".to_owned(),
                "TinyLlama-1.1B-Chat-v1.0".to_owned(),
                "--foreground".to_owned(),
                "--timeout".to_owned(),
                "1800".to_owned(),
            ]
        );
        for argument in serve_argument_array(&invocation()) {
            assert!(
                !argument
                    .chars()
                    .any(|c| c.is_whitespace() || "|;&$><`".contains(c)),
                "the argument '{argument}' must stay a shell-free token"
            );
        }
    }

    #[test]
    fn the_already_running_refusal_is_recognized_not_treated_as_a_crash() {
        let refusal = ExitSummary {
            code: Some(1),
            stdout_excerpt: "[Serve] Daemon already running (PID 4711)".to_owned(),
            stderr_excerpt: String::new(),
        };
        assert!(refused_because_already_running(&refusal));
        let crash = ExitSummary {
            code: Some(1),
            stdout_excerpt: "Traceback (most recent call last):".to_owned(),
            stderr_excerpt: String::new(),
        };
        assert!(!refused_because_already_running(&crash));
    }

    #[tokio::test]
    async fn readiness_succeeds_when_a_daemon_answers_the_handshake() {
        let (endpoint, _) = fake_daemon(envelope_with(status_body())).await;
        let mut child = alive_child();
        let outcome =
            await_readiness(&endpoint, &mut child, &test_timeouts(), &fast_readiness()).await;
        assert!(matches!(outcome, ReadinessOutcome::Ready(_)));
    }

    #[tokio::test]
    async fn readiness_reports_the_childs_death_with_the_engines_diagnostics() {
        let mut child = FakeChild {
            exit: Some(ExitSummary {
                code: Some(1),
                stdout_excerpt: "[Serve] Loading model...".to_owned(),
                stderr_excerpt: "RuntimeError: CUDA out of memory".to_owned(),
            }),
            exit_after_polls: 0,
            stdout: String::new(),
            stderr: String::new(),
        };
        let endpoint = dead_endpoint().await;
        match await_readiness(&endpoint, &mut child, &test_timeouts(), &fast_readiness()).await {
            ReadinessOutcome::ChildExited(exit) => {
                assert_eq!(exit.code, Some(1));
                assert!(exit.stderr_excerpt.contains("CUDA out of memory"));
            }
            other => panic!("expected the child's death, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn readiness_times_out_with_the_childs_output_so_far() {
        let mut child = FakeChild {
            exit: None,
            exit_after_polls: 0,
            stdout: "[Serve] Loading model in background...".to_owned(),
            stderr: "downloading weights: 47%".to_owned(),
        };
        let endpoint = dead_endpoint().await;
        let config = ReadinessConfig {
            deadline: Duration::from_millis(150),
            pause_between_attempts: Duration::from_millis(20),
        };
        match await_readiness(&endpoint, &mut child, &test_timeouts(), &config).await {
            ReadinessOutcome::TimedOut {
                stdout_excerpt,
                stderr_excerpt,
            } => {
                assert!(stdout_excerpt.contains("Loading model"));
                assert!(stderr_excerpt.contains("downloading"));
            }
            other => panic!("expected a readiness timeout, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn readiness_refuses_a_daemon_that_breaks_the_contract() {
        let mut body = status_body();
        body["protocol"] = json!(2);
        let (endpoint, _) = fake_daemon(envelope_with(body)).await;
        let mut child = alive_child();
        match await_readiness(&endpoint, &mut child, &test_timeouts(), &fast_readiness()).await {
            ReadinessOutcome::Refused(reason) => {
                assert!(reason.contains("protocol"), "reason: {reason}")
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn readiness_keeps_waiting_while_a_named_instance_names_no_port() {
        // A named Windows instance has no port file until the daemon binds;
        // "nothing to dial yet" is still loading, not a refusal.
        let mut child = alive_child();
        let endpoint = EndpointRecord::Tcp {
            address: "127.0.0.1".parse().expect("loopback"),
            port: None,
            port_file: Some(PathBuf::from("/definitely/not/a/real/daemon.port")),
        };
        let config = ReadinessConfig {
            deadline: Duration::from_millis(120),
            pause_between_attempts: Duration::from_millis(20),
        };
        match await_readiness(&endpoint, &mut child, &test_timeouts(), &config).await {
            ReadinessOutcome::TimedOut { .. } => {}
            other => panic!("expected the window to keep waiting, got {other:?}"),
        }
    }

    #[test]
    fn an_idle_timeout_death_is_expected_and_carries_the_invocations_timeout() {
        let exit = ExitSummary {
            code: Some(0),
            stdout_excerpt: "[Daemon] Idle timeout (1800s) — shutting down".to_owned(),
            stderr_excerpt: String::new(),
        };
        let death = classify_death(exit, Some(1800));
        assert!(death.expected);
        assert_eq!(death.idle_timeout_secs, Some(1800));
    }

    #[test]
    fn an_external_stop_is_expected_without_an_idle_timeout() {
        let exit = ExitSummary {
            code: Some(0),
            stdout_excerpt: "[Daemon] Received SIGTERM — shutting down".to_owned(),
            stderr_excerpt: String::new(),
        };
        let death = classify_death(exit, Some(1800));
        assert!(death.expected);
        assert_eq!(death.idle_timeout_secs, None);
    }

    #[test]
    fn a_crash_or_a_signal_is_an_unexpected_death() {
        for exit in [
            ExitSummary {
                code: Some(1),
                stdout_excerpt: String::new(),
                stderr_excerpt: "Traceback (most recent call last):".to_owned(),
            },
            ExitSummary {
                code: None, // a signal has no exit code
                stdout_excerpt: String::new(),
                stderr_excerpt: String::new(),
            },
        ] {
            let death = classify_death(exit.clone(), Some(1800));
            assert!(!death.expected, "exit: {exit:?}");
            assert_eq!(death.idle_timeout_secs, None);
        }
    }

    #[tokio::test]
    async fn a_stop_confirms_exit_and_instance_cleanup() {
        let (endpoint, _) = fake_daemon(shutdown_answer()).await;
        let mut child = FakeChild {
            // The daemon exits shortly after taking the request — the stop
            // path must keep polling for the exit.
            exit: Some(ExitSummary {
                code: Some(0),
                stdout_excerpt: "[Daemon] Cleaned up".to_owned(),
                stderr_excerpt: String::new(),
            }),
            exit_after_polls: 2,
            stdout: String::new(),
            stderr: String::new(),
        };
        // The instance files are already gone — the daemon cleaned up.
        let files = vec![PathBuf::from("/definitely/not/a/real/daemon.sock")];
        let outcome = stop_owned(
            &endpoint,
            &mut child,
            &files,
            &test_timeouts(),
            &fast_stop(),
        )
        .await;
        assert_eq!(outcome, StopOutcome::Confirmed);
    }

    #[tokio::test]
    async fn a_stop_that_never_stops_the_child_is_not_stopped() {
        let (endpoint, _) = fake_daemon(shutdown_answer()).await;
        let mut child = alive_child(); // never exits
        let config = StopConfig {
            exit_deadline: Duration::from_millis(150),
            cleanup_grace: Duration::from_millis(100),
        };
        match stop_owned(&endpoint, &mut child, &[], &test_timeouts(), &config).await {
            StopOutcome::Failed { reason } => {
                assert!(reason.contains("did not stop"), "reason: {reason}")
            }
            other => panic!("a stop timeout is never 'stopped', got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_daemon_that_died_before_the_stop_is_confirmed_stopped() {
        let mut child = FakeChild {
            exit: Some(ExitSummary {
                code: Some(0),
                stdout_excerpt: String::new(),
                stderr_excerpt: String::new(),
            }),
            exit_after_polls: 0,
            stdout: String::new(),
            stderr: String::new(),
        };
        let endpoint = dead_endpoint().await; // nothing answers; it is gone
        let outcome = stop_owned(&endpoint, &mut child, &[], &test_timeouts(), &fast_stop()).await;
        assert_eq!(outcome, StopOutcome::Confirmed);
    }

    #[tokio::test]
    async fn a_live_child_that_cannot_be_reached_is_an_explicit_stop_failure() {
        let mut child = alive_child(); // alive, but nothing answers its instance
        let endpoint = dead_endpoint().await;
        match stop_owned(&endpoint, &mut child, &[], &test_timeouts(), &fast_stop()).await {
            StopOutcome::Failed { reason } => {
                assert!(
                    reason.contains("shutdown request failed"),
                    "reason: {reason}"
                )
            }
            other => panic!("expected an explicit failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn files_surviving_a_confirmed_exit_are_reported_stale() {
        // A daemon that crashed before cleanup leaves its socket behind.
        let dir = std::env::temp_dir().join("nbx-studio-lifecycle-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let leftover = dir.join(format!("stale-{}.sock", std::process::id()));
        std::fs::write(&leftover, b"").expect("write the leftover file");
        let mut child = FakeChild {
            exit: Some(ExitSummary {
                code: Some(0),
                stdout_excerpt: String::new(),
                stderr_excerpt: String::new(),
            }),
            exit_after_polls: 0,
            stdout: String::new(),
            stderr: String::new(),
        };
        let endpoint = dead_endpoint().await;
        let outcome = stop_owned(
            &endpoint,
            &mut child,
            std::slice::from_ref(&leftover),
            &test_timeouts(),
            &StopConfig {
                exit_deadline: Duration::from_secs(1),
                cleanup_grace: Duration::from_millis(150),
            },
        )
        .await;
        assert_eq!(outcome, StopOutcome::ConfirmedWithStaleFiles);
        let _ = std::fs::remove_file(&leftover);
    }

    /// Real evidence for the spawner machinery: a real process, real
    /// pipes, real tails — no engine, no model. The drains may lag the
    /// exit by a scheduling beat, so the tails are polled until they
    /// settle, within a deadline.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_real_spawner_reaps_exit_and_output_tails() {
        let mut child = SystemDaemonSpawner
            .spawn(
                Path::new("sh"),
                &["-c".to_owned(), "echo out; echo err >&2; exit 7".to_owned()],
                &[],
            )
            .expect("spawn a real test child");
        assert!(child.pid() > 0);
        let exit = reap_with_deadline(child.as_mut()).expect("the test child exits");
        assert_eq!(exit.code, Some(7));
        assert!(
            settled_contains(child.as_ref(), |stdout, stderr| {
                stdout.contains("out") && stderr.contains("err")
            })
            .await
        );
    }

    /// The tails keep the last few kilobytes of a flood, never the whole
    /// thing — and they keep the END of it, which is what diagnostics
    /// need.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_output_tails_are_bounded() {
        let mut child = SystemDaemonSpawner
            .spawn(
                Path::new("sh"),
                &[
                    "-c".to_owned(),
                    // 1818 whole 11-byte lines: the flood ends on a line
                    // boundary, so the tail's end is deterministic.
                    "yes 0123456789 | head -c 19998; exit 0".to_owned(),
                ],
                &[],
            )
            .expect("spawn a real test child");
        let exit = reap_with_deadline(child.as_mut()).expect("the test child exits");
        assert_eq!(exit.code, Some(0));
        let settled =
            settled_contains(child.as_ref(), |stdout, _| stdout.ends_with("0123456789\n")).await;
        assert!(settled, "the tail must keep the end of the flood");
        let (stdout, _) = child.excerpts();
        assert!(
            stdout.len() <= TAIL_LIMIT + 4,
            "the tail is bounded to ~{TAIL_LIMIT} bytes, got {}",
            stdout.len()
        );
    }

    #[cfg(unix)]
    fn reap_with_deadline(child: &mut dyn OwnedChild) -> Option<ExitSummary> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(exit) = child.try_wait() {
                return Some(exit);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(unix)]
    async fn settled_contains(child: &dyn OwnedChild, done: fn(&str, &str) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let (stdout, stderr) = child.excerpts();
            if done(&stdout, &stderr) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
