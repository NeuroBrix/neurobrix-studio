# Contributing to NeuroBrix Studio

Thank you for working on Studio. This document is short on purpose: read it once,
and you will not be surprised by a review.

## Who decides

Hocine Benkelaya (`@benkelaya`) leads the project and owns its direction — the
roadmap, the rules in this document, and what Studio is ultimately for.

**He does not gate your work.** You both have maintainer access: open pull requests,
review each other, merge into `main`, and keep moving without waiting on anyone.
Nobody should sit blocked on an approval, and holding the project back for the sake
of a rubber stamp would help no one.

What happens instead is a regular pass over what has landed. If something drifts
from the roadmap, from the rules here, or from where the project is heading, it gets
raised and corrected then — in the open, with the reasoning. That is a conversation
about direction, not a verdict on your work, and it is a fair trade for not having
to queue behind a review.

Two things make that trade work. Review each other's pull requests rather than
self-merging when the change is substantial — a second pair of eyes is worth more
than a maintainer's signature ever was. And keep writing pull request descriptions
as carefully as you have been, because those descriptions are now the main way
anyone reconstructs why something is the way it is.

Please propose, disagree, and argue for a different direction whenever you see one —
several decisions here were changed because a contributor pushed back, and that is
how it should work. The earlier it comes the better: an objection in the issue costs
a conversation, the same objection after the code is written costs a rewrite.

## The engine is off-limits

Studio is a client of the [NeuroBrix engine](https://github.com/NeuroBrix/neurobrix).
The dependency runs one way only.

**No pull request in this repository ever requires a change in the engine, and no
change is made to the engine in order to unblock Studio.** If Studio needs something
the engine does not offer, open an issue in the engine repository describing the
need, and stop there. The maintainer decides whether and when it is built. Do not
open a pull request against the engine to unblock yourself.

The freedom you have here stops at that boundary, and deliberately so. The engine
repository keeps owner review on every branch, and that is not an oversight to be
worked around — it is the one place where a wrong change is expensive enough to be
worth the wait.

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
3. **Tests come with the code**, in the same pull request. If something genuinely
   cannot be tested, say so and say why — that is a fine answer. Silence is what
   makes a change hard to accept.
4. **One pull request, one issue.** A scaffold, a documentation rewrite and a
   design plan are reviewed differently and land at different moments, so they are
   much easier to move forward separately than bundled together.

Link the issue with `Refs #N`, and keep `Closes #N` for when every acceptance
criterion is actually met — otherwise the issue closes automatically and the
remaining work quietly disappears with it.

## What a pull request must carry

- The exact commands that were run, and their environment.
- **The checks that were not run**, stated as not run. This is the part we value
  most. Nobody here has all three platforms on their desk, and an honest gap is
  never held against you — it is an unstated one that costs everyone time later.
- What was observed on one platform, left as that platform's result. Browser checks
  are browser evidence; they are not native or GPU validation.

We take your word for what we cannot verify ourselves. That trust is the whole
reason the point above matters.

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
- **No paths in the code.** Locations are resolved at runtime from the platform API,
  never written literally and never assembled by hand from a home directory. Studio's
  data comes from Tauri's path API; anything belonging to the engine is asked of the
  engine. See *Where things live on disk* in [ROADMAP.md](ROADMAP.md). This one comes
  up in review every time, tests and scripts included.

## Licence and third-party content

This repository is Apache 2.0, and every file that declares a licence needs to say
the same thing, package metadata included. It is the one detail we cannot let
through, because the project's intellectual property sits behind it.

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

Keeping the technical conversation on GitHub — issues for subjects, reviews for
code, a mention when someone needs to see something — is worth the small effort.
Chat is fine for arranging a call; it is just that a decision nobody can find later
tends to get made twice.

Same reason for saying in the issue what you are picking up before you start it.
It has already happened here that two people built the same foundation in parallel
without knowing, and neither of them deserved that.
