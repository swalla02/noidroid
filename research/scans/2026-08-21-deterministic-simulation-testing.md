---
date: 2026-08-21
cadence: targeted
question: What does the deterministic simulation testing world know that Paranoid Android does not, and what should we build because of it?
cards_created: [2026-08-21-engine-issued-seed, 2026-08-21-input-tree-not-state-tree, 2026-08-21-a-simulator-per-dependency, 2026-08-21-replay-safe-change-taxonomy]
cards_updated: [2026-08-19-autoseed-and-record, 2026-08-19-verify-by-double-execution, 2026-08-19-process-determinism-ceiling]
landscape_created: [antithesis]
landscape_updated: [hermit]
---

# Scan: deterministic simulation testing

## In one paragraph

I read source and first-party engineering writing from FoundationDB (Flow, `IRandom.h`,
the simulation testing doc), TigerBeetle (`vopr.zig`, `fuzz_tests.zig`, `cfo.zig`),
Antithesis (the deterministic-hypervisor post), madsim (`runtime/mod.rs`, `rand.rs` in
full), RisingWave's adoption retrospective, the sled simulation guide, and Temporal's
determinism and versioning documentation plus the Python SDK's workflow instance. The
headline is not that DST has a technique we should copy — it mostly does not, because
its determinism is bought by owning the runtime and rewriting the system under test,
which C1 forbids and which its own adopters describe as the reason they stopped.
The headline is that **the two most authoritative systems in the field independently
confirm two of our load-bearing design choices, and one of them hands us the exact
mechanism for the seed work that has been sitting unbuilt since 2026-08-19.** Antithesis,
having built a deterministic hypervisor that can reproduce a whole VM bit for bit,
represents state-space exploration as *a tree of inputs consumed at boundary points* —
our `Step`/`Action` model, and C2 — with machine snapshots as an optimisation
underneath. Temporal runs our replay architecture in production and issues the PRNG seed
**from the orchestrator, inside the activation that starts the replay**, which is the
only shape that works for a branching system and is not the shape the existing autoseed
card proposed. Build the engine-issued seed. Ignore almost everything else in this field,
for reasons that are now evidenced rather than assumed.

## What survived

**`2026-08-21-engine-issued-seed` — the seed is an input delivered across the boundary,
not a value the client picks.** Temporal's `_workflow_instance.py` takes
`randomness_seed` off the workflow activation (line 165, applied at 326) and treats
re-seeding as a first-class recorded job (`update_random_seed`, lines 643/1172).
FoundationDB splits randomness into three typed generators, the third of which
(`debugRandom()`) exists solely so observability code cannot shift the stream the
program draws from. The decisive argument for us is branching: if the client mints a
seed, a branch re-executing the prefix mints a *new* one and two things differ instead
of one. Placement is settled too — `Action::Genesis` with `#[serde(default,
skip_serializing_if)]`, which `model.rs:499` explicitly sanctions as needing no
`STEP_VERSION` bump, exactly as `grip` did. This is the run's one build recommendation.

**`2026-08-21-input-tree-not-state-tree` — Antithesis converged on our branching model
from the opposite extreme, then found one thing we cannot express.** Verbatim: "The
points in execution history where the guest ingests input from the Antithesis platform
become possible branch points for future execution. Consequently, the external view of
the exploration of a system is an input tree." And: "We are not replaying every single
execution path from the beginning – we would not waste your time like that!" — i.e.
snapshots as a fast path behind a prefix-shaped abstraction, which is precisely where C2
permits them and where our roadmap already puts them. The thing we cannot express is
their later addition: inputs with an **injection time**, pushed by interrupt rather than
consumed when the program asks. Every intervention we have
(`engine.rs::apply_intervention`) is reachable only from `on_call`/`on_decide`. Our
branch points are exactly the moments the program called us, and we do not say so.

