# Development conventions

These conventions apply to developers and coding agents. Read them alongside
[AGENTS.md](AGENTS.md) and the [roadmap](ROADMAP.md). They describe how to build future
features; the engine integration and resource layer are not implemented yet.

Framework requirements are identified below. Choices about data ownership, resource
classes, and component style are project defaults, not restrictions imposed by Svelte.

## Code organization

Use the versions declared in `package.json`. SvelteKit 3 and adapter-static 4 prereleases
are intentionally pinned; TypeScript stays on the latest version compatible with both
SvelteKit and svelte-check. Check registry releases and peer requirements before upgrades.
Keep SvelteKit options in the Vite plugin, not `svelte.config.js`, and keep desktop version
polling disabled. TypeScript extends `$app/tsconfig` with explicit include/exclude lists.
Preserve the matching Tauri development URL and Vite port.

- Keep route orchestration in `src/routes`, transport and response validation in
  `src/lib/services`, and reactive resource classes in `src/lib/states`.
- Use `.svelte.ts` for modules containing runes, such as `model-manager.svelte.ts`.
  Use ordinary `.ts` for types, validation, and services without reactive state.
- Import library modules through `#lib` with explicit extensions, for example
  `#lib/states/model-manager.svelte.js`.
- Use concrete domain types at service boundaries. Parse external data as `unknown`
  and validate it before treating it as a model, progress event, or response.
- Keep components focused on rendering and interaction. Reuse shadcn-svelte components
  and existing theme tokens; keep native IPC and HTTP details in services.
- Keep theme tokens in `src/routes/layout.css` and component aliases in `components.json`
  aligned with `package.json`. Use the `cn` helper from `src/lib/utils.ts`. Add components
  with the project-pinned `pnpm exec shadcn-svelte add <component>` command.

## Svelte 5 components and reactivity

For new components, use typed `$props()`, event attributes such as `onclick` and
`onsubmit`, and callback props for component events. Use snippets with `{@render}` for
composable markup. Avoid introducing legacy `export let`, `on:`, slots, or event
dispatchers. Preserve compatible third-party APIs rather than rewriting them for style.

Treat props as changing, read-only inputs. Compute dependent values with `$derived` or
`$derived.by`; a one-time assignment from a prop will become stale. Keep local editable
drafts separate and define when a changed entity resets them. Expose `$bindable` only
when two-way binding is an intentional part of a component's public API.

Use `$state` only for values that need to update the UI or reactive computations.
Ordinary bookkeeping, such as request sequence counters, can remain plain fields.
Use immutable `$state.raw` snapshots for resource responses. Destructuring a state object
or resource can capture a stale value; destructuring `$props()` is compiler-supported
and is not the same case. Keep derived computations free of request side effects.

Key model, message, and operation lists by stable domain IDs, not array indexes. Preserve
identity when reordering or streaming updates so focus, drafts, and component state remain
attached to the correct item.

Prefer attachments for new element-bound DOM integrations and `<svelte:window>` or
`<svelte:document>` for global listeners. Attachments must clean up external resources.
Keep existing library actions when their API requires them. Effects are an escape hatch
for synchronizing external systems, not a substitute for derived values or event handlers.

Experimental async rendering remains disabled by project default. Do not enable it as
part of routine component work; use the loader/resource pattern described here.

