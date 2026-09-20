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
