---
name: Hermit (facebookexperimental/hermit)
class: INFRASTRUCTURE
first_seen: 2026-08-19
updated: 2026-08-21
url: https://github.com/facebookexperimental/hermit
licence: BSD-3-Clause / MIT (per repo)
activity: dormant — "maintenance mode", no longer actively developed within Meta
---

## What it is

A hermetic sandbox that launches Linux x86_64 programs and translates their
nondeterministic behaviour into deterministic, repeatable behaviour. Billed for
replay-debugging, reproducible artifacts, chaos-mode concurrency testing and bug
analysis.

## How it works

Three layers:

1. **CLI** — creates isolated namespaces.
2. **Reverie** — intercepts syscalls via `ptrace` and `seccomp`.
3. **Detcore** — the determinism policy: virtualises selected results, *serialises
   threads*, models resources and logical time.

Controlled sources of nondeterminism: thread scheduling, time, randomness, CPUID,
selected file metadata. Overhead is roughly 3–6× native wall clock depending on syscall
frequency and threading. Explicitly not a security boundary; file contents and network
responses remain inputs to the guest.

## What it does that we should learn from

**`hermit run --verify`.** It runs the program twice and compares output, exit status
and its deterministic log, because the authors did not trust their own interception
layer to be complete. A capture layer cannot enumerate the holes it failed to plug, but
it can observe disagreement between two runs. We can do a stronger version of this,
because replaying under our recorded-input oracle removes world drift that Hermit's
live double-run cannot. See `2026-08-19-verify-by-double-execution`.

Second: the **per-program compatibility matrix** (Node 16, OpenJDK 8, SQLite, each with
verified / limited / problematic status). Shipping a matrix instead of a claim is the
same instinct as our Limitations section, and a more granular expression of it.

## Where it is weaker, and why that is interesting

It attempts whole-process determinism, which is the road we decided against (C1), and
its trajectory is the argument: substantial-but-incomplete compatibility, 3–6× cost,
threads serialised rather than determinised, and ultimately maintenance mode at a
company that could afford to finish it. The residual gap is at instruction level —
`rdrand` cannot be made to fault without virtualisation or binary rewriting.

That is not a criticism of Hermit. It is the strongest available evidence that the
boundary-not-process choice was correct.

## Overlap with us

Almost none as a product — different user, different unit of work, no trajectory model,
no branching, no provenance. The overlap is the *problem*: both systems must answer
"did I actually capture enough to reproduce this?" and both refuse to answer it by
assertion.

**Evidence standard:** verified, not asserted — `--verify` plus a compatibility matrix.
Respectable, and the same posture as ours.


## Update 2026-08-21 — Hermit was somebody else's escape plan

RisingWave, having built madsim and adopted deterministic simulation testing across
their database, hit the wall that DST only covers dependencies somebody wrote a
simulator for. Their published way out was Hermit:

> "It is costly and less rewarding to develop simulators for each of them. Currently, we
> only maintain a simulator for Kafka data sources. However, Facebook's deterministic
> execution framework, Hermit, may provide a solution for various connectors by using a
> system-level approach to control the execution order of any process regardless of
> programming language."
> — *Applying Deterministic Simulation: The RisingWave Story (Part 2 of 2)*, 2023-04-25

That was written before Hermit went to maintenance mode. It is the clearest evidence
that the demand for a general, language-agnostic deterministic runtime is real and
unmet: a serious team identified it as the answer to their biggest limitation, and the
answer stopped being developed. Which is also the strongest available statement of why
C1 is a *position* rather than a concession — the general solution is what everyone
wants and nobody has shipped.

Watch trigger unchanged, and now more valuable: a revival or successor would be the
thing that moves the boundary for several projects at once, not just ours.

## Watch triggers

- Any revival, successor project, or upstreaming of Reverie out of Meta.
- Anyone building record/replay on top of Reverie independently.
- A published approach to `rdrand` that does not require virtualisation.

Re-check: 2027-08.

## Changelog

- 2026-08-19 — created.
- 2026-08-21 — updated during the DST scan. RisingWave named Hermit as their planned
  escape from the simulator-per-dependency wall, before it went dormant. Cross-linked
  to `2026-08-21-a-simulator-per-dependency`.
