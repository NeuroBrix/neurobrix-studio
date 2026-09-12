# Contributing to NeuroBrix Studio

Thank you for working on Studio. This document is short on purpose: read it once,
and you will not be surprised by a review.

## Who decides

Hocine Benkelaya (`@benkelaya`) is the project lead and the only maintainer.
He reviews and merges. No one else merges, on any branch, on either repository.

Contributors are expected to propose, to disagree, and to argue a different
direction when they see one. Say it in the issue before writing the code, not in
the pull request after. A well-argued objection is welcome at any time.

## The engine is off-limits

Studio is a client of the [NeuroBrix engine](https://github.com/NeuroBrix/neurobrix).
The dependency runs one way only.

**No pull request in this repository ever requires a change in the engine, and no
change is made to the engine in order to unblock Studio.** If Studio needs something
the engine does not offer, open an issue in the engine repository describing the
need, and stop there. The maintainer decides whether and when it is built. Do not
open a pull request against the engine to unblock yourself.

This is not about trust. The engine is governed by rules that took a long time to
establish, and it has its own schedule.

## The workflow

An issue, then a branch, then tests, then a pull request. In that order, none of
them skipped.

1. **Issue first.** Describe the increment, its acceptance criteria and what is out
   of scope. Wait for it to be accepted before writing code. An issue that has not
   been discussed is a proposal, not an assignment.
2. **One branch per issue**, named `feat/<issue-number>-<short-slug>` or
   `fix/<issue-number>-<short-slug>`.
3. **Tests come with the code**, in the same pull request. A pull request with no
   test is not reviewed, whatever it contains.
4. **One pull request, one issue.** Do not bundle a scaffold, a documentation
   rewrite and a design plan into a single change. They are reviewed differently and
   they are merged at different moments.

Link the issue with `Refs #N`. Use `Closes #N` **only** when every acceptance
criterion of that issue is met by the pull request. Closing an issue that is
half-done loses the remaining work.

## What a pull request must carry

- The exact commands that were run, and their environment.
- **The checks that were not run**, stated as not run. This matters more than the
  ones that passed.
- What was observed on one platform is never claimed for the other two. Browser
  checks are browser checks: they are not native validation and not GPU validation.

## Working rules

- **No fabricated results.** No placeholder models, no simulated inference, no
  invented progress. An action that cannot be performed is shown as unavailable,
  with the reason.
- **Failures are explicit.** A missing, failed or incompatible integration is
  reported with what happened and what to do. Never swallowed, never silently
  retried.
- **No hardcoding.** Versions, paths, ranges and capabilities are read from
  configuration or from the engine itself, never written into the code.
- **Nothing deferred.** A problem found is a problem fixed, not a TODO.

## Licence and third-party content

This repository is Apache 2.0. Every file that declares a licence declares Apache
2.0, including package metadata. A pull request that declares another licence
anywhere is not merged.

Do not vendor third-party content — documentation, component libraries, agent
skills, generated assets — without saying where it comes from, under which licence,
and adding the required attribution. When in doubt, depend on it instead of copying
it, or leave it out.

Personal tooling stays personal. Editor settings, agent configurations and assistant
prompts belong in your own environment or in your global ignore file, not in this
repository.

## Documentation

The `README.md` and `ROADMAP.md` on `main` carry the project's direction and are
owned by the maintainer. Add to them freely — developer setup, commands,
configuration, anything a newcomer needs. Do not rewrite, soften or remove what is
already there in the same change; if you disagree with a rule, open an issue and say
why.

## Talking to each other

Everything technical happens on GitHub, in writing: issues for subjects, pull
request reviews for code, mentions (`@benkelaya`, `@blackalam`, `@street2geek`) when
someone needs to see something. Decisions taken elsewhere do not exist.

If two contributors are working on the same area, say so in the issue before
starting. Two people solving the same problem twice is the one waste this project
cannot afford.
