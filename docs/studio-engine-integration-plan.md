# Studio–engine integration plan

Status: planned; implementation is deferred to a separate session.
Recorded: 2026-09-12. Foundation issue: #1.
Branch when recorded: `feat/NS-1-studio-foundation`.

Read this alongside [README.md](../README.md), [ROADMAP.md](../ROADMAP.md),
[CONVENTIONS.md](../CONVENTIONS.md), and [AGENTS.md](../AGENTS.md).
This document records the integration direction; it does not claim the features exist
or replace the engine's public protocol specification.

## Goal and agreed scope

Build a desktop interface for NeuroBrix with an LM Studio-like local workflow:
discover models, download them, load a model, and chat with streamed responses.
Prioritize clear ownership, simplicity, and performance over general infrastructure.

The user selected:

- Changes may span both Studio and the NeuroBrix engine repository.
- The first integration includes text chat and model discovery/downloads.
- Engine connections are local only.
- Conversations persist across app restarts in local SQLite storage.
- Active work stops on application quit, with confirmation when work is active.
- Start with an existing engine installation; bundled Python and engine packaging
  remain a later roadmap stage.

Defer remote engines, background/tray execution, multi-model scheduling, agents,
media workspaces, and a new Studio account system. LM Studio is a workflow reference,
not a requirement to reproduce every feature or its implementation.

## Current implementation and gaps

Studio currently has a SvelteKit SPA scaffold and a Rust `greet` command. It has no
engine integration, operation manager, or conversation database.

The engine was inspected in the sibling `neurobrix` checkout, whose HEAD was
`50e895f`. These observations describe the inspected working tree, not a guaranteed
release contract; recheck them before implementation.

- `src/neurobrix/serving/protocol.py` implements length-prefixed JSON over Unix
  sockets on macOS/Linux and loopback TCP on Windows. It is not an HTTP API.
- The request envelope contains `method` and `params`; it is not a complete JSON-RPC
  2.0 implementation with request IDs and protocol version negotiation.
- `src/neurobrix/serving/server.py` dispatches generation, chat, completion, template,
  conversation reset, status, and shutdown operations. Connections are handled
  synchronously; a busy generation blocks normal control dispatch.
- Generation streaming reports token IDs and progress fields. Studio needs decoded
  text deltas from the engine, not a frontend tokenizer.
- There is no explicit cancellation method in the inspected dispatcher. A socket
  send failure may interrupt streaming, but is not an acknowledged cancellation API.
- `src/neurobrix/cli/commands/registry.py` owns hub queries, imports, extraction,
  inventory, and removal. Its human-facing output is not a supported machine contract.
- Unix socket location can be overridden through `NBX_SOCKET_PATH`; Windows uses
  a fixed loopback port and does not currently offer equivalent instance isolation.
- Model imports already have licence and hub-access requirements. Studio must preserve
  those requirements and expose recovery steps.

The hosted NeuroBrix documentation returned no readable body through the browsing
tool during planning. Engine findings above came from local source and the local CLI
reference, which may differ from published documentation or installed releases.

## Architecture and ownership

Use three layers:

```text
Svelte UI and typed frontend services
                 |
         Tauri commands/channels
                 |
Rust desktop integration and local persistence
                 |
       Public CLI and native daemon protocol
                 |
          NeuroBrix engine
```

| Operation | Owner |
| --- | --- |
| Layout, navigation, dialogs, drafts, filtering loaded lists | Svelte |
| Initial route reads, component loading/error states, form feedback | Svelte loaders and scoped managers |
| Engine discovery, compatibility checks, process ownership | Rust |
| CLI execution, sockets, timeouts, active operation tracking | Rust |
| Durable conversations, filesystem access, persisted application settings | Rust |
| Hub queries, downloads, extraction, model inventory and removal | Engine, invoked through Rust |
| Model loading, hardware decisions, tokenization, context preparation | Engine |
| Inference, decoded text streaming, cancellation | Engine |
| Rendering text and progress, presentation batching | Svelte |

Rust owns native integration, not a second inference runtime or model downloader.
Do not import Python engine internals, inspect its private cache as an alternative
inventory API, or reproduce its execution and storage rules.

### Options considered

