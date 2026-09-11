# NeuroBrix Studio agent guide

Before changing the app, read [README.md](README.md) for current status,
[ROADMAP.md](ROADMAP.md) for scope, and [CONVENTIONS.md](CONVENTIONS.md) for coding rules.
The app is in development; do not describe the scaffold or planned features as complete.

## Project essentials

- Use Corepack-managed pnpm from the repository root. Follow `packageManager` in
  `package.json` and preserve `pnpm-lock.yaml`.
- Keep the Tauri static SPA architecture: SvelteKit configuration in `vite.config.js`,
  SSR disabled, an `index.html` fallback, and `#lib` imports with explicit extensions.
- Follow CONVENTIONS.md for Svelte 5 syntax, data ownership, component state, and cleanup.
  Initial route data belongs in universal loaders; component-owned queries use scoped
  resource classes in `.svelte.ts` files and typed services.
- Studio reaches the engine only through its public CLI and serving daemon. Never import
  engine internals, duplicate runtime logic, or fabricate models, results, or progress.
- Read versions, paths, and capabilities from their source of truth. Check release and
  peer requirements before dependency upgrades; do not force incompatible tooling.
- Preserve user changes and keep work focused. Follow the issue, branch, tests, and pull
  request workflow when authorized. Report unavailable external steps honestly;
  maintainers merge and publish.

## Skills and documentation tools

Use the project-local [shadcn-svelte skill](.agents/skills/shadcn-svelte/SKILL.md) for
component, styling, and CLI work. Read its relevant references as needed. Adapt its generic
examples to this project's `#lib` aliases and explicit extensions. Use the project-pinned
`pnpm exec shadcn-svelte` by default; use a newer CLI only for an intentional upgrade.
Project conventions take precedence over generic skill examples. Do not copy globally
available or unrelated skills into the repository as part of routine work.

The Svelte MCP configuration is in `.codex/config.toml`. For Svelte work:

1. Use `list-sections`, then `get-documentation` for the relevant sections.
2. Run `svelte-autofixer` on Svelte code you create or edit; address its findings and
   repeat until no issues or suggestions remain.
3. Generate a `playground-link` only when requested or accepted by the user, and only
   for examples that were not written to project files.

If these tools are unavailable, state that limitation, use official documentation, and
run the available local checks. For this project's SvelteKit 3 prerelease, consult the
[v3 docs](https://next.svelte.dev/docs/kit/migrating-to-sveltekit-3); stable docs may
still describe version 2. Never claim an unavailable tool ran.

## Verification and handoff

- Run `pnpm check` and `pnpm build` for frontend changes. Add meaningful behavioral
  tests where warranted; use the scenarios in CONVENTIONS.md.
- For Rust changes, run appropriate Cargo checks from `src-tauri` and validate native
  behavior where possible. `pnpm tauri dev` launches the desktop development app.
- For documentation-only edits, verify links, commands, and consistency with the project.
- Report changes, commands run, and checks not performed. Browser or frontend checks
  do not verify native IPC, packaged routing, installers, GPU support, or other platforms.
