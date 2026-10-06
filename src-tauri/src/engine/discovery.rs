use serde::Deserialize;

use super::process::{EngineLocator, ProcessFailure, ProcessRunner};
use super::version::{check_supported, VersionCheck, VersionError, SUPPORTED_ENGINE_RANGE};
use super::{EngineAvailability, DISCOVERY_ARGS, DISCOVERY_TIMEOUT};

pub const EXECUTABLE_NAME: &str = "neurobrix";
const INFO_SCHEMA: &str = "neurobrix.info/1";
const DIAGNOSTIC_LIMIT: usize = 400;

/// Engine→Rust contract for `info --json`. Unknown fields ignored;
/// `protocol`/`operations` absent means "not supplied"
#[derive(Deserialize)]
struct InfoRecord {
    schema: String,
    /// Envelope key on every record; preferred over the info-only `version` key.
    engine: String,
    protocol: Option<u32>,
    /// Dispatcher method names, surfaced as `capabilities`.
    operations: Option<Vec<String>>,
}

/// Read-only probe; every failure maps to a typed availability state.
pub fn discover(locator: &dyn EngineLocator, runner: &dyn ProcessRunner) -> EngineAvailability {
    let Some(executable) = locator.locate(EXECUTABLE_NAME) else {
        return EngineAvailability::Absent;
    };

    let outcome = match runner.run(&executable, DISCOVERY_ARGS, DISCOVERY_TIMEOUT) {
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

    let record: InfoRecord = match serde_json::from_str(&outcome.stdout) {
        Ok(record) => record,
        Err(error) => {
            return EngineAvailability::ContractFailure {
                reason: "The engine's status response was not valid JSON.".into(),
                diagnostics: Some(excerpt(&error.to_string())),
            };
        }
    };

    if record.schema != INFO_SCHEMA {
        return EngineAvailability::ContractFailure {
            reason: format!(
                "The engine reported status schema '{}', which this version of \
                 Studio does not understand.",
                excerpt(&record.schema)
            ),
            diagnostics: None,
        };
    }

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

fn excerpt(text: &str) -> String {
    if text.chars().count() <= DIAGNOSTIC_LIMIT {
        return text.to_owned();
    }
    let cut: String = text.chars().take(DIAGNOSTIC_LIMIT).collect();
    format!("{cut}…")
}

fn non_empty_excerpt(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(excerpt(trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::process::{EngineLocator, ProcessFailure, ProcessOutcome, ProcessRunner};
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

    struct FakeRunner {
        outcome: Result<ProcessOutcome, ProcessFailure>,
        seen: Mutex<(Option<PathBuf>, Vec<String>)>,
    }
    impl ProcessRunner for FakeRunner {
        fn run(
            &self,
            program: &Path,
            args: &[&str],
            _timeout: Duration,
        ) -> Result<ProcessOutcome, ProcessFailure> {
            let mut seen = self.seen.lock().expect("fake runner lock");
            seen.0 = Some(program.to_owned());
            seen.1 = args.iter().map(|a| a.to_string()).collect();
            self.outcome.clone()
        }
    }

    fn runner_with(outcome: Result<ProcessOutcome, ProcessFailure>) -> FakeRunner {
        FakeRunner {
            outcome,
            seen: Mutex::new((None, Vec::new())),
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

    /// Verified 0.5.4 record (issue #9, paths redacted); unknown fields
    /// kept to prove they are ignored. The interpreter key is omitted
    /// because the guard bans that literal in this module.
    fn info_record(engine_version: &str) -> String {
        format!(
            r#"{{"schema":"neurobrix.info/1","engine":"{engine_version}","version":"{engine_version}","package":"/src/neurobrix","cache":"/home/user/.neurobrix/cache","store":"/home/user/.neurobrix/store","models":[{{"name":"test-model","size_bytes":384061043}}],"hardware_profiles":["a100-80g","h100-80g"],"torch":null,"cuda_available":false,"gpus":[],"protocol":1,"endpoint":{{"kind":"unix","path":"/home/user/.neurobrix/daemon.sock"}},"operations":["generate","chat","complete","template","new_chat","status","shutdown"]}}"#
        )
    }

    #[test]
    fn absent_when_executable_is_not_found() {
        let runner = runner_with(ok(&info_record("0.5.4")));
        let result = discover(&FixedLocator(None), &runner);
        assert!(matches!(result, EngineAvailability::Absent));
        // The probe must never run when nothing was found.
        assert!(runner.seen.lock().expect("fake runner lock").0.is_none());
    }

    #[test]
    fn executable_found_runs_probe_on_located_path() {
        let runner = runner_with(ok(&info_record("0.5.4")));
        let result = discover(&FixedLocator(Some(EXECUTABLE.into())), &runner);
        assert!(matches!(result, EngineAvailability::Compatible { .. }));
        let (program, _) = runner.seen.lock().expect("fake runner lock").clone();
        assert_eq!(program.as_deref(), Some(Path::new(EXECUTABLE)));
    }

    #[test]
    fn compatible_version_passes_the_reported_payload_through() {
        let result = discover_with(ok(&info_record("0.5.4")));
        match result {
            EngineAvailability::Compatible {
                engine_version,
                protocol_version,
                capabilities,
            } => {
                assert_eq!(engine_version, "0.5.4");
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
    fn missing_daemon_block_still_reports_compatible() {
        // Windows named instance, no port file: the daemon block becomes one key.
        let record = r#"{"schema":"neurobrix.info/1","engine":"0.5.4","cache":"/home/user/.neurobrix/cache","store":"/home/user/.neurobrix/store","daemon":"unavailable (no port file)"}"#;
        let result = discover_with(ok(record));
        match result {
            EngineAvailability::Compatible {
                engine_version,
                protocol_version,
                capabilities,
            } => {
                assert_eq!(engine_version, "0.5.4");
                assert_eq!(protocol_version, None);
                assert_eq!(capabilities, None);
            }
            other => panic!("expected Compatible, got {other:?}"),
        }
    }

    #[test]
    fn uses_the_fixed_argument_array_with_no_shell_string() {
        let runner = runner_with(ok(&info_record("0.5.4")));
        discover(&FixedLocator(Some(EXECUTABLE.into())), &runner);
        let (program, args) = runner.seen.lock().expect("fake runner lock").clone();
        assert_eq!(program.as_deref(), Some(Path::new(EXECUTABLE)));
        assert_eq!(args, vec!["info".to_owned(), "--json".to_owned()]);
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
        let result = discover_with(ok(r#"{"schema":"neurobrix.info/1"}"#));
        assert!(matches!(result, EngineAvailability::ContractFailure { .. }));
    }

    #[test]
    fn contract_failure_for_unsupported_schema() {
        let result = discover_with(ok(r#"{"schema":"neurobrix.info/2","engine":"0.5.4"}"#));
        match result {
            EngineAvailability::ContractFailure { reason, .. } => {
                assert!(reason.contains("neurobrix.info/2"));
            }
            other => panic!("expected ContractFailure, got {other:?}"),
        }
    }

    #[test]
    fn contract_failure_for_malformed_version() {
        let result = discover_with(ok(
            r#"{"schema":"neurobrix.info/1","engine":"not.a.version"}"#,
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
