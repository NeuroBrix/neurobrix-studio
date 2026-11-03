# The engine daemon protocol Studio consumes

Status: implemented in Studio issue [#15](https://github.com/NeuroBrix/neurobrix-studio/issues/15)
against engine `0.5.4` (the floor of the tested range `>=0.5.4,<0.6`). The
engine itself was not changed. The native checks at the end say exactly
what was verified and how — what did not run is recorded as not run.

The engine has **no standalone wire-protocol document**. The specification
today is its source and its tests, so this record cites the file each fact
comes from, and Studio's tests mirror the engine fixtures they depend on.

## The wire format

From `serving/protocol.py` (engine repo):

```text
[4-byte big-endian uint32 length][UTF-8 JSON payload]
```

- The engine caps messages at 64 MiB (`MAX_MESSAGE_SIZE`). Studio enforces
  the same cap **before allocating a payload buffer**
  (`src-tauri/src/engine/transport.rs`), so a corrupt length can never
  force a huge allocation.
- Truncated frames, invalid lengths, invalid UTF-8 and invalid JSON are
  contract failures, never crashes, and never silently dropped data.

## The envelopes

From `serving/protocol.py` (`make_request` / `make_response`):

```jsonc
// request — every call
{ "method": "status", "params": {} }

// response — success, or the same head with "error"
{ "protocol": 1, "engine": "0.5.4", "result": { /* … */ } }
{ "protocol": 1, "engine": "0.5.4", "error": "Unknown method: x" }
```

- `PROTOCOL_VERSION = 1` is stamped on **every** response; Studio refuses a
  protocol version it does not know instead of guessing
  (`transport::validate_envelope`).
- The engine stamp is checked against the tested range
  (`version::SUPPORTED_ENGINE_RANGE`) on **every** envelope, not just the
  first.
- `result` may be JSON `null` (`make_response()` with no result) — present
  is present, and it is a result, not an error.
- The `error` value is a **plain string**. Studio carries it verbatim
  (`DaemonCallError::DaemonError`); it does not expect structured error
  variants that do not exist on the wire.
- The envelope's stamps and the identity record inside `status` must name
  the same engine and protocol — one response, one engine
  (`client::connect_and_read_status`).

## The endpoint record and transports

From `serving/protocol.py` (`Instance.endpoint()`); mirrored in Studio's
`endpoint::EndpointRecord` with fixtures from the engine's
`tests/unit/serving/test_each_windows_instance_has_its_own_port.py`:

```jsonc
// Unix and macOS — AF_UNIX socket
{ "kind": "unix", "path": "/home/user/.neurobrix/daemon.sock" }

// Windows, the DEFAULT instance — a fixed loopback port, no port file
{ "kind": "tcp", "address": "127.0.0.1", "port": 19384 }

// Windows, a NAMED instance (NBX_SOCKET_PATH set) — the OS hands out the
// port at bind, records it in <stem>.port, and the clients of that
// instance read it there
{ "kind": "tcp", "address": "127.0.0.1", "port": null, "port_file": "C:/…/daemon.port" }
```

- Studio reads the endpoint from the engine's records — the same
  `info --json` probe discovery runs, executed with the Studio instance in
  the environment (see below) — and **never recomputes or hardcodes** an
  address (`daemon::resolve_endpoint`, resolved to a dialable address on
  every connection).
- A missing port file is the normal stopped state of a named Windows
  instance, not a failure. A port file that names no usable port is
  reported (`ResolveError::UnusablePortFile`), never guessed around.

## The identity record and `status`

From `serving/engine.py` (`daemon_identity` + `get_status`), read the same
way by `status` and `neurobrix info --json`
(`tests/unit/cli/test_the_daemon_names_its_protocol.py` on the engine
side):

```jsonc
{
  "engine": "0.5.4",          // identity
  "protocol": 1,
  "endpoint": { /* as above */ },
  "operations": ["generate", "chat", "complete", "template",
                  "new_chat", "status", "shutdown"],
  "model": "TinyLlama-1.1B-Chat-v1.0",   // serving state
  "hardware": "v100-32g",
  "family": "llm",            // null until the engine resolves one
  "loaded": true,
  "mode": "compiled",
  "warm_serving": true
  // optional extras (strategy, vram_used_gb, context) are ignored
}
```

- The handshake is connect → `status` → validate the identity record
  (protocol, engine inside the tested range, endpoint shape, required
  operations) **before any other operation**
  (`client::connect_and_read_status`). Stage 2 requires the `status` and
  `shutdown` operations.
- Studio renders what `status` reports — model, hardware, family, loaded —
  nothing invented.

## What Studio calls

Stage 2 invokes exactly two methods: `status` (the handshake and every
read) and `shutdown` (the owned stop). The remaining operations —
`generate`, `chat`, `complete`, `template`, `new_chat` — are Stage 3
inference work. The frontend reaches none of them directly: it has three
Tauri commands (`daemon_status`, `start_owned_daemon`, `stop_owned_daemon`
in `src-tauri/src/lib.rs`) and no method proxy, no CLI arguments, no shell
execution — the `method` names are Rust's choice alone.

## The lifecycle contract

From `cli/commands/serve.py` and `serving/server.py`:

- `neurobrix serve --model <id> [--hardware <hw>] [--timeout N]
  [--foreground]`; `--model` is required. Studio always passes
  `--foreground`, so the spawned child **is** the daemon: no fork, no
  detach; the handle Studio holds is the daemon's PID.
- The engine **refuses** to start when the instance already has a daemon:
  `cmd_serve` exits 1 and prints `[Serve] Daemon already running (PID …)`.
  Studio reports that as a conflict — with the pre-existing daemon's
  identity when its probe can read one — and never kills or adopts it.
- The PID file is written **before** the model loads; the socket binds
  **after**. Readiness is therefore a validated `status` within a bounded
  window — never process spawn, never PID-file presence. In
  `--foreground` the daemon emits no ready signal on Linux (the
  Windows/macOS ready file is a side effect of the background path, not a
  foreground contract), so status polling is the uniform readiness
  check on every platform.
- An early child death surfaces the engine's own stdout/stderr excerpts
  (bounded tails, drained off-thread so a chatty daemon can never block
  on a full pipe).
- SIGINT/SIGTERM are handled; instance files (socket, pid, and a named
  Windows instance's port file — the log persists by the engine's own
  design) are cleaned on exit (`_cleanup`). Studio confirms a stop by
  process exit AND instance-file cleanup; a stop that times out is
  reported as not stopped, never assumed.
- The idle timeout (`--timeout`, engine default 1800) stops an idle
  daemon; the daemon logs `[Daemon] Idle timeout (Ns) — shutting down`.
  Studio passes the timeout explicitly so the invocation records it, and
  classifies that death as expected (with the recorded timeout) while
  never-zero exits and signals are unexpected.
- `neurobrix stop` escalates shutdown → SIGTERM → force kill against the
  instance's daemon regardless of who started it. It is **not** Studio's
  stop path; Studio stops only a daemon it owns, through the `shutdown`
  method on its own connection.

## The instance-selection decision

The engine keys daemon identity by `NBX_SOCKET_PATH`
(`serving/protocol.py`, `Instance`): on Unix the value is the socket path
itself; on Windows the stem derives the `.pid`, `.log` and `.port` files.

Studio names its own instance:

- Value: `<application cache dir>/engine/daemon.sock` — obtained from
  Tauri's path API (`app_cache_dir`), never a hand-built location.
- Set identically for the owned child (spawn environment), Studio's
  connections (the endpoint record read back from the engine), and the
  `info --json` probe that reports the endpoint.
- Studio creates the instance directory itself; the engine only creates
  its own default daemon directory.

Why: Studio's daemons never collide with a user's terminal daemon on the
default instance (`~/.neurobrix`), and the instance files live under
Studio's own application-data location, per the roadmap's rule that no
path is written into the code.

## Studio's budgets

`daemon::DaemonConfig` — Studio's own choices; the engine's contracts set
the scale:

| Budget | Value | Rationale |
| --- | --- | --- |
| connect / read / write | 2 s / 5 s / 5 s | the daemon serves one connection at a time; bounded reads keep a busy generation from hanging a read |
| readiness deadline | 300 s | the engine's own background start waits up to 300 s for its ready signal — Studio's budget matches that scale, not faster hardware than the user has |
| stop exit / cleanup grace | 10 s / 5 s | the engine's own stop path uses 3–5 s budgets |
| idle timeout passed to serve | 1800 s | recorded in the invocation and read back from it, never hardcoded at read |

## Known limitation

The daemon serves **one connection at a time** (`listen(1)` and a serial
accept loop, `serving/server.py`): a status read during another client's
generation blocks behind it. Studio uses short-lived connections and the
bounded reads above. Responsive control is Stage 3 engine work; this
limitation stands until then.

## Where each piece lives in Studio

All engine access stays inside `src-tauri/src/engine` (a test enforces
the boundary):

| Concern | File |
| --- | --- |
| Framing, envelope stamps | `transport.rs` |
| Instance naming, endpoint record, dial resolution | `endpoint.rs` |
| The shared `info --json` record | `info.rs` |
| Connect, handshake, `status`, `shutdown` | `client.rs` |
| Owned child, readiness, confirmed stop, death classification | `lifecycle.rs` |
| Session ownership, states, command semantics | `daemon.rs` |
| Tauri commands | `src-tauri/src/lib.rs` |

## Verification record

What was verified, and how — and what was **not** run. A behaviour
observed on one platform is not claimed for the others.

| Verified | How | Evidence |
| --- | --- | --- |
| Framing: exact-length reads, the 64 MiB cap before allocation, truncated frames, invalid JSON/UTF-8, bounded read/write timeouts | in-memory duplex unit tests | `transport.rs` |
| Envelopes: missing stamps, unknown protocol, engine outside the tested range, unexpected result/error variants | unit tests against `make_response`'s shape | `transport.rs` |
| Identity: valid record, missing operations, endpoint shapes (`unix`, `tcp`, `tcp` + `port_file`), record-vs-envelope disagreement | fixture tests referencing the engine's instance tests | `info.rs`, `endpoint.rs`, `client.rs` |
| Handshake, `status`, `shutdown`, refusal mapping | tests against fake daemons on **real** sockets (loopback TCP everywhere; Unix sockets where that transport exists) | `client.rs` |
| Readiness (success, timeout, refusal, no-port-yet), confirmed stop, stop timeout ≠ stopped, death classification, stale files, start after death, the real spawner with real processes and bounded pipes | scripted-child tests; two unix-gated tests exercise the real spawner | `lifecycle.rs` |
| Manager states and command semantics: conflict with identity, duplicate rejection, ownership (an external daemon is never stopped), readiness-timeout recovery, expected/unexpected deaths | scripted children + persistent fake daemons on real sockets | `daemon.rs` |
| Handshake and status against a **real compatible engine** | **Not run yet.** Recorded with engine version, protocol version, OS, architecture and transport when run | issue #15 native verification |
| Owned start/readiness/stop against a real engine | **Not run yet** | issue #15 native verification |
| Windows transport behavior | **Not run.** The `tcp` and port-file record shapes are exercised by fixtures on Linux — fixture evidence, not native Windows evidence | — |

All checks above run with `cargo test --manifest-path
src-tauri/Cargo.toml --locked` (104 tests at the time of writing), with
`cargo clippy --locked --all-targets -- -D warnings` and
`cargo fmt --all --check` clean, on Linux x86_64.