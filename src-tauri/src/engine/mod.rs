pub mod discovery;
pub mod process;
pub mod version;

use serde::Serialize;

/// Read-only result of engine discovery. None of these is a crash or a
/// silent degradation; the frontend renders each explicitly.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EngineAvailability {
    /// No `neurobrix` executable on PATH.
    Absent,
    /// Validated engine inside the tested range, with everything the
    /// discovery contract reported. Nothing here is invented.
    Compatible {
        engine_version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        protocol_version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        capabilities: Option<Vec<String>>,
    },
    /// Engine found and understood, but outside the tested range.
    Incompatible {
        engine_version: String,
        supported_range: String,
    },
    /// Engine present (or status unknowable): non-zero exit, timeout,
    /// spawn failure, or no discovery support in the installed version.
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

/// Single fixed probe command, executed as an argument array. Never a
/// shell string. Phase 2 reconciles this with the shipped contract.
pub const DISCOVERY_ARGS: &[&str] = &["discover"];
/// Bounded so a wedged engine can never hang the UI thread's await.
pub const DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Unit tests inject fake locators/runners; this is the real wiring used by
/// the Tauri command.
pub fn discover() -> EngineAvailability {
    let locator = process::PathEngineLocator;
    let runner = process::SystemProcessRunner;
    discovery::discover(&locator, &runner)
}
