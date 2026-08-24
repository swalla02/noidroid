---
date: 2026-08-21
cadence: open
question: "Where is the next capability gap in computer use for agents, and what would Paranoid Android have to build to be useful for open-source RL post-training on agent trajectories?"
cards_created:
  - 2026-08-21-unverified-fork-in-branching-rl
  - 2026-08-21-rollout-graph-already-exists
  - 2026-08-21-reproducibility-bought-by-mocking-the-world
  - 2026-08-21-reward-computed-over-an-unaddressed-state
cards_updated:
  - 2026-08-19-kernel-enforced-capture-boundary
landscape_created:
  - verifiers-prime-intellect
  - agentenv-kvcache
  - shepherd
  - openenv
---

# Scan: computer-use capability gaps, and earning a place in RL post-training

> **Framing.** The owner has re-timed the RL direction (C8's own clause: "Re-time it, do
> not re-argue it"). Nothing below leans on C9. C1, C2, C3 and C4 still bind and every
> recommendation here is compatible with all four — there is no similarity score, no
> threshold and no fuzzy match anywhere in this report. Where a sub-direction is a bad
> bet I name the sub-direction and the evidence, not a constraint number.

## In one paragraph

I went at both questions through primary sources — two RL algorithm papers read in full,
the `verifiers` v1 and `verl` source read from `main`, the OpenEnv environment-spec RFC,
the OSWorld and WebArena issue trackers, and the Shepherd repository. **The answer to Q2
is not a format and not a dataset — it is a verified fork.** The most efficient thing
happening in agentic RL right now is branching a rollout instead of resampling it, and it
splits cleanly in two: Tree-GRPO re-executes the prefix and is therefore confined to
stateless retrieval tools, while Branching Policy Optimization forks the sandbox itself
and states the requirement as *Assumption 1 (Snapshot fidelity)* with **no post-restoration
check anywhere in its algorithm**. Kimi's AgentENV is industrialising the same unverified
fork at 50 ms and 16 children per node, and its README never says what a fork loses.
`Mode::Branch` plus `state_root` plus hash equality is exactly the missing third option,
and it is already built. The **negative** half is just as important: I expected to find
that our trajectory model was a better rollout store, and it is not — `verifiers`'
`MessageNode` graph is already parent-linked, prefix-sharing and token-exact, and building
a rollout format would be building a worse one. What their format does not have, by an
explicit `state: ... exclude=True` and an explicit line in their `replay` docstring, is any
address for the world. That one field is the whole opportunity. On Q1, the computer-use
field's answer to reproducibility is to **replace the world with a mock** — OSWorld 2.0
ships hosted mocked websites pinned per release after fifteen months and 300+ issues of
real-web drift, and its published leaderboard number is one run with, in the maintainer's
own words, no variance estimate. Our browser adapter's record-and-re-serve approach is the
unoccupied third option there too, and I do not yet know how often it actually matches.

## What survived

**`2026-08-21-unverified-fork-in-branching-rl` — PROTOTYPE. The headline.**
Branching Policy Optimization (arXiv 2607.14171) snapshots the sandbox at high-entropy
steps, forks K siblings, and computes each action's advantage against its siblings'
returns. It states *Assumption 1: for every state `s` on an on-policy trajectory,
`rest(snap(s))` produces a state with identical transition distribution to `s`*, justified
by "the resumability of typical sandboxes (Docker overlayfs, CRIU, Python interpreter
pickling, browser session export)". Algorithm 1 contains no digest, no comparison, no
validation. When that assumption fails nothing crashes — the siblings are no longer
comparable, the advantage is biased, the loss still goes down, and credit was assigned in a
world that was quietly not the one the action was taken in. That is this project's stated
worst failure mode expressed as a gradient. Tree-GRPO (arXiv 2509.21240) avoids the
problem by re-executing tool calls, which is sound only because every environment it
evaluates on is a stateless retriever or a search API — a restriction the paper never
states. `Mode::Branch { at, .. }` in `engine.rs` re-executes the prefix under a
recorded-input oracle *and* checks the re-derived `state_root`, and a fork point in a world
we cannot verify is already labelled by `checkpoint.rs` rather than trusted. Nobody else
is doing the third thing.

