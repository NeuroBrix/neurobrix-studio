//! The `info --json` record contract — one owner for the record both
//! consumers parse: discovery maps it to an availability state, and the
//! daemon work reads it (with the Studio-named instance in the
//! environment) to learn where to connect.

use serde::Deserialize;

use super::endpoint::EndpointRecord;

/// Schema stamp the engine puts on every info record
/// (`cli/json_out.py`, one table for every read command). Refusing an
/// unknown schema is refusing to guess what it might mean — an engine
/// answering `info/1` (0.5.x) is told so rather than half-parsed.
pub const INFO_SCHEMA: &str = "neurobrix.info/2";

/// Engine→Rust contract for `info --json` (schema `neurobrix.info/2`).
/// Unknown fields are ignored — the record also carries the compute
/// branches, the detected devices, models and paths that other callers
/// read; `protocol`, `operations` and `endpoint` absent means "not
/// supplied" — the serving module may not import on a minimal install,
/// and info.py records that as a string instead of hiding it.
#[derive(Debug, Deserialize)]
pub struct InfoRecord {
    pub schema: String,
    /// Envelope key on every record; preferred over the info-only `version`.
    pub engine: String,
    /// The protocol version this engine serves (wire-protocol.md §5).
    pub protocol: Option<u64>,
    /// The dispatcher's method names — where Studio checks the ones it
    /// calls (`status`, `shutdown`); in 0.6.x this includes `cancel`.
    pub operations: Option<Vec<String>>,
    /// Where this instance's daemon listens, exactly as the engine reports
    /// it (protocol.py `Instance.endpoint()`); never recomputed by Studio.
    pub endpoint: Option<EndpointRecord>,
}

/// Every way an info record can fail its contract; none is a crash.
#[derive(Debug, PartialEq)]
pub enum InfoError {
    /// The payload is not the record contract at all — including an
    /// unexpected variant, which fails the record instead of being
    /// silently dropped.
    InvalidJson(String),
    UnsupportedSchema(String),
}

/// Parse and validate one `info --json` payload.
pub fn parse_info(stdout: &str) -> Result<InfoRecord, InfoError> {
    let record: InfoRecord =
        serde_json::from_str(stdout).map_err(|error| InfoError::InvalidJson(error.to_string()))?;
    if record.schema != INFO_SCHEMA {
        return Err(InfoError::UnsupportedSchema(record.schema));
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The endpoint key in the verified 0.6.2 record (the Unix shape from
    /// the engine's instance tests, wire-protocol.md §1).
    #[test]
    fn a_verified_record_carries_its_endpoint() {
        let record = parse_info(
            r#"{"schema":"neurobrix.info/2","engine":"0.6.2","protocol":1,"protocol_range":[1,1],"operations":["generate","chat","complete","template","new_chat","status","cancel","shutdown"],"endpoint":{"kind":"unix","path":"/home/user/.neurobrix/daemon.sock"}}"#,
        )
        .expect("the verified record parses");
        assert_eq!(
            record.endpoint,
            Some(EndpointRecord::Unix {
                path: PathBuf::from("/home/user/.neurobrix/daemon.sock")
            })
        );
        assert_eq!(
            record.operations.as_deref(),
            Some(
                [
                    "generate", "chat", "complete", "template", "new_chat", "status", "cancel",
                    "shutdown"
                ]
                .map(String::from)
                .as_slice()
            )
        );
    }

    /// A minimal install that cannot import the serving module reports no
    /// serving fields; absent means "not supplied", never an error.
    #[test]
    fn a_record_without_a_serving_block_reports_no_endpoint() {
        let record = parse_info(
            r#"{"schema":"neurobrix.info/2","engine":"0.6.2","daemon":"unavailable (no port file)"}"#,
        )
        .expect("absent optional fields are not failures");
        assert_eq!(record.endpoint, None);
        assert_eq!(record.protocol, None);
        assert_eq!(record.operations, None);
    }

    #[test]
    fn an_unknown_schema_is_refused_not_guessed_at() {
        let error = parse_info(r#"{"schema":"neurobrix.info/3","engine":"0.7.0"}"#)
            .expect_err("an unknown schema is refused");
        assert_eq!(
            error,
            InfoError::UnsupportedSchema("neurobrix.info/3".to_owned())
        );
    }

    #[test]
    fn an_unknown_endpoint_kind_fails_the_whole_record() {
        // Unexpected variants are contract failures, never silently
        // dropped into a half-parsed record.
        let error = parse_info(
            r#"{"schema":"neurobrix.info/2","engine":"0.6.2","endpoint":{"kind":"quic","path":"x"}}"#,
        )
        .expect_err("an unknown endpoint kind fails the record");
        assert!(matches!(error, InfoError::InvalidJson(_)));
    }
}
