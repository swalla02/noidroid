---
id: 2026-08-21-a-simulator-per-dependency
title: DST's price is a hand-written simulator per dependency, and everyone who adopts it stops paying at one or two
discovered: 2026-08-21
updated: 2026-08-21
categories: [deterministic replay, simulation, negative-signal, unserved-problem, reproducibility]
class: RESEARCH
recommendation: WATCH
transferability: LOW
novelty: DIFFERENT
confidence: HIGH
touches: [docs, clients]
---

## Discovery

Deterministic simulation testing reproduces a run from a seed of a few bytes instead of
a recording of megabytes. The price is stated plainly by everyone who has adopted it:
**the system under test must be written for the simulator, and every external dependency
needs a simulator someone writes and maintains by hand.** RisingWave — whose team wrote
madsim, the leading Rust DST runtime — reports that they got as far as one:

> "Limited to Rust language projects: RisingWave has introduced more external data
> source connectors that may use other languages or depend on external processes. It is
> costly and less rewarding to develop simulators for each of them. **Currently, we only
> maintain a simulator for Kafka data sources.**"

Their stated hope for escaping this was Hermit, which is in maintenance mode and no
longer developed at Meta (`research/landscape/hermit.md`).

## Source

Primary, read directly:

- RisingWave Labs, "Applying Deterministic Simulation: The RisingWave Story (Part 2 of
  2)", 2023-04-25,
  <https://www.risingwave.com/blog/applying-deterministic-simulation-the-risingwave-story-part-2-of-2/>
  — read in full. The quote above is from "Challenges and Limitations", along with the
  admission that test time grows until it breaches the CI budget and that they now trade
  fault probability against runtime.
- sled simulation guide, <https://sled.rs/simulation.html> — the adoption precondition
  stated as step one: "Step 1: write your code in a way that can be deterministically
  tested on top of a simulator." The whole recipe assumes a state machine of the form
  `fn receive(msg, at) -> [(msg, destination)]`, i.e. a system whose I/O has already
  been inverted.
- madsim README, <https://github.com/madsim-rs/madsim> — the tax itemised: replace
  `tokio`, `tonic`, `etcd-client`, `rdkafka`, `aws-sdk-s3` with madsim forks, plus five
  `[patch.crates-io]` overrides (`quanta`, `getrandom`, `tokio-retry`,
  `tokio-postgres`, `tokio-stream`), plus build with `RUSTFLAGS="--cfg madsim"`.
- TigerBeetle, `src/scripts/cfo.zig`
  <https://github.com/tigerbeetle/tigerbeetle/blob/main/src/scripts/cfo.zig> — the
  seed-storage rules in the header comment: "Keep seeds for at most `commit_count_max`
  distinct commits. Prefer fresher commits (based on commit time stamp). For each commit
  and fuzzer combination, keep at most `seed_count_max` seeds."
- FoundationDB, `documentation/sphinx/source/testing.rst` — "Simulation is able to
  conduct a deterministic simulation of an entire FoundationDB cluster within a
  single-threaded process", enabled by "and tightly integrated with Flow, our
  programming language for actor-based concurrency".
- Temporal, `https://docs.temporal.io/workflow-definition.md` — "all operations that do
  not purely mutate the Workflow Execution's state should occur through a Temporal SDK
  API."

## What is interesting

**The reproduction artifact differs, and so does what it is valid against.**

DST's artifact is a seed. TigerBeetle's simulator prints `SEED=…` and expands it into
the twenty-odd derived parameters it produced (cluster size, packet-loss probability,
partition mode, latency distributions) — the seed *is* the experiment, not merely its
entropy. But the CFO's storage rules give the game away: seeds are kept **per commit**,
and stale commits are evicted. A seed is a reproduction handle **relative to an
identical binary and an identical simulator**. Change the code and the seed still runs,
but it no longer runs the same experiment.

Our artifact is a recording. It costs orders of magnitude more bytes and buys the
opposite property: it stays valid when the binary changes. That is not a nicety, it is
the product — `bisect` re-runs a recorded prefix against a *different* choice, and
`Replay { live }` re-runs it against a *different* model or prompt. A seed cannot express
either, because under a seed the run is a function of the code and you have just changed
the code.

**The coverage differs too, in the direction people do not expect.** A seed reproduces
everything *inside the simulator* perfectly and nothing outside it at all. RisingWave's
S3, etcd and Kafka are reproducible because someone wrote `madsim-aws-sdk-s3`,
`madsim-etcd-client` and a Kafka simulator; their other connectors are not reproducible
at all, and the honest reason given is that writing more simulators is "costly and less
rewarding". A recording covers whatever crossed the boundary, whether or not anyone
understood it — which is why our browser adapter can reproduce a page nobody modelled.

**The two families and the hole between them.** Set them side by side:

| | reproduces | requires | valid when code changes | branch from the middle |
| --- | --- | --- | --- | --- |
| DST (FDB, TigerBeetle, madsim) | everything simulated | system written for it; a simulator per dependency | no | yes, cheaply — re-run from the seed |
| Durable execution (Temporal) | whatever crossed the SDK | code written to the determinism rules | only via explicit version markers | not the goal; replay exists to resume |
| Whole-process determinism (Hermit) | most of one process | Linux x86-64, 3–6× cost | n/a | no |
| Us | whatever crossed the boundary | route side effects through a client | **yes, by design** | **yes, that is the product** |

