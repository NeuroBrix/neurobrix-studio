<p align="center">
  <img src="https://raw.githubusercontent.com/NeuroBrix/neurobrix/main/assets/logo_NeuroBrix.png" alt="NeuroBrix" width="200"/>
</p>

<h1 align="center">NeuroBrix Studio</h1>

<p align="center">
  <strong>The desktop application for the NeuroBrix inference engine.</strong><br/>
  A desktop interface for running AI models locally.
</p>

<p align="center">
  <a href="https://neurobrix.es">Website</a> &nbsp;|&nbsp;
  <a href="https://github.com/NeuroBrix/neurobrix">Engine</a> &nbsp;|&nbsp;
  <a href="https://neurobrix.es/models">Model Hub</a> &nbsp;|&nbsp;
  <a href="ROADMAP.md">Roadmap</a>
</p>

---

> **Early development — no public release or installer yet.**
> The repository contains a starter interface and a Tauri shell. Engine integration,
> model management, and inference are planned in the [roadmap](ROADMAP.md).

## About

NeuroBrix Studio aims to make the [NeuroBrix inference engine](https://github.com/NeuroBrix/neurobrix)
accessible through a desktop app for macOS, Windows, and Linux.

The goal is a single installer that includes the Python runtime, engine, and dependencies,
so users can run supported models without configuring a development environment.
That installer is planned work; see [stage 7 of the roadmap](ROADMAP.md#stage-7--the-self-contained-installer).

## Architecture

Studio provides the desktop interface. The engine handles model execution. Integration
will use the engine's public `neurobrix` CLI and serving daemon; Studio must not import
engine internals or duplicate its runtime logic.

Studio and the engine have independent release versions. Each Studio release will document
its tested engine compatibility range and which platform builds were verified.

The application uses:

- **Tauri 2 and Rust** for the native shell.
- **Svelte 5 and SvelteKit 3** for the interface, configured as a static single-page app.
- **TypeScript 6 and Vite 8** for development and builds.
- **Tailwind CSS 4 and shadcn-svelte** for styling and UI components.
- **Corepack and pnpm** for package-manager selection and dependency installation.

SvelteKit 3 and its static adapter currently use pinned prereleases. Exact dependency
versions and the pnpm pin are declared in [package.json](package.json).

## Development

### Current setup status

The SvelteKit 3 dependencies and lockfile are installed and verified with `pnpm check`,
`pnpm build`, and `pnpm install --frozen-lockfile`. Native launch, packaged routing,
and installer builds have not been verified.

Lucide is pinned to 1.44.0 because 1.45.0 was inside pnpm's minimum release-age window
when installed. Supply-chain policies remain enabled with no exceptions.

### Prerequisites

- Node.js satisfying the `engines.node` requirement in [package.json](package.json).
- Corepack 0.34.7 or newer, using a release compatible with your Node.js version.
- Rust and the [Tauri prerequisites for your operating system](https://tauri.app/start/prerequisites/).

The commands below run from the repository root. To set up dependencies, enable
Corepack's pnpm shim and install:

```sh
corepack enable pnpm
pnpm install --frozen-lockfile
```

Corepack selects the pnpm version from `package.json`. The frozen lockfile keeps
dependency installs repeatable.

### Commands

| Command | Purpose |
| --- | --- |
| `pnpm tauri dev` | Start Vite and open the desktop application. |
| `pnpm dev` | Start the browser frontend at `http://localhost:1420`. |
| `pnpm check` | Run Svelte and TypeScript checks. |
| `pnpm lint` | Check Biome lint rules, formatting, and import organization. |
| `pnpm lint:fix` | Apply Biome formatting, import organization, and safe lint fixes. |
| `pnpm format` | Format supported project files with Biome. |
| `pnpm format:check` | Check formatting without changing files. |
| `pnpm build` | Generate the static frontend in `build/`. |
| `pnpm tauri build` | Build the native app and platform bundles. |

Native commands, including the starter screen's greeting, require the Tauri application.
Opening the frontend in a browser does not provide native IPC.

### Frontend configuration

SvelteKit configuration lives in the `sveltekit()` plugin in [vite.config.js](vite.config.js).
The static adapter generates an `index.html` fallback, the root layout disables SSR, and
Tauri loads `../build` relative to its configuration file. This setup has no SvelteKit
server runtime, so server endpoints and server-only load functions cannot be used.

Library imports use `#lib/*`, declared in `package.json`. TypeScript extends `$app/tsconfig`.
These conventions follow the [SvelteKit 3 migration guide](https://next.svelte.dev/docs/kit/migrating-to-sveltekit-3).
TypeScript is kept on version 6 to satisfy the selected SvelteKit and svelte-check peer requirements.

### Components and styling

Global styles and theme tokens live in [src/routes/layout.css](src/routes/layout.css).
The shadcn-svelte setup uses the Vega preset, neutral colors, Lucide icons, and bundled
Inter fonts. Add `dark` to the root HTML element to select the dark theme tokens.

The Button component is available under `src/lib/components/ui/button`. After dependency
setup is complete, add further components with the shadcn-svelte CLI:

```sh
pnpm exec shadcn-svelte add input
```

Component aliases are configured in [components.json](components.json). Use explicit file
extensions when importing through `#lib`, for example `#lib/components/ui/button/index.js`.
Component generation with the new SvelteKit 3 aliases still needs verification.

### Development conventions

Biome is pinned in `package.json` and configured in [biome.json](biome.json). It handles
JavaScript, TypeScript, JSON, CSS (including Tailwind directives), HTML, and Svelte.
Full Svelte/HTML support is experimental in Biome, so keep running `pnpm check` and
`pnpm build` alongside `pnpm lint`. Rust uses Cargo tooling; Markdown, YAML, static
assets, generated output, and vendored agent skills are outside this Biome setup.
For Zed integration, use the [Biome extension](https://biomejs.dev/reference/zed/)
with the project configuration. Editor preferences remain local.

[CONVENTIONS.md](CONVENTIONS.md) defines the shared coding conventions for developers and
agents. Use page loaders for initial route data and scoped Svelte 5 resource classes for
component-owned requests. The guide covers Svelte 5 component syntax and reactive
ownership, SPA navigation and preloading, typed services, error handling, and request
cleanup. It includes a resource-class example and separates project defaults from
framework requirements.

### Coding agents

[AGENTS.md](AGENTS.md) is the entry point for agents. Detailed coding rules live in
[CONVENTIONS.md](CONVENTIONS.md), shared with human contributors.

The optional [shadcn-svelte skill](.agents/skills/shadcn-svelte/SKILL.md) provides component
and CLI workflows. Its source metadata is recorded in `skills-lock.json`.
No additional skills are required to build or run the app.

The repository includes a Codex-specific Svelte MCP configuration in `.codex/config.toml`.
Zed and other editors need their own MCP setup; this file does not configure them.
See the [Svelte agent guide](https://svelte.dev/docs/ai/instructions) for the tool workflow.

## Contributing

Follow the [roadmap](ROADMAP.md) and keep changes focused. The project workflow is an
issue, a branch, relevant tests, and a pull request. Maintainers review, merge, and publish.

- Show unavailable features and failures clearly. Do not simulate inference, models, or progress.
- Read versions, paths, and capabilities from their source of truth instead of duplicating them.
- Record what was tested and what remains unverified. Browser checks do not establish
  native behavior, GPU compatibility, or support for another operating system.

## Licence

The repository's [LICENSE](LICENSE) contains the Apache License 2.0.
The scaffold's `package.json` still declares MIT; that metadata needs to be reconciled
before distribution.

---

<p align="center">
  <sub>
    Developed by <a href="https://www.linkedin.com/in/hocine-benkelaya/">Hocine Benkelaya</a> &middot;
    Managed by <a href="https://wizworks.io/">WizWorks OÜ</a>,
    a property of <a href="https://neuralnetworkholding.com/">NEURAL NETWORK HOLDING LTD</a>
  </sub>
</p>
