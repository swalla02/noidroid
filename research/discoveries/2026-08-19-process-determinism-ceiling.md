---
id: 2026-08-19-process-determinism-ceiling
title: The ceiling on process-level determinism is the ISA, not effort — C1 holds, with a sharper reason
discovered: 2026-08-19
updated: 2026-08-21
categories: [deterministic replay, interposition / interception, negative-signal, capture honesty]
class: RESEARCH
recommendation: IGNORE
transferability: LOW
novelty: REFINEMENT
confidence: HIGH
touches: [docs]
---

## Discovery

Two independent, well-executed attempts to make an arbitrary Linux process
deterministic from the outside — Meta's Hermit and an individual engineer's
`stillness` — both land on the same wall, and it is not a wall of effort. Hermit is now
in maintenance mode inside Meta with "substantial but incomplete" Linux compatibility;
the `stillness` author concludes flatly that "deterministic computing is impossible in
x86 due to fundamental properties of the ISA", while noting that "pretty good"
determinism is reachable.

## Source

- Primary: <https://josnyder.com/blog/2026/deterministic.html> — the implementation
  writeup, including which sources of nondeterminism were handled and the one that was
  not.
- Primary: <https://github.com/facebookexperimental/hermit> — README, maintenance
  status, compatibility matrix, overhead figures.

## What is interesting

The `stillness` writeup is the useful one because it enumerates its own defeat
precisely. It handles: network (netns), filesystem (mountns + chroot), threading
(disabled outright), ASLR (`personality()`), the nondeterministic syscalls
(`getrandom`, `getpid`, `clock_gettime`, with the vDSO pointer zeroed to force the
syscall path), `rdtsc` (trapped and emulated), `CPUID` (faulted and emulated), and
`AT_RANDOM`/`rseq`.

Then: **`rdrand`**. There is no control-register bit and no `prctl` that makes it
fault. The only ways out are full virtualisation or binary rewriting — a different
class of system entirely. Hermit's numbers say something similar from the other side:
3–6× native wall-clock, threads serialised, and a per-program compatibility matrix
(Node 16, OpenJDK 8, SQLite each with caveats) shipped because the general claim could
not be made.

The second interesting thing is the *shape of the honesty*. Neither project claims
completeness. Hermit ships a compatibility matrix and a `--verify` mode; `stillness`
ships a blog post naming the instruction that beat it.

## Why it matters to Paranoid Android

This is confirmation, not news, and that is the point of filing it.

Constraint **C1** says zero-code capture is not achievable and pursuing it produces a
worse system; we capture the boundary, not the process. That decision is recorded in
`docs/direction.md` with the reasoning "capturing enough from *outside* an
uninstrumented process to reconstruct it is not portably possible". This scan looked
hard for evidence against it and found the opposite, with a sharper articulation
available:

> The limit is not that nobody has built the interception layer well enough. Meta
> built it, with ptrace + seccomp + namespaces + a scheduling policy layer, and the
> residual hole is a single unfaultable instruction plus a 3–6× tax plus a
> per-program compatibility matrix.

That is worth having in our own words, because "not portably possible" invites the
reply "have you tried harder". "There is no control-register bit that makes `rdrand`
fault" does not.

Second-order consequence, and it cuts the other way: the parts of the stack *below*
the impossible bit are ordinary and effective. Netns for egress, mount namespaces for
the filesystem boundary — those are exactly what
`2026-08-19-kernel-enforced-capture-boundary` is about. Rejecting whole-process
determinism does not mean rejecting kernel enforcement of a boundary we have already
chosen.

## Transferability

**LOW** as a technique — we are deliberately not in this business. HIGH as an
argument: this is the citation to reach for the next time zero-code capture is
proposed.

## Novelty

**REFINEMENT.** The decision is already made (C1). What is new is the mechanism-level
reason and a dated, primary-source citation for it, plus the maintenance status of the
most credible attempt.

## Limitations and negative signal

This card is mostly negative signal, and it is the good kind:

- Hermit: **maintenance mode**, no longer actively developed within Meta. The most
  resourced attempt at this stopped.
