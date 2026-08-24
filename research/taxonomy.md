# Taxonomy

*Categories for `categories:` in card frontmatter. Flexible on purpose — the taxonomy
should evolve as discoveries do. Add a category when you have a second card that needs
it, not on the first.*

Marked `[active]` where we are currently looking hardest.

## Core mechanism
- trajectory systems `[active]`
- deterministic replay `[active]`
- deterministic simulation testing `[active]` — seed-and-replay, deterministic schedulers,
  simulated worlds. Opened 2026-08-21; the distinction from `deterministic replay` is that
  there is no recording — the run is a function of a seed and the binary.
- record/replay systems `[active]`
- checkpointing `[active]`
- snapshotting
- copy-on-write
- content-addressed storage
- event sourcing
- state reconstruction `[active]`
- time-travel debugging
- causal debugging
- provenance `[active]`
- distributed tracing

## Capture and boundaries
- interposition / interception `[active]` — syscalls, LD_PRELOAD, ptrace, eBPF, shims
- proxies and man-in-the-middle capture `[active]`
- sandboxing
- virtualisation
- process isolation
- environment reconstruction / hermeticity `[active]`
- clock and randomness control `[active]`
- subprocess and async capture `[active]`

## Exploration
- counterfactual reasoning
- state-space exploration
- fault injection `[active]`
- chaos engineering
- property-based testing
- fuzzing
- differential testing
- regression generation
- causal inference
- model-based RL `[active]`
- simulation `[active]`
- digital twins

## Domains where the transferable idea lives
- robotics / ROS `[active]`
- physical simulation
- laboratory automation `[active]`
- instrument and device protocols — SiLA 2, OPC-UA, LAP, ophyd
- scientific workflows
- experiment tracking
- reproducibility
- scientific provenance
- databases and storage engines
- build systems
- game engines and netcode
- emulators

## Agents
- agent frameworks
- agent evaluation `[active]`
- agent observability
- computer-use agents `[active]`
- browser automation `[active]`
- AI-assisted debugging
- agent effect boundaries `[active]` — commit/rollback scope around an agent's *external*
  side effects. Opened 2026-08-24. Distinct from `checkpointing`: the subject is not how
  state is saved but **what a restore fails to take back**, and it is where `EffectKind`
  and the deny-by-default rule get compared against the field.

## RL post-training
*Opened 2026-08-21. This is where the trainer-side landscape lives — the question is not
"how do we do RL" but "what does an RL pipeline need from a trajectory".*
- rollout collection and storage `[active]`
- branching and tree rollouts `[active]` — Tree-GRPO, BPO, shared-prefix reuse
- verifiable rewards and reward integrity `[active]`
- RL environment standards and interfaces `[active]` — OpenEnv, verifiers, E2B
- agent sandbox substrates — microVM fork, CoW containers, snapshot/resume

## Cross-cutting
- negative-signal — approaches that failed, and why
- unserved-problem — the same wall hit by unrelated projects
- competitive-landscape

## Changelog
- 2026-08-19 — seeded.
- 2026-08-20 — marked `robotics / ROS` and `laboratory automation` active after the
  RL/robotics/labs scan; added `instrument and device protocols` under *Domains*, which
  is where the SiLA 2 and LAP findings live and which had no home. No other category
  opened — every other finding in that scan fitted the existing list.
- 2026-08-21 — opened **RL post-training** as a top-level category after the computer-use /
  RL scan. Four cards needed a home that neither *Exploration* nor *Agents* gave them: the
  subject is the trainer-side pipeline, not the algorithm and not the agent. Marked
  `agent evaluation`, `computer-use agents` and `browser automation` active. No other
  category opened — `checkpointing`, `state reconstruction`, `provenance` and
  `counterfactual reasoning` already covered the mechanism side of every finding.
- 2026-08-24 — added **agent effect boundaries** under *Agents* after the computer-use
  rollback scan. Three cards needed a home that `checkpointing` did not give them: the
  eight-paper transaction cluster, the `--live` irreversible defect, and the attended-state
  finding are all about the effects a restore does *not* undo, which is a different subject
  from how a checkpoint is taken. Nothing else opened — `capture honesty`,
  `environment reconstruction / hermeticity` and `state reconstruction` housed the rest, and
  `computer-use agents` stays active.
- 2026-08-21 — added **deterministic simulation testing** under *Core mechanism* after the
  DST scan, and marked `fault injection` and `simulation` active. It needed its own line
  rather than living under `deterministic replay`: DST reproduces a run from a seed with no
  recording at all, which is a different mechanism with a different validity window (only
  against an identical binary). Nothing else opened — `counterfactual reasoning`,
  `state-space exploration` and `clock and randomness control` already housed every other
  finding.
