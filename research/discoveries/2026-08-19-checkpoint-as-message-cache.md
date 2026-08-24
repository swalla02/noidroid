---
id: 2026-08-19-checkpoint-as-message-cache
title: Bluesky's checkpoint is a message cache — a deterministic prefix, shipped in beamline control since 2015
discovered: 2026-08-19
updated: 2026-08-19
categories: [checkpointing, laboratory automation, state reconstruction, deterministic replay]
class: INFRASTRUCTURE
recommendation: WATCH
transferability: MEDIUM
novelty: REFINEMENT
confidence: HIGH
touches: [engine, checkpoint, proto]
---

## Discovery

Bluesky is the experiment-control framework running the NSLS-II synchrotron and a good
part of the DOE light-source estate. Its `RunEngine` consumes plans as generators of
`Msg(command, obj, *args)` — a mediated boundary with exactly the shape of our
`call`/`result` protocol. Its **checkpoint is not a snapshot of anything**: it is a
`deque` of the messages processed since the last checkpoint, and rewinding means
splicing those messages back onto the plan stack and executing them again. Constraint
C2 — "a checkpoint is a deterministic prefix, not a memory snapshot" — is, in the
autonomous-lab domain, not a design position but the incumbent implementation.

## Source

Primary, read in full:
- <https://github.com/bluesky/bluesky/blob/main/src/bluesky/run_engine.py>
  - `_UNCACHEABLE_COMMANDS` (line 370)
  - the message loop's caching branch (lines 1660–1668)
  - `_rewind()` (lines 1035–1055)
  - `_checkpoint`, `_reset_checkpoint_state`, `_clear_checkpoint`, `_rewindable`
    (lines 2438–2498)
- <https://github.com/bluesky/bluesky/blob/main/src/bluesky/preprocessors.py>
  — `rewindable_wrapper` (line 718)
- Docs: <https://blueskyproject.io/bluesky/main/state-machine.html>,
  <https://blueskyproject.io/bluesky/main/msg.html>

## What is interesting

Four mechanisms, in descending order of how much they should make us think.

**1. The checkpoint is the instruction log, not the state.** `_rewind()` is six lines:

> `new_plan = ensure_generator(list(self._msg_cache))` … "Returns … A new plan made from
> the messages in the message cache."

Resume after a pause or a beam-loss suspension means *re-running the instructions*, on
real hardware, against a world nobody snapshotted. The devices rebuild their own state
by being driven, precisely as our program rebuilds its own state by being re-executed.

**2. Non-replayability is declared per verb, statically, at the framework level.**
`_UNCACHEABLE_COMMANDS = ["pause", "subscribe", "unsubscribe", "stage", "unstage",
"monitor", "unmonitor", "open_run", "close_run", "install_suspender", …]`. A message on
that list is executed but never cached, so a rewind never repeats it. Note what is
*absent* from the list: `set` (move a motor), `trigger`, `read`. Bluesky's implicit
`EffectKind` model says a motor move is a `write` — re-driving puts it back — while
staging a device or opening a run is not repeatable. Same two-way split as ours, same
answer on the interesting case, arrived at independently.

**3. `clear_checkpoint` is a prospective refusal.** `async def _clear_checkpoint`
sets `self._msg_cache = None`, and the docs say: "Incorporating `clear_checkpoint()` in
a plan makes it un-resuming. If a pause or suspension are requested, the plan will
abort instead." A plan author who is about to do something that must not be repeated
declares it, and the framework's response is to remove the ability to rewind rather
than to rewind badly. That is `Reach::Unreachable`, declared forward by the author
instead of computed backward from the chain.

**4. `rewindable_wrapper(plan, rewindable=False)` scopes it.** A region of a plan can
be marked non-rewindable and the flag is restored afterwards via `finalize_wrapper`.
Bluesky's own suspender machinery uses it: it yields `Msg('rewindable', None, False)`
around the recovery sequence so that the recovery itself is never replayed.

## Why it matters to Paranoid Android

Three things, and only one of them is a build.

**It is independent confirmation of C2 from a domain that had to get it right.** Not a
paper, not a prototype: a decade of beamtime where a wrong rewind wastes a proposal's
allocation. When someone next argues for a memory-snapshot checkpoint, this is the
citation. It is also the answer to "would the deterministic-prefix model survive
contact with an autonomous lab" — it already has, under a different name.

