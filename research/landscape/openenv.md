---
name: OpenEnv
class: POTENTIAL INTEGRATION
first_seen: 2026-08-21
updated: 2026-08-21
url: https://github.com/meta-pytorch/OpenEnv
licence: BSD-3-Clause (per PyTorch-org convention; not verified against LICENSE this run)
activity: active — moved from Meta + Hugging Face to a nine-org steering committee
---

## What it is

The interface standard for agentic RL execution environments. A Gymnasium-shaped
`reset()` / `step(action)` / `state()` contract, served as a containerised FastAPI server
over HTTP with a WebSocket transport, so any trainer that speaks the protocol can drive
any conformant environment. Integrations are shipping in TRL, verl, TorchForge, SkyRL and
`verifiers`; the steering committee includes the PyTorch Foundation, vLLM, SkyRL and
Stanford.

If any single interface becomes "the socket" for open agentic RL, this is it.

## How it works

From `rfcs/002-env-spec.md`:

- **Three baseline APIs.** `reset()` starts an episode and returns the initial
  observation; `step(action)` executes one action and returns an observation; `state()`
  gives "visibility into the current episode state and metadata".
- **Rewards are environment-computed** and ride on the observation:
  `Observation{done, reward, metadata}`. Decision 2's rationale explicitly permits
  environments to "use internal state and context **not visible to clients** for reward
  computation".
- **Two interfaces, deliberately separated.** HTTP is for RL orchestration (`reset`,
  `step`, `get_state`); a separate MCP interface carries agent–environment tool
  interaction. The RFC is emphatic that they must not blur.
- **Lifecycle is orchestration-only.** "Suspend, resume, snapshots, port refresh, and
  egress changes are orchestration operations, never MCP tools exposed to the agent." The
  RFC then warns that automatic suspend / scale-to-zero / idle timeouts "can drop a live
  transport mid-episode, so providers choose conservative defaults for RL rollouts and
  document the failure mode."
- **Cloud sandbox providers** are adapters onto a `ContainerProvider` contract, mapping
  "create from a source artifact (image, disk image, snapshot, template)" onto
  `start_container(image=...)`. Conformance is tested by whether the exposed URL proxies a
  WebSocket upgrade — "a `200` on `/health` does **not** prove conformance."
- **`seed()` is not in the baseline.** "Additional APIs (e.g., `render()`, `seed()`) will
  be explored in follow-up RFCs."

## What it does that we should learn from

The conformance line is excellent and we should steal the sentiment: *a health check is
not a conformance test*. Our environment model's §12 conformance table lists what an
environment must do; it does not yet say how you would catch one that claims a row it
does not meet. OpenEnv picked one behaviour that cannot be faked by returning `200` and
made that the test. `docs/environment-model.md` §12 wants the equivalent, and after #52 we
finally have the machinery for it — a run that never re-drove reports `opaque` and names
the world, which is exactly a conformance signal.

Also worth noting: they explicitly refuse to let the *agent* touch snapshots and resume.
Lifecycle is the orchestrator's, never the agent's. That is the same instinct as our
"irreversible effects are never performed during replay" — authority over time-travel
belongs to the harness, not the program.

## Where it is weaker, and why that is interesting

Two holes, and they are the two we care about.

**No reproducibility primitive.** No seed in the baseline, no determinism statement, no
snapshot/restore in the core protocol. An OpenEnv episode is not designed to be repeatable
and the spec does not pretend otherwise — but neither does it say so out loud, which means
the trainers integrating it inherit the gap silently.

**The reward is deliberately opaque.** Decision 2 lets the environment compute a reward
from state the client cannot see. That is a reasonable encapsulation choice and it makes a
reward permanently unauditable from outside the container. It is the structural version of
`2026-08-21-reward-computed-over-an-unaddressed-state`.

## Overlap with us

None as a product; substantial as a seam. `state()` is a declared, standardised
observation endpoint on every conformant environment — which is precisely what
`Situation::report` in `crates/noidroid-core/src/env.rs` wants and currently has to be
hand-wired per adapter. An OpenEnv environment could be given `witnessed` grip generically:
call `state()`, hash the response, report it as the world's observation. One adapter,
every conformant environment, no per-environment work — and the honest limit is that
`state()` returns whatever the environment chooses to expose, so the fingerprint is only
as good as the environment's own disclosure, which is exactly what `witnessed` means.

## Watch triggers

- The follow-up RFC on `seed()`. If a seed lands in the standard, `2026-08-19-autoseed-and-record`
  gets a standardised place to live and our story for RL environments improves for free.
- Any RFC on snapshot/restore or episode replay in the core protocol — that is either the
  moment to integrate or the moment the seam closes.
- Whether `state()` grows a stability or content contract. Today it is "state and
  metadata", which is too loose to fingerprint reliably.
- RFC 005 (`actions()` / MCP tool discovery), which would tell us whether effect kinds are
  ever going to be declarable at this layer — `2026-08-19-reversibility-is-not-in-the-instrument-standard`
  says they were not in SiLA 2 and this is the same question in a new standard.

## Changelog

- 2026-08-21 — created. Read `rfcs/002-env-spec.md` directly (Decisions 1–3, the cloud
  sandbox provider invariants, the systems-built-on-OpenEnv section). Did not read the
  implementation, RFC 005, or any integration.
