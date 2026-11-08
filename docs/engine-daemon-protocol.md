# The engine daemon protocol Studio consumes

Status: implemented in Studio issue [#15](https://github.com/NeuroBrix/neurobrix-studio/issues/15),
updated for the engine's 0.6.2 release per its wire-protocol document.
The engine itself was not changed. The native checks at the end say
exactly what was verified and how — what did not run is recorded as not
run.

**The authoritative specification is the engine's
[`docs/reference/wire-protocol.md`](https://github.com/NeuroBrix/neurobrix/blob/main/docs/reference/wire-protocol.md)**
(written for a reader with no access to the engine's code, protocol
version 1). This record states what Studio consumes from it and where it
enforces each part; the shapes below are cited to that document and to
the engine sources it names.

## The wire format

Framing is unchanged (wire-protocol.md §2):

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

From wire-protocol.md §3 (`make_request` / `make_response`):

```jsonc
// request — stamped with the protocol this client speaks
{ "protocol": 1, "method": "status", "params": {} }

// response — success, or the same head with "error"
{ "protocol": 1, "engine": "0.6.2", "result": { /* … */ } }
{ "protocol": 1, "engine": "0.6.2", "error": "Unknown method: x" }

// a work method's answer also carries an "op_id"; Stage 2 calls no work
// method, and the extra key is simply not one Studio reads
```

- `protocol` (still version **1** in 0.6.x) is stamped on **every**
  response; Studio refuses a protocol version it does not know instead of
  guessing (`transport::validate_envelope`).
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

Unchanged (wire-protocol.md §1), mirrored in Studio's
`endpoint::EndpointRecord` with fixtures from the engine's instance
tests:

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

## The identity records

`neurobrix info --json` (schema **`neurobrix.info/2`**) and the `status`
method carry the identity with one difference (wire-protocol.md §5, §6):

```jsonc
// both carry:
"engine": "0.6.2",
"protocol": 1,
"protocol_range": [1, 1],        // inclusive [min, max] of client versions served
"endpoint": { /* as above */ }

// info --json additionally carries the dispatcher's METHOD names:
"operations": ["generate", "chat", "complete", "template",
               "new_chat", "status", "cancel", "shutdown"]

// status instead replaces "operations" with its LIVE operations —
// records of the work running right now (op_id, method, state, steps),
// which Stage 2 does not model and so does not parse
```

- The handshake is connect → `status` → validate the identity record
  (protocol version, the served range containing Studio's version, the
  endpoint shape, the engine inside the tested range) **before any other
  operation** (`client::connect_and_read_status`). The daemon enforces the
  same range at the first frame and closes the connection on a refusal;
  Studio checks first and says so before the daemon has to.
- The method names the handshake requires — `status` and `shutdown` —
  live on the **info record** in 0.6.x, so that check belongs to the
  endpoint resolution (`daemon::resolve_endpoint`),
  once per session.
- The `status` result's serving fields are rendered as reported — model,
  hardware, family, loaded — nothing invented. Its optional extras
  (`strategy`, `vram_used_gb`, `context`) are ignored.

## What Studio calls

Stage 2 invokes exactly two methods: `status` (the handshake and every
read) and `shutdown` (the owned stop). The remaining operations —
`generate`, `chat`, `complete`, `template`, `new_chat`, and the new
`cancel` (answered only once the native work has really stopped) — are
Stage 3 inference work. The frontend reaches none of them directly: it
has three Tauri commands (`daemon_status`, `start_owned_daemon`,
`stop_owned_daemon` in `src-tauri/src/lib.rs`) and no method proxy, no
CLI arguments, no shell execution — the `method` names are Rust's choice
alone.

## The lifecycle contract

Unchanged in 0.6.x (wire-protocol.md §7 and the engine's
`cli/commands/serve.py`, `serving/server.py`):

- `neurobrix serve --model <id> [--hardware <hw>] [--timeout N]
  [--foreground]`; `--model` is required. Studio always passes
  `--foreground`, so the spawned child **is** the daemon: no fork, no
  detach; the handle Studio holds is the daemon's PID.
- The engine **refuses** to start when the instance already has a daemon:
  `cmd_serve` exits 1 and prints `[Serve] Daemon already running (PID …)`
  — verified unchanged in 0.6.2. Studio reports that as a conflict — with
  the pre-existing daemon's identity when its probe can read one — and
  never kills or adopts it.
- The PID file is written **before** the model loads; the socket binds
  **after**. Readiness is therefore a validated `status` within a bounded
  window — never process spawn, never PID-file presence. (0.6.x also
  emits a ready record on the foreground child's stdout — one
  `NBX-READY {…}` line, or `--ready-fd`/`--ready-file`; Studio keeps
  status polling as its uniform check, and the drained stdout tail
  carries the ready or failed record as diagnostics. Waiting on the
  record instead is a possible later increment, not a requirement.)
- An early child death surfaces the engine's own stdout/stderr excerpts
  (bounded tails, drained off-thread so a chatty daemon can never block
  on a full pipe).
- SIGINT/SIGTERM are handled; instance files (socket, pid, and a named
  Windows instance's port file — the log persists by the engine's own
  design) are cleaned on exit. Studio confirms a stop by process exit
  AND instance-file cleanup; a stop that times out is reported as not
  stopped, never assumed.
- The idle timeout (`--timeout`, engine default 1800) stops an idle
  daemon; the daemon logs `[Daemon] Idle timeout (Ns) — shutting down`
  — verified unchanged in 0.6.2. Studio passes the timeout explicitly so
  the invocation records it, and classifies that death as expected (with
  the recorded timeout) while never-zero exits and signals are
  unexpected.
- `neurobrix stop` escalates shutdown → SIGTERM → force kill against the
  instance's daemon regardless of who started it. It is **not** Studio's
  stop path; Studio stops only a daemon it owns, through the `shutdown`
  method on its own connection.

## The instance-selection decision

Unchanged. The engine keys daemon identity by `NBX_SOCKET_PATH`
(wire-protocol.md §1): on Unix the value is the socket path itself; on
Windows the stem derives the `.pid`, `.log` and `.port` files.

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
| connect / read / write | 2 s / 5 s / 5 s | bounded reads; control methods no longer wait behind a running generation in 0.6.x, but a hung socket still fails as a timeout |
| readiness deadline | 300 s | the engine's own background launcher waits up to 300 s for its ready record — Studio's budget matches that scale, not faster hardware than the user has |
| stop exit / cleanup grace | 10 s / 5 s | the engine's own stop path uses 3–5 s budgets |
| idle timeout passed to serve | 1800 s | recorded in the invocation and read back from it, never hardcoded at read |

## A former limitation, resolved

Stage 2 opened against a daemon that served **one connection at a
time**, recorded as a known limitation: a status read during another
client's generation blocked behind it. **0.6.x resolves it** — the daemon
answers control methods while it generates (per-request threads under an
operations manager, per its dispatch). Studio's short-lived connections
and bounded reads remain, but no read waits behind a generation. Cancellation
of running work is Stage 3, on the new `cancel` method.

## Where each piece lives in Studio

All engine access stays inside `src-tauri/src/engine` (a test enforces
the boundary):

| Concern | File |
| --- | --- |
| Framing, envelope stamps | `transport.rs` |
| Instance naming, endpoint record, dial resolution | `endpoint.rs` |
| The shared `info --json` record (schema 2) | `info.rs` |
| Connect, handshake, `status`, `shutdown` | `client.rs` |
| Owned child, readiness, confirmed stop, death classification | `lifecycle.rs` |
| Session ownership, states, command semantics | `daemon.rs` |
| Tauri commands | `src-tauri/src/lib.rs` |

## Verification record

What was verified, and how — and what was **not** run. A behaviour
observed on one platform is not claimed for the others.

| Verified | How | Evidence |
| --- | --- | --- |
| The consumed contract facts | read from the engine's `docs/reference/wire-protocol.md` (as shipped at 0.6.2) and its sources; the lifecycle string markers re-verified against the 0.6.2 tree | this document |
| Framing: exact-length reads, the 64 MiB cap before allocation, truncated frames, invalid JSON/UTF-8, bounded read/write timeouts | in-memory duplex unit tests | `transport.rs` |
| Envelopes: missing stamps, unknown protocol, engine outside the tested range, unexpected result/error variants | unit tests against `make_response`'s shape | `transport.rs` |
| Identity: valid record, the served protocol range, endpoint shapes (`unix`, `tcp`, `tcp` + `port_file`), record-vs-envelope disagreement | fixture tests referencing the engine's wire-protocol and instance tests | `info.rs`, `endpoint.rs`, `client.rs` |
| The info record's method names carry `status` and `shutdown`; a dispatcher without them is refused by name | fixture test against the schema-2 record | `daemon.rs` |
| Handshake, `status`, `shutdown`, refusal mapping; requests stamped with the protocol | tests against fake daemons on **real** sockets (loopback TCP everywhere; Unix sockets where that transport exists) | `client.rs` |
| Readiness (success, timeout, refusal, no-port-yet), confirmed stop, stop timeout ≠ stopped, death classification, stale files, start after death, the real spawner with real processes and bounded pipes | scripted-child tests; two unix-gated tests exercise the real spawner | `lifecycle.rs` |
| Manager states and command semantics: conflict with identity, duplicate rejection, ownership (an external daemon is never stopped), readiness-timeout recovery, expected/unexpected deaths | scripted children + persistent fake daemons on real sockets | `daemon.rs` |
| Handshake and status against a **real compatible engine** | **Not run yet.** Recorded with engine version, protocol version, OS, architecture and transport when run | issue #15 native verification |
| Owned start/readiness/stop against a real engine | **Not run yet** | issue #15 native verification |
| Windows transport behavior | **Not run.** The `tcp` and port-file record shapes are exercised by fixtures on Linux — fixture evidence, not native Windows evidence | — |

All checks above run with `cargo test --manifest-path
src-tauri/Cargo.toml --locked` (106 tests at the time of writing), with
`cargo clippy --locked --all-targets -- -D warnings` and
`cargo fmt --all --check` clean, on Linux x86_64.