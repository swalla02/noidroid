---
id: 2026-08-21-rollout-graph-already-exists
title: The RL stack already stores rollouts as a parent-linked graph — and deliberately drops the world
discovered: 2026-08-21
updated: 2026-08-21
categories: [trajectory systems, content-addressed storage, event sourcing, agent evaluation, negative-signal]
class: INFRASTRUCTURE
recommendation: WATCH
transferability: MEDIUM
novelty: PRESENT
confidence: HIGH
touches: [model, store, bundle, proto]
---

## Discovery

Prime Intellect's `verifiers` v1 — the environment/rollout layer under `prime-rl` and the
Environments Hub — stores a rollout as a **graph of `MessageNode`s with parent pointers**,
each node holding only the tokens it adds to the cumulative sequence, so a training sample
is a root-to-leaf walk and "branching falls out of the walk". A `Trace` is versioned
(`TRACE_VERSION`), carries the task's content `hash`, records every model call, and
records every request/response rewrite the interception server performed.

And then, one line:

```python
state: StateT = Field(default_factory=State, exclude=True)
```

**The environment state is excluded from the serialised trace.** Their `replay` CLI is
explicit about the consequence: it "recomputes everything computable from the saved
transcript" and "runtime-requiring signals don't run offline, so a replay carries offline
scores only."

## Source

Primary, read directly from `main`:
- `verifiers/v1/graph.py` — the `MessageNode` docstring and `parent` field.
- `verifiers/v1/trace.py` — `Trace`, `Branch`, `MessageNode` path properties
  (`token_ids`, `sampled_mask`, `logprobs`, `advantages`, `spread`), `TraceTask.hash`,
  `InterceptRecord`, `state: ... exclude=True`.
- `verifiers/v1/cli/replay.py` — the module docstring defining what replay means.
- `docs/v1/architecture.md` — orchestrator / rollout / runtime / harness / interception
  server.
- For contrast, `verl/trainer/ppo/ray_trainer.py` `_write_generations` — what the other
  major trainer keeps on disk.

## What is interesting

**The data structure we would have proposed is already shipped.** From `graph.py`:

> "A rollout is a graph of `MessageNode`s — one per distinct message, each linked to its
> predecessor. The conversation is a path from a root to a leaf; branches (compaction,
> subagents) are simply multiple leaves, so branching falls out of the walk. Each node
> stores only the tokens it *adds* to the cumulative sequence, keeping size linear in
> turns."

