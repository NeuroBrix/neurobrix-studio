//! Test-only fixtures shared by the engine's test modules. Compiled out of
//! normal builds. Fakes are allowed in tests, never as fabricated product
//! behavior (CONVENTIONS.md) — these stand in for a real daemon on real
//! sockets, so what they exercise is the real transport and framing.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use super::endpoint::{ConnectAddress, EndpointRecord};
use super::transport;

/// Loopback sockets are fast; this budget only bounds a wedged fake.
pub(crate) const TEST_IO: Duration = Duration::from_secs(5);

/// The bounded I/O budget shared by the transport-consuming tests.
pub(crate) fn test_timeouts() -> transport::TransportTimeouts {
    transport::TransportTimeouts {
        connect: TEST_IO,
        read: TEST_IO,
        write: TEST_IO,
    }
}

/// The status body the engine's `get_status` + `daemon_identity` produce
/// (verified 0.5.4 record; paths redacted).
pub(crate) fn status_body() -> Value {
    json!({
        "engine": "0.5.4",
        "protocol": 1,
        "endpoint": {"kind": "tcp", "address": "127.0.0.1", "port": 19384},
        "operations": [
            "generate", "chat", "complete", "template",
            "new_chat", "status", "shutdown"
        ],
        "model": "TinyLlama-1.1B-Chat-v1.0",
        "hardware": "v100-32g",
        "family": "llm",
        "loaded": true,
        "mode": "compiled",
        "warm_serving": true
    })
}

/// Wrap a result in the envelope every engine response carries
/// (`make_response`): the stamps plus exactly one of result/error.
pub(crate) fn envelope_with(result: Value) -> Value {
    json!({"protocol": 1, "engine": "0.5.4", "result": result})
}

/// A one-connection fake daemon on a real loopback socket: accepts, reads
/// one request frame, answers one canned frame, and records what arrived.
pub(crate) async fn fake_daemon(answer: Value) -> (EndpointRecord, Arc<Mutex<Option<Value>>>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind a loopback listener");
    let bound = listener.local_addr().expect("the bound address");
    let received = Arc::new(Mutex::new(None));
    let recorded = Arc::clone(&received);
    tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let Ok(request) = transport::read_frame(&mut socket, TEST_IO).await else {
            return;
        };
        *recorded.lock().expect("record lock") = Some(request);
        let _ = transport::write_frame(&mut socket, &answer, TEST_IO).await;
    });
    (
        EndpointRecord::Tcp {
            address: bound.ip(),
            port: Some(bound.port()),
            port_file: None,
        },
        received,
    )
}

/// A many-connection fake daemon on a real loopback socket: every
/// connection is read one frame and answered one canned frame, and every
/// request is recorded. For manager flows that connect repeatedly
/// (probe, readiness, reads).
pub(crate) async fn persistent_fake_daemon(
    answer: Value,
) -> (EndpointRecord, Arc<Mutex<Vec<Value>>>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind a loopback listener");
    let bound = listener.local_addr().expect("the bound address");
    let received = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&received);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let Ok(request) = transport::read_frame(&mut socket, TEST_IO).await else {
                continue;
            };
            recorded.lock().expect("record lock").push(request);
            let _ = transport::write_frame(&mut socket, &answer, TEST_IO).await;
        }
    });
    (
        EndpointRecord::Tcp {
            address: bound.ip(),
            port: Some(bound.port()),
            port_file: None,
        },
        received,
    )
}

/// The dialable address of a record the tests control.
pub(crate) fn dialable(record: &EndpointRecord) -> ConnectAddress {
    record.resolve().expect("the fixture's endpoint resolves")
}

/// An endpoint where nothing listens: a port bound then released. Reads
/// against it fail as "not listening" — the normal stopped state.
pub(crate) async fn dead_endpoint() -> EndpointRecord {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let bound = listener.local_addr().expect("address");
    drop(listener);
    EndpointRecord::Tcp {
        address: bound.ip(),
        port: Some(bound.port()),
        port_file: None,
    }
}