**`2026-08-21-a-simulator-per-dependency` — the unserved problem, named.** DST's
reproduction artifact is a seed of a few bytes; ours is a recording of megabytes. The
trade is not size, it is validity. TigerBeetle's continuous fuzzing orchestrator stores
seeds **per commit** and evicts stale commits — a seed reproduces a run only against an
identical binary. And a seed only covers what someone wrote a simulator for: RisingWave,
who *wrote madsim*, report that they maintain exactly one connector simulator (Kafka)
because "it is costly and less rewarding to develop simulators for each of them", and
their published escape plan was Hermit, which is now dormant. The row nobody occupies is
ours: a program you did not write, talking to dependencies nobody will simulate, re-run
faithfully from the middle with one thing different.

**`2026-08-21-replay-safe-change-taxonomy` — Temporal says the dominant divergence cause
is code change, not capture.** Their docs name two causes of a non-determinism error and
list code changes first. Around it they publish a taxonomy of which edits are
replay-safe (arguments, timeouts, timer durations) and which are not (identity,
ordering). Our `actions_agree` is stricter than theirs on exactly the axis they had to
loosen: we compare `Call.args` and `Decide.options` for equality. For us that is
probably correct — a divergence caused by a code change is our *output*, not our failure
— but it is inherited rather than argued, and we have no answer to the first question a
user will ask: "I changed my agent, will my recordings still replay?"

## Looked at, not pursued

- **TigerBeetle's `canary` fuzzer** (`src/fuzz_tests.zig`: `if (seed % 100 == 0)
  std.process.exit(1)`, run on every commit and every PR) — a deliberately failing test
  that proves the failure-*reporting* pipeline is alive. We already have the library-level
  version: `tests/tolerance_slice.rs::an_undeclared_clock_makes_every_replay_diverge`
  asserts a divergence is detected. The delta is only that ours does not exercise the CLI
  exit path. Noted in the verify card; not worth its own recommendation.
- **`noidroid replay` as a CI regression test**, the analogue of Temporal's
  `WorkflowReplayer`. Already works: `cmd_replay` returns `ExitCode::from(1)` on
  divergence. Mechanically present; only the framing is missing, and that is covered by
  the change-taxonomy recommendation.
- **BUGGIFY** (FoundationDB's declared fault-injection sites, reimplemented in madsim's
  `GlobalRng::buggify`) — declared points where the simulator may inject a fault with some
  probability. We have the same idea in a stricter form: named `Failure` injections
  applied deliberately at a chosen step, not probabilistically. Probabilistic injection
  needs many cheap runs, which is DST's economics, not ours.
- **Trajectory shrinking / minimal reproducers** (sled's guide: drop initial requests
  until a minimal set still breaks the invariant). Structurally unavailable to us:
  `engine.rs::key()` is `format!("{}:{kind}:{target}", self.index)`, so removing a step
  renumbers every key after it. Recorded in the simulator-per-dependency card as the
  thing that would have to change if "smallest reproducer" ever became a goal.
- **turmoil** (tokio-rs) — opened the README only, confirmed it is the same shape as
  madsim with a narrower scope (network simulation for tokio). Nothing madsim did not
  already tell us; not read deeply, and I am not claiming otherwise.
- **Antithesis's guidance/search component** — the thing I most wanted. The post says
  outright it is withheld; the docs page says only "it uses RL". Recorded as a watch
  trigger on the Antithesis landscape entry.

## Negative findings

Four, and the first two are the valuable ones.

1. **DST cannot be applied to a system that already exists.** sled's guide makes it step
   one: "write your code in a way that can be deterministically tested on top of a
   simulator." FoundationDB wrote a programming language. madsim requires replacing five
   crates and force-patching five more, under `RUSTFLAGS="--cfg madsim"`. This is a
   warning against ever proposing DST for us, and an opportunity: it is the precise
   reason a boundary recorder exists.

2. **Every adopter stops writing simulators.** RisingWave got to one. Their stated hope
   was a general language-agnostic deterministic runtime (Hermit) that has since gone
   dormant. Unmet demand, confirmed by someone who needed it and said so in public before
   the answer died.

3. **Entropy-source interposition does not work on Linux.** madsim overrides `getrandom`
   as an `extern "C"` symbol; their own determinism test carries
   `#[cfg_attr(target_os = "linux", ignore)]` with the note that the Rust `rand` crate
   reaches `SYS_getrandom` without passing through the symbol. Their shipped answer is a
   `[patch.crates-io]` fork. Warning to us: any seeding we do is language-level and
   partial, and must be reported as such in the same sentence that reports it.

