//! Short-lived daemon connections: connect, call `status` (the handshake),
//! close. The daemon serves ONE connection at a time (server.py binds with
//! `listen(1)` and a serial accept loop), so Studio never parks a
//! connection — a status read during another client's generation would
//! block behind it. Responsive control is Stage 3 engine work; the
//! limitation is recorded in the docs.

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::{TcpStream, UnixStream};

use super::endpoint::ConnectAddress;
use super::transport::{self, EnvelopeBody, TransportError, TransportTimeouts};
use super::version::{check_supported, VersionCheck, VersionError, SUPPORTED_ENGINE_RANGE};

/// The operations Stage 2 invokes; validated by the handshake before any
/// other operation is attempted.
pub const REQUIRED_OPERATIONS: &[&str] = &["status", "shutdown"];

/// The `status` result: the identity record merged with serving state
/// (serving/engine.py `get_status` + `daemon_identity`). Optional extras
/// (`strategy`, `vram_used_gb`, `context`) are ignored — Studio renders
/// what is reported, nothing invented.
#[derive(Debug, PartialEq, Deserialize)]
pub struct StatusRecord {
    // -- identity (daemon_identity) --
    pub engine: String,
    pub protocol: u64,
    pub endpoint: super::endpoint::EndpointRecord,
    pub operations: Vec<String>,
    // -- serving state (get_status) --
    pub model: String,
    pub hardware: String,
    pub family: Option<String>,
    pub loaded: bool,
    pub mode: String,
    pub warm_serving: bool,
}

/// Every way a daemon call can fail; none is a crash. "Nothing is
/// listening" is a normal state — while a daemon loads or when none runs —
/// carried inside the transport variant.
#[derive(Debug, PartialEq)]
pub enum DaemonCallError {
    Transport(TransportError),
    Envelope(transport::EnvelopeError),
    /// The envelope was valid but the answer does not follow the contract
    /// this call expects — the status record, its identity, or a required
    /// operation.
    Contract(String),
    /// The daemon answered its own failure; a plain string, carried
    /// verbatim, never invented into structured variants.
    DaemonError(String),
}

impl std::fmt::Display for DaemonCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DaemonCallError::Transport(error) => error.fmt(f),
            DaemonCallError::Envelope(error) => error.fmt(f),
            DaemonCallError::Contract(reason) => {
                write!(f, "the daemon's answer breaks its contract: {reason}")
            }
            DaemonCallError::DaemonError(text) => {
                write!(f, "the daemon reported an error: {text}")
            }
        }
    }
}

impl std::error::Error for DaemonCallError {}

impl From<TransportError> for DaemonCallError {
    fn from(error: TransportError) -> Self {
        DaemonCallError::Transport(error)
    }
}

impl From<transport::EnvelopeError> for DaemonCallError {
    fn from(error: transport::EnvelopeError) -> Self {
        DaemonCallError::Envelope(error)
    }
}

