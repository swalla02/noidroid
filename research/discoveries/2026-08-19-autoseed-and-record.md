---
id: 2026-08-19-autoseed-and-record
title: Minari seeds the run for you and records the seed, rather than capturing randomness
discovered: 2026-08-19
updated: 2026-08-21
categories: [clock and randomness control, reproducibility, experiment tracking, model-based RL]
class: INFRASTRUCTURE
recommendation: INVESTIGATE  # design question answered 2026-08-21 — see 2026-08-21-engine-issued-seed
transferability: MEDIUM
novelty: MISSING
confidence: MEDIUM
touches: [clients, engine, model]
---

## Discovery

Minari is Farama's dataset format for offline RL. When it collects an episode it does
not attempt to capture the environment's randomness. It **generates a seed if the caller
did not supply one** — `seed = secrets.randbits(AUTOSEED_BIT_SIZE)` — passes it to
`env.reset(seed=seed)`, and stores it with the episode. Opting out is a named flag,
`options={"minari_autoseed": False}`, and the opt-out is itself part of the reset call
that gets recorded. Reproducibility is achieved by *controlling* the nondeterminism and
recording the control value, not by observing its consequences.

## Source

Primary, read directly:
- <https://github.com/Farama-Foundation/Minari/blob/main/minari/data_collector/data_collector.py>
  — `DataCollector.reset`, lines 176–212. Docstring: "If no seed is set, one will be
  automatically generated, for reproducibility, unless `minari_autoseed=False` in the
  `options` dictionary."
- <https://github.com/Farama-Foundation/Minari/blob/main/minari/dataset/minari_dataset.py>
  — `MinariDatasetSpec` holds `env_spec`, `eval_env_spec` and `minari_version`;
  `recover_environment()` rebuilds the environment from the stored `EnvSpec` JSON.
- Farama announcement, <https://farama.org/Announcing-Minari>.

## What is interesting

**The move.** There are three things you can do about an unmediated source of
nondeterminism: capture every value it produces, freeze it, or set it and write down
what you set. Minari does the third. It is cheap, it is portable, it needs no
interposition, and — the property that matters here — **it does not fail open.** A
seeded `random` module makes one source of divergence disappear; every source that was
not seeded still diverges as loudly as before. Contrast freezing a clock, which makes a
wrong value look right.

**The opt-out is data.** `minari_autoseed=False` travels in the `options` dict that is
recorded with the episode, so a reader of the dataset can tell whether the episode was
auto-seeded. That is structurally the same choice as our `--allow-gaps`, which is
carried on the trajectory so a replay makes the same allowance.

**The second half is weaker and worth knowing.** `recover_environment()` rebuilds the
environment from a stored `EnvSpec` JSON — an environment manifest, in our vocabulary —
but the manifest names an `entry_point`, not code. The docs say plainly that the library
providing it must be installed. And in `minari_dataset.py` there is this, live in the
loader:

```python
env_spec = env_spec.replace('"order_enforce": true,', "")  # for gymnasium 1.0.0 compatibility
```

A string patch on a serialised environment spec, to survive an upstream schema change.
That is the ugly-integration marker: the environment identity is recorded but not
versioned, so the loader repairs it by regex.

## Why it matters to Paranoid Android

Issue #30 — "the clock and randomness are not captured, and break replays silently or
loudly" — has already rejected one option with a good reason. Its text says: "The
tempting fix is to freeze the clock. It is the wrong one: freezegun searches
module-level imports and misses values hidden inside objects, time-machine is
CPython-only, and a freeze that covers most clocks converts a *loud* mismatch into a
*silent* wrong value — fail-open." The chosen direction is detect and report.

That reasoning is airtight **for the clock** and I am not reopening it. The question
this card raises is whether it also holds for the PRNG, because the failure modes are
not symmetric:

- A frozen clock returns a value the program would not have seen. Fail-open.
- A seeded PRNG returns a value the program genuinely computed, and any *unseeded*
  source — `os.urandom`, `secrets`, `uuid4`, a C extension's own RNG — still produces
  the same loud divergence it produces today. Fail-loud.

I checked: `grep -rn "seed" --include=*.rs --include=*.py` across `crates/` and
`clients/` returns nothing. We neither seed nor record a seed anywhere. So today a
program that calls `random.random()` outside a mediated call diverges on replay with no
explanation, and the user's only route is `volatile=`.

Relevance to the domains this scan was asked about is direct: **an RL rollout's entire
reproducibility story is the seed.** Minari records it, Gymnasium's `reset(seed=)` is
the whole API, and the ALE determinism bug in
`2026-08-19-snapshot-omits-derived-state` turned out to be a stochasticity setting
nobody surfaced. If we ever record an RL rollout, the seed is the single most important
thing on the trajectory and we currently have nowhere to put it.

Bears on: capture honesty, replay fidelity, and the auto-capture client
(`clients/python/`, the `sitecustomize.py` path).

## Transferability

MEDIUM. The mechanism is trivially portable — set the seed in the auto-capture bootstrap
and record it as part of the run's genesis. What is not obvious is where it *lives* in
our model. Options, all with a problem:

- As an effect on the genesis step: makes it content and therefore hashed, which is
  correct (it is a real input to the execution) but touches step bytes.
- As per-run metadata like `StepNote`: does not survive export, and a bundle that
  replays differently on another machine is exactly what we do not want.
- As a declared environment fact, once #48's manifest exists.

