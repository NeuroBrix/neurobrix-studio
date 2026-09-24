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
release. Today's range is `>=0.5.4,<0.6`: `0.5.4` is the first engine that can describe
itself to a program, so it is the oldest one Studio can talk to at all.

Studio and the engine version independently. Studio starting at `0.0.1` while the engine is
at `0.5.4` is normal and expected; the two numbers are not meant to converge.

## Platforms

Every release produces **three artefacts carrying the same version number**: macOS, Windows
and Linux. A platform whose artefact could not be produced or verified is stated as such
rather than quietly omitted.

## Stack

Tauri 2 (Rust) for the native shell, SvelteKit as a static single-page application,
TypeScript, and Tailwind CSS. React and Electron were considered and are viable; this
application uses SvelteKit and Tauri.

The application currently uses:

- **Tauri 2 and Rust** for the native shell.
- **Svelte 5 and SvelteKit 3** for the interface, configured as a static single-page app.
- **TypeScript 6 and Vite 8** for development and builds.
- **Tailwind CSS 4 and shadcn-svelte** for styling and UI components.
- **Corepack and pnpm** for package-manager selection and dependency installation.

SvelteKit 3 and its static adapter currently use pinned prereleases. Exact dependency
versions and the pnpm pin are declared in [package.json](package.json).

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

## Development

### Current setup status

The SvelteKit 3 dependencies and lockfile are installed and verified with `pnpm check`,
`pnpm build`, and `pnpm install --frozen-lockfile`. The test setup has also passed
five Vitest tests on macOS arm64. The desktop smoke test covers navigation and reloads;
Windows/Linux execution and installers remain unverified.

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
