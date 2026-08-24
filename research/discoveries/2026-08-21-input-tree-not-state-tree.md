---
id: 2026-08-21-input-tree-not-state-tree
title: Given total control of the machine, Antithesis still models exploration as a tree of inputs — and then had to add injection time
discovered: 2026-08-21
updated: 2026-08-21
categories: [counterfactual reasoning, state-space exploration, deterministic replay, checkpointing, fault injection]
class: INFRASTRUCTURE
recommendation: WATCH
transferability: MEDIUM
novelty: DIFFERENT
confidence: HIGH
touches: [engine, model, cli]
---

## Discovery

Antithesis built a deterministic hypervisor — a fork of FreeBSD's bhyve — that can
reproduce the entire state of a virtual machine bit for bit. Having paid that price,
they did **not** model state-space exploration as a tree of machine states. They model
it as a tree of *inputs consumed at the points where the guest asks for input*:

> "The points in execution history where the guest ingests input from the Antithesis
> platform become possible branch points for future execution. Consequently, the
> external view of the exploration of a system is an input tree."

That is our object model, arrived at from the opposite extreme of the control spectrum.
Then comes the part we should read carefully:

> "Later on, we also added interrupt injection to 'push' actions into the guest when we
> wanted to preempt its current activity instead of waiting for the next predetermined
> i/o exchange point. You can still conceptualize this as an input tree but now inputs
> have an **injection time** rather than just being consumed whenever the guest is
> ready."

## Source

Primary: Alex Pshenichkin (Antithesis), "So you think you want to write a deterministic
hypervisor?", 2024-03-20, <https://antithesis.com/blog/deterministic_hypervisor/> —
read in full. Sections used: "Determinism in Antithesis", "A deterministic view of
time", "CPU parallelism", "Deterministic I/O", "One of many building blocks".

Supporting: <https://antithesis.com/docs/introduction/how_antithesis_works/> — the
product-level description ("tens or hundreds of thousands of alternate universes", an
RL-driven guidance component choosing where to explore).

## What is interesting

**The convergence.** Antithesis controls everything: guest time is "a function of only
the deterministic state and execution history of the guest", all time sources (TSC,
HPET) are virtualised, and guest↔host communication runs over a bespoke `VMCALL`
instruction. With that much control, the natural exploration primitive would be "clone
the machine state here and run both ways". Instead the *external* representation of the
whole search is the sequence of inputs the guest asked for. Machine snapshots are an
optimisation underneath — "We are not replaying every single execution path from the
beginning – we would not waste your time like that!" — not the identity of a branch.

Our `Step { parent, index, action, effects, .. }` with `Action::Call | Decide` is the
same abstraction: a branch is identified by the inputs consumed up to a point, and the
one that differs. C2 ("a checkpoint is a deterministic prefix, not a memory snapshot")
is the same choice, made because we have no alternative rather than because we chose it
among alternatives — and it is worth knowing that the people with the alternative made
the same call.

**The refinement we lack.** A pure input tree can only branch where the program chose to
ask. Antithesis found that insufficient and added time-indexed injection: an input that
arrives when *they* decide, preempting whatever the guest was doing. Our entire
intervention vocabulary is reactive — `Intervention::ReplaceResult | ReplaceDecision |
Fail` and the named `Failure` injections all answer a `Request::Call` the program made.
There is no way to express "at index k, something happened that the program did not ask
about."

**The parallelism answer, again.** They run one VM per physical core, pinned; guest
concurrency comes from the guest OS scheduler, which they also control and use as a
fault-injection surface. Nobody determinises real parallelism (see the update to
`2026-08-19-process-determinism-ceiling`).

## Why it matters to Paranoid Android

Two things, one reassuring and one not.

The reassuring one is citable: **the input tree is not a compromise forced by low-code
capture.** When someone argues that a real counterfactual engine needs process images,
this post is the counter-example — the team that built the deterministic hypervisor
represents its search as an input tree anyway, and uses snapshots only to avoid
re-executing prefixes. That is precisely the snapshot fast-path already on our roadmap,
positioned exactly where C2 says it may go: behind the checkpoint interface, not
replacing it.