**`2026-08-21-rollout-graph-already-exists` — WATCH, and it is the run's most useful
negative.** `verifiers/v1/graph.py`: "A rollout is a graph of `MessageNode`s — one per
distinct message, each linked to its predecessor... branches (compaction, subagents) are
simply multiple leaves, so branching falls out of the walk. Each node stores only the
tokens it *adds*." That is our `Step { parent, ... }` argument, made independently, and
theirs is better on the axis they care about. Meanwhile `verl`'s persisted rollout is
`tokenizer.batch_decode(..., skip_special_tokens=True)` of the prompt and the response,
plus a ground truth and a float — a detokenised transcript, which our own README calls a
bad basis for attribution. The gap in both is the same and it is stated by them:
`state: StateT = Field(default_factory=State, exclude=True)`, and
"Runtime-requiring signals don't run offline, so a replay carries offline scores only."

**`2026-08-21-reward-computed-over-an-unaddressed-state` — PROTOTYPE, and the bridge to
#52/#53.** A verifiable reward is a checker run against a final state that nobody names.
So you cannot re-check a reward without re-running the episode, cannot distinguish a buggy
checker from a wrong state from a tampered checker, and — this is the new part — cannot
distinguish a *measured* reward from an *echoed* one. AgentRewardBench finds ~30% of
trajectories that LLM judges called successful were failures on expert review, while
rule-based checkers reject valid solutions; the reward-hacking literature reaches for a
78.4%-precision classifier to detect what is mechanically a file that changed. And
OpenEnv's RFC 002 Decision 2 makes it structural rather than accidental: environments may
"use internal state and context **not visible to clients** for reward computation". We
snapshot the workspace after every step; `noidroid checkout-tree` already puts any step's
tree back on disk. After #52 the run additionally carries `Report::served` and
`Situation::achieved()`, so a score can be printed as measured-or-recomputed with a
mechanism behind it. That is what "verified rollout" can honestly mean for us.

**`2026-08-21-reproducibility-bought-by-mocking-the-world` — INVESTIGATE.** OSWorld-Verified
absorbed 300+ issues over fifteen months, and stripped of the task-ambiguity row every
category is world drift: anti-crawling, geo-blocking, DOM structure changes, URL parameter
changes, load-time sensitivity. Their conclusion — "providing reliable rewards consumes
more human resources than we imagined". OSWorld 2.0's fix is release-pinned **hosted mocked
websites**; CUA-Gym synthesises mock web applications for the same reason; WebArena was
mocked from the start. In OSWorld issue #382 the maintainer says the published
Claude-4-Sonnet score "corresponds to one full evaluation run" and that he could not afford
repeats, so there is no variance estimate on the field's flagship computer-use metric.
WebArena issue #206 asks for a faster website reset and has zero replies. Our browser
adapter records the real page's responses and re-serves them, which is the third option
and is unoccupied — but I have not measured how often a real page re-drives to an exact
digest, and until I have, that is a hypothesis and not a claim.

## Looked at, not pursued

- **Schedule-level shared-prefix reuse (arXiv 2606.01143)** — real mechanism (prefix K/V
  computed once, gradients accumulated across suffixes) but strictly inside the
  transformer. Its limitations section discusses MoE routing; there is no mention of
  environment state, rollback or side effects. Nothing to take.
- **The Rollout Infrastructure Tax (arXiv 2607.01415)** — measures 110× variation in
  cold-start latency and a 1.8× spread in projected worker-hours across four execution
  substrates. Useful for sizing our restore cost against, no mechanism to take. Abstract
  and metadata only; I did not read the body.
- **verl's `DataProto` / parquet training path** — the tokens, masks and logprobs live in
  memory and are consumed by the trainer. That is the trainer's business, we should not
  touch it, and there is nothing there we could do better.
- **CUA-Gym (arXiv 2605.25624)** — generator/discriminator loop producing 32,112 RLVR
  tuples over 110 environments, with mock web applications underneath. Folded into the
  mocking card as a third instance; abstract only.
