//! The `info --json` record contract — one owner for the record both
//! consumers parse: discovery maps it to an availability state, and the
//! daemon work reads it (with the Studio-named instance in the
//! environment) to learn where to connect.

use serde::Deserialize;

use super::endpoint::EndpointRecord;

/// Schema stamp the engine puts on every info record
/// (`cli/commands/info.py`). Refusing an unknown schema is refusing to
/// guess what it might mean.
pub const INFO_SCHEMA: &str = "neurobrix.info/1";

/// Engine→Rust contract for `info --json`. Unknown fields are ignored;
/// `protocol`, `operations` and `endpoint` absent means "not supplied" —
/// the serving module may not import on a minimal install, and info.py
/// records that as a string instead of hiding it behind a default.
#[derive(Debug, Deserialize)]
pub struct InfoRecord {
    pub schema: String,
    /// Envelope key on every record; preferred over the info-only `version`.
    pub engine: String,
    pub protocol: Option<u64>,
    /// Dispatcher method names, surfaced as `capabilities`.
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

    /// The endpoint key in the verified 0.5.4 record (engine issue #9,
    /// paths redacted) — the Unix shape from the engine's instance tests
    /// (tests/unit/serving/test_each_windows_instance_has_its_own_port.py).
    #[test]
    fn a_verified_record_carries_its_endpoint() {
        let record = parse_info(
            r#"{"schema":"neurobrix.info/1","engine":"0.5.4","protocol":1,"operations":["status","shutdown"],"endpoint":{"kind":"unix","path":"/home/user/.neurobrix/daemon.sock"}}"#,
        )
        .expect("the verified record parses");
        assert_eq!(
            record.endpoint,
            Some(EndpointRecord::Unix {
                path: PathBuf::from("/home/user/.neurobrix/daemon.sock")
            })
        );
    }

    /// A minimal install that cannot import the serving module reports no
    /// serving fields; absent means "not supplied", never an error.
    #[test]
    fn a_record_without_a_serving_block_reports_no_endpoint() {
        let record = parse_info(
            r#"{"schema":"neurobrix.info/1","engine":"0.5.4","daemon":"unavailable (no port file)"}"#,
        )
        .expect("absent optional fields are not failures");
        assert_eq!(record.endpoint, None);
        assert_eq!(record.protocol, None);
        assert_eq!(record.operations, None);
    }

    #[test]
    fn an_unknown_schema_is_refused_not_guessed_at() {
        let error = parse_info(r#"{"schema":"neurobrix.info/2","engine":"0.5.4"}"#)
            .expect_err("an unknown schema is refused");
        assert_eq!(
            error,
            InfoError::UnsupportedSchema("neurobrix.info/2".to_owned())
        );
    }

    #[test]
    fn an_unknown_endpoint_kind_fails_the_whole_record() {
        // Unexpected variants are contract failures, never silently dropped
        // into a half-parsed record.
        let error = parse_info(
            r#"{"schema":"neurobrix.info/1","engine":"0.5.4","endpoint":{"kind":"quic","path":"x"}}"#,
        )
        .expect_err("an unknown endpoint kind fails the record");
        assert!(matches!(error, InfoError::InvalidJson(_)));
    }
}