The unflattering one: our branch-point set is exactly the set of moments the program
called us. For our domain — an agent that pulls from tools, models and pages — that is
close to complete, because almost nothing pushes at an agent. But it is not complete,
and the gaps are nameable: a tool that returns after the agent gave up, a rate limit
arriving out of band, a file changed by another process mid-step, a user interrupt. We
cannot represent any of those, and today we do not say so. The honest statement is not
"we support fault injection" but "we support fault injection **at points the program
asked about**".

Bears on: counterfactual exploration, branching, `engine.rs::apply_intervention`, and
the README's account of what branching can do.

## Transferability

MEDIUM, and asymmetric. The input-tree validation transfers immediately and costs
nothing — it is an argument, not a feature. The injection-time mechanism does **not**
transfer without preemption: to deliver an input the program did not request, you must
be able to interrupt it, which means owning the runtime. C1 forbids that and I am not
proposing it.

There is a weaker version that does fit: a step whose `Action` records an event the
program *received* rather than *requested*, declared by an adapter that is already in
the loop — the browser adapter watching a page mutate, or a `--watch` directory
changing under the program. That is not preemption; it is a second party in the same
session reporting an arrival. Whether that is worth having depends entirely on whether
any real workload has one, which today none of ours does. Hence WATCH, not INVESTIGATE.

## Novelty

DIFFERENT for the injection-time half — it solves branch-point placement in a way our
model structurally cannot express. PRESENT for the input-tree half: `model.rs` already
is this, and the card exists so we stop treating "they have snapshots, we have prefixes"
as a gap when the people with snapshots agree with us about the representation.

## Limitations and negative signal

- The post is explicit that it withholds the interesting part: "I've also left out one
  key design pillar … all the functionality that allows efficient state exploration and
  time-travel debugging." The mechanism for *choosing* which branch to explore is not
  described anywhere I could reach, and the docs page describes it only as "it uses RL".
  I could not open a primary source for the search strategy; treat any claim about it as
  unverified.
- Their determinism is not free of holes either: the instructions-retired performance
  counter they drive simulated time from "isn't quite deterministic, even in its special
  'precision' mode … about one in a trillion instructions would be miscounted".
- One VM per physical core means throughput comes from breadth, not speed. Their stated
  trade is "throughput over latency" — the opposite of what a developer running one
  `noidroid branch` wants.
- This is a commercial platform describing its own product. The mechanism claims are
  specific and falsifiable enough to trust; the efficacy claims are marketing and I have
  not used it.

## Recommendation

WATCH — with one thing to do now that costs nothing.

Trigger to re-check: Antithesis publishing the promised follow-up on state exploration
and time-travel debugging. That post, if it appears, is the most likely source in this
whole field for how to *choose* branch points, which is roadmap item 4 (guided
multi-branch exploration) and which we currently have no theory of beyond `bisect`'s
exhaustive sweep.

## Proposed action

Documentation, not code. In the README's account of branching (and in
`docs/direction.md` if it makes a claim there), state the boundary explicitly:
interventions apply at recorded interaction points, and an event the program never asked
about cannot be injected. One sentence. It converts an unstated limit into a stated one,
which is the tie-breaker this project's prioritisation rubric puts first.

Do **not** build unrequested-event injection. There is no workload asking for it, and
the general mechanism needs preemption we have ruled out.

## Confidence

HIGH on the mechanism and the quotes — the post was read in full and the quoted
sentences are verbatim. HIGH on our own limitation: `engine.rs::apply_intervention` is
reached only from `on_call` / `on_decide`, i.e. only from a request the program made.
LOW on anything about how Antithesis chooses which branches to explore, which is
deliberately unpublished.

## Evidence

- Primary: <https://antithesis.com/blog/deterministic_hypervisor/> — the input tree,
  injection time, the PMC nondeterminism figure, one-core-per-VM, the refusal to replay
  every path from the beginning.
- Supporting: <https://antithesis.com/docs/introduction/how_antithesis_works/> — fault
  injection plus an RL guidance component; "alternate universes" framing.
- Ours: `crates/noidroid-core/src/model.rs` (`Step`, `Action`),
  `crates/noidroid-core/src/engine.rs` (`apply_intervention`, reachable only from
  `on_call`/`on_decide`), C2 in `research/constraints.md`.

## Changelog

- 2026-08-21 — created.