That is our `Step { parent, index, ... }` argument, made independently, for the token
side. They even have the invariant we would state: "By construction `concat(node.token_ids
along a path)` reproduces the exact `prompt_ids + completion_ids` the model saw." Anyone
proposing that Paranoid Android become the storage format for RL rollouts is proposing a
worse `verifiers.Trace`, without the renderer, the token spans, the logprob alignment or
the trainer integration.

**What the other trainer keeps is far weaker, and that is the more common case.** verl's
`_write_generations` writes one JSONL line per sample per global step:

```python
inputs  = self.tokenizer.batch_decode(batch.batch["prompts"],   skip_special_tokens=True)
outputs = self.tokenizer.batch_decode(batch.batch["responses"], skip_special_tokens=True)
...
base_data = {"input": inputs, "output": outputs, "gts": gts, "score": scores, "step": ...}
```

A **detokenised** prompt string, a detokenised response string, a ground truth, and a
scalar. Special tokens stripped, tool-call boundaries gone, observations inlined into
prose, environment untouched. The training tensors live in `DataProto` in memory and are
consumed and discarded. So on disk, the artefact of a verl rollout is the exact object
our README says is a bad basis for attribution — a transcript.

**The gap is on one specific axis and it is stated by them, not inferred by me.**
`verifiers` records the token side of a rollout with real care and records the world side
not at all, by an explicit `exclude=True`. The consequence is written into their replay
docstring: anything that needs the runtime cannot be recomputed. Their `Branch` type
covers branching *within* a trace (compaction, subagents) — multiple leaves of one graph
— not counterfactual re-execution from an earlier point.

**Their interception server is our proxy.** From `architecture.md`: the harness "does
_not_ call the provider endpoint directly", model traffic goes through an interception
server which "uses the endpoint that the harness expects, so Codex will use OpenAI
Responses, while Claude Code will use the Anthropic Messages API", builds traces live, and
can rewrite tool responses "to block reward hacks". That is `clients/python/noidroid/
proxy.py` (`af81680`, "record an agent you did not write") with a different motive, and
they have `runtimes/docker/egress.py` where we have `clients/python/noidroid/fence.py`.
Independent convergence on both of our two least obvious integration choices.

## Why it matters to Paranoid Android

Three consequences, in order of importance:

1. **Do not build a rollout dataset format.** `bundle.rs` exports a trajectory for a
   human; that is a different job from feeding a trainer, and the trainer side is taken.
   Any proposal that starts "we could be the storage layer for RL rollouts" is competing
   with a versioned, token-aligned, renderer-aware format maintained by the people who
   own the trainer. `model.rs` should not grow tokens, logprobs, masks or advantages.
2. **The seam is `Trace.state`, the field they exclude.** A `state_root` from `tree.rs`
   is a 32-byte address of the workspace after a step. It is exactly the thing their
   format has a hole for, it is cheap, and it does not require them to adopt our object
   model — a digest per node, resolvable against a store, is an opaque string to them.
   That is the smallest possible thing we could contribute to their format, and it is the
   thing that makes their `replay` able to do something it currently cannot.
3. **The proxy is the right shape and now has a named host.** `verifiers` runs harnesses
   we did not write (Codex, Claude Code, mini-swe-agent, browser_use, terminus) behind an
   interception server. That is precisely the population `--proxy` was built for, and the
   interception layer is a documented place to sit.

## Transferability

MEDIUM. The `state_root`-per-node idea transfers as data with no dependency on our engine,
which is its strength. Everything else about their design assumes a token-centric world we
do not model and should not: their node identity is a message, ours is a step; their
`Branch` is a leaf of one graph, ours is a step whose parent is in another trajectory.
The two models are compatible at the level of "a step carries an address" and incompatible
at every level below that. Do not try to unify them.

## Novelty

PRESENT for the data structure — parent-linked, prefix-sharing, per-node incremental
content is what both systems do, and theirs is more developed on the token axis. MISSING
for the world axis: verified against their source, nothing in `Trace` addresses the
environment, and their replay explicitly cannot reach it. Verified against ours:
`crates/noidroid-core/src/model.rs` has no token, logprob or reward concept and
`grep -rn "reward\|advantage\|logprob" crates/` is empty, so nothing here is already built
on our side either.

## Limitations and negative signal

- This is a fast-moving v1. `docs/v1/` and `docs/legacy/` both exist in the repo, the
  legacy environment API is still present, and `TRACE_VERSION` implies the format has
  already moved once. Anything we build against it should assume it moves again.
- I read the type definitions and the docstrings, not the tests, and I did not run it.
  The claim "state is not persisted" rests on `exclude=True` plus the replay docstring,
  which agree — but I have not confirmed there is no side-channel that persists runtime
  state elsewhere (`utils/artifacts.py` exists and I did not open it).
- Negative for us: their format's completeness on the token axis means the "we store
  rollouts better" pitch is dead. Say so out loud rather than discovering it in a design
  review.

## Recommendation

WATCH, with one concrete trigger — and the *acting* recommendation lives in the fork-point
card, not here. Re-check when `verifiers` v1 stabilises its `Trace` schema or when a
`state`-bearing field appears; that is the moment a `state_root` contribution is either
welcome or foreclosed.

## Proposed action

No build. Two cheap things: (a) record in `constraints.md` that we are not building a
rollout/dataset format, with this card as the reason; (b) when the fork-point report from
`2026-08-21-unverified-fork-in-branching-rl` is prototyped, emit it as one JSON object per
fork point keyed by an id the trainer already has, so it can be joined onto a `Trace`
without either side adopting the other's model.

## Confidence

HIGH on the mechanism — the `verifiers` and `verl` code was read directly from `main` and
the quoted lines are verbatim. MEDIUM on the ecosystem claim that these two are
representative; I did not open OpenRLHF, SkyRL, AReaL, slime, ROLL or TRL this run, and
the "everyone stores a transcript plus a scalar" generalisation rests on two data points.

## Evidence

- Primary: `PrimeIntellect-ai/verifiers` `verifiers/v1/graph.py`, `trace.py`,
  `cli/replay.py`, `docs/v1/architecture.md`.
- Primary: `volcengine/verl` `verl/trainer/ppo/ray_trainer.py` lines 453–545.
- Counter-evidence: their `Branch` type and multi-leaf graph mean they are not naive about
  branching; they simply branch inside a transcript rather than re-entering a world.

## Changelog
- 2026-08-21 — created.
