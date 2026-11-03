//! The engine's serving wire protocol: length-prefixed JSON over a stream.
//! Wire format: `[4-byte big-endian uint32 length][UTF-8 JSON payload]`
//! (serving/protocol.py). The engine caps messages at 64 MiB; Studio refuses
//! anything larger BEFORE allocating, so a corrupt length can never force a
//! huge allocation. Every I/O is deadline-bounded: a wedged daemon can
//! never hang a command.

use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::version::{check_supported, VersionCheck};

/// `MAX_MESSAGE_SIZE` from serving/protocol.py — the cap both sides enforce.
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024;
const HEADER_SIZE: usize = 4;

/// The protocol version this Studio build speaks. This is Studio's own
/// declaration of what it understands — refusing anything else is the
/// point, so an engine protocol bump is a deliberate Studio change.
pub const SUPPORTED_PROTOCOL_VERSION: u64 = 1;

/// Bounded I/O so a silent daemon fails as a timeout, never a hang.
#[derive(Debug, Clone, Copy)]
pub struct TransportTimeouts {
    pub connect: Duration,
    pub read: Duration,
    pub write: Duration,
}

/// Every transport failure is one of these; none is a crash.
#[derive(Debug, PartialEq)]
pub enum TransportError {
    /// Nothing is listening at the endpoint — normal while a daemon loads.
    NotListening,
    Connect(String),
    Io(String),
    Timeout {
        operation: &'static str,
    },
    /// The daemon closed the stream before the frame began.
    Closed,
    /// Bytes arrived, then the stream ended mid-frame.
    TruncatedFrame,
    /// Declared length over the cap; refused before allocating.
    MessageTooLarge {
        declared: u64,
    },
    InvalidUtf8,
    InvalidJson(String),
}

/// Read one frame; the cap is checked before the payload is allocated.
pub async fn read_frame<S>(stream: &mut S, timeout: Duration) -> Result<Value, TransportError>
where
    S: AsyncRead + Unpin,
{
    let deadline = Instant::now() + timeout;
    let mut header = [0u8; HEADER_SIZE];
    read_exact_until(stream, &mut header, deadline).await?;
    let declared = u32::from_be_bytes(header) as u64;
    if declared > MAX_MESSAGE_SIZE as u64 {
        return Err(TransportError::MessageTooLarge { declared });
    }
    let mut payload = vec![0u8; declared as usize]; // allocated only after the cap check
    read_exact_until(stream, &mut payload, deadline).await?;
    let text = std::str::from_utf8(&payload).map_err(|_| TransportError::InvalidUtf8)?;
    serde_json::from_str(text).map_err(|error| TransportError::InvalidJson(error.to_string()))
}

/// Write one frame. Studio's requests are tiny; the cap is enforced anyway.
pub async fn write_frame<S>(
    stream: &mut S,
    payload: &Value,
    timeout: Duration,
) -> Result<(), TransportError>
where
    S: AsyncWrite + Unpin,
{
    let bytes = serde_json::to_vec(payload).expect("JSON values always serialize");
    let len = u32::try_from(bytes.len()).map_err(|_| TransportError::MessageTooLarge {
        declared: bytes.len() as u64,
    })?;
    let deadline = Instant::now() + timeout;
    let mut frame = Vec::with_capacity(HEADER_SIZE + bytes.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(&bytes);
    write_all_until(stream, &frame, deadline).await
}

/// Exactly `buffer.len()` bytes before `deadline`, or a typed failure.
/// The deadline is total, so a slow-drip sender cannot extend it per chunk.
async fn read_exact_until<S>(
    stream: &mut S,
    buffer: &mut [u8],
    deadline: Instant,
) -> Result<(), TransportError>
where
    S: AsyncRead + Unpin,
{
    let mut filled = 0;
    while filled < buffer.len() {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err(TransportError::Timeout { operation: "read" });
        };
        let read = tokio::time::timeout(remaining, stream.read(&mut buffer[filled..]))
            .await
            .map_err(|_| TransportError::Timeout { operation: "read" })?
            .map_err(|error| TransportError::Io(error.to_string()))?;
        if read == 0 {
            return Err(if filled == 0 {
                TransportError::Closed
            } else {
                TransportError::TruncatedFrame
            });
        }
        filled += read;
    }
    Ok(())
}

