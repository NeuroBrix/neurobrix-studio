## What this changes

<!-- One paragraph. What the reader gets that they did not have before. -->

Closes #

## Verification

<!-- Exact commands, and the environment they ran in. -->

```
```

**Checks that did NOT run, and why:**

<!-- Required. "None" is an acceptable answer only if it is true. -->

**Platforms verified:** <!-- macOS / Windows / Linux — say which, and how. -->

## Rules

- [ ] Studio reaches the engine only through the CLI and the serving daemon — no import of the Python package, no second execution path.
- [ ] No fabricated results: no placeholder models, no simulated inference, no invented progress.
- [ ] Failures are explicit: nothing swallowed, nothing retried in silence.
- [ ] No hardcoded versions, paths or capabilities — read from configuration or from the engine.
- [ ] Browser tests are not presented as native or GPU validation.