The hole is the row nobody occupies: *a program you did not write, talking to
dependencies nobody will ever simulate, re-run faithfully from the middle with one thing
different.* Temporal has the replay mechanism but aims it at resumption and needs the
program written to its rules. DST has the branching but needs the world rewritten. That
gap is not a market observation, it is a mechanism observation, and it is the clearest
statement of our position I have found in any source.

## Why it matters to Paranoid Android

Three concrete consequences.

**1. It settles what a seed can and cannot do for us**, and therefore bounds
`2026-08-21-engine-issued-seed`. Seeding narrows the divergence surface for in-process
randomness. It does not make the seed a substitute for the recording, and any
implementation that starts to treat it as one — "just store the seed, we can re-run it"
— is reintroducing a dependency on the binary being identical, which we do not have and
do not want.

**2. It is the argument against the obvious "why not just be deterministic?" question.**
The answer is now evidenced rather than asserted: because you would have to rewrite the
program, fork its dependency graph, and then hand-write a simulator for every service it
talks to — and the team best placed to do that got one simulator deep and stopped.

**3. It puts a name on what we should be measuring.** The DST world's headline number is
seeds per second. Ours cannot be. The comparable honesty metric for us is *what fraction
of the program's interactions with the world crossed the boundary we mediate* — which is
what `--auto`'s `unhooked()` list and the `--allow-gaps` refusal already gesture at, and
what `2026-08-19-verify-by-double-execution` would measure directly.

## Transferability

LOW as a technique — deliberately. This card exists to establish where the boundary
between the two families is, not to move us across it. The one mechanism worth stealing
is small and is filed separately (`2026-08-21-engine-issued-seed`).

One further mechanism is worth noting but not adopting: DST failure artifacts are
*shrunk*. The sled guide's last step is "drop out some of the initial client requests …
until you have a minimal set of commands that cause your invariant to be broken." We
cannot do this: `engine.rs::key()` is `format!("{}:{kind}:{target}", self.index)`, so
effect keys are positional and removing a step renumbers everything after it. Trajectory
minimisation is structurally unavailable to us, and if "smallest reproducer" ever becomes
a goal, that positional key is the thing that would have to change.

## Novelty

DIFFERENT. It is the same problem — reproduce an execution — solved by controlling the
world rather than recording it. Recording it as a card so that the next time DST is
raised, the question is not "should we do this" but "which of the four rows are we in,
and why".

## Limitations and negative signal

This card *is* the negative signal. The recurring wall, hit independently:

- **RisingWave**: one simulator written, the rest abandoned as not worth it; their named
  escape hatch (Hermit) is dead.
- **sled**: the precondition is architectural and up front — you cannot retrofit DST
  onto a system that already exists.
- **madsim**: ten dependencies replaced or forked before you start; and their own
  entropy interposition does not work on Linux (see the seed card).
- **FoundationDB**: they wrote a programming language. The testing doc's framing is that
  simulation "is enabled by and tightly integrated with Flow".
- **RisingWave again**: DST test time grows monotonically until it breaks the CI budget,
  and the mitigation is to lower the fault-injection probability — i.e. to test less.

The counter-consideration I owe the reader: DST finds bugs we structurally cannot. Every
one of RisingWave's recovery and scaling bugs is a concurrency bug in a distributed
system. We record sequential programs. Nothing in this card claims our approach is
better at anything except the thing in the table.

## Recommendation

WATCH — no build. Re-check on one trigger: **a general-purpose deterministic runtime
that does not require rewriting the system under test.** That is the thing that would
move the boundary in the table, and Hermit's maintenance mode is the current evidence
that it is not coming. If one appears, C1's reopen condition is what to test it against —
reconstruction-grade capture, demonstrated by replay, not by tracing.

## Proposed action

No code. Use the table above when the README or `docs/direction.md` next needs to say
what this tool is *instead of*. The one-line version, which I would put in front of any
reader who asks why we do not just make things deterministic:

> A seed reproduces a run only against the binary that produced it, and only for the
> parts someone wrote a simulator for. A recording reproduces a run against a binary you
> have since changed, for whatever actually crossed the boundary.

## Confidence

HIGH. Five primary sources read directly, four of them source or first-party
engineering writeups, and the load-bearing quotes are verbatim. The synthesis in the
table is mine and is the part to argue with; each row is sourced but the framing is not
anyone else's claim.

## Evidence

- Primary: <https://www.risingwave.com/blog/applying-deterministic-simulation-the-risingwave-story-part-2-of-2/> — "we only maintain a simulator for Kafka"; CI-time growth; Hermit named as the hoped-for escape.
- Primary: <https://sled.rs/simulation.html> — DST's architectural precondition.
- Primary: <https://github.com/madsim-rs/madsim> — the dependency-replacement tax, itemised.
- Primary: <https://github.com/tigerbeetle/tigerbeetle/blob/main/src/scripts/cfo.zig> — seeds stored per commit; stale commits evicted.
- Primary: <https://github.com/apple/foundationdb/blob/main/documentation/sphinx/source/testing.rst> — simulation enabled by a bespoke language.
- Counter-evidence: the same RisingWave post — DST found panics, deadlocks and correctness bugs under node kills and rescaling that no boundary recorder would ever surface.
- Ours: `research/landscape/hermit.md`, `research/constraints.md` C1/C2, `crates/noidroid-core/src/engine.rs::key`.

## Changelog

- 2026-08-21 — created.
