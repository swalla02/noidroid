---
id: 2026-08-21-engine-issued-seed
title: The seed is an input delivered across the boundary, not a value the client picks
discovered: 2026-08-21
updated: 2026-08-21
categories: [clock and randomness control, deterministic replay, reproducibility, state reconstruction]
class: INFRASTRUCTURE
recommendation: PROTOTYPE
transferability: HIGH
novelty: MISSING
confidence: HIGH
touches: [proto, model, engine, clients]
---

## Discovery

Every mature deterministic-execution system that seeds a PRNG has the *orchestrator*
issue the seed and record it, rather than letting the program generate one and write it
down afterwards. Temporal ships `randomness_seed` inside the workflow activation that
starts a replay, and re-seeding mid-execution is itself a recorded event
(`UpdateRandomSeed`). FoundationDB goes further and splits randomness into three typed
generators — `deterministicRandom()`, `nondeterministicRandom()` and `debugRandom()` —
so that observability code cannot perturb the stream the program's behaviour depends on.

This is a different shape from the one `2026-08-19-autoseed-and-record` proposed after
reading Minari, where the *client* mints a seed and stores it with the episode. For a
branching system the difference is decisive.

## Source

Primary, read directly:

- `temporalio/sdk-python`, `temporalio/worker/_workflow_instance.py`
  <https://github.com/temporalio/sdk-python/blob/main/temporalio/worker/_workflow_instance.py>
  - line 165: `randomness_seed: int` — a field of the workflow *activation*, i.e. it
    arrives from the server as part of what starts the (re-)execution.
  - line 326–328: `self._random = random.Random(det.randomness_seed)`, and
    `self._current_seed` retained.
  - line 643–644, 1172–1180: `update_random_seed` is a first-class activation job.
    `_apply_update_random_seed` re-seeds and fires registered callbacks, so libraries
    holding their own generator can be told. **Re-seeding is an event in the history,
    not a side effect.**
  - line 1461, 1974, 1977: `workflow_random()`, `workflow_random_seed()`,
    `workflow_register_random_seed_callback()` — the seed is readable by the program.
- Temporal docs, `https://docs.temporal.io/develop/python/workflows/basics.md`
  (fetched as markdown): "Use `workflow.random()` to get a deterministic
  `random.Random` instance **seeded per Workflow Execution**. Never use
  `random.random()` or other `random` module functions directly." Same for
  `workflow.uuid4()`.
- Temporal docs, `https://docs.temporal.io/workflow-definition.md` lines 251–276:
  "all operations that do not purely mutate the Workflow Execution's state should occur
  through a Temporal SDK API."
- FoundationDB, `flow/include/flow/IRandom.h` lines 214–231
  <https://github.com/apple/foundationdb/blob/main/flow/include/flow/IRandom.h>:

  > `deterministicRandom()` — "This generator should only be used in contexts where the
  > choice to call it is deterministic."
  > `nondeterministicRandom()` — "cannot be manually seeded and may be called in
  > non-deterministic contexts."
  > `debugRandom()` — "returns a deterministic random number generator initialized with
  > the same seed … The main use-case is to generate deterministic random numbers
  > **without changing the determinism of the simulator**. This is useful for things
  > like generating random UIDs for debug transactions."

- madsim, `madsim/src/sim/rand.rs`
  <https://github.com/madsim-rs/madsim/blob/main/madsim/src/sim/rand.rs> — the
  counter-example: they tried to seize the OS entropy source by symbol interposition
  (`#[no_mangle] extern "C" fn getrandom`, lines 197+) and their own test carries
  `#[cfg_attr(target_os = "linux", ignore)]` with the note "Deterministic rand is only
  available on macOS. On linux, the call stack is `rand` -> `getrandom` ->
  `SYS_getrandom`, which is hard to intercept." Their shipped answer is in the README:
  a `[patch.crates-io]` fork of the `getrandom` crate.

## What is interesting

Three mechanisms, in increasing order of how much they change our plan.

**1. The seed travels in the thing that starts the execution.** Temporal's worker does
not choose a seed. It receives one in the activation, alongside the event history it is
about to replay. On replay the same activation carries the same seed, so the same
`random.Random` sequence is reconstructed. The seed is an *input*, indistinguishable in
kind from an activity result — it is served, not sampled.

**2. Re-seeding is an event, not a mutation.** `UpdateRandomSeed` exists because after a
reset or continue-as-new the seed must change, and a change that is not in the history
is a change that replay cannot reproduce. The general rule this encodes: *if a control
value can change during a run, the change has to be a recorded step or the run is not
reconstructible.*