- `stillness`: single-threaded only by construction — threading is *disabled*, not
  determinised. Our own limitation ("sequential programs only") is the same
  limitation, arrived at honestly by everyone who tries.
- 3–6× overhead is the price even when it works.
- Explicitly not a security boundary — Hermit says so — so it would not even give us
  the egress guarantee for free.


## Update 2026-08-21 — a third ceiling, and the parallelism answer is unanimous

The DST scan found two things that sharpen this card.

**A second unfaultable-hardware result, from someone who owned the hypervisor.**
Antithesis built a deterministic fork of FreeBSD's bhyve and drives simulated time from
the Intel performance-monitoring counter for instructions retired. Their own writeup:

> "The PMC instructions retired count isn't quite deterministic, even in its special
> 'precision' mode. Based on our testing, about one in a trillion instructions would be
> miscounted due to some unknown quirk of the CPU."

Plus a second one: the threshold interrupt is delivered through the APIC, so "dozens of
instructions will be processed before the CPU is actually notified", with variable
overhead. So the ceiling is not only `rdrand`. Even the measurement apparatus you would
use to *define* deterministic time is nondeterministic at the margin, and the people
who worked around it did so with a custom hypervisor, a custom kernel logger and 50 GiB
of trace per 20-minute run.

**Nobody determinises parallelism. Everybody eliminates it.** Six independent projects,
all of which had the resources and motive to do otherwise:

| project | what it does about threads |
| --- | --- |
| FoundationDB | single-threaded process; concurrency via Flow actors |
| TigerBeetle | single-threaded state machine, static allocation |
| Hermit | threads serialised, not determinised |
| `stillness` | threading disabled outright |
| madsim | `set_allow_system_thread(false)` by default — "not allowed by default because it may cause non-determinism" |
| Antithesis | **one VM pinned per physical core**; guest concurrency comes from the guest OS scheduler, which they also control |

Our README limitation — "Sequential programs only. Threads, async races, concurrent
interleavings out of scope" — is therefore not a gap relative to the field. It is the
field's unanimous answer, and the one project that spent the most money on the problem
(Antithesis) arrived at the most extreme version of it.

That has a consequence beyond documentation: any future proposal to capture async or
threaded execution (#33 currently *refuses* async SDK surfaces) should be read against
this table. The credible options are "serialise it" or "refuse it". "Determinise it" is
not on the list.

## Recommendation

**IGNORE** — as an implementation direction. C1 stands and is strengthened.

Use it as documentation: the README's first limitation ("Not zero-code") currently
asserts the claim; it could cite this instead. That is a docs change for a human to
make, not a code change.

## Proposed action

No engineering action. One optional docs action: add a footnote to the "Not zero-code"
limitation in `README.md` pointing at Hermit's maintenance status and the `rdrand`
ceiling, so the claim carries evidence. Re-check Hermit's status in twelve months
(watch trigger: a revival, or a successor project out of Meta).

## Confidence

**HIGH.** Both primary sources read directly; both state their own limits explicitly.

## Evidence

- Primary: <https://josnyder.com/blog/2026/deterministic.html>
- Primary: <https://github.com/facebookexperimental/hermit>
- Supporting: <https://antithesis.com/blog/deterministic_hypervisor/> — the PMC
  instructions-retired figure (~1 in 10^12 miscounted), APIC interrupt-delivery jitter,
  and one-VM-per-core as the answer to parallelism.
- Supporting: <https://github.com/madsim-rs/madsim/blob/main/madsim/src/sim/runtime/mod.rs>
  — `set_allow_system_thread`, disabled by default for the same reason.
- Supporting: <https://news.ycombinator.com/item?id=40076848> — practitioner
  discussion of Hermit's overhead and scope.
- Ours: `docs/direction.md` (C1), `README.md` § Limitations, `research/constraints.md`.

## Changelog

- 2026-08-19 — created.
- 2026-08-21 — updated during the DST scan. Added Antithesis's PMC nondeterminism
  figure as a second hardware-level ceiling independent of `rdrand`, and the
  six-project table showing that parallelism is universally eliminated rather than
  determinised. Recommendation unchanged (IGNORE); C1 and the sequential-only
  limitation both strengthened.
