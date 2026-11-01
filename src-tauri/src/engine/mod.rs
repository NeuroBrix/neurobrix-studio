// Staged for #15: the daemon client — connect, handshake, status and
// shutdown — is landed and tested against fake in-process daemons ahead
// of the lifecycle that owns and stops a real one (the next phase). The
// allow comes off when the lifecycle calls into it.
#[allow(dead_code)]
pub mod client;
pub mod discovery;
// Staged for #15: the client dials the resolved addresses and the record
// type is live through the shared info contract. Naming the Studio
// instance and resolving endpoints from the info record wait for the
// daemon manager; the allow comes off with it.
#[allow(dead_code)]
pub mod endpoint;
pub mod info;
pub mod process;
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
/// contract: `neurobrix info --json` (schema `neurobrix.info/1`).
pub const DISCOVERY_ARGS: &[&str] = &["info", "--json"];
/// Bounded so a wedged engine cannot hang the await; generous because
/// `info --json` imports the compute stack, slow when installed.
pub const DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Real wiring for the Tauri command; tests call `discovery::discover` with fakes.
pub fn discover() -> EngineAvailability {
    let locator = process::PathEngineLocator;
    let runner = process::SystemProcessRunner;
    discovery::discover(&locator, &runner)
}