The hard question is not the seeding, it is whether a seed we chose counts as `real`
provenance. My reading: yes — the program really ran with it, exactly as it really ran
with whatever entropy it got otherwise. But that should be argued, not assumed.

## Novelty

MISSING. Nothing in the codebase seeds anything or records a seed. This is not the same
as #30's detect-and-report direction; it is a second, complementary move for one of the
two sources #30 names, and #30's stated objection does not obviously apply to it.

## Limitations and negative signal

The strongest argument against: **partial seeding creates a false sense of coverage.**
Seed `random` and `numpy.random` and a user reasonably concludes their run is
deterministic; then a dependency uses its own `Random()` instance, or `os.urandom`, and
the run diverges anyway. That is not fail-open — the divergence is still loud — but it
is a documentation and expectation hazard of the same family as the one #30 warns about.
Any implementation would have to say exactly which sources it seeded, in the same
sentence that says it seeded them.

Second: it does nothing for a program whose randomness is on the other side of a
mediated call, which is already handled. The benefit is confined to in-process
randomness in the auto-capture path.

Third, from Minari itself: recording the seed does not make an episode reproducible
across library versions. Their loader's regex patch on `env_spec` is proof that the
manifest they record is not sufficient, and our ambient-environment gap is the same gap.
Recording a seed narrows the divergence surface; it does not close it, and we should not
claim it does.


## Update 2026-08-21 — the open design question is answered, and Minari's shape is wrong for us

This card left one thing open: "The hard question is not the seeding, it is where it
*lives* in our model." The DST scan answered it from two production systems, and the
answer is not Minari's.

Minari has the **client** mint a seed and store it with the episode. Temporal has the
**orchestrator** issue the seed inside the activation that starts the (re-)execution,
and makes re-seeding a recorded event of its own. For a system whose product is
branching, only the second works: if the client mints the seed, a branch re-executing
the prefix mints a *new* one, and the branch differs from its parent in two ways instead
of one. See `2026-08-21-engine-issued-seed` for the mechanism, the exact file and type
changes, and the acceptance tests.

Two further corrections to this card from that run:

- **The "partial seeding" limitation is worse than stated, and is not fixable by trying
  harder.** madsim overrides `getrandom` as an `extern "C"` symbol to seize the OS
  entropy source; their own test is `#[ignore]`d on Linux because the Rust `rand` crate
  reaches `SYS_getrandom` directly. Their shipped fix is a `[patch.crates-io]` fork of
  the dependency. Language-level seeding is the ceiling for anyone who does not own the
  dependency graph.
- **A hazard this card did not name:** FoundationDB ships a third generator,
  `debugRandom()`, seeded identically but on a separate stream, precisely so that
  debug/observability code does not shift the program's own draws. A shared stream makes
  the *number of draws* part of program state. Our client currently uses neither `random`
  nor `uuid` (checked), so the rule is free to adopt now.

The spike proposed below is still the right spike. Its step 1 should read "the engine
issues the seed and the bootstrap applies it", not "the bootstrap seeds with a value
recorded on the run".

## Recommendation

INVESTIGATE — answer whether PRNG seeding is fail-loud where clock freezing is
fail-open, because if it is, #30's rejection does not cover it and there is a cheap win.

## Proposed action

A half-day spike, no production code:

1. In a scratch branch, have the auto-capture bootstrap (`sitecustomize.py` path in
   `clients/python/`) seed `random` and, if importable, `numpy.random`, with a value
   recorded on the run.
2. Take an example whose replay currently diverges on an unmediated `random` call and
   confirm it now replays clean.
3. Then deliberately introduce a source that seeding does *not* cover — `os.urandom` or
   `uuid.uuid4()` — and confirm the divergence is still loud and still localised to the
   step.

Step 3 is the experiment. If seeding ever converts a divergence into a silent wrong
value, the answer is no and this card becomes an IGNORE citing #30, which is a result
worth having. Report to issue #30 rather than opening a new issue.

## Confidence

MEDIUM. The Minari mechanism is HIGH — I read `data_collector.py` and
`minari_dataset.py` at the named lines. The claim about *our* behaviour is HIGH (grep
returns nothing). The MEDIUM is on the argument: whether seeding is genuinely fail-loud
in all cases is a claim I have reasoned about but not tested, which is precisely why the
recommendation is a spike and not a build.

## Evidence

- Primary: <https://github.com/Farama-Foundation/Minari/blob/main/minari/data_collector/data_collector.py>
  — `secrets.randbits` autoseed, the recorded opt-out flag, the docstring rationale.
- Primary: <https://github.com/Farama-Foundation/Minari/blob/main/minari/dataset/minari_dataset.py>
  — `env_spec` stored as JSON and repaired by string replacement on load.
- Supporting: <https://farama.org/Announcing-Minari> — "dedicated tooling (e.g. storing
  episode seeds and recovering the original environment) so that any user can completely
  replicate experiments".
- Counter-evidence: issue #30 in this repository — the reasoning that rejected clock
  freezing, which a careless reading of this card would appear to contradict. It does
  not; the distinction is fail-open versus fail-loud, and if that distinction does not
  survive the spike, #30 wins.

## Changelog

- 2026-08-19 — created.
- 2026-08-21 — updated during the DST scan. The placement question is answered by
  Temporal's `randomness_seed`; Minari's client-minted shape is wrong for a branching
  system. Superseded in design by `2026-08-21-engine-issued-seed`, which carries the
  recommendation forward from INVESTIGATE to PROTOTYPE.