/// A connection to the daemon's stream. Both socket kinds are Unpin, so
/// the delegation below needs no pin projection.
pub enum DaemonStream {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl tokio::io::AsyncRead for DaemonStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            DaemonStream::Unix(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
            DaemonStream::Tcp(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for DaemonStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            DaemonStream::Unix(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
            DaemonStream::Tcp(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            DaemonStream::Unix(stream) => std::pin::Pin::new(stream).poll_flush(cx),
            DaemonStream::Tcp(stream) => std::pin::Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            DaemonStream::Unix(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
            DaemonStream::Tcp(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
        }
    }
}

/// Connect with a bounded timeout. A refusal or a missing socket is
/// "nothing is listening" — the normal state while a daemon loads or when
/// none is running — and never a loud failure.
pub async fn connect(
    address: &ConnectAddress,
    timeouts: &TransportTimeouts,
) -> Result<DaemonStream, TransportError> {
    match address {
        ConnectAddress::Unix(path) => {
            match tokio::time::timeout(timeouts.connect, UnixStream::connect(path)).await {
                Ok(Ok(stream)) => Ok(DaemonStream::Unix(stream)),
                Ok(Err(error)) => match error.kind() {
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound => {
                        Err(TransportError::NotListening)
                    }
                    _ => Err(TransportError::Connect(error.to_string())),
                },
                Err(_) => Err(TransportError::Timeout {
                    operation: "connect",
                }),
            }
        }
        ConnectAddress::Tcp { host, port } => {
            match tokio::time::timeout(timeouts.connect, TcpStream::connect((*host, *port))).await {
                Ok(Ok(stream)) => Ok(DaemonStream::Tcp(stream)),
                Ok(Err(error)) => match error.kind() {
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound => {
                        Err(TransportError::NotListening)
                    }
                    _ => Err(TransportError::Connect(error.to_string())),
                },
                Err(_) => Err(TransportError::Timeout {
                    operation: "connect",
                }),
            }
        }
    }
}

/// One request on one connection: write the request frame, read the
/// response frame, validate the envelope's stamps — every time.
async fn call(
    stream: &mut DaemonStream,
    method: &str,
    params: Value,
    timeouts: &TransportTimeouts,
) -> Result<transport::Envelope, DaemonCallError> {
    let request = json!({ "method": method, "params": params });
    transport::write_frame(stream, &request, timeouts.write).await?;
    let response = transport::read_frame(stream, timeouts.read).await?;
    Ok(transport::validate_envelope(response)?)
}

/// The handshake: connect, call `status`, and validate the identity record
/// — protocol, engine version, endpoint shape, required operations —
/// BEFORE any other operation. One short-lived connection per call.
pub async fn connect_and_read_status(
    address: &ConnectAddress,
    timeouts: &TransportTimeouts,
) -> Result<StatusRecord, DaemonCallError> {
    let mut stream = connect(address, timeouts).await?;
    let envelope = call(&mut stream, "status", json!({}), timeouts).await?;
    let result = match envelope.body {
        EnvelopeBody::Result(result) => result,
        EnvelopeBody::Error(text) => return Err(DaemonCallError::DaemonError(text)),
    };
    let record: StatusRecord = serde_json::from_value(result)
        .map_err(|error| DaemonCallError::Contract(error.to_string()))?;
    validate_identity(&record)?;
    // The response's stamps and its identity record must agree: one
    // response, one engine. A daemon that stamps itself one version and
    // reports another is refused, not guessed about.
    if envelope.engine_version != record.engine {
        return Err(DaemonCallError::Contract(format!(
            "the daemon stamped its response as engine {} but its identity \
             record reports {}",
            envelope.engine_version, record.engine
        )));
    }
    Ok(record)
}

/// Stage 2's only other wire call: the shutdown request, on its own
/// short-lived connection. Sent only to a daemon that passed the status
/// handshake (the lifecycle enforces that), and the stamps are still
/// re-checked on this envelope. Confirmation is process exit and
/// instance-file cleanup — never this response alone.
pub async fn request_shutdown(
    address: &ConnectAddress,
    timeouts: &TransportTimeouts,
) -> Result<(), DaemonCallError> {
    let mut stream = connect(address, timeouts).await?;
    let envelope = call(&mut stream, "shutdown", json!({}), timeouts).await?;
    match envelope.body {
        EnvelopeBody::Result(_) => Ok(()),
        EnvelopeBody::Error(text) => Err(DaemonCallError::DaemonError(text)),
    }
}

/// Validate the identity record the handshake requires. The endpoint shape
/// and every required field are enforced by deserializing into
/// `StatusRecord`; the protocol and engine versions and the required
/// operations are checked here.
fn validate_identity(record: &StatusRecord) -> Result<(), DaemonCallError> {
    if record.protocol != transport::SUPPORTED_PROTOCOL_VERSION {
        return Err(DaemonCallError::Contract(format!(
            "the daemon reports protocol version {}, which this version of \
             Studio does not understand (it speaks version {})",
            record.protocol,
            transport::SUPPORTED_PROTOCOL_VERSION
        )));
    }
    match check_supported(&record.engine) {
        Ok(VersionCheck::Compatible) => {}
        Ok(VersionCheck::Incompatible) => {
            return Err(DaemonCallError::Contract(format!(
                "the daemon serves engine version {}, which is outside the \
                 tested range {}",
                record.engine, SUPPORTED_ENGINE_RANGE
            )));
        }
        Err(VersionError::Malformed) => {
            return Err(DaemonCallError::Contract(format!(
                "the daemon reports an engine version that is not valid \
                 SemVer: '{}'",
                record.engine
            )));
        }
    }
    for required in REQUIRED_OPERATIONS {
        if !record
            .operations
            .iter()
            .any(|operation| operation == required)
        {
            return Err(DaemonCallError::Contract(format!(
                "the daemon does not answer the required '{required}' operation"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testing::{
        dead_endpoint, dialable, envelope_with, fake_daemon, status_body, test_timeouts,
    };
    use std::time::Duration;

    #[tokio::test]
    async fn a_status_handshake_reads_the_identity_through_a_real_socket() {
        let (endpoint, received) = fake_daemon(envelope_with(status_body())).await;
        let record = connect_and_read_status(&dialable(&endpoint), &test_timeouts())
            .await
            .expect("a compliant daemon handshake succeeds");
        assert_eq!(record.model, "TinyLlama-1.1B-Chat-v1.0");
        assert_eq!(record.engine, "0.5.4");
        assert_eq!(record.protocol, 1);
        assert!(record.loaded);
        // The handshake sent exactly one status request on that connection.
        let request = received.lock().expect("record lock").clone();
        assert_eq!(request, Some(json!({"method": "status", "params": {}})));
    }

    #[tokio::test]
    async fn a_shutdown_request_reaches_the_engine_method() {
        let (endpoint, received) = fake_daemon(json!({
            "protocol": 1,
            "engine": "0.5.4",
            "result": {"status": "ok"}
        }))
        .await;
        request_shutdown(&dialable(&endpoint), &test_timeouts())
            .await
            .expect("a confirmed shutdown result is accepted");
        let request = received.lock().expect("record lock").clone();
        assert_eq!(request, Some(json!({"method": "shutdown", "params": {}})));
    }

    #[tokio::test]
    async fn nothing_listening_is_a_normal_state_not_a_loud_failure() {
        let outcome =
            connect_and_read_status(&dialable(&dead_endpoint().await), &test_timeouts()).await;
        assert_eq!(
            outcome,
            Err(DaemonCallError::Transport(TransportError::NotListening))
        );
    }

    #[tokio::test]
    async fn a_daemon_error_answer_is_carried_verbatim() {
        let (endpoint, _) = fake_daemon(json!({
            "protocol": 1,
            "engine": "0.5.4",
            "error": "Unknown method: status"
        }))
        .await;
        let outcome = connect_and_read_status(&dialable(&endpoint), &test_timeouts()).await;
        assert_eq!(
            outcome,
            Err(DaemonCallError::DaemonError(
                "Unknown method: status".to_owned()
            ))
        );
    }

    #[tokio::test]
    async fn an_answer_without_its_stamps_is_refused() {
        let (endpoint, _) = fake_daemon(json!({"result": status_body()})).await;
        let outcome = connect_and_read_status(&dialable(&endpoint), &test_timeouts()).await;
        assert_eq!(
            outcome,
            Err(DaemonCallError::Envelope(
                transport::EnvelopeError::MissingStamps
            ))
        );
    }

    #[tokio::test]
    async fn an_identity_from_a_future_protocol_is_refused_with_the_number() {
        let mut body = status_body();
        body["protocol"] = json!(2);
        let (endpoint, _) = fake_daemon(envelope_with(body)).await;
        match connect_and_read_status(&dialable(&endpoint), &test_timeouts()).await {
            Err(DaemonCallError::Contract(reason)) => {
                assert!(reason.contains("protocol version 2"), "reason: {reason}");
            }
            other => panic!("expected a contract refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_identity_from_an_untested_engine_is_refused() {
        let mut body = status_body();
        body["engine"] = json!("0.7.0");
        // The envelope stamp stays inside the range; the record is what
        // disagrees — the record itself is validated, not just the envelope.
        let (endpoint, _) = fake_daemon(envelope_with(body)).await;
        match connect_and_read_status(&dialable(&endpoint), &test_timeouts()).await {
            Err(DaemonCallError::Contract(reason)) => {
                assert!(
                    reason.contains("0.7.0") && reason.contains("tested range"),
                    "reason: {reason}"
                );
            }
            other => panic!("expected a contract refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_identity_missing_a_required_operation_is_refused() {
        let mut body = status_body();
        body["operations"] = json!(["generate", "chat", "status"]);
        let (endpoint, _) = fake_daemon(envelope_with(body)).await;
        match connect_and_read_status(&dialable(&endpoint), &test_timeouts()).await {
            Err(DaemonCallError::Contract(reason)) => {
                assert!(reason.contains("shutdown"), "reason: {reason}");
            }
            other => panic!("expected a contract refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn the_stamps_and_the_identity_must_name_the_same_engine() {
        let mut body = status_body();
        body["engine"] = json!("0.5.5"); // the record says 0.5.5...
        let (endpoint, _) = fake_daemon(json!({
            "protocol": 1,
            "engine": "0.5.4", // ...while the stamps say 0.5.4
            "result": body
        }))
        .await;
        match connect_and_read_status(&dialable(&endpoint), &test_timeouts()).await {
            Err(DaemonCallError::Contract(reason)) => {
                assert!(
                    reason.contains("0.5.4") && reason.contains("0.5.5"),
                    "reason: {reason}"
                );
            }
            other => panic!("expected a contract refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_status_record_parses_identity_and_serving_state() {
        let record: StatusRecord =
            serde_json::from_value(status_body()).expect("the verified record parses");
        assert_eq!(record.model, "TinyLlama-1.1B-Chat-v1.0");
        assert_eq!(record.hardware, "v100-32g");
        assert_eq!(record.family, Some("llm".to_owned()));
        assert!(record.loaded);
        assert_eq!(record.mode, "compiled");
        assert!(record.warm_serving);
        assert!(record.operations.contains(&"status".to_owned()));
        assert!(record.operations.contains(&"shutdown".to_owned()));
    }

    #[test]
    fn a_null_family_is_none_not_a_failure() {
        // The engine reports family as null until it resolves one.
        let mut body = status_body();
        body["family"] = Value::Null;
        let record: StatusRecord = serde_json::from_value(body).expect("null family parses");
        assert_eq!(record.family, None);
    }

    #[test]
    fn a_record_without_an_endpoint_fails_the_identity() {
        // The identity record always carries an endpoint; its absence is
        // a contract failure, not a default.
        let mut body = status_body();
        body.as_object_mut().expect("an object").remove("endpoint");
        let outcome: Result<StatusRecord, _> = serde_json::from_value(body);
        assert!(outcome.is_err());
    }

    #[test]
    fn an_endpoint_of_an_unknown_kind_fails_the_record() {
        let mut body = status_body();
        body["endpoint"] = json!({"kind": "quic", "address": "127.0.0.1"});
        let outcome: Result<StatusRecord, _> = serde_json::from_value(body);
        assert!(
            outcome.is_err(),
            "unexpected variants are contract failures"
        );
    }

    /// The Unix connect path, exercised where the transport is Unix —
    /// macOS and Linux. Windows runs the TCP path (the engine's transport
    /// there is loopback TCP); this test never claims Windows behavior.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_unix_daemon_is_reached_through_the_unix_connect_path() {
        use tokio::net::UnixListener;

        let dir = std::env::temp_dir().join("nbx-studio-client-tests");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(format!("daemon-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path); // a leftover from an earlier run
        let listener = UnixListener::bind(&path).expect("bind the unix listener");
        let answer = envelope_with(status_body());
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            if transport::read_frame(&mut socket, Duration::from_secs(5))
                .await
                .is_ok()
            {
                let _ = transport::write_frame(&mut socket, &answer, Duration::from_secs(5)).await;
            }
        });

        let record = connect_and_read_status(&ConnectAddress::Unix(path.clone()), &test_timeouts())
            .await
            .expect("the unix transport reaches the same handshake");
        assert_eq!(record.model, "TinyLlama-1.1B-Chat-v1.0");
        let _ = std::fs::remove_file(&path);
    }
}
