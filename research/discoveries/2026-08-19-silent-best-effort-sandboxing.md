---
id: 2026-08-19-silent-best-effort-sandboxing
title: A shipped sandbox returned Ok() while enforcing nothing — the exact failure this project claims it cannot survive
discovered: 2026-08-19
updated: 2026-08-19
categories: [negative-signal, sandboxing, capture honesty]
class: INSPIRATION
recommendation: IGNORE
transferability: HIGH
novelty: DIFFERENT
confidence: MEDIUM
touches: [engine, cli, clients]
---

## Discovery

NVIDIA's OpenShell shipped v0.0.26 with Landlock filesystem confinement that did not
confine anything on Landlock-capable kernels. The sandbox process ran unconfined and
the supposedly-restricted directory stayed writable, and **nobody was told**: the
integration's `BestEffort` compatibility mode caught every failure in the setup
sequence, logged a finding, and returned `Ok(())`.

## Source

- Primary: <https://github.com/NVIDIA/OpenShell/issues/803> — the bug report with the
  root-cause analysis.

## What is interesting

The root cause is ordinary and that is what makes it worth filing. `drop_privileges()`
ran *before* Landlock was applied, so the path file descriptors Landlock needs
(`open(path, O_PATH|O_CLOEXEC)`) were opened as the unprivileged sandbox user (uid 998)
instead of root. Under a container runtime, an AppArmor profile, or a restrictive
mount, those opens fail. Reasonable enough.

The damage was done by the error policy, not the bug. `BestEffort` exists because
Landlock's own documentation recommends best-effort compatibility handling — the ABI is
versioned and kernels vary, so the guidance is to degrade to whatever the running
kernel supports. That guidance is sound for a *hardening* layer, where partial
enforcement is strictly better than none. It is catastrophic for a layer someone
*relies on*, because the two states — "enforced" and "silently not enforced" — are
indistinguishable from outside.

Two earlier PRs (#599, #677) were shipped as fixes and did not fix it, which is the
tell: when the failure state is invisible, you cannot tell a real fix from a claimed
one.

## Why it matters to Paranoid Android

This is a warning aimed directly at
`2026-08-19-kernel-enforced-capture-boundary`, and it arrived attached to the same
mechanism.

`docs/direction.md` states the rule: "Every silent gap is worse than a loud refusal,
and any change that turns a loud failure into a quiet one is wrong even when it is more
convenient." OpenShell is a live demonstration of what the convenient version costs.
If we adopt Landlock and follow the standard integration pattern, we will write a
`best_effort` path, because every guide recommends one — and a fence that reports
itself installed while enforcing nothing is *worse than the cooperative fence we have
today*, which at least never claimed more than Python sockets.

The generalisation is worth keeping beyond Landlock: **a capability probe must report
the level it achieved, never a boolean, and the achieved level must be carried into
whatever artifact claims it.** We already do this elsewhere and it works —
`Trajectory::allow_gaps` records that a recording was made with known capture holes and
carries it so that replaying makes the same allowance rather than pretending. That is
the pattern to reuse; it is already in `crates/noidroid-core/src/model.rs`.

## Transferability

**HIGH**, as a constraint rather than a technique. It costs nothing to honour and it
would be expensive to learn the same way.

## Novelty

**DIFFERENT.** Not a capability we lack — the opposite. It is an external
demonstration that our stated rule has teeth, and a specific place we were about to be
tempted to break it.

## Limitations and negative signal

The whole card is negative signal. One caveat on the evidence: I read the issue report
and its root-cause analysis, not the OpenShell source, and the fetched content did not
include maintainer responses or a confirmed fix. The mechanism described is specific
and internally consistent, but this is a reported bug rather than one I reproduced.
Confidence is MEDIUM for that reason and should not be raised without reading the code.

## Recommendation

**IGNORE** as a thing to build. Record it as a constraint on how we build the Landlock
spike.

## Proposed action

No engineering action of its own. Attach one acceptance criterion to the Landlock spike
in `2026-08-19-kernel-enforced-capture-boundary`:

> The fence has **no boolean state**. It reports an achieved enforcement level
> (`kernel(abi=N)` / `cooperative` / `none`), that level is recorded on the trajectory
> the way `allow_gaps` already is, and a replay of a trajectory recorded under kernel
> enforcement refuses to run under a weaker level rather than degrading to it. If the
> spike produces a code path where a failure to install returns success, the spike has
> failed regardless of whether the happy path works.

Propose promoting the general rule to `research/constraints.md` after the spike:
*capability probes report a level, never a boolean, and the level travels with the
artifact.*

## Confidence

**MEDIUM.** Primary source read; no maintainer confirmation or code inspection.

## Evidence

- Primary: <https://github.com/NVIDIA/OpenShell/issues/803>
- Context: <https://docs.kernel.org/userspace-api/landlock.html> — where the
  best-effort guidance comes from.
- Ours: `docs/direction.md` § "The rule that governs everything";
  `crates/noidroid-core/src/model.rs` (`Trajectory::allow_gaps`, the pattern to copy);
  `clients/python/noidroid/fence.py`.

## Changelog

- 2026-08-19 — created.