- **AgentRewardBench (arXiv 2504.08942)** and the reward-hacking detection literature —
  folded into the reward card as evidence that both available oracles are miscalibrated.
- **E2B** — surfaced twice (AgentENV is API-compatible with it; Shepherd is sponsored by
  it) and not opened. It is the sandbox interface the ecosystem has already standardised
  on, which makes it the thing an adapter would target, not a thing to study.

## Negative findings

**Opportunities.**

1. *The fork is never verified.* BPO assumes it; AgentENV documents what a snapshot
   captures and never what it loses; Shepherd rests on a stated weak-coupling assumption.
   Three unrelated projects, one hole, and it is the hole our engine was built around.
2. *Tree rollouts are silently confined to stateless tools.* Tree-GRPO's restriction is a
   property of its evaluation set, not a stated limitation, so it will propagate as a
   general technique into settings where it is unsound.
3. *There is no cheap reset for a stateful web environment.* WebArena #206, unanswered.
   RL work on it reports four parallel sessions and manual server restarts.
4. *A reward has no address.* Structural in OpenEnv by Decision 2, absent from `verifiers`
   by `exclude=True`, and reduced to text-plus-a-float in `verl`.

**Warnings.**

5. *Our data structure is not the differentiator.* `verifiers` already ships the
   parent-linked, prefix-sharing, incremental-content trajectory graph, with token
   alignment we do not have and do not want. Any pitch built on "we store rollouts better"
   is dead on arrival.
6. *A direct competitor exists and is ahead of us on reach.* Shepherd — MIT, ~2.4k stars,
   alpha v0.3.0, Stanford-affiliated authors — describes itself as "a runtime substrate
   that turns an agent's execution into a reversible, Git-like trace, so meta-agents can
   observe, fork, replay, and revert any run", and it has **shipped kernel-enforced grants
   on macOS Seatbelt and Linux Landlock**, which is our open INVESTIGATE card. It is an
   agent framework, so it asks for your program rewritten as bodyless `@task` functions,
   and its rewind is a CoW checkout resting on an explicitly stated weak-coupling
   assumption. On reach they are ahead; on evidence we are ahead; evidence is the only
   axis we said we compete on.
7. *The emerging environment standard has no reproducibility primitive.* OpenEnv's
   baseline is `reset` / `step` / `state`; `seed()` is deferred to a follow-up RFC, and
   snapshot/suspend/resume are explicitly orchestration-only and outside the core protocol.
8. *"Reproducible computer-use evaluation" currently means a pinned mock plus one run.*
   Anyone in this space who hears us say "reproducible episode" will hear something much
   weaker than we mean, and we will have to spend a sentence disambiguating it every time.

## What we now know that we did not

1. **Branching rollouts is the live technique in agentic RL**, with a measured +5.8 on
   SWE-bench Verified and 38.7% fewer gradient steps in BPO, and its correctness rests on
   an assumption its own algorithm never checks.
2. **Snapshot cost is public and it is the number we will be measured against**: 1,920 ms
   for a Docker overlayfs snapshot at a SWE-bench branch point; under 50 ms to resume a
   Firecracker microVM in AgentENV; 110× spread across substrates in the infrastructure-tax
   paper. We have never measured our own restore.
3. **The rollout store is solved on the token axis and empty on the world axis**, in the
   source, in both leading stacks, with the reason written in their own docstrings.
4. **`state()` is a standardised endpoint on every OpenEnv-conformant environment**, which
   is a ready-made hook for `Situation::report` / `session.observe(of=..., state=...)` and
   would give a whole class of environments `witnessed` grip with one adapter.
5. **OpenEnv guarantees a reward may depend on state the client cannot see**, by design
   decision, so external reward auditability is foreclosed at the standard level unless
   something else holds the state.
6. **The computer-use benchmark field spent fifteen months proving that real-web episodes
   are not reproducible**, and concluded by replacing the web.
7. **The flagship OSWorld score is a single run**, confirmed by the maintainer in
   issue #382, because a full evaluation is too expensive to repeat.
