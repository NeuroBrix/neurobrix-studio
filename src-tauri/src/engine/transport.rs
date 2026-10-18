//! The engine's serving wire protocol: length-prefixed JSON over a stream.
//! Wire format: `[4-byte big-endian uint32 length][UTF-8 JSON payload]`
//! (serving/protocol.py). The engine caps messages at 64 MiB; Studio refuses
//! anything larger BEFORE allocating, so a corrupt length can never force a
//! huge allocation. Every I/O is deadline-bounded: a wedged daemon can
//! never hang a command.

use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

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
    serde_json::from_str(text).map_err(TransportError::InvalidJson)
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
