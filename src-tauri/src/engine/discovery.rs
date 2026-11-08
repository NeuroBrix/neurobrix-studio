use super::info::{parse_info, InfoError};
use super::process::{EngineLocator, ProcessFailure, ProcessRunner};
use super::version::{check_supported, VersionCheck, VersionError, SUPPORTED_ENGINE_RANGE};
use super::{excerpt, non_empty_excerpt, EngineAvailability, DISCOVERY_ARGS, DISCOVERY_TIMEOUT};

pub const EXECUTABLE_NAME: &str = "neurobrix";

/// Read-only probe; every failure maps to a typed availability state.
pub fn discover(locator: &dyn EngineLocator, runner: &dyn ProcessRunner) -> EngineAvailability {
    let Some(executable) = locator.locate(EXECUTABLE_NAME) else {
        return EngineAvailability::Absent;
    };

    // The availability probe never names an instance: it reads the
    // installed engine itself, not a particular daemon instance.
    let outcome = match runner.run(&executable, DISCOVERY_ARGS, &[], DISCOVERY_TIMEOUT) {
        Ok(outcome) => outcome,
        Err(ProcessFailure::Spawn(message)) => {
            return EngineAvailability::Unavailable {
                reason: "The engine was found, but it could not be started.".into(),
                diagnostics: Some(excerpt(&message)),
            };
        }
        Err(ProcessFailure::Timeout { stderr }) => {
            return EngineAvailability::Unavailable {
                reason: format!(
                    "The engine did not respond within {} seconds.",
                    DISCOVERY_TIMEOUT.as_secs()
                ),
                diagnostics: non_empty_excerpt(&stderr),
            };
        }
    };

    // A signal-killed process has no exit code: report it, never guess.
    let Some(exit_code) = outcome.exit_code else {
        return EngineAvailability::Unavailable {
            reason: "The engine stopped unexpectedly while reporting its status.".into(),
            diagnostics: non_empty_excerpt(&outcome.stderr),
        };
    };

    if exit_code != 0 {
        // Engines before 0.5.4 do not know `--json`: cannot answer, not broken.
        return EngineAvailability::Unavailable {
            reason: format!(
                "The installed engine does not support the status command \
                 (exit code {exit_code}). It may need to be upgraded to an \
                 engine version that provides `neurobrix info --json`."
            ),
            diagnostics: non_empty_excerpt(&outcome.stderr),
        };
    }

    let record = match parse_info(&outcome.stdout) {
        Ok(record) => record,
        Err(InfoError::InvalidJson(detail)) => {
            return EngineAvailability::ContractFailure {
                reason: "The engine's status response was not valid JSON.".into(),
                diagnostics: Some(excerpt(&detail)),
            };
        }
        Err(InfoError::UnsupportedSchema(schema)) => {
            return EngineAvailability::ContractFailure {
                reason: format!(
                    "The engine reported status schema '{}', which this version of \
                     Studio does not understand.",
                    excerpt(&schema)
                ),
                diagnostics: None,
            };
        }
    };

    match check_supported(&record.engine) {
        Ok(VersionCheck::Compatible) => EngineAvailability::Compatible {
            engine_version: record.engine,
            protocol_version: record.protocol.map(|value| value.to_string()),
            capabilities: record.operations,
        },
        Ok(VersionCheck::Incompatible) => EngineAvailability::Incompatible {
            engine_version: record.engine,
            supported_range: SUPPORTED_ENGINE_RANGE.to_owned(),
        },
        Err(VersionError::Malformed) => EngineAvailability::ContractFailure {
            reason: format!(
                "The engine reported a version that is not valid SemVer: '{}'.",
                excerpt(&record.engine)
            ),
            diagnostics: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::process::{EngineLocator, ProcessFailure, ProcessOutcome, ProcessRunner};
    use crate::engine::DIAGNOSTIC_LIMIT;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::Duration;

    const EXECUTABLE: &str = "/usr/local/bin/neurobrix";

    struct FixedLocator(Option<PathBuf>);
    impl EngineLocator for FixedLocator {
        fn locate(&self, _name: &str) -> Option<PathBuf> {
            self.0.clone()
        }
    }

    /// What the fake recorded: the program it was asked to run, its
    /// arguments, and the environment pairs it was asked to set. Named
    /// fields, not tuple positions — call sites assert what they mean.
    #[derive(Clone, Default)]
    struct RecordedCall {
        program: Option<PathBuf>,
        args: Vec<String>,
        env: Vec<(String, String)>,
    }

    struct FakeRunner {
        outcome: Result<ProcessOutcome, ProcessFailure>,
        seen: Mutex<RecordedCall>,
    }
    impl ProcessRunner for FakeRunner {
        fn run(
            &self,
            program: &Path,
            args: &[&str],
            env: &[(String, String)],
            _timeout: Duration,
        ) -> Result<ProcessOutcome, ProcessFailure> {
            let mut seen = self.seen.lock().expect("fake runner lock");
            seen.program = Some(program.to_owned());
            seen.args = args.iter().map(|a| a.to_string()).collect();
            seen.env = env.to_vec();
            self.outcome.clone()
        }
    }

    fn runner_with(outcome: Result<ProcessOutcome, ProcessFailure>) -> FakeRunner {
        FakeRunner {
            outcome,
            seen: Mutex::new(RecordedCall::default()),
        }
    }

    fn discover_with(outcome: Result<ProcessOutcome, ProcessFailure>) -> EngineAvailability {
        discover(
            &FixedLocator(Some(EXECUTABLE.into())),
            &runner_with(outcome),
        )
    }

    fn ok(stdout: &str) -> Result<ProcessOutcome, ProcessFailure> {
        Ok(ProcessOutcome {
            exit_code: Some(0),
            stdout: stdout.to_owned(),
            stderr: String::new(),
        })
    }

    fn exit_with(code: i32, stderr: &str) -> Result<ProcessOutcome, ProcessFailure> {
        Ok(ProcessOutcome {
            exit_code: Some(code),
            stdout: String::new(),
            stderr: stderr.to_owned(),
        })
    }

    /// Verified 0.6.2 record (paths redacted); unknown fields kept to prove
    /// they are ignored. The schema-2 record carries the compute branches
    /// and the detected devices where schema-1 carried `torch` and
    /// `cuda_available` — Studio consumes neither. The interpreter key is
    /// omitted because the guard bans that literal in this module.
    fn info_record(engine_version: &str) -> String {
        format!(
            r#"{{"schema":"neurobrix.info/2","engine":"{engine_version}","version":"{engine_version}","package":"/src/neurobrix","cache":"/home/user/.neurobrix/cache","store":"/home/user/.neurobrix/store","models":[{{"name":"test-model","size_bytes":384061043}}],"hardware_profiles":["a100-80g","h100-80g"],"branches":{{"torch":null,"triton":null}},"gpus":[],"protocol":1,"protocol_range":[1,1],"endpoint":{{"kind":"unix","path":"/home/user/.neurobrix/daemon.sock"}},"operations":["generate","chat","complete","template","new_chat","status","cancel","shutdown"]}}"#
        )
    }

    #[test]
    fn absent_when_executable_is_not_found() {
        let runner = runner_with(ok(&info_record("0.6.2")));
        let result = discover(&FixedLocator(None), &runner);
        assert!(matches!(result, EngineAvailability::Absent));
        // The probe must never run when nothing was found.
        assert!(runner
            .seen
            .lock()
            .expect("fake runner lock")
            .program
            .is_none());
    }

    #[test]
    fn executable_found_runs_probe_on_located_path() {
        let runner = runner_with(ok(&info_record("0.6.2")));
        let result = discover(&FixedLocator(Some(EXECUTABLE.into())), &runner);
        assert!(matches!(result, EngineAvailability::Compatible { .. }));
        let seen = runner.seen.lock().expect("fake runner lock").clone();
        assert_eq!(seen.program.as_deref(), Some(Path::new(EXECUTABLE)));
    }

    #[test]
    fn compatible_version_passes_the_reported_payload_through() {
        let result = discover_with(ok(&info_record("0.6.2")));
        match result {
            EngineAvailability::Compatible {
                engine_version,
                protocol_version,
                capabilities,
            } => {
                assert_eq!(engine_version, "0.6.2");
                assert_eq!(protocol_version, Some("1".to_owned()));
                assert_eq!(
                    capabilities,
                    Some(vec![
                        "generate".to_owned(),
                        "chat".to_owned(),
                        "complete".to_owned(),
                        "template".to_owned(),
                        "new_chat".to_owned(),
                        "status".to_owned(),
                        "cancel".to_owned(),
                        "shutdown".to_owned(),
                    ])
                );
            }
            other => panic!("expected Compatible, got {other:?}"),
        }
    }

    #[test]
    fn incompatible_reports_version_and_supported_range() {
        let result = discover_with(ok(&info_record("0.7.0")));
        match result {
            EngineAvailability::Incompatible {
                engine_version,
                supported_range,
            } => {
                assert_eq!(engine_version, "0.7.0");
                assert_eq!(supported_range, SUPPORTED_ENGINE_RANGE);
            }
            other => panic!("expected Incompatible, got {other:?}"),
        }
    }

    #[test]
    fn an_engine_answering_the_old_schema_is_refused_not_half_parsed() {
        // 0.5.x answers `info/1`; the schema this build speaks is 2.
        let old = info_record("0.6.2").replace("neurobrix.info/2", "neurobrix.info/1");
        let result = discover_with(ok(&old));
        match result {
            EngineAvailability::ContractFailure { reason, .. } => {
                assert!(reason.contains("neurobrix.info/1"), "reason: {reason}");
            }
            other => panic!("expected ContractFailure, got {other:?}"),
        }
    }

    #[test]
    fn missing_daemon_block_still_reports_compatible() {
        // Windows named instance, no port file: the daemon block becomes one key.
        let record = r#"{"schema":"neurobrix.info/2","engine":"0.6.2","cache":"/home/user/.neurobrix/cache","store":"/home/user/.neurobrix/store","daemon":"unavailable (no port file)"}"#;
        let result = discover_with(ok(record));
        match result {
            EngineAvailability::Compatible {
                engine_version,
                protocol_version,
                capabilities,
            } => {
                assert_eq!(engine_version, "0.6.2");
                assert_eq!(protocol_version, None);
                assert_eq!(capabilities, None);
            }
            other => panic!("expected Compatible, got {other:?}"),
        }
    }

    #[test]
    fn uses_the_fixed_argument_array_with_no_shell_string() {
        let runner = runner_with(ok(&info_record("0.6.2")));
        discover(&FixedLocator(Some(EXECUTABLE.into())), &runner);
        let seen = runner.seen.lock().expect("fake runner lock").clone();
        assert_eq!(seen.program.as_deref(), Some(Path::new(EXECUTABLE)));
        assert_eq!(seen.args, vec!["info".to_owned(), "--json".to_owned()]);
        // The availability probe must not name an engine instance —
        // availability is about the installed engine, not a daemon.
        assert!(seen.env.is_empty());
        for arg in DISCOVERY_ARGS {
            assert!(
                !arg.chars()
                    .any(|c| c.is_whitespace() || "|;&$><`".contains(c)),
                "discovery arguments must stay shell-free tokens"
            );
        }
    }

    #[test]
    fn contract_failure_for_invalid_json() {
        let result = discover_with(ok("not json at all"));
        match result {
            EngineAvailability::ContractFailure { diagnostics, .. } => {
                assert!(diagnostics.is_some());
            }
            other => panic!("expected ContractFailure, got {other:?}"),
        }
    }

    #[test]
    fn contract_failure_for_missing_required_fields() {
        let result = discover_with(ok(r#"{"schema":"neurobrix.info/2"}"#));
        assert!(matches!(result, EngineAvailability::ContractFailure { .. }));
    }

    #[test]
    fn contract_failure_for_unsupported_schema() {
        let result = discover_with(ok(r#"{"schema":"neurobrix.info/3","engine":"0.6.2"}"#));
        match result {
            EngineAvailability::ContractFailure { reason, .. } => {
                assert!(reason.contains("neurobrix.info/3"));
            }
            other => panic!("expected ContractFailure, got {other:?}"),
        }
    }

    #[test]
    fn contract_failure_for_malformed_version() {
        let result = discover_with(ok(
            r#"{"schema":"neurobrix.info/2","engine":"not.a.version"}"#,
        ));
        match result {
            EngineAvailability::ContractFailure { reason, .. } => {
                assert!(reason.contains("not.a.version"));
            }
            other => panic!("expected ContractFailure, got {other:?}"),
        }
    }

    #[test]
    fn unavailable_for_non_zero_exit_with_stderr_excerpt() {
        let result = discover_with(exit_with(2, "usage: neurobrix info\n"));
        match result {
            EngineAvailability::Unavailable {
                reason,
                diagnostics,
            } => {
                assert!(reason.contains("exit code 2"));
                assert_eq!(diagnostics.as_deref(), Some("usage: neurobrix info"));
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn long_diagnostics_are_truncated_to_a_bounded_excerpt() {
        let stderr = "x".repeat(DIAGNOSTIC_LIMIT + 100);
        let result = discover_with(exit_with(1, &stderr));
        match result {
            EngineAvailability::Unavailable { diagnostics, .. } => {
                let diagnostics = diagnostics.expect("long stderr produces diagnostics");
                assert_eq!(diagnostics.chars().count(), DIAGNOSTIC_LIMIT + 1);
                assert!(diagnostics.ends_with('…'));
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn unavailable_on_timeout() {
        let result = discover_with(Err(ProcessFailure::Timeout {
            stderr: String::new(),
        }));
        match result {
            EngineAvailability::Unavailable {
                reason,
                diagnostics,
            } => {
                let expected = format!("{} seconds", DISCOVERY_TIMEOUT.as_secs());
                assert!(reason.contains(&expected));
                assert_eq!(diagnostics, None);
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn unavailable_when_killed_by_signal() {
        let result = discover_with(Ok(ProcessOutcome {
            exit_code: None,
            stdout: String::new(),
            stderr: "killed".to_owned(),
        }));
        match result {
            EngineAvailability::Unavailable { diagnostics, .. } => {
                assert_eq!(diagnostics.as_deref(), Some("killed"));
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn spawn_failure_is_unavailable_not_a_crash() {
        let result = discover_with(Err(ProcessFailure::Spawn("permission denied".to_owned())));
        match result {
            EngineAvailability::Unavailable { diagnostics, .. } => {
                assert_eq!(diagnostics.as_deref(), Some("permission denied"));
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }
}