4. **The determinism ceiling is hardware, twice over.** Beyond the known `rdrand` result,
   Antithesis reports that the Intel PMC instructions-retired counter miscounts about one
   instruction in 10^12 even in precision mode, and that APIC interrupt delivery has
   variable, unknowable latency. They needed a custom kernel logger and ~50 GiB of trace
   per 20-minute run to find their own nondeterminisms. C1 strengthened.

## What we now know that we did not

- **A branching system must have the engine issue the seed.** Client-minted seeds
  (Minari's shape, and what the autoseed card proposed) break the "one thing is
  different" invariant at every branch.
- **The seed has a home that costs no format break.** `Action::Genesis`, `default` on
  read and skipped on write — the rule `model.rs:499` states and `grip` already used.
- **Our branching representation is what the best-resourced alternative also chose.** Not
  a compromise. Citable, verbatim, from Antithesis.
- **Nobody determinises parallelism.** Six independent projects — FoundationDB,
  TigerBeetle, Hermit, `stillness`, madsim, Antithesis — all eliminate it (single thread,
  serialised threads, threading disabled, `set_allow_system_thread(false)`, one VM pinned
  per core). Our "sequential programs only" is the field's unanimous answer, and #33's
  refusal of async surfaces should be read against that table: the credible options are
  "serialise" or "refuse", not "determinise".
- **A seed is a reproduction handle only against an identical binary.** TigerBeetle's CFO
  stores them per commit and evicts stale ones. Ours are valid across code changes, which
  is what makes `bisect` and `Replay { live }` possible at all.
- **Observability must not share the program's entropy stream.** FoundationDB ships
  `debugRandom()` for this. Our Python client currently uses neither `random` nor `uuid`
  (checked), so the rule is free to adopt now and expensive later.
- **We ship a documented footgun with no documentation.** `Response.delivery` lets a
  program see it is being replayed. Temporal ships the same affordance
  (`workflow.unsafe.is_replaying`) with an explicit warning never to branch on it.

## Still unknown

Stated plainly, including what the commission asked for and I did not reach.

- **Shadow** (the syscall-interposing discrete-event network simulator) — **not opened.**
  It is the one system in the commission that sits between DST and interposition, and its
  determinism caveats are the interesting part. Highest-value single leftover.
- **Deterministic-OS work** (dOS, Determinator, DMP, CoreDet, Dthreads) — **not opened.**
  I expect it to be confirmatory of the parallelism table rather than new, but that is a
  prediction, not a finding.
- **loom and shuttle** (Rust concurrency model-checkers) — not opened. shuttle's
  serialised failing *schedule* as a compact replayable artifact is the one idea there
  that might not be a duplicate of the seed finding.
- **Resonate** — not opened; expected to be a smaller Temporal.
- **How Antithesis chooses which branches to explore.** Deliberately unpublished. This is
  the direct input to roadmap item 4 (guided multi-branch exploration) and we have no
  source for it from anyone.
- **Whether `Decide.options` equality is a localisation problem in practice.** The
  experiment is specified in the change-taxonomy card and takes half a day.
- **Whether the seed belongs on `Action::Genesis` or as a declared environment fact**
  under the shipped environment model. I did not work through `env.rs`.

# Recommended Actions

### 1. Build the engine-issued seed: the engine mints it, the genesis records it, the client applies it

The engine mints a `u64` at genesis in `Mode::Record`, stores it on
`Action::Genesis { command, seed: Option<u64> }` (`default` on read, skipped on write —
no `STEP_VERSION` bump, per `model.rs:499` and the `grip` precedent), returns it in the
`Hello` reply via a new optional `Response.seed` field, and serves the recorded value in
`Mode::Replay` and `Mode::Branch`. The `sitecustomize` bootstrap seeds `random` and
`numpy.random` from it and reports which sources it seeded. Add the rule that no noidroid
client code may draw from the seeded generators.

**Why now:** the spike has been open since 2026-08-19 and was blocked on a design
question the card stated explicitly — where the seed lives. Two production systems answer
it, and the answer rules out the shape the card proposed. Also: every RL and rollout
recommendation from the 2026-08-21 scan assumes a trajectory can carry a seed, and it
cannot.

**Impact:** 3 — converts "the clock and randomness are not captured" into "randomness is
seeded and the seed is content on the trajectory; the clock is not", and makes a branch
honestly differ in one thing rather than two.
**Relevance:** 3 — reconstruction fidelity and branching, directly.
**Feasibility:** 3 — additive on the wire, additive in the format by a rule the codebase
already states, contained in `engine.rs` plus one client file.
**Novelty:** 3 — we have no seed anywhere; grep confirms it again this run.
**Score:** 81

**Cost:** one PR. The risk that blows it up: `env.rs` may be the right home instead of
`Action::Genesis`, and nobody has checked what a declared environment does with a seed.
Half a day of design before writing code.

**What we would learn:** does seeding stay fail-loud? The acceptance test is that
`uuid.uuid4()` and `os.urandom` **still** diverge loudly and still localise to the step.
If seeding ever turns a divergence into a silent wrong value, the answer is no, #30's
fail-open objection applies, and both seed cards become IGNORE. That is a real possible
"no".

**Touches:** `crates/noidroid-core/src/model.rs` (`Action::Genesis`),
`crates/noidroid-core/src/proto.rs` (`Response`), `crates/noidroid-core/src/engine.rs`
(`run`, the hello path), `clients/python/noidroid/_bootstrap`.
**Evidence:** `2026-08-21-engine-issued-seed`, `2026-08-19-autoseed-and-record` (updated).

---

### 2. Run the five-edit experiment and publish "what you can change and still replay"

Record a trajectory from `examples/reference/agent.py`, then make five edits one at a
time — add a tool to a `Decide` option set, rename a tool, reorder two independent calls,
change a prompt string inside a call's args, add a call — and record for each which
`DivergenceKind` fires, at which index, and whether the report points at the edit.
Publish the result as a table. Separately, add Temporal's `is_replaying` warning to the
client and protocol docs, or stop sending `delivery` to the program at all.

**Why now:** Temporal, running our architecture at the largest scale anyone has,
reports code change as the *first* cause of replay failure and answers it with a
published taxonomy. We have the same matching predicate, a stricter one, and no
taxonomy. This is also the cheapest possible test of whether our divergence localisation
survives ordinary editing — which is the pitch (C6).

**Impact:** 2 — no new capability, but it makes the central capability trustworthy across
code edits and answers the first question a user asks.
**Relevance:** 3 — replay and divergence localisation.
**Feasibility:** 3 — no production code for the experiment; a docs PR after.
**Novelty:** 2 — a named improvement on something we already do.
**Score:** 36

**Cost:** half a day for the experiment, a short docs PR after. Risk: if the experiment
finds that a single tool addition diverges far from the edit, this stops being
documentation and becomes an engine issue, which is a bigger piece of work — and worth
knowing.

**What we would learn:** is `actions_agree`'s equality on `Call.args` and
`Decide.options` the strictness the product needs, or does it destroy localisation for
ordinary edits? A clean result ("every edit diverges at the edit") closes it as docs.

**Touches:** `crates/noidroid-core/src/engine.rs` (`actions_agree`,
`describe_mismatch`) as the subject, `examples/reference/agent.py` as the fixture,
`README.md`/`docs/` as the output, `clients/python/noidroid/__init__.py` and
`crates/noidroid-core/src/proto.rs` for the `delivery` warning.
**Evidence:** `2026-08-21-replay-safe-change-taxonomy`.

---

### 3. State the branch-point boundary in the README: interventions apply where the program asked

One sentence. Branching and fault injection operate at recorded interaction points; an
event the program never asked about cannot be injected.

**Why now:** Antithesis started with a pure input tree, found it insufficient, and added
time-indexed injection. That is direct evidence that the limit is real rather than
theoretical, and it names it in someone else's words before a user hits it in ours. This
project's own tie-breaker puts "converts a silent gap into a stated one" first.

**Impact:** 2 — no capability change; removes an unstated limit, which this project
treats as the thing that matters most.
**Relevance:** 3 — branching and counterfactual exploration.
**Feasibility:** 3 — a documentation sentence.
**Novelty:** 2 — we knew the mechanism; we did not know it was a named boundary others
crossed deliberately.
**Score:** 36

**Cost:** minutes. No risk.
**What we would learn:** nothing — this is not an experiment. It is included because it
is nearly free and because leaving it out is the failure mode `docs/direction.md` names.
**Touches:** `README.md`, possibly `docs/direction.md`.
**Evidence:** `2026-08-21-input-tree-not-state-tree`.

---

### Carried and strengthened, not re-ranked

`2026-08-19-verify-by-double-execution` (open, score 81) gets a third independent
instance and a ready-made acceptance test. madsim's `Runtime::check_determinism` runs the
same future twice on one seed, logging one byte per entropy draw in run one and comparing
positionally in run two, panicking with `"non-determinism detected at {time:?}"`. Its
`should_panic` doctest — a future that reads `/dev/urandom` and sleeps for that many
nanoseconds — is exactly the test case `noidroid run --verify` should ship with. No
change to the recommendation or its rank; the card is updated.

## Explicitly not recommended

- **Owning the scheduler, the async runtime, or a hypervisor.** The evidence says this is
  the only way to get DST's guarantees, and it also says what it costs: rewrite the
  system under test, fork its dependency graph, hand-write a simulator per dependency,
  and still hit a hardware ceiling. RisingWave, who built the runtime, stopped at one
  connector simulator. C1 holds and is now evidenced from three directions rather than
  asserted. I am saying this plainly rather than filtering it, as instructed: there is no
  partial version of DST that buys anything without the rewrite.
- **Temporal-style `GetVersion` / patching.** Its purpose is to let a replay *survive* a
  code change. Our product is telling you where the code change first mattered. Adopting
  it would be adopting a mechanism designed to suppress our output. It is also on its
  third iteration at Temporal, which is what a hard problem looks like.
- **Time-indexed / unrequested-event injection.** Needs preemption, needs the runtime.
  No workload is asking for it; for pull-shaped agent workloads the gap is small.
- **Entropy-source interposition** (`LD_PRELOAD`/symbol override on `getrandom`). madsim
  tried it, it does not work on Linux, and their fix was to fork the dependency. Do not
  spend a day rediscovering this.
- **Treating a seed as a substitute for a recording.** A seed is valid only against the
  binary that produced it. Any design that starts storing seeds *instead of* effects
  reintroduces a dependency we deliberately do not have.
- **Probabilistic fault injection (BUGGIFY).** It needs thousands of cheap runs to pay
  off. Our runs are expensive and deliberate; named injection at a chosen step is the
  right shape for us.
- **Trajectory minimisation / shrinking.** `key()` is positional, so removing a step
  renumbers everything downstream. If a smallest-reproducer feature is ever wanted, the
  positional key is the thing to reopen — not the shrinking algorithm.
- **A CLI-level canary test.** TigerBeetle's canary is excellent, but
  `tolerance_slice.rs::an_undeclared_clock_makes_every_replay_diverge` already asserts
  the detector fires, and `cmd_replay` already exits 1. The remaining delta is not worth
  a test.