8. **Kernel-enforced sandboxing on Seatbelt and Landlock is shipped, by a competitor**,
   with the scope limits documented (privileged container on Linux, whole-profile grants,
   sub-root grants deferred, Windows refused). Our Landlock card is updated accordingly:
   the spike is smaller than we scoped it.
9. **`--proxy` is the industry-standard architecture**: `verifiers`' interception server
   sits between an unmodified harness (Codex, Claude Code) and the provider, speaking that
   harness's own dialect, building the trace live, and rewriting tool responses. We built
   the same thing in `af81680` for a different reason.

## Still unknown

- **Our own checkpoint restore cost.** Nothing in this run measured it, and every Q2
  recommendation is conditioned on it. This is the single most decision-relevant unknown
  and it is a day's work.
- **How often a real-web browser re-drive reproduces the page digest exactly.** The claim
  in the mocking card is a hypothesis until this number exists.
- **The rest of the trainer landscape.** TRL, OpenRLHF, SkyRL, AReaL, slime, ROLL, rLLM
  and Tinker were not opened. The "everyone persists a transcript plus a scalar"
  generalisation rests on two data points and should be treated as provisional.
- **Shepherd's source.** Everything in that landscape entry is their README, their concepts
  docs and a fetched paper summary. Whether the rewind is genuinely byte-identical, and
  whether the enforcement is what it claims, are unverified — and
  `2026-08-19-silent-best-effort-sandboxing` is the reason not to assume.
- **AgentENV's source.** Same discount. The absence of a stated fork residue is directly
  observable in the README; the timings are not verified.
- **Computer-use trajectory datasets** — OpenCUA/AgentNet, AgentTrek, OS-Genesis,
  GUI-Odyssey, AndroidControl. Not reached at all. This was an explicit sub-question of
  Q1 and it is unanswered.
- **WindowsAgentArena and the OS-level successors.** Its issue tracker returned nothing on
  any of my queries and I did not read the repository.
- **Deterministic simulation testing** (Antithesis, TigerBeetle, FoundationDB). Carried
  over untouched for the **third** consecutive run. It is the most likely source of ideas
  for branching and state-space exploration, and it keeps losing to whatever the run's
  headline question is. It should be its own targeted scan, not a rider on an open one.

# Recommended Actions

Ranked by Impact × Relevance × Feasibility × Novelty. Three items tie at 36; the
tie-breaks are stated.

### 1. Emit a fork-point evidence record, and prove a group of sibling branches is verified

**What to build.** A machine-readable record, one line per fork point, produced by
branching a recorded trajectory at each of N indices:

```
{ fork_index, step_address, recorded_state_root, rederived_state_root,
  evidence, grounding, reach, served, divergence }
```

`evidence` / `grounding` / `reach` come straight from `checkpoint::at()`; `served` from
`Report::served` as landed in #52; `divergence` is `None` or the existing `Divergence`.
No reward, no advantage, no trainer integration, no group statistics — the artefact is the
evidence, and it is the trainer's job to drop or down-weight siblings whose fork point did
not verify. Surface it as `noidroid branch --at <a,b,c> --json` or a sibling subcommand;
the loop over `at` values is CLI-level.

**Step 0, which can kill it.** Measure wall-clock restore-and-branch at step k as a
function of k, on the reference environment and on the browser example. The numbers to
beat are public: 1,920 ms per BPO snapshot, sub-50 ms per AgentENV resume. If our restore
is tens of seconds at k=25, verified forking is not a rollout-collection primitive and this
item becomes an offline-analysis tool instead of an RL one — which is still worth building,
but it is a different pitch and should be re-scoped before the code is written.

**Why now:** branching rollouts became the efficiency frontier in agentic RL this year and
its central assumption is unchecked and unmentioned by its own authors. The mechanism that
checks it is already in `engine.rs`; what is missing is a machine-readable way to say so.
And #52 just added the last piece — a run can now distinguish "the adapter checked" from
"nobody checked" — so the record can carry an honest per-fork evidence label rather than a
uniform badge.

