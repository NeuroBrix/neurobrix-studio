<p align="center">
  <img src="https://raw.githubusercontent.com/NeuroBrix/neurobrix/main/assets/logo_NeuroBrix.png" alt="NeuroBrix" width="200"/>
</p>

<h1 align="center">NeuroBrix Studio</h1>

<p align="center">
  <strong>The desktop application for the NeuroBrix inference engine.</strong><br/>
  Download one file. Install it. Run any model.
</p>

<p align="center">
  <a href="https://neurobrix.es">Website</a> &nbsp;|&nbsp;
  <a href="https://github.com/NeuroBrix/neurobrix">Engine</a> &nbsp;|&nbsp;
  <a href="https://neurobrix.es/models">Model Hub</a> &nbsp;|&nbsp;
  <a href="ROADMAP.md">Roadmap</a>
</p>

---

> **Status: in development. Not alpha, not released.**
> There are no tags, no downloads and no installers yet. The first version, `0.0.1`, is
> published only when the application is judged functional — not before. Nothing in this
> repository should be presented as available software.

## What this is

NeuroBrix Studio is a desktop application for **macOS, Windows and Linux**. It exists for one
kind of person: someone who is not a developer, who knows how to download an application and
install it, and who wants to run AI models without ever opening a terminal.

That goal sets the hardest requirement in this repository, and it is a packaging requirement
rather than a user-interface one: **one downloaded file must be enough.** The installer brings
its own Python runtime, installs the matching engine and its dependencies, and prepares the
machine — so that the user never installs anything by hand. See
[stage 7 of the roadmap](ROADMAP.md#stage-7--the-self-contained-installer).

## What this is not

Studio is **not** a second way to run models. It is a client of the engine.

- The engine is [`NeuroBrix/neurobrix`](https://github.com/NeuroBrix/neurobrix): a Python
  runtime, published on PyPI, that executes any model on any hardware.
- Studio talks to it **only** through the engine's public surface — the `neurobrix` command
  line and the serving daemon.
- Studio never imports the Python package, never reaches into the runtime, and never
  reimplements any part of it.

This is an architectural rule, not a preference. A second execution path into the runtime
would mean two behaviours to keep correct, two places for a defect to hide, and results that
differ depending on which door the user came through. A test enforces the rule.

## Relationship to the engine

The dependency runs **one way only**: Studio depends on the engine, the engine never depends
on Studio.

Each Studio release declares the range of engine versions it has been tested against — a
range and not a pinned version, so that an engine patch release does not require a Studio
release. Today's range is `>=0.5.3,<0.6`.

Studio and the engine version independently. Studio starting at `0.0.1` while the engine is
at `0.5.3` is normal and expected; the two numbers are not meant to converge.

## Platforms

Every release produces **three artefacts carrying the same version number**: macOS, Windows
and Linux. A platform whose artefact could not be produced or verified is stated as such
rather than quietly omitted.

## Stack

Tauri 2 (Rust) for the native shell, SvelteKit as a static single-page application,
TypeScript, Tailwind CSS. React and Electron were considered and are viable; this application
uses SvelteKit and Tauri.

## Working rules

These come from the engine project and apply here without change.

- **No fabricated results.** No placeholder models, no simulated inference, no invented
  progress. An action that cannot be performed is shown as unavailable, with the reason.
- **Failures are explicit.** A missing, failed or incompatible engine integration is reported
  to the user with what happened and what to do — never swallowed, never retried in silence.
- **What was not verified is written down.** A check that did not run is recorded as not run.
  A behaviour observed on one platform is not claimed for the other two.
- **No hardcoding.** Versions, paths, ranges and capabilities are read from configuration or
  from the engine itself, never written into the code.
- **Every increment is an issue, a branch, tests and a pull request**, in that order.

## Getting started

Install Node.js 22.12+ (or a newer supported LTS), Corepack, Rust, and the
[Tauri prerequisites for your operating system](https://tauri.app/start/prerequisites/).
Run these commands from the repository root:

```sh
corepack enable pnpm
pnpm install --frozen-lockfile
pnpm tauri dev
```

Corepack selects the pnpm version pinned in `package.json` (currently 12.4.1).
Use Corepack 0.34.7 or newer; choose a Corepack release compatible with your Node.js version.

`pnpm tauri dev` starts the Vite development server on port 1420 and opens the native
application. `pnpm dev` starts only the browser frontend; native commands require the
Tauri application.

```sh
pnpm check        # Svelte and TypeScript checks
pnpm build        # Static frontend output in build/
pnpm tauri build  # Build the native application and platform bundles
```

The frontend follows the [Tauri SvelteKit guide](https://tauri.app/start/frontend/sveltekit/):
`@sveltejs/adapter-static` emits an `index.html` fallback, the root layout disables SSR,
and Tauri loads `../build` relative to `src-tauri/tauri.conf.json`. This is a client-side
SPA; server routes and server-only load functions are not supported by this setup.

The current UI is the generated starter screen. Engine integration and the rest of the
foundation remain planned in the [roadmap](ROADMAP.md).

### UI components and styling

Tailwind CSS 4 runs through the Vite plugin. Global styles and shadcn theme tokens live in
`src/routes/layout.css`, imported by the root Svelte layout. The shadcn-svelte configuration
in `components.json` uses the Vega preset, neutral colors, Lucide icons, and bundled Inter
fonts. Dark theme tokens are available through the `dark` class on the root HTML element.

Add components from the repository root:

```sh
pnpm exec shadcn-svelte add input
```

The Button component is already available:

```svelte
<script lang="ts">
  import { Button } from "$lib/components/ui/button/index.js";
</script>

<Button>Continue</Button>
```

### Agent setup

Repository guidance lives in [AGENTS.md](AGENTS.md). The project-local
`.codex/config.toml` connects Codex to the official Svelte MCP server for documentation
and Svelte code analysis, following the [Svelte agent guide](https://svelte.dev/docs/ai/instructions).
It does not require an application dependency or API key.

Restart your Codex client after changing MCP configuration and open this repository as
a trusted project. Check `/mcp` for the `svelte` server. Project configuration is described
in the [Codex MCP documentation](https://developers.openai.com/codex/mcp).

## Licence

Apache 2.0, the same as the engine. See [LICENSE](LICENSE).

---

<p align="center">
  <sub>
    Developed by <a href="https://www.linkedin.com/in/hocine-benkelaya/">Hocine Benkelaya</a> &middot;
    Managed by <a href="https://wizworks.io/">WizWorks OÜ</a>,
    a property of <a href="https://neuralnetworkholding.com/">NEURAL NETWORK HOLDING LTD</a>
  </sub>
</p>