async fn write_all_until<S>(
    stream: &mut S,
    bytes: &[u8],
    deadline: Instant,
) -> Result<(), TransportError>
where
    S: AsyncWrite + Unpin,
{
    let mut written = 0;
    while written < bytes.len() {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err(TransportError::Timeout { operation: "write" });
        };
        let n = tokio::time::timeout(remaining, stream.write(&bytes[written..]))
            .await
            .map_err(|_| TransportError::Timeout { operation: "write" })?
            .map_err(|error| TransportError::Io(error.to_string()))?;
        written += n;
    }
    Ok(())
}

/// A validated response envelope. The engine stamps `protocol` and `engine`
/// on every response (`make_response`); Studio refuses what it does not
/// know instead of guessing, and re-checks the stamps on every envelope,
/// not just the first.
#[derive(Debug, PartialEq)]
pub struct Envelope {
    pub engine_version: String,
    pub body: EnvelopeBody,
}

/// The engine's response carries `result` or a plain `error` string — there
/// are no structured error variants on the wire, so none are invented here.
#[derive(Debug, PartialEq)]
pub enum EnvelopeBody {
    /// Present even when the engine emitted `"result": null`
    /// (`make_response()` with no result).
    Result(Value),
    /// The engine's own error message, carried verbatim.
    Error(String),
}

/// Every way a response envelope can fail validation; none is a crash.
#[derive(Debug, PartialEq)]
pub enum EnvelopeError {
    /// No `protocol` or `engine` stamp — including a payload that is not
    /// an object at all.
    MissingStamps,
    /// A protocol this Studio build was never written against.
    UnknownProtocol(u64),
    /// Valid SemVer, but outside the tested engine range.
    EngineOutsideTestedRange(String),
    MalformedEngineVersion(String),
    /// `result` and `error` both present, both absent, or a non-string error.
    UnexpectedVariant,
}

/// Validate one response envelope: the stamps first (protocol, then the
/// engine version range), then exactly one of `result`/`error`. The shape
/// mirrors `make_response` in serving/protocol.py and the engine's own
/// protocol tests (tests/unit/cli/test_the_daemon_names_its_protocol.py).
pub fn validate_envelope(mut value: Value) -> Result<Envelope, EnvelopeError> {
    let Value::Object(object) = &mut value else {
        return Err(EnvelopeError::MissingStamps);
    };
    let Some(protocol) = object.get("protocol").and_then(Value::as_u64) else {
        return Err(EnvelopeError::MissingStamps);
    };
    if protocol != SUPPORTED_PROTOCOL_VERSION {
        return Err(EnvelopeError::UnknownProtocol(protocol));
    }
    let Some(engine) = object
        .get("engine")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return Err(EnvelopeError::MissingStamps);
    };
    match check_supported(&engine) {
        Ok(VersionCheck::Compatible) => {}
        Ok(VersionCheck::Incompatible) => {
            return Err(EnvelopeError::EngineOutsideTestedRange(engine));
        }
        Err(_) => return Err(EnvelopeError::MalformedEngineVersion(engine)),
    }
    // The body is taken, not cloned: a large result passes through once.
    let body = match (object.remove("result"), object.remove("error")) {
        (Some(result), None) => EnvelopeBody::Result(result),
        (None, Some(Value::String(error))) => EnvelopeBody::Error(error),
        _ => return Err(EnvelopeError::UnexpectedVariant),
    };
    Ok(Envelope {
        engine_version: engine,
        body,
    })
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::NotListening => {
                write!(f, "nothing is listening at the endpoint")
            }
            TransportError::Connect(reason) => write!(f, "could not connect: {reason}"),
            TransportError::Io(reason) => write!(f, "the socket I/O failed: {reason}"),
            TransportError::Timeout { operation } => {
                write!(f, "the {operation} did not finish within its deadline")
            }
            TransportError::Closed => {
                write!(f, "the daemon closed the connection before replying")
            }
            TransportError::TruncatedFrame => {
                write!(f, "the daemon's reply frame was cut short")
            }
            TransportError::MessageTooLarge { declared } => write!(
                f,
                "the declared frame length ({declared} bytes) is over the 64 MiB cap"
            ),
            TransportError::InvalidUtf8 => write!(f, "the reply payload is not valid UTF-8"),
            TransportError::InvalidJson(reason) => {
                write!(f, "the reply payload is not valid JSON: {reason}")
            }
        }
    }
}

impl std::error::Error for TransportError {}