**Impact:** 3 — it is a claim no other system in this space can currently make, and it
converts a silent statistical corruption into a named per-fork-point fact.
**Relevance:** 3 — branching, checkpoints, reconstruction fidelity and capture honesty at
once. Dead centre.
**Feasibility:** 2 — the branch machinery exists and nothing hashed moves, but N branches
from one trajectory, a stable record shape, and the honest handling of a fork point whose
world is `opaque` is more than one PR. And it inherits C1: the harness has to be a
noidroid client, which is a real adoption cost and is not free.
**Novelty:** 3 — nothing in the codebase emits per-fork-point evidence; nothing outside it
emits any.
**Score:** 54

**Cost:** a day for step 0, then a release-sized piece of work. The risk that blows it up
is step 0's answer.
**What we would learn:** on a well-behaved environment, how many fork points fail to
re-derive their recorded `state_root`. **Zero is a real and publishable result** — it would
say Assumption 1 holds in practice and that our verification is insurance rather than a
fix, which should lower this card's priority permanently.
**Touches:** `crates/noidroid-core/src/engine.rs` (`Mode::Branch`, `Report`),
`checkpoint.rs` (`at`, `evidence_over`), `env.rs` (`Situation::achieved`, `served`),
`crates/noidroid-cli/src/main.rs`.
**Evidence:** `2026-08-21-unverified-fork-in-branching-rl`,
`2026-08-19-snapshot-omits-derived-state`, `2026-08-19-unverified-world-redrive`.

---

### 2. Settle #53 question 1 using "recomputed vs measured reward" as the forcing case