| Approach | Decision |
| --- | --- |
| Frontend talks directly to an HTTP engine | Defer: requires a new transport and splits process control from inference. |
| Rust bridges the public CLI and native daemon | Recommended: fits existing interfaces and centralizes native ownership. |
| Rust becomes a separate orchestration server | Defer: queues, another API server, and generalized scheduling are unnecessary for this scope. |

Keep one small engine manager, specific operation methods, and ordinary typed services.
Do not introduce a generic job framework, dependency-injection container, or schema
generation pipeline before a concrete need appears.

## Contract changes to implement

### Engine public interface

- Add machine-readable discovery that works without loading a model. Report engine
  version, protocol version, endpoint information, supported operations, and availability.
  Read paths and capabilities from the engine's source of truth instead of duplicating
  constants in Studio.
- Document the native protocol and add a compatibility handshake. Preserve existing
  CLI consumers and human-facing commands while introducing the new contract.
- Provide JSON results for discovery, hub queries, inventory, and removal. Provide
  newline-delimited JSON for long-running CLI progress; send diagnostics to stderr.
  Never require Studio to parse terminal tables, prompts, or progress bars.
- Extend caller-managed completion to stream decoded text deltas and a terminal result.
  The engine applies its chat template and context policy; Studio supplies messages.
- Add operation identity, structured failures, and explicit cancellation. Keep control
  requests responsive while inference runs; adding a dispatch case to the current
  synchronous loop is insufficient.
- Confirm cancellation only when native work has stopped. A cancelled UI subscription,
  disconnect, or timeout must not be presented as confirmed engine cancellation.
- Keep download, extraction, and installation lifecycle in the engine. Emit real byte
  counts and processing phases, clean up incomplete work, and expose an installed model
  only after successful completion. Use indeterminate UI when a phase has no known total.
- Preserve licence acceptance and gated-access behavior. Never silently pass an
  acceptance flag; obtain explicit acceptance for the relevant model when required.

The first implementation increment must specify exact command names, message shapes,
compatibility rules, and cancellation semantics in the engine's public contract with
fixtures. These are proposed capabilities, not names or schemas already implemented.

### Rust desktop interface

- Expose specific Tauri commands for discovery/status, model operations, generation,
  cancellation, and conversation persistence. Do not expose arbitrary shell execution
  or an unrestricted engine-method proxy to the frontend.
- Use argument arrays for CLI execution and asynchronous native I/O. Keep a small
  engine manager containing connection state, owned child handles, and active operations.
- Use Tauri channels for operation streams. Bound buffering, preserve text and terminal
  results, and coalesce progress/display updates without delaying cancellation.
- Launch an owned foreground daemon. Detect pre-existing daemon conflicts rather than
  automatically taking ownership or terminating another program's process.
- Normalize transport, compatibility, contract-validation, and engine errors into
  structured failures with actionable messages.
- On application quit, confirm active work, cancel it, and stop only owned processes.
  Forced termination is an explicit recovery action, not successful cancellation.

### Frontend services and data ownership

- Use universal page loaders for initial route data. They perform reads only; opening
  or preloading a route must not start a daemon, download, or generation.
- Put Tauri calls and Valibot parsing in typed services. Components call scoped resource
  and operation managers, following the existing conventions.
- Route changes must not accidentally stop native work. Component disposal removes its
  subscription; explicit user actions or application shutdown cancel operations.
- Maintain one owner for each response. Refresh loader-owned data through targeted
  invalidation and component-owned data through resource methods.
- Display unavailable and unsupported capabilities honestly. Keep pending cancellation
  distinct from cancelled, failed, and completed states.

### Validation and type ownership

Rust validates messages as they cross the engine boundary. Frontend services validate
Tauri responses/events with Valibot and infer their output types. Loaders, managers,
and components consume those types without repeatedly parsing unchanged data.

Maintain public contract examples and shared fixtures exercised by engine, Rust, and
TypeScript tests. These tests guard against drift; TypeScript annotations or Rust
deserialization alone do not prove semantic engine compatibility.

### Conversations and settings

- Rust owns SQLite storage and migrations for conversations and messages. The frontend
  owns the active presentation and unsent drafts.
- Keep durable conversation history in Studio. Submit the selected conversation's
  messages to caller-managed completion rather than coupling saved chats to one global
  daemon conversation.
