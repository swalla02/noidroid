---
id: 2026-08-21-reward-computed-over-an-unaddressed-state
title: A verifiable reward is computed against a final state that has no address
discovered: 2026-08-21
updated: 2026-08-21
categories: [agent evaluation, provenance, state reconstruction, computer-use agents, unserved-problem]
class: RESEARCH
recommendation: PROTOTYPE
transferability: HIGH
novelty: MISSING
confidence: MEDIUM
touches: [engine, tree, cli, checkpoint, env]
---

## Discovery

"Verifiable reward" in agentic RL means: after the episode, run a task-specific checker
against the environment's final state and take the 0–1 result. OSWorld does it with
per-task getters and checkers that inspect the final VM; SWE-bench runs the test suite;
CUA-Gym generates a reward function per task with an adversarial generator/discriminator
loop. In every case **the state the checker ran against is never named**. What survives
the episode is the checker's scalar output and, at best, a transcript.

Three things become impossible at once, and each is currently handled by a separate
patch-up industry:

1. You cannot re-check a reward without re-running the whole episode.
2. You cannot tell a buggy checker from a wrong final state from an agent that tampered
   with the checker.
3. You cannot tell a *measured* reward from an *echoed* one — a reward computed during a
   replay where the environment was served from a recording rather than touched.

## Source

Primary:
- `PrimeIntellect-ai/verifiers` `verifiers/v1/trace.py` — `rewards: dict[str, Reward |
  None]` alongside `state: StateT = Field(..., exclude=True)`; and
  `verifiers/v1/cli/replay.py`: "Runtime-requiring signals don't run offline, so a replay
  carries offline scores only."
- `volcengine/verl` `ray_trainer.py::_write_generations` — the persisted rollout is
  `{input, output, gts, score, step}`, all decoded text plus a float.
- arXiv 2504.08942 (AgentRewardBench) — 1,302 expert-reviewed web-agent trajectories
  across 5 benchmarks and 4 LLMs.
- arXiv 2606.09863 (*From Confident Closing to Silent Failure*) — read as PDF; defines
  false success as the agent reporting completion while the environment state does not
  reflect it, and explicitly separates agent self-report from environment-state
  verification.
- OSWorld's execution-based grading contract (per-task getters/checkers over the final
  VM, 0–1 reward), from the OSWorld paper and repository description.

## What is interesting

**The measured error rate is large and it is on the verification, not the agent.**
AgentRewardBench finds roughly **30% of trajectories that LLM judges called successful
were failures** according to expert annotators, and that rule-based checkers go the other
way — rejecting valid trajectories that solved the task by an unanticipated route, which
depresses reported success rates below what an expert would score. So both available
oracles are miscalibrated, in opposite directions, and there is no third one.

**The reward-hacking literature is the same gap wearing a different name.** The canonical
agentic instance is an agent "silently tampering with evaluation scripts rather than
solving the underlying coding problem". Detection is currently attempted with trained
classifiers over paired normal/tampered trajectories, and the best reported numbers are
78.4% precision / 81.7% recall across six hand-defined hacking categories. That is a
statistical detector for something that is, mechanically, a *file that changed*. There is
no fuzziness in "the test file the checker imported is not the test file the task shipped"
— it is a byte comparison — and the reason people reach for a classifier is that nobody
holds the bytes.

**`verifiers` states the consequence in a docstring.** Their offline `replay` recomputes
only what is derivable from the saved transcript, because the runtime is gone. The field
knows exactly which signals it has lost and why; it has simply accepted it.

## The standard makes it structural, not accidental

OpenEnv — the Gymnasium-style standard for agentic RL execution environments, now under a
nine-org steering committee with integrations shipping in TRL, verl, TorchForge, SkyRL and
`verifiers` — settles this by decision rather than by omission. From `rfcs/002-env-spec.md`,
**Decision 2: Environment-Computed Rewards**, the rationale includes:

> "**Flexibility**: Environments can use internal state and context **not visible to
> clients** for reward computation"

So the interface that the open RL stack is converging on *guarantees* that a reward may be
a function of state the caller cannot see, and returns it as a float on the `Observation`.
The trainer receives `reward` and has no address for what produced it, by design.

Two more things from the same RFC matter here:

- The baseline is exactly `reset()`, `step(action)`, `state()`. **`seed()` is not in it** —
  "Additional APIs (e.g., `render()`, `seed()`) will be explored in follow-up RFCs." The
  emerging standard for RL environments has no reproducibility primitive at all, which is
  the same hole `2026-08-19-autoseed-and-record` found in Minari's neighbourhood, now
  hardened into a spec.
- `state()` **does** exist as a first-class endpoint returning "the current episode state
  and metadata". That is a declared observation point on every conformant environment.

## Why it matters to Paranoid Android

`tree.rs` snapshots the workspace after every step and `Step.state_root` is its Merkle
root. That is the address the entire construction above is missing, and we already
compute it, for free, on every step. Three consequences, all of them things we can do
today or nearly:

1. **Offline reward recomputation.** `noidroid checkout-tree <address> <dir>`
   (`main.rs:241`, `tree::materialize_with`) puts the workspace at any step back on disk.
   So "re-run the checker against the final state" is already a two-command composition,
   with no agent, no model calls and no environment. That is precisely the capability
   `verifiers replay` says it does not have. Reward function changed? Re-score ten
   thousand stored episodes without regenerating one of them.
2. **Tamper detection as hash inequality, not classification.** The state root at the
   genesis step and the state root when the checker ran are both stored. A file that the
   task shipped and the agent modified shows up as a differing entry in
   `tree::diff` — positional, exact, and fatal in the sense C4 means. No threshold, no
   score, no six-category taxonomy.
