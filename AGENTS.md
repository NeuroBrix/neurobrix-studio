# NeuroBrix Studio agent guide

Read README.md and ROADMAP.md before changing the app. The project is in development;
the current screen is a scaffold, not a working inference product.

## Project conventions

- Use Corepack-managed pnpm from the repository root, following the packageManager
  pin in package.json. Preserve pnpm-lock.yaml.
- The stack is Svelte 5, SvelteKit 2, TypeScript, Tauri 2, Tailwind CSS 4, and shadcn-svelte.
- Keep the frontend a static SPA: SSR is disabled in src/routes/+layout.ts,
  the adapter emits an index.html fallback, and Tauri loads ../build.
- Do not introduce SvelteKit server endpoints or server-only load functions.
- Use Svelte 5 runes for new reactive components. Reuse shadcn components under
  src/lib/components/ui and the cn helper in src/lib/utils.ts.
- Theme tokens and Tailwind imports belong in src/routes/layout.css; Tailwind runs
  through the Vite plugin. Add components with pnpm exec shadcn-svelte add <component>.
- Preserve the Tauri development URL and matching Vite port when editing build settings.

## Svelte documentation and tools

Adapted from https://svelte.dev/docs/ai/instructions.
The project configures Svelte MCP in .codex/config.toml.

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