**3. Observability must not draw from the stream the program draws from.** This is
`debugRandom()` and it is the subtlest of the three. A shared generator makes the
*number of draws* part of the program's state: add one `uuid4()` for a log line and
every subsequent value in the program shifts. FoundationDB's comment on
`deterministicRandom()` — "should only be used in contexts where the choice to call it
is deterministic" — is the same hazard from the other side: it is not enough that the
generator is seeded; the *reachability of each call site* must itself be deterministic.

**4. The negative result.** madsim wanted what our `--auto` path would want: seize
randomness for a program that did not ask. It does not work on Linux, because the Rust
`rand` crate reaches `SYS_getrandom` without going through the libc symbol they
override. The company whose product is determinism resolved this by forking the
dependency in `[patch.crates-io]` — i.e. by owning the dependency graph, which we
cannot ask of a user. Anything we do at the entropy source is therefore
language-level (`random.seed`, `numpy.random.seed`) and cannot be complete.

## Why it matters to Paranoid Android

`grep -rn "seed\|Seed" --include=*.rs crates/` returns nothing outside comments, and
`grep -rn seed clients/python/` returns nothing at all. Confirmed again this run: **we
neither issue nor record a seed.** `clients/python/noidroid/auto.py`'s own docstring
lists "time, randomness, and the filesystem" among what it does not capture.

The reason this matters more for us than for Minari is **branching**. A branch is a step
whose parent belongs to another trajectory (`model.rs`, `Step { parent, .. }`); the
whole promise is "the same execution with one thing different". If the *client* mints a
seed at process start, a branch re-executing the prefix mints a **new** seed, and the
prefix is no longer the same execution — two things differ, and one of them is
undeclared. Only an engine-issued seed, re-served from the trajectory, keeps a branch
honest.

Where it lands in our code, concretely:

- `proto.rs` — `Request::Hello` is the exact analogue of Temporal's activation: it is
  the handshake and it "commits the genesis step". Its reply is `Response::ack()`, which
  carries nothing. Every field of `Response` is
  `#[serde(skip_serializing_if = "Option::is_none")]`, so adding `seed: Option<u64>` is
  purely additive; existing clients ignore it.
- `model.rs` — `Action::Genesis { command: Vec<String> }` is where the seed has to live
  if it is to survive `bundle.rs` export. `StepNote` will not do: it is per-run and, as
  the autoseed card already noted, does not travel. The byte-pinning test's own
  docstring (`model.rs` line 499) sanctions the change: "a field that is `default` on
  read and skipped on write when absent leaves these bytes unchanged, and needs no
  version bump." `grip` is the shipped precedent — `STEP_VERSION` stayed at 1.
- `engine.rs` — `run()` mints the seed in `Mode::Record`, and reads it off the recorded
  genesis in `Mode::Replay` / `Mode::Branch`. Provenance `real`: the program genuinely
  ran with it. Delivery `executed` on record, `replayed` on reconstruct — which is
  exactly what the C3 two-axis split is for, and is the tidiest available demonstration
  that the split does real work.
- `clients/python/` — the `_bootstrap` `sitecustomize` path seeds `random` and, if
  importable, `numpy.random` from the value in the hello reply, and reports which
  sources it seeded in the same breath.

FoundationDB's `debugRandom()` converts into a hard rule for that client patch: **the
noidroid client must never draw from the seeded generator.** I checked — today
`clients/python/noidroid/*.py` uses neither `random` nor `uuid`, so the rule is
currently free to adopt and would be expensive to adopt later.

Bears on: capture honesty, replay fidelity, branching, and the integration boundary.

## Transferability

HIGH. The mechanism is small, the placement question that the autoseed card left open is
answered by Temporal's design, and the on-disk-format objection is answered by the
`grip` precedent. Nothing here needs us to own a runtime: we are *issuing a control
value across a boundary the program already crosses*, which is the thing our
architecture is built to do.

The part that does not transfer is completeness. Temporal can say "never call
`random.random()` directly" because a workflow that does is simply broken; they have a
sandbox that detects it. We cannot say that, and must not imply it.

## Novelty

