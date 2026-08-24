---
name: verifiers (Prime Intellect)
class: POTENTIAL INTEGRATION
first_seen: 2026-08-21
updated: 2026-08-21
url: https://github.com/PrimeIntellect-ai/verifiers
licence: Apache-2.0 (stated on repo; not verified against LICENSE file this run)
activity: active — v1 shipped 2026-07, `docs/v1/` alongside a `docs/legacy/` tree
---

## What it is

The environment and rollout layer of the open RL post-training stack: a taskset defines
the work, a harness solves it, a rollout runs inside a runtime, and the result is a
`Trace`. It backs `prime-rl` and the Prime Intellect Environments Hub, and it is the
closest thing the open ecosystem has to a standard for "what an agent rollout is".

## How it works

Four moving parts, from `docs/v1/architecture.md`:

- An **orchestrator** distributes rollout requests to workers; the client owns the
  taskset.
- A **rollout** is one loaded task + harness + tools, with "an independent trace and
  runtime state".
- A **runtime** is `subprocess` (debug only — they warn about cross-subprocess side
  effects), `docker`, or a remote sandbox (`prime`, `modal`).
- An **interception server** sits between the harness and the provider API. The harness
  "does _not_ call the provider endpoint directly"; the server speaks whatever dialect the
  harness expects (OpenAI Responses for Codex, Anthropic Messages for Claude Code), builds
  the trace live, sets sampling parameters the harness does not expose, and can rewrite
  tool responses and server-side search results "to block reward hacks".

The trace itself (`verifiers/v1/trace.py`, `graph.py`):

- `Trace` is versioned (`TRACE_VERSION`), carries `TraceTask{type, data, key, hash}`,
  `AgentInfo`, `tools`, `nodes`, `calls`, `request_rewrites`, `response_rewrites`,
  `rewards`, `metrics`, `errors`, `timing`.
- `nodes` is a graph of `MessageNode`s with `parent: int | None`. Each node stores only
  the tokens it *adds*. A `Branch` is a root-to-leaf path and becomes one training
  sample; `token_ids`, `sampled_mask`, `logprobs`, `advantages` and named loss-weight
  streams are all derived by walking the path and spreading per-node values onto sampled
  positions.
- The stated invariant: "By construction `concat(node.token_ids along a path)` reproduces
  the exact `prompt_ids + completion_ids` the model saw."
- `state: StateT = Field(default_factory=State, exclude=True)` — runtime state is not
  serialised.
- `verifiers replay` re-scores a saved run offline and says so: "Runtime-requiring signals
  don't run offline, so a replay carries offline scores only."

There is also `runtimes/docker/egress.py`, i.e. an egress control at the same place we put
`clients/python/noidroid/fence.py`.

## What it does that we should learn from

The `MessageNode` docstring is the argument we make for `Step`, arrived at independently:
store each unit once, link to the predecessor, let branching fall out of the walk, keep
size linear in turns. Theirs is better than ours on the axis they care about — the token
alignment is exact and load-bearing, and `spread()` is a genuinely careful piece of work
for attributing per-node values onto per-token positions.

They also record the *interventions their own infrastructure made*: `request_rewrites`
and `response_rewrites` are `InterceptRecord`s on the trace. That is a delivery axis in
the C3 sense, in a system that has no vocabulary for it, and it exists because they
discovered they needed to know when the harness was handed something other than what the
provider said. We should note that as convergent evidence for C3, not as a thing to copy.

And their runtime taxonomy is honest in a way ours is not yet: the `subprocess` runtime's
documentation *warns* that one rollout can alter config files another depends on. They
name the leak rather than pretending isolation.

## Where it is weaker, and why that is interesting

`state` is excluded. The entire world side of a rollout — the files, the page, the
container — is absent from the persisted artefact, and their own replay docstring names
the consequence. So the format that owns rewards cannot express what a reward was computed
against, and a rollout cannot be re-entered, only re-read.

That is not an oversight; it is the same trade-off we take from the other side. They
optimise for the training sample and treat the world as ephemeral because a fresh rollout
is always available. We optimise for re-entering an execution that already happened and
have no story for generating a fresh one.

## Overlap with us

We share the parent-linked prefix-sharing trajectory and, in `--proxy`, the interception
architecture almost exactly. We do not share the evidence standard: their trace **asserts**
what happened (it is a faithful log of what the interception server saw) and makes no
reconstruction claim at all, so there is nothing for them to verify. Ours claims a
reconstruction and verifies it by hash equality.

The practical seam is one field. A `state_root` digest per node would be an opaque string
to them and the missing half to us — and it is the only thing either project needs from
the other.

## Watch triggers

- A `state` or environment-address field appearing in `Trace`. That is either an
  invitation or a foreclosure, and it decides whether the seam above exists.
- `TRACE_VERSION` moving, or `docs/legacy/` being deleted — the format settling.
- The `browser_use` harness (`verifiers/v1/harnesses/browser_use/`) gaining any
  reproducibility story; it is the harness closest to our browser adapter.
- Environments Hub gaining any notion of rollout provenance or submitted-rollout
  verification. Not present today as far as I could see.

## Changelog

- 2026-08-21 — created. Read `docs/v1/architecture.md`, `verifiers/v1/trace.py`,
  `graph.py`, `types.py`, `rollout.py`, `cli/replay.py` from `main`. Did not run it, did
  not read the tests, did not open `utils/artifacts.py`.
