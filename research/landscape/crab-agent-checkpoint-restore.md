---
name: Crab (HKUST) — semantics-aware checkpoint/restore for agent sandboxes
class: INFRASTRUCTURE
first_seen: 2026-08-24
updated: 2026-08-24
url: https://www.alphaxiv.org/abs/2604.28138
licence: unknown — no repository located
activity: active — preprint, April 2026
---

## What it is

A host-side runtime that makes checkpoint/restore of agent sandboxes *selective* rather
than uniform, by inferring from OS-visible effects whether a given agent turn changed
anything worth checkpointing. Its framing is the **agent–OS semantic gap**: agent
frameworks see tool calls but not their concrete OS effects; the OS sees state changes but
not the turn-level context that says whether they matter for recovery. Crab sits between
the two and reports that **over 75% of agent turns produce no recovery-relevant state**,
making most full checkpoints unnecessary.

## How it works

Three components, none requiring modification to the agent or the C/R backend.

- **Coordinator** — an HTTP reverse proxy on the agent↔LLM control path. An outbound LLM
  request marks a turn boundary; the Coordinator logs the request/response pair, dispatches
  a checkpoint job asynchronously *into the LLM inference wait window*, and buffers the
  response until that job is durable. If the response lands first, the job is promoted to a
  high-priority queue because its latency is now on the critical path.
- **Inspector** — in-kernel eBPF `sys_enter`/`sys_exit` tracepoints on filesystem syscalls,
  cgroups to bound which processes are the sandbox's, and soft-dirty page tracking
  (`/proc/PID/pagemap`, `/proc/PID/clear_refs`) for memory. Computes **net** change since
  the last checkpoint, so a temp file created and deleted in one turn counts as nothing.
  Classifies each turn: no checkpoint / filesystem-only / process-only / full.
- **C/R Engine** — OpenZFS incremental snapshots for the filesystem, runc-CRIU for process
  dumps, and a versioned manifest `C_i = (P_j, F_k)` pairing the latest process and
  filesystem artefacts into a consistent recoverable state, published transactionally.

Measured: Inspector accuracy 100% on process change (no false positives or negatives) and
98.3% on filesystem change with **zero false negatives** and 2.3% false positives.
Checkpoint p50/p95/p99 = 0.1/0.7/1.0 s, bimodal (filesystem-only 20–100 ms, process
700–1000 ms). Restore under 1 s. End-to-end overhead 0–1.9% versus fault-free execution;
up to 87% of turns skipped. RL tree rollouts: 40.0–64.2% token reduction.

## What it does that we should learn from

**The error direction is stated and it is the right one.** A false positive costs an
unnecessary filesystem checkpoint; a false negative would cost correctness; they measured
zero of the latter. Any detector we build for roadmap item 2 ("detecting unmediated effects
beyond the workspace") should be held to exactly that asymmetry, and Crab is the evidence
that it is achievable with commodity kernel facilities. This is now written into
`2026-08-19-kernel-enforced-capture-boundary`.

**Net change, not raw change.** The Inspector deliberately ignores transient effects that
cancel within a turn. `tree::snapshot` already gets this free by content-addressing — an
unchanged workspace yields the same `state_root` and `Store::put` is a no-op — but we pay
the *walk-and-hash* cost every step where Crab pays nothing. That is a performance
observation, not a correctness one, and it should not be dressed up as more.

**Checkpointing hidden inside the inference wait.** The median exposed delay is zero
because the checkpoint runs while the model is thinking. If our restore cost ever becomes a
problem, this is the trick — the agent is idle for seconds per turn and we currently do
nothing with that window.

**Their fast-forward is our checkpoint.** For the agent-in-a-sandbox deployment, after a
restore the Coordinator replays *cached historical LLM responses* to the agent until its
logical progress matches the restored checkpoint head. They needed a deterministic prefix
served from a recording to reconcile a snapshot with a process that outlived it. C2,
re-derived by someone solving a different problem.

## Where it is weaker, and why that is interesting

**"100% recovery correctness" is a benchmark outcome, not a re-derivation.** It means the
task still passed, on Terminal-Bench and SWE-Bench, with one injected crash. Nothing in
Crab compares the restored state to the state that was checkpointed. This is the same hole
as BPO's Assumption 1 and AgentENV's silent fork — see
`2026-08-21-unverified-fork-in-branching-rl`, which now lists Crab as its fourth instance.
The sharp part: their Inspector *already computes* the net-change information that would
let them verify a restore. They use it only to decide whether to checkpoint.

**The comparison baselines are the interesting data.** "Chat-only" recovery scored 8–28%
correctness and "Chat+FS" 28–42% on Terminal-Bench — that is the measured cost of
recovering an agent from its transcript alone, which is what most frameworks do.

**Deployment cost.** eBPF tracepoint attachment and CRIU both want privilege; this is a
host-side runtime for a fleet operator, not something a developer installs. That is the
opposite of our posture and it is why the *mechanism* transfers and the *product* does not.

## Overlap with us

Small and clean. Crab optimises the cost of checkpointing; we make a checkpoint verifiable.
They are a plausible *substrate* underneath a very different tool, not a competitor — and
the roadmap's "snapshot fast-path behind the same checkpoint interface" is precisely the
place their mechanism could sit, if C2's verification story survived the change
(`2026-08-19-snapshot-omits-derived-state` is the acceptance criterion).

They make no fidelity claim we also make. They claim efficiency and task-level recovery,
and they back both.

## Watch triggers

- **A repository appears.** None located; `orx paper` surfaced no associated GitHub. Source
  would let us check whether the Inspector's zero-false-negative result survives reading.
- **The Inspector is used to verify a restore rather than to schedule one.** That would make
  them the first system in this space with an evidence story, and it would close the gap
  `2026-08-21-unverified-fork-in-branching-rl` says is ours.
- Adoption by an RL stack (verl, verifiers, SkyRL) as a rollout substrate.

## Changelog

- 2026-08-24 — created, from the computer-use rollback scan. Report read via `orx paper`;
  no source read.
