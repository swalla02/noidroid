---
name: Bluesky (NSLS-II experiment control)
class: INFRASTRUCTURE
first_seen: 2026-08-19
updated: 2026-08-19
url: https://github.com/bluesky/bluesky
licence: BSD-3-Clause
activity: active
---

## What it is

An experiment-control framework for scientific instruments, developed at NSLS-II
(Brookhaven) and used across DOE light sources and a widening set of laboratory and
autonomous-experiment installations. A measurement procedure is a Python generator
yielding `Msg(command, obj, *args, **kwargs)` instructions; a `RunEngine` consumes them,
dispatches each to a coroutine, and emits a stream of documents (`start`, `descriptor`,
`event`, `stop`) describing what happened.

## How it works

- **The boundary is the message.** Every interaction with the physical world — `read`,
  `set`, `trigger`, `stage`, `open_run` — is a `Msg` handed to the engine, which owns
  dispatch. The plan never touches a device directly. This is the same mediation shape
  as our `call`/`result` protocol, in a domain that arrived at it for safety reasons.
- **State lives in the devices**, described by the `ophyd` abstraction. The engine keeps
  almost none of it.
- **The checkpoint is a `deque` of messages.** `RunEngine._msg_cache` accumulates every
  cacheable message since the last `checkpoint`; `_rewind()` turns that deque back into
  a plan and splices it onto the plan stack. Resuming after a pause or a beam-loss
  suspension re-executes those instructions against the real instruments.
- **Non-replayability is a static verb list**, `_UNCACHEABLE_COMMANDS`, plus a plan-level
  `clear_checkpoint` that makes the plan abort rather than pause, plus a scoped
  `rewindable_wrapper` context.
- **Guarantee offered:** that a plan interrupted at an arbitrary point can be restarted
  from a place its author declared safe. Nothing more; in particular, nothing about
  whether restarting produced the same physics.

## What it does that we should learn from

That the deterministic-prefix checkpoint is not an idea, it is the incumbent. When a
domain has expensive, partly irreversible physical actions and cannot snapshot its
world, it converges on "keep the instruction log, re-run it" without anybody arguing
about memory images. Our C2 has a decade of beamtime behind it that we were not citing.

Second, `clear_checkpoint`. Declaring forward that a region must not be re-entered is
ergonomically better than our per-effect declaration for a long procedure written by a
domain expert. We should not adopt it — our retrospective computation works on
recordings whose authors never considered the question — but it is a good idea and
knowing why we decline it is worth something.

Third, the seam: `RunEngine.msg_hook` is called with every message before dispatch. If
an autonomous-lab user ever appears, an adapter is a hook and a verb-to-`EffectKind`
table.

## Where it is weaker, and why that is interesting

**It has no evidence standard at all.** A rewind re-executes `read` messages against
live detectors and emits the new values as new events. Nothing compares them to the
values obtained before the interruption; nothing in the document stream marks which
readings came from a repeat. A user reading a completed run cannot tell where a rewind
occurred. That is the exact gap in `2026-08-19-unverified-world-redrive`, and it is
instructive that a mature, careful, safety-conscious system does not consider the
comparison part of the job — because for their purpose (finish the scan) it is not.

**The checkpoint is singular and sliding.** `_reset_checkpoint_state()` empties the
cache at each new checkpoint, so only the most recent one exists. You cannot return to
the checkpoint before last, and there is no addressing, no immutability, no history.
Rewinding is recovery, never exploration: there is no branch, no intervention, and no
way to ask what a different choice would have produced.

## Overlap with us

They make the "return to an earlier point by re-execution" claim and back it — for
resumption. They make no reconstruction-fidelity claim and therefore do not need to back
one. There is no competition here: the overlap is one primitive, implemented for a
narrower purpose, and the parts we would call the product (immutable addressed history,
verified reconstruction, branching, divergence localisation) are absent by design.

**Evidence standard: asserts, does not verify.** No comparison is performed at any point
in the rewind path.

## Watch triggers

- Any addition to `RunEngine` of a comparison between pre- and post-rewind readings, or
  any marking of repeated events in the document stream — that would mean the domain has
  started to care about the thing we are selling.
- A bluesky-adjacent project offering counterfactual re-execution of a completed run
  ("what if we had scanned the other range").
- Adoption of bluesky by a self-driving-lab platform we hear about from a real user —
  the C9 condition for building anything here.

## Changelog

- 2026-08-19 — created. Read `run_engine.py` and `preprocessors.py` in full at the
  relevant paths; see `research/discoveries/2026-08-19-checkpoint-as-message-cache.md`.