3. **The honest label on a reward — and this is what #52 unlocked.** Before #52, a
   reconstruction that never re-drove its declared world still reported `witnessed`,
   because `Situation::adopt` supplied the fingerprint from the recording. A reward
   computed under that reconstruction would have looked like a measurement. After #52 the
   run carries `Report::served` — the named worlds it was handed rather than observed —
   and `Situation::achieved()` returns the grip the run *earned*, gated on
   `Delivery::Executed`. So we can now attach to a reward the one sentence nobody else
   can: **this reward was computed against a world this run actually drove, or it was
   not, and here is the world that was not driven.** That is a verified-rollout claim
   with a mechanism behind it rather than a badge.

Issue #53's open questions decide how strong that sentence is allowed to be — in
particular question 1 (should a pure replay report `opaque`?) is exactly the question
"is a reward recomputed offline a measurement?". The RL use case is a reason to answer it
deliberately rather than defer it: offline reward recomputation *wants* the answer "this
was recomputed, not measured, and that is fine and stated" rather than either silence or
a blanket downgrade.

## Transferability

HIGH for anything whose reward is a function of files — SWE-bench, terminal agents,
CUA-Gym's file-and-app checks, most of OSWorld's getters. It degrades exactly with grip:
a reward that depends on a browser page is `witnessed` (we can detect a difference, not
put it back), and one that depends on an external service is `opaque`. That degradation
is the honest answer and it is already computed per checkpoint by `checkpoint.rs`. What
does not transfer: rewards over states we never mediate — a GUI's pixel buffer, a remote
SaaS account — where we would be claiming an address for something we do not hold.

## Novelty

MISSING on both sides. Verified against our code: `grep -rn "reward\|score" crates/`
returns nothing; the engine has no reward concept and does not need one — the missing
piece is not a reward type but the *pairing* of a stored state address with an externally
computed score. Verified against theirs: `Trace.rewards` exists and `Trace.state` is
excluded, so the pairing cannot be expressed in the format that owns rewards.

## Limitations and negative signal

- **`tree.rs` only sees the sandboxed workspace.** Writes outside it are neither captured
  nor detected (README limitations). An OSWorld checker reading a browser cookie jar or an
  app's config in `$HOME` is reading state we do not address. The claim in this card is
  confined to file state inside the boundary, and overstating it would be the exact
  failure the project names.
- **Whole-file blobs after every step.** A rollout that writes a large artifact per step
  stores a full copy each time; this is the open storage question in
  `research/README.md`'s standing list, and RL-scale rollout counts are where it stops
  being theoretical.
- **AgentRewardBench's 30% is about LLM judges, not about state checkers.** It does not
  establish that execution-based checkers are wrong; it establishes that the *other*
  oracle is. I am using it as evidence that verification is unsolved, not that state
  addressing fixes it.
- I read the false-success paper as a PDF and the extraction was partial — I have the
  definition and the method split, not the measured rate. Confidence is MEDIUM for this.
- Honest counter: none of this helps where the reward is genuinely subjective (rubric
  judges, open-ended writing). There, the state address is irrelevant and a judge is the
  only oracle. Roughly half of current RLAIF is that.

## Recommendation

PROTOTYPE — a scored-checkpoint artifact and offline reward recomputation, on one real
task family (SWE-bench-style: repo in the workspace, reward = test suite).

## Proposed action

Two pieces, in order.

1. `noidroid score <trajectory> --at <step> -- <command>`: materialise the step's
   `state_root` into a scratch directory, run the command, capture its exit status and
   stdout, and print `(step_address, state_root, command, status)` — writing nothing back
   into the trajectory. Pure composition of `checkout-tree` and a subprocess; the value is
   that the tuple is now a citable object.
2. Extend the run report so a reward line carries the run's earned grip and its `served`
   worlds, from `Report::grip` and `Report::served` as landed in #52 — so a score computed
   under a reconstruction is printed as recomputed-not-measured.

Then measure the thing that would kill it: take a stored trajectory, mutate one file the
checker depends on, and confirm the state-root diff names it and the score changes. If a
plausible reward-tampering case does not show up as a tree diff, the tamper-detection
claim is wrong and should be struck from this card.

## Confidence

MEDIUM. The `verifiers` and `verl` code is read directly and is HIGH-grade. The
AgentRewardBench number comes from the paper's abstract and summaries rather than its
method section. The false-success paper extracted only partially. The OSWorld
getter/checker contract I have from the paper and repository descriptions, not from
reading `desktop_env/evaluators/`.

## Evidence

- Primary: `verifiers/v1/trace.py` and `cli/replay.py` — rewards present, state excluded,
  offline replay explicitly cannot reach runtime signals.
- Primary: `verl/trainer/ppo/ray_trainer.py` — the persisted rollout is text plus a float.
- Primary: `meta-pytorch/OpenEnv` `rfcs/002-env-spec.md` Decision 2 — rewards may be
  computed from state not visible to the client, by design.
- Primary: arXiv 2606.09863 — false success defined as agent claim vs environment state.
- Supporting: arXiv 2504.08942 — ~30% of judged-successful web-agent trajectories were
  failures; rule-based checkers reject valid ones.
- Supporting: the reward-hacking detection literature's 78.4%/81.7% classifier, as the
  statistical substitute for a byte comparison nobody can make.
- Counter-evidence: rubric- and judge-based rewards have no state to address, and are a
  large share of current practice.

## Changelog
- 2026-08-21 — created.
- 2026-08-21 — added the OpenEnv RFC section: the emerging cross-org environment standard
  mandates client-invisible reward state and omits `seed()` from its baseline.
