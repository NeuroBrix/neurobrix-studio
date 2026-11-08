pub mod client;
pub mod daemon;
pub mod discovery;
pub mod endpoint;
pub mod info;
pub mod lifecycle;
pub mod process;
#[cfg(test)]
pub(crate) mod testing;
pub mod transport;
pub mod version;

use serde::Serialize;

/// Read-only discovery result; every state is rendered explicitly, never a crash or silent fallback.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EngineAvailability {
    Absent,
    /// Only what the discovery contract reported; nothing invented.
    Compatible {
        engine_version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        protocol_version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        capabilities: Option<Vec<String>>,
    },
    Incompatible {
        engine_version: String,
        supported_range: String,
    },
    /// Engine present but cannot answer: non-zero exit, timeout, spawn failure, or no discovery support.
    Unavailable {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostics: Option<String>,
    },
    /// Engine answered, but the payload failed validation.
    ContractFailure {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostics: Option<String>,
    },
}

/// Fixed probe, argument array only — never a shell string. Shipped
/// contract: `neurobrix info --json` (schema `neurobrix.info/2`). The
/// daemon work runs the SAME probe with the Studio-named instance in the
/// environment, to read the endpoint it must dial.
pub const DISCOVERY_ARGS: &[&str] = &["info", "--json"];
/// Bounded so a wedged engine cannot hang the await; generous because
/// `info --json` imports the compute stack, slow when installed.
pub const DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Diagnostics excerpts are bounded: the last thing the engine said,
/// never an unbounded dump.
pub(crate) const DIAGNOSTIC_LIMIT: usize = 400;

/// Bound a diagnostic to `DIAGNOSTIC_LIMIT` characters.
pub(crate) fn excerpt(text: &str) -> String {
    if text.chars().count() <= DIAGNOSTIC_LIMIT {
        return text.to_owned();
    }
    let cut: String = text.chars().take(DIAGNOSTIC_LIMIT).collect();
    format!("{cut}…")
}

/// `None` when there is nothing worth excerpting.
pub(crate) fn non_empty_excerpt(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(excerpt(trimmed))
    }
}

/// Real wiring for the Tauri command; tests call `discovery::discover` with fakes.
pub fn discover() -> EngineAvailability {
    let locator = process::PathEngineLocator;
    let runner = process::SystemProcessRunner;
    discovery::discover(&locator, &runner)
}