See the official [Svelte best practices](https://svelte.dev/docs/svelte/best-practices)
and [props ownership guidance](https://svelte.dev/docs/svelte/$props).

## Choose one data owner

| Data | Owner | Refresh mechanism |
| --- | --- | --- |
| Initial data required by a route | Universal `+page.ts` loader | Targeted loader invalidation |
| Initial data shared by nested routes | Universal `+layout.ts` loader | Targeted loader invalidation |
| Component-specific queries, search, or lazy panels | A scoped resource class | Explicit resource methods |
| Inference streams, download progress, or telemetry | A scoped operation/subscription manager | Events or a bounded polling loop |

Do not fetch the same initial data in both a loader and `onMount`. Keep a resource near
the component or layout that owns its lifetime; share it through typed `createContext`
when several children need it. Establish and consume context during component
initialization; keep the provided reactive instance stable.

Avoid mutable module-level singleton managers by project default so ownership, reset,
and disposal remain explicit. A strictly client-only SPA can use shared module state;
it is not forbidden by SvelteKit. Any app-wide manager needs a documented lifetime and
reset policy. See [Svelte context](https://svelte.dev/docs/svelte/context) and
[SvelteKit state management](https://next.svelte.dev/docs/kit/state-management).

## Static SPA constraints

Keep SSR disabled, client rendering enabled, and the static adapter's `index.html`
fallback for the Tauri frontend. Native-dependent routes must not be prerendered. Do not
replace the fallback with a web-host example without checking Tauri's serving behavior.

There is no SvelteKit server in the packaged app. Server loaders, server endpoints,
server form actions, and remote functions requiring that server cannot supply runtime
features here. Use the service layer to reach native capabilities or the engine.

Keep native calls and browser-dependent side effects out of module initialization so
build tooling can inspect modules without starting application work. Treat browser
preview and the Tauri webview as distinct environments.

These constraints follow the project's deployment model and the
[SvelteKit SPA guide](https://next.svelte.dev/docs/kit/single-page-apps).

## Initial route data

Use typed `PageLoad` or `LayoutLoad` functions and typed page props from the generated
`./$types` module. Return plain data or an explicit availability result. Loaders should
read data, not trigger downloads, inference, daemon startup, or other mutations.

This app is a static Tauri SPA with SSR disabled. Use universal loaders, never
`+page.server.ts`, `+layout.server.ts`, server endpoints, or server-only remote functions.
Do not enable prerendering on routes that require native APIs.

For HTTP requests, pass the loader's `fetch` into the service. For native reads, use the
typed Tauri service wrapper. Detect unavailable native functionality in browser preview
and report it explicitly; never substitute fake data.

Start independent reads together and await data needed to render the route. Fetch optional
panel data when the panel needs it. Avoid waiting for parent data unless it is required.

Use a stable `depends(...)` key for reads SvelteKit cannot track, such as native IPC.
After a successful mutation, invalidate the affected loader dependency rather than all
routes. Let the loader's returned data remain the source of truth; read it reactively so
reruns update the page. A layout loader does not necessarily rerun on every navigation.

SvelteKit can reuse page and layout instances when route parameters change. `onMount`
is not a navigation hook. Read new page data reactively; reconcile or replace resources
when their entity identity changes. Use a keyed remount only when resetting the whole
subtree is intended, rather than discarding unrelated navigation or draft state.

The current `src/app.html` enables data preloading on hover, which can run a loader even
if the user never navigates. Loaders must therefore be safe to call speculatively. For
expensive native reads, set the relevant link's `data-sveltekit-preload-data` to `"tap"`,
or to `false` in SvelteKit 3 to disable data preloading. Code-only preloading can remain
enabled. See [link options](https://next.svelte.dev/docs/kit/link-options).

Treat an absent or incompatible engine as an explicit availability state the page can
render. Route-blocking failures belong in the route error boundary. An error is never
an empty successful result.

## Component-owned resources

Use Svelte 5 resource classes for component-owned asynchronous reads. Services perform
the request; the class manages state and request lifetime; the component renders state
and calls methods such as `load`, `refresh`, or `cancel`.

Expose data, pending state, and a useful error. Distinguish an initial load from refreshing
existing data. Keep the last successful result during a refresh of the same resource,
and show a refresh failure alongside that result. On an identity change, clear or replace
the old result so it cannot appear to belong to the new model or conversation.

Use an `AbortController` when the transport supports it, plus a request sequence guard.
Only the latest request may commit data, errors, or completion state. The guard is still
needed for transports that cannot abort, including ordinary Tauri command invocations.
Canceling a UI read does not imply the native operation stopped.

The following is a reference pattern, not an installed application service. It represents
one resource identity. The injected reader must validate responses and throw an `Error`
with a safe, actionable message when the read fails.

```ts
// src/lib/states/read-resource.svelte.ts
export class ReadResource<T> {
  data = $state.raw<T | undefined>(undefined);
  isLoading = $state(false);
  error = $state<string | null>(null);

  #sequence = 0;
  #controller: AbortController | undefined;

  constructor(private readonly read: (signal: AbortSignal) => Promise<T>) {}

  async load(): Promise<void> {
    const sequence = ++this.#sequence;
    this.#controller?.abort();
    const controller = new AbortController();
    this.#controller = controller;
    this.isLoading = true;
    this.error = null;

    try {
      const data = await this.read(controller.signal);
      if (sequence !== this.#sequence || controller.signal.aborted) return;
      this.data = data;
    } catch (error: unknown) {
      if (sequence !== this.#sequence || controller.signal.aborted) return;
      this.error = error instanceof Error ? error.message : "Unable to load data. Try again.";
    } finally {
      if (sequence === this.#sequence) {
        this.isLoading = false;
        this.#controller = undefined;
      }
    }
  }

  cancel(): void {
    ++this.#sequence;
    this.#controller?.abort();
    this.#controller = undefined;
    this.isLoading = false;
  }
}
```

Here `undefined` means no successful result yet; an empty array can be a successful result.
Treat `$state.raw` data as immutable snapshots and replace it when it changes. Keep the
resource instance intact when reading reactive properties; do not destructure its fields
into nonreactive local constants. Bind class methods through callbacks when using them
as event handlers so they retain `this`.

Prefer explicit event handlers for user-triggered requests. Use a synchronous `onMount`
callback for an initial component-owned read, start the asynchronous method inside it,
and return synchronous cleanup. An `async` onMount callback cannot return an unmount
cleanup function in the expected way.

For fetching, use `$effect` only when request inputs must react to changes. Capture the
intended inputs synchronously, keep incidental resource-state reads untracked, and return
cleanup that cancels the superseded request. Do not trigger another request by tracking the loading
state that the request itself changes. Use `$derived` for computed state.

If loader data must seed a resource, pass it in without fetching again. Define one owner
after that handoff, and explicitly reconcile later loader updates or recreate the resource
on identity changes. Never maintain two independently refreshed copies of the same list.

## Transport, mutations, and live updates

- Obtain engine addresses and command capabilities from configuration or discovery.
  Do not hardcode a localhost port, endpoint, or unverified response shape.
- Check HTTP status and validate payloads. A TypeScript generic on `invoke` or a type
  assertion on JSON does not validate the external response at runtime.
- Keep mutation state separate from read state. Prevent duplicate submissions; do not
  silently retry mutations. After success, refresh the owning resource or invalidate its
  loader. Keep drafts and user input intact when a request fails.
- Use a dedicated manager for streaming output and operation progress. Track operation
  identity and ignore late events from a previous run. Progress must come from the engine.
- Stop streams, timers, and listeners on disposal. If a subscription resolves after its
  owner has been disposed, immediately unsubscribe it. A component that borrows a shared
  manager must not dispose it; the owning layout or context provider does that.
- Prefer engine events for live updates. Where polling is needed, schedule the next read
  after the previous one finishes, use a configured interval, and stop when unused.
- A cancel action for inference or downloads must use the engine's cancellation contract.
  Distinguish requesting cancellation from confirmation that the operation has stopped.

## Loading and error UI

Provide distinct initial-loading, refreshing, empty, unavailable, and error states. Offer
an explicit retry where useful. Keep focus visible, label controls, announce meaningful
status changes, and allow users to select and copy model output and errors.

Keep failure handling at the appropriate boundary. Route-blocking loader failures use
the route's error UI. A `<svelte:boundary>` can contain rendering and effect errors, but
does not catch ordinary asynchronous request failures or errors in event handlers.
Resource and mutation methods must handle those explicitly and expose useful state.
Likewise, a boundary's pending UI does not automatically track a resource's `isLoading`.
See [Svelte boundaries](https://svelte.dev/docs/svelte/svelte-boundary).

## Verification

When implementing a resource, verify success, empty results, unavailable services, malformed
responses, failed refreshes, overlapping requests completing out of order, cancellation,
and disposal. Verify loaders are not duplicated on mount and identity changes cannot show
the previous entity's data. Use deferred promises or controlled test transports for races.

For component and navigation changes, add relevant behavioral coverage for changed
props, list reordering, reused route instances, hover preloading without navigation, and
owner cleanup. Verify errors reach the intended route, rendering, or resource UI.

For SPA routing changes, verify direct entry and reload on a nested route, internal
navigation, and back/forward navigation. Repeat applicable routing checks in the packaged
Tauri app to verify fallback and asset loading. A Vite browser check alone does not
establish that packaged routes work.

Run the project's type checks and build once dependencies are available. Document any
checks that could not run. Examples here do not establish that engine integration exists.

## Framework references

- [SvelteKit loaders](https://next.svelte.dev/docs/kit/load)
- [Rune-enabled modules](https://svelte.dev/docs/svelte/svelte-js-files)
- [Effects and cleanup](https://svelte.dev/docs/svelte/$effect)
- [Lifecycle hooks](https://svelte.dev/docs/svelte/lifecycle-hooks)