MISSING, and a **REFINEMENT of `2026-08-19-autoseed-and-record`** rather than a
duplicate of it. That card established *whether* to seed (yes, because seeding is
fail-loud where a clock freeze is fail-open). This one establishes *who issues the seed
and where it lives*, which that card explicitly left open ("The hard question is not the
seeding, it is where it *lives* in our model. Options, all with a problem"). The answer
from two independent production systems is: the orchestrator issues it, it is content,
and re-issuing it is an event.

## Limitations and negative signal

- **Partial seeding is still partial.** madsim's Linux failure is the strongest evidence
  yet that `os.urandom`, `secrets` and C-extension generators cannot be caught at the
  entropy source without owning the dependency graph. The autoseed card's warning about
  false coverage stands and gets sharper: whatever we seed must be *named in the run
  report*, and everything else must remain loudly divergent.
- **A seeded PRNG changes the sampling story.** Two records of the same command would
  get different seeds (good, they are different executions) but a `--verify` double
  execution wants the *same* seed. Those are different needs and the design has to say
  which is which. Temporal's answer is that the orchestrator decides; ours would be that
  `Mode::Record` mints and every reconstructing mode serves.
- **The call-site-reachability trap.** FoundationDB's warning means a seeded stream is
  only reproducible if the *decision to draw* is itself reproducible. A program that
  draws inside `if time.time() % 2` is not saved by seeding. This is not a reason not to
  seed; it is a reason not to advertise seeding as determinism.
- **It does nothing for the proxy or browser capture paths**, where the program's
  randomness is on the far side of a boundary we already mediate.

## Recommendation

PROTOTYPE — build the engine-issued seed as specified below. The design question that
blocked the autoseed spike is now answered by primary sources, and the on-disk-format
risk is retired by the `grip` precedent.

## Proposed action

One PR, plus the spike from the autoseed card folded into it as the acceptance test.

1. `model.rs`: `Action::Genesis { command, #[serde(default, skip_serializing_if =
   "Option::is_none")] seed: Option<u64> }`. Assert `format_is_pinned` still passes and
   `STEP_VERSION` stays at 1.
2. `proto.rs`: add `seed: Option<u64>` to `Response`; `Response::ack()` keeps `None`,
   the hello path fills it.
3. `engine.rs`: mint in `Mode::Record` (from OS entropy, once, at genesis); on
   `Mode::Replay` and `Mode::Branch`, read it from the recorded genesis and serve the
   same value. A branch inherits its parent's seed by construction, because it shares
   the genesis.
4. `clients/python/noidroid/_bootstrap`: seed `random` and `numpy.random` when present;
   record which were seeded; print them in what `--auto` already prints about what it
   hooked.
5. Add to `CONTRIBUTING.md` or the client docstring the `debugRandom()` rule: **no
   noidroid client code may draw from the seeded generators.**

**How we would know it failed.** Three checks, and any of them failing kills it:
- A program calling `random.random()` outside a mediated call replays faithfully. If it
  does not, the seeding is not reaching the right generator.
- `uuid.uuid4()` / `os.urandom` **still diverge loudly and are still localised to the
  step**. If seeding converts either into a silent wrong value, stop — #30's fail-open
  objection then applies and this becomes an IGNORE.
- Branching at step k twice produces the same seed both times, and the same seed as the
  parent. If not, the seed is not really content.

Report to issue #30 alongside the autoseed spike, not as a new issue.

## Confidence

HIGH on the mechanism: `_workflow_instance.py` and `IRandom.h` read at the named lines,
`rand.rs` read in full including the ignored test. HIGH on our own state (grep). MEDIUM
on one design judgement — that the seed belongs on `Action::Genesis` rather than as a
declared environment fact under the shipped environment model. I did not work through
what `env.rs` would do with it, and someone should before the PR.

## Evidence

- Primary: <https://github.com/temporalio/sdk-python/blob/main/temporalio/worker/_workflow_instance.py> — `randomness_seed` in the activation; `update_random_seed` as a recorded job.
- Primary: <https://github.com/apple/foundationdb/blob/main/flow/include/flow/IRandom.h> — the three-generator split and the call-site-reachability warning.
- Primary: <https://github.com/madsim-rs/madsim/blob/main/madsim/src/sim/rand.rs> — entropy-source interposition, and its documented failure on Linux.
- Supporting: <https://docs.temporal.io/develop/python/workflows/basics.md> — "seeded per Workflow Execution", `workflow.uuid4()`.
- Ours: `research/discoveries/2026-08-19-autoseed-and-record.md` (the question this answers), issue #30 (the clock-freeze rejection this must not contradict), `crates/noidroid-core/src/model.rs:499` (the format rule that makes it cheap).

## Changelog

- 2026-08-21 — created.
