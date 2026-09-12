# NeuroBrix Studio — Roadmap

**Status: in development. Not alpha. No tags, no releases, no downloads.**

The first version, `0.0.1`, is published when the application is judged functional by its
author — not on a date, and not because a stage number was reached. Until then this
repository holds work in progress and nothing here is announced as available.

---

## The goal that shapes every stage

Studio's user is not a developer. They download one file, install it, and use it.

That single sentence decides most of the engineering. It means the application cannot assume
Python, cannot assume the engine, cannot assume a package manager, and cannot ask the user to
open a terminal — not once, not for a first run, not to recover from an error. Stage 7 is
where that promise is actually kept, and every stage before it is built knowing stage 7 is
coming.

## The rule that does not change

Studio reaches the engine **only** through its public surface: the `neurobrix` command line
and the serving daemon. It never imports the Python package and never opens a second
execution path into the runtime. Every stage below respects this, and a test enforces it.

## Where things live on disk

No path is ever written into the code. Every location below is resolved at runtime,
on the machine it runs on, from the platform's own API. A literal path in a source
file is a defect, and so is a path assembled by hand from a home directory.

**The application** is installed where the operating system installs applications:
`/Applications` on macOS, `Program Files` on Windows, the distribution's own location
on Linux. The bundler decides this; Studio does not.

**Studio's own data** — conversation history, settings, logs, caches — lives in the
per-user location each platform defines for application data, under the application
identifier `es.neurobrix.studio`. Studio obtains these paths from Tauri's path API
(`appDataDir`, `appConfigDir`, `appLogDir`, `appCacheDir`) and never constructs them.
In practice this resolves to `~/Library/Application Support/…` on macOS, `%APPDATA%\…`
on Windows and `$XDG_DATA_HOME/…` on Linux, but Studio does not know or care which.

**Models, engine artefacts and engine caches belong to the engine.** They live where
the engine puts them, under the engine's own root, and Studio never chooses a location
for them, never writes into that tree directly, and never keeps a second copy. When
Studio needs to know where something is, it asks the engine. Two components deciding
where models live means models in two places and a user who cannot find either.

**The embedded runtime** shipped by the installer at stage 7 is installed inside the
application's own data location, isolated from any Python already on the machine.

---

## Stage 0 — Foundation

The native shell and the interface skeleton, with no engine integration at all.

Tauri 2 with a SvelteKit static single-page application. Workspace, Models, History and
Settings navigation that works by keyboard and identifies the active page. One real native
command that reports application version, operating system and architecture — and that
reports nothing about GPU compatibility, because at this stage nothing knows. Dependency
locks, formatting, type checks, behavioural tests and path-scoped CI. Developer documentation
covering commands, architecture, tests and remaining work.

Inference and model management stay visibly unavailable. No fake models, no generated
results.

*Out of scope: real inference, engine installation, model downloads, persistence, signed
installers, automatic updates.*

## Stage 1 — Finding the engine

Studio detects whether an engine is installed, reads its version, and decides whether that
version is inside the range this Studio build was tested against.

The three outcomes are all first-class: engine present and compatible, engine present but
outside the tested range, engine absent. Each is shown to the user in plain language with
what it means and what happens next. None of them is a crash, and none is a silent
degradation.

## Stage 2 — The engine contract

A structured, versioned contract between Studio and the engine's command line and daemon,
plus the runtime lifecycle: starting the daemon, knowing it is healthy, stopping it cleanly,
and surviving it dying.

The contract is what makes the rest testable. It is written down, it declares what Studio
sends and what it expects back, and the tests cover malformed responses and transport
failures as well as the happy path.

## Stage 3 — Running a model

Inference with streaming output and working cancellation.

Cancellation matters more than it sounds: a user who starts a video generation by mistake
must be able to stop it, and the daemon must be left in a state that accepts the next
request.

## Stage 4 — Managing models

Inventory of what is installed, import from the model hub with real progress, and removal
that actually frees the disk.

Progress comes from the engine, never from a timer. Disk space is checked before a download
begins, not discovered when it fails.

## Stage 5 — Chat and history

A conversation workspace with local history in SQLite. History is the user's data: it stays
on their machine, it can be exported, and it can be deleted.

## Stage 6 — Media workspaces

Separate spaces for image, audio and video, because their inputs, their waiting times and
their outputs have nothing in common. A video generation that takes twenty minutes needs a
different interface from a chat reply.

## Stage 7 — The self-contained installer

**This is the stage that makes Studio what it is meant to be, and it is a project in itself.**

One downloaded file installs everything: the application, an embedded Python runtime, the
matching engine version and its dependencies. The user installs nothing by hand and never
opens a terminal.

What this stage must solve, written honestly because none of it is small:

- **Embedded Python runtime**, shipped inside the installer, isolated from anything already
  on the machine — a user's existing Python must not be modified or shadowed.
- **Engine and dependencies installed offline**, from artefacts carried in the package, so
  the installation works on a slow or absent connection and produces the same result on every
  machine.
- **Installer size**, which will be large, and what is downloaded on demand instead.
- **GPU support detection**, not GPU driver installation. Studio detects what the machine can
  do and tells the user plainly; installing vendor drivers from an application installer
  requires administrator rights and often a reboot, and doing it silently would be
  irresponsible. Where a driver is missing, the user is told which one and pointed to the
  vendor.
- **First run**, which does the heavy work once, shows real progress, and can be resumed after
  an interruption rather than starting over.
- **Uninstall**, which removes what was installed and says what it is leaving behind — models
  and history are the user's data and are not deleted without asking.
- **Updating the engine underneath a Studio that keeps working**, and refusing an engine
  version outside the tested range rather than running it and hoping.

## Stage 8 — Distribution

Code signing and Apple notarization, Windows signing, Linux packaging, and an update path.
Three artefacts per release with the same version number.

## Stage 9 — `0.0.1`

The first version, published when the author judges the application functional.

Its release notes state the engine range it was tested against, which platforms were verified
and how, and which checks did not run.

---

## How work happens here

Each increment is **an issue, then a branch, then tests, then a pull request** — in that
order, and none of them skipped. Commits stay focused. Pull requests complete the template,
link their issue and carry the evidence of verification: the exact commands, the environment,
and the checks that were *not* run.

Maintainers review, merge and publish. There is no automatic merge and no automatic release.

Browser tests are browser tests. They are not native validation and they are not GPU
validation, and a report must not let them be read as either.