**What to build.** Decide, once, whether a pure `noidroid replay` reports `opaque` — and
decide it against a concrete downstream consumer rather than in the abstract. The RL case
supplies one: a reward recomputed offline against a materialised `state_root` is genuinely
not a measurement, and the useful output is neither silence nor a blanket downgrade but the
sentence *"this score was recomputed from the recording, not measured against the world,
and here is the world that was not driven."* Then write the answer into
`docs/environment-model.md` §7.1 and give run grip and trajectory grip **distinct names**
(#53 question 3), because after #52 they are different numbers wearing the same word.

**Why now:** #52 has landed and #53 is open with the questions still unanswered. Every
additional consumer of the run report — and the fork-point record in item 1 is the next one
— hardens whatever the current answer accidentally is. Deciding it after item 1 ships means
deciding it under a compatibility constraint.

**Impact:** 3 — it fixes what every reconstruction is allowed to claim, which is the
project's one sentence.
**Relevance:** 3 — capture honesty, directly.
**Feasibility:** 3 — an open issue, a decision, a small change on the unhashed delivery
axis, and named tests. No step bytes move.
**Novelty:** 2 — not a new capability; the RL framing is new information that bears on the
decision.
**Score:** 54

**Cost:** a design decision plus a PR. The risk is over-refusal — if every replay prints
`opaque`, the word stops carrying information and readers will learn to ignore it. That is
the same risk flagged on #52 and it is the criterion to hold the change to.
**What we would learn:** whether a reader given a replay report and a record report can
tell, unprompted, which numbers were measured. If they cannot, the naming is wrong.
**Touches:** `crates/noidroid-core/src/env.rs`, `engine.rs`,
`crates/noidroid-cli/src/main.rs`, `docs/environment-model.md` §7.1. Issue #53.
**Evidence:** `2026-08-21-reward-computed-over-an-unaddressed-state`,
`2026-08-19-unverified-world-redrive`.

---

### 3. Add the browser adapter's re-drive mute, and assert the report changes

**What to build.** The browser equivalent of the reference environment's `REFERENCE_MUTE`:
an environment variable that suppresses `Browser._reconstruct`'s re-drive, plus a test
asserting that with it set, the run report names the page as a world that was served rather
than driven. This is #53 question 5 and it is a handful of lines.

**Why now:** the browser adapter is our only real-world environment adapter and the
strongest instance of the pattern #52 fixed. Right now nothing proves that removing its
re-drive changes anything, which means the honesty guarantee is asserted rather than tested
in the one place it matters most.

**Impact:** 2 — converts an untested honesty claim into a tested one on the shipped adapter.
**Relevance:** 3 — capture honesty and the environment contract.
**Feasibility:** 3 — an env var and a test.
**Novelty:** 2 — a named improvement to a mechanism that exists.
**Score:** 36 — ranked above the other 36s on tie-break 1 (does it make a silent failure
loud?) and tie-break 4 (cheapest disproof).

**Cost:** part of a PR. The risk is that the browser's `_reconstruct` path is entangled
enough that muting it changes more than the re-drive, which would make the test prove the
wrong thing.
**What we would learn:** whether the #52 mechanism actually fires on the adapter it was
written for. If the report is byte-identical with and without the re-drive, #52 is
incomplete and we need to know that now rather than after item 1 depends on it.
**Touches:** `clients/python/noidroid/browser.py` (`_reconstruct`, `_report_world`),
`crates/noidroid-core/tests/`, issue #53.
**Evidence:** `2026-08-21-reproducibility-bought-by-mocking-the-world`, issue #53 Q5.

---

### 4. Ship `noidroid score`: re-run a checker against a step's state, offline

**What to build.** `noidroid score <trajectory> --at <step> -- <command>`: materialise that
step's `state_root` into a scratch directory via `tree::materialize_with`, run the command
there, capture exit status and stdout, and print
`(step_address, state_root, command, status, run_grip, served)` — writing nothing back into
the trajectory. It is a composition of `checkout-tree` and a subprocess; the value is that
the tuple becomes a citable object, and that it does the thing `verifiers replay` says in
its own docstring that it cannot: recompute a runtime-requiring signal without regenerating
the episode.

**Why now:** reward functions change constantly in RL post-training, and re-scoring today
means re-running every episode. We already hold the addressed state that makes re-scoring
free. This is the smallest artefact that makes `state_root` legible to somebody outside the
project.

**Impact:** 2 — no change to what reconstruction can claim; it exposes something we already
hold.
**Relevance:** 2 — CLI and reporting, serving provenance rather than fidelity.
**Feasibility:** 3 — one subcommand over existing machinery, single PR.
**Novelty:** 3 — a capability we lack and the two leading RL stacks explicitly lack.
**Score:** 36

**Cost:** one PR. The risk: it looks like an eval harness, which is on the do-not-build
list. The discipline that keeps it honest is that it stores nothing, judges nothing and
knows nothing about tasks — it materialises a tree and runs a command.
**What we would learn:** whether reward tampering shows up as a tree diff. Take a stored
trajectory, mutate a file the checker depends on, and check that `diff` names it. **If a
plausible tampering case does not appear as a tree diff, the tamper-detection claim in the
card is wrong and must be struck.**
**Touches:** `crates/noidroid-cli/src/main.rs`, `crates/noidroid-core/src/tree.rs`
(read-only), `engine.rs` for the grip line.
**Evidence:** `2026-08-21-reward-computed-over-an-unaddressed-state`,
`2026-08-21-rollout-graph-already-exists`.

---

### 5. Write an OpenEnv adapter: `state()` becomes a declared world

**What to build.** ~50 lines in `clients/python/`: wrap an OpenEnv `EnvClient` so
`step(action)` goes through `nd.call(...)` with a declared `EffectKind`, and after each step
call `nd.observe(of="openenv:<env_id>", state=client.state())`. That gives every conformant
OpenEnv environment `witnessed` grip and a comparable fingerprint with zero per-environment
work, because `state()` is in the standard's three-method baseline.

**Why now:** OpenEnv has moved to a nine-org steering committee with integrations shipping
in TRL, verl, TorchForge, SkyRL and `verifiers`. It is the one interface in this space that
looks like it will hold, and `state()` is a declared observation point that our environment
model already has a shape for. Doing this later means doing it against N framework
integrations instead of one spec.

**Impact:** 2 — gives a whole class of environments a grip they cannot get today; does not
change the core claim.
**Relevance:** 2 — environments and adapters.
**Feasibility:** 3 — the client surface already exists (`Session.observe`, `proto.rs`
`observe`); this is Python glue.
**Novelty:** 3 — we have no RL-environment adapter of any kind.
**Score:** 36 — ranked last of the 36s because it is adoption work (C10's parking is a
sequencing decision, and this is the sequencing decision) and because its value is
conditional on items 1 and 4 being worth integrating with.

**Cost:** a day or two, plus an example. The risk that blows it up: `state()` is specified
as "current episode state and metadata" with no content or stability contract, so an
environment that includes a timestamp or a request id makes every reconstruction diverge —
accurate and useless, exactly as `Session.observe`'s own docstring warns. If most real
OpenEnv environments return unstable state, this adapter cannot produce a usable
fingerprint and should be abandoned.
**What we would learn:** whether `state()` is stable enough to fingerprint across three or
four real environments from the ecosystem. A "no" is a genuinely useful finding and belongs
back in the OpenEnv landscape entry as a watch trigger.
**Touches:** `clients/python/noidroid/` (new module), `examples/`.
**Evidence:** `research/landscape/openenv.md`,
`2026-08-21-reward-computed-over-an-unaddressed-state`.

## Explicitly not recommended

**Do not build a rollout or trajectory dataset format for trainers.** This was the obvious
answer to Q2 and it is wrong. `verifiers`' `Trace` is versioned, parent-linked,
prefix-sharing, token-exact, renderer-aware, and maintained by the people who own the
trainer; `verl` consumes parquet and `DataProto` and dumps JSONL for inspection. Competing
there means building a worse version of a thing that already works, and it would drag
tokens, masks, logprobs and advantages into `model.rs` — a `STEP_VERSION` break in exchange
for losing. The seam is one field they do not have (a `state_root` digest, opaque to them),
not a format. Evidence: `2026-08-21-rollout-graph-already-exists`.

**Do not build a reward model, a judge, a rubric, or anything that scores a trajectory.**
`noidroid score` in item 4 runs somebody else's checker and stores nothing. The moment we
decide whether an episode succeeded, we are an eval harness, and AgentRewardBench is the
evidence that this is a hard, crowded, actively researched problem with no good answer —
~30% false-positive rate on LLM judges, systematic false negatives on rule-based checkers.
We have no advantage there and would inherit both error modes.

**Do not try to compete with microVM fork on restore latency.** AgentENV resumes a
Firecracker snapshot in under 50 ms and forks 16 children on one node. A deterministic
prefix replay will never be that, and C2 is not a performance claim — it is an evidence
claim. If the answer to a user's problem is "restore fast and do not check", they should
use AgentENV, and we should say so. Our claim is the one they cannot make: the fork point
re-derived the recorded address, or here is where it stopped matching.

**Do not use the fact that Shepherd exists as a reason to become an agent framework.**
Shepherd is ahead of us on reach, on enforcement and on ergonomics precisely *because* it
owns the program: a task is a bodyless `@task` function in their type system. That is the
trade C1 already refused from the other direction, and taking it would cost us `--proxy`
and `--watch`, which record agents nobody rewrote. The right response to Shepherd is to be
the one with the verification, not the one with the nicer decorator.

**Do not treat a mocked benchmark environment as a target.** OSWorld 2.0's hosted mocked
websites and CUA-Gym's mock applications are already deterministic by construction; a
recording of a mock adds nothing that the mock does not have. Our value is confined to
episodes against a world that moves, which is exactly the part those projects gave up on.
Anywhere the requirement is "generate fresh diverse rollouts", a mock beats us outright and
we should not argue.

**Do not add a divergence threshold, a sibling-similarity score, or a fork-validity number
to make item 1 more forgiving.** BPO's Assumption 1 is stated distributionally
("identical transition distribution"), and the temptation when a fork point does not
re-derive will be to accept it as close enough. That is C4, it is the AV threshold from
`2026-08-19-log-replay-validity-modes` in new clothes, and the whole point of item 1 is
that we have an oracle and they do not. A fork point either re-derived its recorded address
or it did not.

**Do not chase computer-use benchmark leaderboards or build an OSWorld/WebArena runner.**
The benchmarks are crowded, expensive, maintained by well-resourced labs, and — per issue
#382 — not even self-reproducible. Being a better OSWorld is not available and would not be
worth having. The finding worth keeping from Q1 is the *mechanism* comparison: they mock,
we record, and nobody has measured which reproduces a real episode more faithfully. Item 3
plus the digest-match measurement in the mocking card is how we find out cheaply.