- Store completion status and preserve partial responses as interrupted after failure
  or cancellation. Use bounded periodic checkpoints and terminal writes, not a database
  write for every token. Mark unfinished operations interrupted after a restart.
- Purely visual transient state stays in Svelte. Persist durable application preferences
  through Rust; do not create an additional global state framework.

## Delivery sequence

Each increment follows the project's issue, branch, verification, and PR workflow when
authorized. This plan spans roadmap stages; saving it does not mark those stages complete.

1. **Public contract foundation.** Recheck the engine implementation, finalize the
   protocol specification and fixtures, add discovery/capability output, and make
   compatibility checks testable without loading model weights.
2. **Rust integration.** Implement typed transport, owned daemon lifecycle, discovery,
   status, structured failures, and channel delivery. Verify the real native boundary.
3. **Model workflow.** Add machine-readable engine model commands and Studio browsing,
   download progress/cancellation, inventory refresh, and removal. Preserve licence
   requirements and report authentication requirements through the existing hub flow.
4. **Chat workflow.** Implement responsive engine control, decoded completion streaming,
   cancellation, SQLite history, and route-safe UI subscriptions.
5. **End-to-end verification.** Exercise the packaged application with a real compatible
   engine and model, then update README status and record platform-specific evidence.

Default limits: one loaded model, one active generation, and one active download.
Reject duplicate operations explicitly. Model switching waits for generation to stop.
Do not remove a model being loaded, used, or installed. Defer download resume across
application restarts; interrupted imports must remain recoverable through a clean retry.

Public downloads are the first supported hub workflow. A new Studio login system is
out of scope; gated models must show the existing access requirement and recovery path.

## Acceptance and verification

Use the testing stack in [CONVENTIONS.md](../CONVENTIONS.md#test-layers): Vitest for
logic and real-browser Svelte components, Cargo tests for native logic, and WebdriverIO
with its optional embedded driver for native workflows. The initial scaffold smoke
test covers workspace navigation and reloads, without application IPC. Add deterministic engine protocol fixtures
as the contract is implemented; run real-engine and GPU acceptance separately.
Keep the `e2e` feature out of production builds and do not equate instrumented-app
success with installer or cross-platform validation.

- Discovery handles absent, incompatible, and unavailable engines without loading models.
- Contract tests cover malformed messages, truncated frames, invalid lengths, unexpected
  variants, and incompatible versions across the engine, Rust, and Valibot boundaries.
- Native integration handles disconnects, crashes, stale endpoints, pre-existing daemon
  conflicts, and timeouts without claiming work has stopped when it has not.
- Generation cancellation is responsive and acknowledged; a subsequent generation
  succeeds. Text arrives in order and exactly one terminal outcome is recorded.
- Downloads cover failure, cancellation, extraction failure, duplicate requests, licence
  requirements, unknown totals, and incomplete-install cleanup.
- Navigation does not duplicate requests or cancel active native work. Returning to a
  view recovers its operation status and current content.
- Conversation tests cover persistence, reopening, partial-response checkpoints,
  interruption recovery, and separation between saved conversations.
- Quit tests verify confirmation during active work, cleanup of owned processes, and
  preservation of unrelated engine processes.
- Run frontend checks/build, appropriate Rust and engine tests, and real packaged IPC
  checks. Test transports deterministically without a GPU where possible; verify real
  inference separately on target hardware.
- Verify Unix and Windows transport behavior independently before claiming support.
  Browser success, CPU tests, or a frontend build are not proof of packaged routing,
  GPU compatibility, or native cancellation.

## Handoff for the implementation session

Start with increment 1. Read both repositories' current instructions and check their
working trees before editing. Revalidate the source observations and finalize the exact
public contract before adding Studio features that depend on it. The engine checkout is
separate from this repository and may require separate write authorization.

No engine compatibility version range or platform support claim is established by this
plan. Record the tested range and actual verification results during implementation.

## References

- [NeuroBrix engine repository](https://github.com/NeuroBrix/neurobrix)
- [NeuroBrix documentation](https://docs.neurobrix.es/en/home)
- [LM Studio workflow reference](https://lmstudio.ai/)
- [Tauri frontend communication and channels](https://tauri.app/develop/calling-frontend/)
- [Valibot parsing](https://valibot.dev/guides/parse-data/)
- [Valibot type inference](https://valibot.dev/guides/infer-types/)