impl std::fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnvelopeError::MissingStamps => {
                write!(f, "the response is missing its protocol or engine stamp")
            }
            EnvelopeError::UnknownProtocol(version) => write!(
                f,
                "the response speaks protocol version {version}, which this version \
                 of Studio does not understand (it speaks version {SUPPORTED_PROTOCOL_VERSION})"
            ),
            EnvelopeError::EngineOutsideTestedRange(version) => write!(
                f,
                "the response reports engine version {version}, outside the \
                 tested range"
            ),
            EnvelopeError::MalformedEngineVersion(version) => write!(
                f,
                "the response reports an engine version that is not valid \
                 SemVer: '{version}'"
            ),
            EnvelopeError::UnexpectedVariant => write!(
                f,
                "the response carries both or neither of result and error"
            ),
        }
    }
}

impl std::error::Error for EnvelopeError {}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tokio::io::AsyncWriteExt;

    use super::*;

    /// Generous on purpose: the paused tokio clock auto-advances to timers,
    /// so the timeout tests fire without a real wait and the I/O tests are
    /// in-memory and fast.
    const TEST_IO: Duration = Duration::from_secs(5);

    // -- framing -----------------------------------------------------------

    #[tokio::test(start_paused = true)]
    async fn a_frame_round_trips_exactly_its_declared_length() {
        let (mut reader, mut writer) = tokio::io::duplex(4096);
        let payload = json!({"method": "status", "params": {}});
        write_frame(&mut writer, &payload, TEST_IO)
            .await
            .expect("write succeeds");
        assert_eq!(read_frame(&mut reader, TEST_IO).await, Ok(payload));
    }

    #[tokio::test(start_paused = true)]
    async fn a_payload_arriving_in_chunks_is_read_to_exact_length() {
        // One byte per write: exact-length reading must loop, never trust a
        // single read to deliver the whole declared length.
        let (mut reader, mut writer) = tokio::io::duplex(4096);
        let payload = br#"{"x":1}   "#;
        writer
            .write_all(&(payload.len() as u32).to_be_bytes())
            .await
            .expect("header");
        for byte in payload {
            writer.write_all(&[*byte]).await.expect("payload byte");
        }
        assert_eq!(read_frame(&mut reader, TEST_IO).await, Ok(json!({"x": 1})));
    }

    #[tokio::test(start_paused = true)]
    async fn a_declared_length_over_the_cap_is_refused_before_the_payload_is_read() {
        // MAX_MESSAGE_SIZE is the engine's own cap (protocol.py). The frame
        // is refused before its bytes are read — before allocating for them.
        let (mut reader, mut writer) = tokio::io::duplex(4096);
        writer
            .write_all(&(MAX_MESSAGE_SIZE as u32 + 1).to_be_bytes())
            .await
            .expect("header");
        assert_eq!(
            read_frame(&mut reader, TEST_IO).await,
            Err(TransportError::MessageTooLarge {
                declared: MAX_MESSAGE_SIZE as u64 + 1
            })
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_frame_cut_short_mid_payload_is_a_truncated_frame() {
        let (mut reader, mut writer) = tokio::io::duplex(4096);
        writer
            .write_all(&10u32.to_be_bytes())
            .await
            .expect("header");
        writer.write_all(b"abc").await.expect("partial payload");
        drop(writer); // the daemon died mid-frame
        assert_eq!(
            read_frame(&mut reader, TEST_IO).await,
            Err(TransportError::TruncatedFrame)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_stream_closed_before_any_byte_is_a_clean_close_not_a_truncation() {
        let (mut reader, writer) = tokio::io::duplex(4096);
        drop(writer);
        assert_eq!(
            read_frame(&mut reader, TEST_IO).await,
            Err(TransportError::Closed)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_frame_that_is_not_json_is_a_contract_failure() {
        let (mut reader, mut writer) = tokio::io::duplex(4096);
        send_raw_frame(&mut writer, b"not json at all").await;
        assert!(matches!(
            read_frame(&mut reader, TEST_IO).await,
            Err(TransportError::InvalidJson(_))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn a_frame_that_is_not_utf8_is_a_contract_failure() {
        let (mut reader, mut writer) = tokio::io::duplex(4096);
        send_raw_frame(&mut writer, &[0b1111_1111, 0b1100_0000]).await;
        assert_eq!(
            read_frame(&mut reader, TEST_IO).await,
            Err(TransportError::InvalidUtf8)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_read_that_never_arrives_fails_as_a_bounded_timeout() {
        // The peer stays alive but silent: refusing to hang is the point.
        let (mut reader, _peer) = tokio::io::duplex(4096);
        assert_eq!(
            read_frame(&mut reader, TEST_IO).await,
            Err(TransportError::Timeout { operation: "read" })
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_write_that_never_drains_fails_as_a_bounded_timeout() {
        // A frame larger than the duplex buffer with nobody reading the
        // other end: the write must fail as a bounded timeout, never block.
        let (_reader, mut writer) = tokio::io::duplex(16);
        let payload = json!({"filler": "x".repeat(2048)});
        assert_eq!(
            write_frame(&mut writer, &payload, TEST_IO).await,
            Err(TransportError::Timeout { operation: "write" })
        );
    }

    async fn send_raw_frame(writer: &mut tokio::io::DuplexStream, payload: &[u8]) {
        writer
            .write_all(&(payload.len() as u32).to_be_bytes())
            .await
            .expect("header");
        writer.write_all(payload).await.expect("payload");
    }

    // -- envelopes ---------------------------------------------------------

    #[test]
    fn a_result_envelope_passes_its_stamps_and_body() {
        let envelope = validate_envelope(json!({
            "protocol": 1,
            "engine": "0.5.4",
            "result": {"status": "ok"}
        }))
        .expect("a valid envelope validates");
        assert_eq!(envelope.engine_version, "0.5.4");
        assert_eq!(envelope.body, EnvelopeBody::Result(json!({"status": "ok"})));
    }

    #[test]
    fn a_null_result_is_a_result_not_a_missing_one() {
        // make_response() with no result emits `"result": null` — present.
        let envelope = validate_envelope(json!({"protocol": 1, "engine": "0.5.4", "result": null}))
            .expect("a null result is a valid envelope");
        assert_eq!(envelope.body, EnvelopeBody::Result(Value::Null));
    }

    #[test]
    fn an_error_envelope_carries_the_plain_error_string_verbatim() {
        // The engine's error is a plain string (server.py's unknown-method
        // path); Studio passes it through, never invents structured
        // variants the wire does not have.
        let envelope = validate_envelope(json!({
            "protocol": 1,
            "engine": "0.5.4",
            "error": "Unknown method: frobnicate"
        }))
        .expect("a valid envelope validates");
        assert_eq!(
            envelope.body,
            EnvelopeBody::Error("Unknown method: frobnicate".to_owned())
        );
    }

    #[test]
    fn missing_protocol_or_engine_stamps_are_refused() {
        for payload in [
            json!({}),
            json!({"engine": "0.5.4", "result": {}}),
            json!({"protocol": 1, "result": {}}),
            json!("a string is not an envelope"),
        ] {
            assert_eq!(
                validate_envelope(payload),
                Err(EnvelopeError::MissingStamps)
            );
        }
    }

    #[test]
    fn an_unknown_protocol_version_is_refused_with_the_reported_number() {
        let error = validate_envelope(json!({"protocol": 2, "engine": "0.5.4", "result": {}}))
            .expect_err("a protocol Studio does not know is refused, not guessed at");
        assert_eq!(error, EnvelopeError::UnknownProtocol(2));
    }

    #[test]
    fn an_engine_outside_the_tested_range_is_refused() {
        let error = validate_envelope(json!({"protocol": 1, "engine": "0.7.0", "result": {}}))
            .expect_err("an engine outside the tested range is refused");
        assert_eq!(
            error,
            EnvelopeError::EngineOutsideTestedRange("0.7.0".to_owned())
        );
    }

    #[test]
    fn an_engine_version_that_is_not_semver_is_refused() {
        let error = validate_envelope(json!({
            "protocol": 1,
            "engine": "not.a.version",
            "result": {}
        }))
        .expect_err("a malformed engine stamp is refused");
        assert_eq!(
            error,
            EnvelopeError::MalformedEngineVersion("not.a.version".to_owned())
        );
    }

    #[test]
    fn both_or_neither_result_and_error_is_an_unexpected_variant() {
        for payload in [
            json!({"protocol": 1, "engine": "0.5.4", "result": {}, "error": "both"}),
            json!({"protocol": 1, "engine": "0.5.4"}),
            json!({"protocol": 1, "engine": "0.5.4", "error": 42}),
        ] {
            let outcome = validate_envelope(payload.clone());
            assert_eq!(
                outcome,
                Err(EnvelopeError::UnexpectedVariant),
                "payload: {payload}"
            );
        }
    }

    #[test]
    fn the_stamps_are_checked_before_the_variant() {
        // An envelope wrong in several ways reports the most fundamental
        // mismatch first: an unknown protocol outranks a bad variant.
        let error = validate_envelope(json!({
            "protocol": 2,
            "engine": "0.9.9",
            "result": {},
            "error": "x"
        }))
        .expect_err("refused");
        assert_eq!(error, EnvelopeError::UnknownProtocol(2));
    }
}
