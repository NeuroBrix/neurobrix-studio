use serde::Deserialize;

use super::process::{EngineLocator, ProcessFailure, ProcessRunner};
use super::version::{check_supported, VersionCheck, VersionError, SUPPORTED_ENGINE_RANGE};
use super::{EngineAvailability, DISCOVERY_ARGS, DISCOVERY_TIMEOUT};

pub const EXECUTABLE_NAME: &str = "neurobrix";
const DISCOVERY_SCHEMA: &str = "neurobrix.discovery/1";
const DIAGNOSTIC_LIMIT: usize = 400;

/// The engine→Rust contract document. Unknown fields are ignored on
/// purpose (forward compatibility); the `schema` string pins the shape.
#[derive(Deserialize)]
struct DiscoveryDocument {
    schema: String,
    engine: EngineSection,
    protocol: Option<ProtocolSection>,
    capabilities: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct EngineSection {
    version: String,
}

#[derive(Deserialize)]
struct ProtocolSection {
    version: String,
}

/// Read-only: locate the engine, run the fixed discovery probe, and map
/// every failure into a typed availability state.
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
        // Today this is the honest state for every released engine: the
        // installed version does not know `neurobrix discover` yet.
        return EngineAvailability::Unavailable {
            reason: format!(
                "The installed engine does not support status discovery \
                 (exit code {exit_code}). It may need to be upgraded to a \
                 version that provides it."
            ),
            diagnostics: non_empty_excerpt(&outcome.stderr),
        };
    }

    let document: DiscoveryDocument = match serde_json::from_str(&outcome.stdout) {
        Ok(document) => document,
        Err(error) => {
            return EngineAvailability::ContractFailure {
                reason: "The engine's status response was not valid JSON.".into(),
                diagnostics: Some(excerpt(&error.to_string())),
            };
        }
    };

    if document.schema != DISCOVERY_SCHEMA {
        return EngineAvailability::ContractFailure {
            reason: format!(
                "The engine reported status schema '{}', which this version of \
                 Studio does not understand.",
                excerpt(&document.schema)
            ),
            diagnostics: None,
        };
    }

    match check_supported(&document.engine.version) {
        Ok(VersionCheck::Compatible) => EngineAvailability::Compatible {
            engine_version: document.engine.version,
            protocol_version: document.protocol.map(|protocol| protocol.version),
            capabilities: document.capabilities,
        },
        Ok(VersionCheck::Incompatible) => EngineAvailability::Incompatible {
            engine_version: document.engine.version,
            supported_range: SUPPORTED_ENGINE_RANGE.to_owned(),
        },
        Err(VersionError::Malformed) => EngineAvailability::ContractFailure {
            reason: format!(
                "The engine reported a version that is not valid SemVer: '{}'.",
                excerpt(&document.engine.version)
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

    fn valid_payload(version: &str) -> String {
        format!(
            r#"{{"schema":"neurobrix.discovery/1",
                 "engine":{{"name":"neurobrix","version":"{version}"}},
                 "protocol":{{"version":"1"}},
                 "capabilities":["chat","completion"],
                 "paths":{{"cache":"/c","store":"/s"}}}}"#
        )
    }

    fn runner_with(outcome: Result<ProcessOutcome, ProcessFailure>) -> FakeRunner {
        FakeRunner {
            outcome,
            seen: Mutex::new((None, Vec::new())),
        }
    }

    fn ok(stdout: &str) -> Result<ProcessOutcome, ProcessFailure> {
        Ok(ProcessOutcome {
            exit_code: Some(0),
            stdout: stdout.to_owned(),
            stderr: String::new(),
        })
    }

    #[test]
    fn absent_when_executable_is_not_found() {
        let runner = runner_with(ok(&valid_payload("0.5.3")));
        let result = discover(&FixedLocator(None), &runner);
        assert!(matches!(result, EngineAvailability::Absent));
        // The probe must never run when nothing was found.
        assert!(runner.seen.lock().expect("fake runner lock").0.is_none());
    }

    #[test]
    fn incompatible_reports_version_and_supported_range() {
        let runner = runner_with(ok(&valid_payload("0.7.0")));
        let result = discover(&FixedLocator(Some(EXECUTABLE.into())), &runner);
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
    fn uses_the_fixed_argument_array_with_no_shell_string() {
        let runner = runner_with(ok(&valid_payload("0.5.3")));
        discover(&FixedLocator(Some(EXECUTABLE.into())), &runner);
        let (program, args) = runner.seen.lock().expect("fake runner lock").clone();
        assert_eq!(program.as_deref(), Some(Path::new(EXECUTABLE)));
        assert_eq!(args, vec!["discover".to_owned()]);
        for arg in DISCOVERY_ARGS {
            assert!(
                !arg.chars()
                    .any(|c| c.is_whitespace() || "|;&$><`".contains(c)),
                "discovery arguments must stay shell-free tokens"
            );
        }
    }

    // … remaining tests, one per acceptance case:
    //  executable_found_runs_probe                (seen program == located path)
    //  compatible_version_passes_through_payload  (protocol + capabilities preserved)
    //  contract_failure_for_invalid_json
    //  contract_failure_for_missing_required_fields   (e.g. {"schema":"neurobrix.discovery/1"})
    //  contract_failure_for_unsupported_schema    ("neurobrix.discovery/2")
    //  contract_failure_for_malformed_version     ("not.a.version")
    //  unavailable_for_non_zero_exit_with_stderr_excerpt  (exit 2, argparse usage on stderr)
    //  unavailable_on_timeout
    //  unavailable_when_killed_by_signal          (exit_code: None)
    //  spawn_failure_is_unavailable_not_a_crash
}