**Our reach computation is strictly better and we should know why.** Bluesky's
irreversibility marking is *static per command verb* plus *manual per plan*. Ours is
per-call, declared by the caller, and `checkpoint::at` derives `Reach` by walking the
chain and looking at each effect's `EffectKind` and the grip at that step. That means we
can answer "is step 7 reachable?" for a recording made by someone who never thought
about the question, and bluesky cannot. Their model requires the plan author to have
remembered `clear_checkpoint`.

**The adapter seam, if a lab user ever appears, is already there and is small.**
`RunEngine.msg_hook` is called with every `Msg` before dispatch, and `preprocessors`
supply a documented plan-mutation mechanism. A bluesky adapter for our protocol is a
`msg_hook` plus a translation of `Msg.command` to a target and an `EffectKind` — the
mediation point exists and is stable API. That is worth recording precisely so that it
is *not* built today: it is C10 adoption work, and there is no user.

Subsystems: `checkpoint.rs` (`Reach`), `engine.rs` (mode handling), `proto.rs` (the
shape of the boundary).

## Transferability

MEDIUM. The confirmation transfers completely; the mechanisms mostly do not, because
their design solves a smaller problem.

Their checkpoint is **sliding and singular**: `_reset_checkpoint_state()` clears
`_msg_cache` at every checkpoint, so only the most recent one exists and you cannot
return to the one before it. Rewind exists to resume after an interruption, not to
explore. There is no branch, no immutability, no addressing, and no comparison of the
values obtained the second time with the values obtained the first. Everything that
makes a trajectory a trajectory is missing, on purpose.

The one mechanism worth a second look is `rewindable_wrapper` — a *scoped* declaration
that a region is not returnable. We express the same thing per-effect. Theirs is more
ergonomic for a protocol author writing a long procedure; ours is more honest for a
reader of a recording made by someone else. I do not think we should adopt it.

## Novelty

REFINEMENT, and mostly of our confidence rather than of our code. `checkpoint::at`
already computes reach from effect kinds and grip, which subsumes `_UNCACHEABLE_COMMANDS`
and `clear_checkpoint`. What is genuinely absent from our side is nothing; what is
absent from theirs is verification, immutability and branching.

## Limitations and negative signal

The negative signal is the interesting part. **Bluesky rewinds against live instruments
and never compares.** On resume, cached `read` messages are re-executed, the detector
is read again, and the new value is emitted as a new event document. Nobody checks
whether it matches the first reading, and nothing in the run record marks which readings
were obtained the second time around. In our vocabulary the entire post-rewind segment
is `delivery: executed` presented as if it were the original — there is no delivery axis
at all. This is the lab-domain instance of
`2026-08-19-unverified-world-redrive`, and it is the strongest available argument that
the axis we already have is not obvious.

Second, `IllegalMessageSequence("Cannot 'checkpoint' after 'create' and before 'save'")`
— a checkpoint inside a data point is a runtime error. They discovered that a checkpoint
in the middle of a composite operation is meaningless. We get this free: a step *is* the
unit, so there is no "inside a step".

## Recommendation

WATCH — nothing to build, but it settles C2 with a decade of production evidence, and it
names the integration seam for the day a lab user exists.

## Proposed action

None now. Cite this card in place of re-arguing C2. Re-open on the trigger below and,
if it fires, the first spike is a `msg_hook` adapter translating `Msg` to `call` with an
`EffectKind` table seeded from `_UNCACHEABLE_COMMANDS` — half a day, not a subsystem.

## Confidence

HIGH. `run_engine.py` and `preprocessors.py` were downloaded and read at the named line
numbers; the doc statements were fetched from the current docs, not from search
snippets.

## Evidence

- Primary: <https://github.com/bluesky/bluesky/blob/main/src/bluesky/run_engine.py> —
  `_rewind` builds a plan from the message cache; `_UNCACHEABLE_COMMANDS` is the
  reversibility table; `_clear_checkpoint` sets the cache to `None`.
- Primary: <https://blueskyproject.io/bluesky/main/state-machine.html> — "the RunEngine
  will 'rewind' through the plan to the most recent checkpoint, the last safe place to
  restart"; "Incorporating `clear_checkpoint()` in a plan makes it un-resuming."
- Supporting: <https://github.com/bluesky/bluesky/blob/main/src/bluesky/preprocessors.py>
  — `rewindable_wrapper` scopes non-rewindability over a region.
- Counter-evidence: none found. The closest is that their checkpoint is singular and
  sliding, which limits how much of their design can be read as agreeing with ours.

## Changelog

- 2026-08-19 — created.
