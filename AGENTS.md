# NeuroBrix Studio agent guide

Read README.md, ROADMAP.md, and [CONVENTIONS.md](CONVENTIONS.md) before changing the app.
The project is in development; the current screen is a scaffold, not a working inference product.

## Project conventions

- Use Corepack-managed pnpm from the repository root, following the packageManager
  pin in package.json. Preserve pnpm-lock.yaml.
- The stack is Svelte 5, SvelteKit 3 (currently prerelease), TypeScript 6, Tauri 2,
  Tailwind CSS 4, and shadcn-svelte. Read package.json for exact versions.
- Prefer the latest mutually compatible tooling. Check registry releases and peer
  requirements before upgrading; do not force incompatible major versions. SvelteKit 3
  and adapter-static 4 prereleases are intentional and pinned exactly. TypeScript 7 is
  not yet supported by the current SvelteKit and svelte-check peer requirements.
- Configure SvelteKit and its static adapter in the sveltekit() Vite plugin in
  vite.config.js. Do not recreate svelte.config.js. Keep version polling disabled for
  the bundled desktop frontend.
- Use #lib subpath imports from package.json with explicit file extensions, and keep
  components.json aliases aligned. The root tsconfig.json extends $app/tsconfig and
  declares its own include/exclude lists. Do not restore the legacy $lib alias.
- Keep the frontend a static SPA: SSR is disabled in src/routes/+layout.ts,
  the adapter emits an index.html fallback, and Tauri loads ../build.
- Do not introduce SvelteKit server endpoints or server-only load functions.
- Use Svelte 5 runes for new reactive components. Reuse shadcn components under
  src/lib/components/ui and the cn helper in src/lib/utils.ts.
- Theme tokens and Tailwind imports belong in src/routes/layout.css; Tailwind runs
  through the Vite plugin. Add components with pnpm exec shadcn-svelte add <component>.
- Preserve the Tauri development URL and matching Vite port when editing build settings.

## Svelte 5 and SPA checklist

- Follow [CONVENTIONS.md](CONVENTIONS.md) for the full rules and framework references.
- Use typed $props, callback props, modern event attributes, and snippets in new code.
  Keep props read-only; use $bindable only for intentional two-way component APIs.
- Derive values from changing inputs with $derived or $derived.by. Avoid stale copies
  and effect-driven state mirroring. Key entity lists with stable domain IDs.
- Share scoped managers with typed createContext. Prefer attachments for new DOM
  integrations and Svelte window/document elements for global listeners; clean up resources.
- Preserve the client-rendered Tauri SPA and index.html fallback. Do not add server
  actions, server-dependent remote functions, or prerendering of native-dependent routes.
- Account for route-instance reuse and speculative hover loads. onMount is not a
  navigation hook; reset resources by identity and limit expensive link preloading.
- Handle asynchronous requests and event-handler errors explicitly; svelte:boundary
  does not catch them. Keep experimental async rendering disabled by default.
- Validate relevant nested-route reloads, back/forward navigation, and packaged routing.
  Browser success is not proof of packaged fallback behavior.

## Data loading conventions

- Follow [CONVENTIONS.md](CONVENTIONS.md) for data ownership, resource classes, and cleanup.
- Use universal +page.ts loaders for initial route data and +layout.ts only for shared
  route data. Keep loaders read-only and return data rather than mutating managers.
- Use scoped resource classes in .svelte.ts files for component-owned queries. Keep
  transport and runtime validation in typed services; do not fetch directly throughout UI code.
- Do not duplicate loader requests on mount. Refresh through the data's owner: targeted
  loader invalidation or an explicit resource method.
- Guard against stale responses and dispose requests, listeners, and timers. Separate
  initial loading, refresh, empty, unavailable, and failure states. Never hide errors as [].
- Use dedicated operation managers for streaming and mutations. Canceling a UI request
  does not cancel native work unless the engine confirms it.

## Svelte documentation and tools

Adapted from https://svelte.dev/docs/ai/instructions.
The project configures Svelte MCP in .codex/config.toml.
For v3-specific behavior, consult https://next.svelte.dev/docs/kit/migrating-to-sveltekit-3
and the v3 documentation; stable documentation may still describe SvelteKit 2.

1. Begin Svelte or SvelteKit work with list-sections. Match its use_cases to the task,
   then fetch every relevant section with get-documentation before implementing.
2. Run svelte-autofixer on Svelte code you create or edit before delivering it.
   Address the reported issues and suggestions, then repeat until it reports none.
3. Offer playground-link only for examples outside the repository. Generate a link
   only after the user requests or accepts it; never for code saved in this project.

If these tools are unavailable, state that limitation, consult the official Svelte
documentation directly, and run the available local checks. Never claim an MCP check ran
when it did not.

## Architecture and working rules

- Studio communicates with the engine only through its public CLI and serving daemon.
  Never import the Python engine or reimplement its runtime.
- Show unavailable functionality honestly. Never fabricate models, inference results,
  progress, or compatibility claims. Report failures and recovery steps explicitly.
- Read versions, paths, compatibility ranges, and capabilities from their configuration
  or source of truth rather than duplicating them in application code.
- Follow the documented issue, branch, tests, and pull request workflow when authorized.
  If external workflow steps are unavailable or unauthorized, report that limitation;
  do not claim they were completed. Maintainers merge and publish.
- Preserve existing user changes and keep each increment focused.

## Verification

- Run pnpm check and pnpm build for frontend changes.
- For Rust changes, run appropriate Cargo checks from src-tauri and validate native
  behavior where possible. pnpm tauri dev launches the desktop development app.
- Add behavioral tests when a behavior change warrants them; do not invent test results.
- Report the commands run and any checks not performed. Frontend build or browser checks
  do not verify native IPC, installers, GPU support, or other operating systems.
