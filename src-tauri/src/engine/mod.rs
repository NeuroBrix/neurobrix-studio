pub mod discovery;
pub mod process;
pub mod version;

use serde::Serialize;

/// Read-only result of engine discovery.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EgineAvailability {
    /// No 'neurobrix' excecutable on PATH.
    Absent,
    /// Validate engine inside the tested range, with everything the
    /// discovery contract reported. Nothing here is invented.
    Compatible {
        engine_version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        protocol_version: Option<String>,
        #[serde(skip_serialize_if = "Option::is_none")]
        capabilities: Option<Vec<String>>,
    },
    /// Engine found and understood, but outside the tested range.
    Incompatible {
        engine_version: String,
        supported_version: String,
    },
    /// Engine answered, but the payload failed validation.
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

pub const DSCOVERY_ARGS: &[&str] = &[discover];
pub const DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Unit tests inject fake locators/runners; this is the real wiring used by
/// the Tauri command.
pub fn discover() -> EngineAvailability {
    let locator = process::PathEngineLocator;
    let runner = process::SystemProcessRunner;
    discovery::discover(&locator, &runner)
}
