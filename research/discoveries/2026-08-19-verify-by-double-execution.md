---
id: 2026-08-19-verify-by-double-execution
title: Hermit verifies its own determinism by running twice and diffing — we can do it better with our oracle
discovered: 2026-08-19
updated: 2026-08-21
categories: [deterministic replay, record/replay systems, capture honesty, differential testing, robotics / ROS]
class: INSPIRATION
recommendation: PROTOTYPE
transferability: HIGH
novelty: MISSING
confidence: HIGH
touches: [engine, cli]
---

## Discovery

Hermit — Meta's hermetic Linux sandbox — ships `hermit run --verify`, which executes a
program twice under its determinism enforcement and compares output, exit status and
its internal deterministic log. It is a self-check: the tool does not assume it
determinised the program, it *measures* whether it did, and tells you when it did not.

## Source

- Primary: <https://github.com/facebookexperimental/hermit> — README, the `--verify`
  flag and the compatibility matrix.
- Supporting: <https://josnyder.com/blog/2026/deterministic.html> — an independent
  implementation (`stillness`) reaching the same conclusion about needing to check
  rather than assume.

## What is interesting

Hermit's architecture is three layers — namespaces, a ptrace+seccomp interception
layer (Reverie), and a policy layer (Detcore) that virtualises clock, scheduling,
randomness, CPUID and selected file metadata. That stack is deep, and Meta still did
not trust it: the README states Linux compatibility is "substantial but incomplete,
especially for uncommon syscalls and complex record/replay workloads", and the answer
to that honesty problem is not more interception, it is `--verify`.

The important move is epistemic. A capture layer cannot enumerate what it failed to
capture — if it could enumerate the hole it would have plugged it. But it *can* run
the program twice and observe that the two runs disagree. Disagreement is evidence of
an uncaptured source of nondeterminism, obtained without knowing what that source was.

## Why it matters to Paranoid Android

We have exactly this problem, named in `docs/direction.md`: "between the program and
the world there are still openings — the clock, randomness, subprocesses, async SDK
paths — and for each one the honest answer is currently 'we do not look'." Issue #29
(`noidroid doctor`) attacks it by enumeration: probe which SDK surfaces are hooked,
whether the client version matches, whether the fence can install. That is a static
preflight and it is worth building, but by construction it only finds the holes we
already thought of. `--auto`'s refusal has the same ceiling — it refuses on the holes
it knows to look for.

The differential check finds the ones we did not think of.

And our version can be *cleaner than Hermit's*. Hermit runs twice live, so a
difference between the two runs conflates "we failed to determinise it" with "the
world moved". We do not have to: `Mode::Replay` in `crates/noidroid-core/src/engine.rs`
serves every mediated input back from the recording, which removes world drift by
construction. So:

> Record, then immediately replay the recording you just made. The program has not
> changed and the world has been factored out, so **any divergence is a capture gap**
> — and `DivergenceKind` already localises it to a step and says whether it was an
> unexpected call, a key mismatch, or an unmediated workspace write.

We already re-derive object addresses and compare them. What is missing is doing it at
the one moment it constitutes evidence about the recording rather than about a later
edit, and saying so in those words.

### It also gives the in-flight environment model something to stand on

`docs/environment-model.md` (#48, uncommitted) defines a checkpoint's **evidence** —
`captured` / `witnessed` / `none` — as what *would* be compared on reconstruction.
That is a description of a capability, and the document is careful to say `none` "is
not an error, it is the truthful description of a robot".

But nothing currently *exercises* it at record time. `run --verify` is the operation
that turns `evidence: captured` from a property of the recording into a fact somebody
checked, and it degrades exactly as the model says it should: under `witnessed` grip
the check compares fingerprints and reports rather than repairs; under `none` there is
nothing to compare and the honest output is "this recording cannot be verified", which
is a sentence worth printing.

The two pieces of work fit together and neither subsumes the other.

### Cross-domain corroboration (added 2026-08-20)

The same move turns up, independently, in autonomous-vehicle resimulation. Applied
Intuition's engineering writing on closed-loop log replay states the validation
procedure in one sentence: "triage and engineering teams need to be able to trust that a
re-simulation is accurate and reproducible. This can be validated by running
re-simulations on log sections **without a disengagement** and confirming that the ego
divergence is small."

That is Hermit's `--verify` argument arriving from a fourth domain and from an entirely
different tradition: pick a segment where nothing *should* differ, re-run the
reconstruction machinery over it, and treat any divergence as evidence about the
machinery rather than about the system under test. Two independent industries reaching
for the same self-check — one for a syscall-interception layer, one for a sensor-log
replay pipeline — raises the confidence that this is the right shape of answer rather
than a Hermit idiosyncrasy.

It also sharpens the argument for *our* version being the stronger one. Both Hermit and
Applied Intuition are stuck with a **soft** criterion — "the two runs agree closely
enough", "ego divergence is small" — because neither has an oracle that removes world
drift. Under `Mode::Replay` with every mediated input served from the recording, our
criterion is hash equality, and the threshold disappears. The AV people had to choose a
number; we do not, and that difference is worth saying out loud in the product text when
this ships. See `2026-08-19-log-replay-validity-modes` for why we should decline the
rest of their approach.

## Transferability

**HIGH.** No new mechanism is required. `run` already produces a trajectory and
`replay` already compares hashes and reports divergence; this wires the second to the
first and reinterprets the result. The genuinely new work is the *reporting*: a
divergence surfaced here means "your recording has a hole at step k", which is a
different sentence from the one `replay` prints today.

Note also what does *not* transfer: Hermit's whole determinism stack. See
`2026-08-19-process-determinism-ceiling` — that part confirms a decision we already
made rather than reopening it.

## Novelty

**MISSING.** Grepped: no `doctor`, no double-execution check
(`grep -rn "doctor" crates/`). `Command::Verify` exists but does something unrelated —
it re-hashes stored objects to detect tampering on disk, not capture gaps in a
recording. There is no facility today that tells a user, before they trust a
recording, that the recording is incomplete. `--auto`'s refusal and issue #29 are both
enumerative; this is empirical, and the two are complementary rather than competing.

## Limitations and negative signal

Honest failure modes, and they must be in the product text or this becomes the kind of
green tick the project despises:

- **False negatives are structural.** Uncaptured nondeterminism that never reaches a
  mediated call key and never touches the workspace is invisible to the check. A
  passing verify means "no gap *observable through our own surfaces*", not "no gap".
  That sentence has to be what the tool prints.
- **Cost is one extra reconstruction per recording.** Free in external calls, not free
  in wall time. Opt-in flag, not default, at least initially.
- **A genuinely nondeterministic program is indistinguishable from a capture gap** by
  this method, and both are honest reasons to distrust the recording — but the report
  must not name a cause it did not establish.
- Hermit's own `--verify` is documented as less compatible in record/replay mode than
  in plain deterministic-run mode, which is a hint that the check is most useful when
  the mediation surface is narrow. Ours is narrow by design.

## Recommendation

**PROTOTYPE** — the highest-value thing found in this scan, and it fits behind
machinery that already exists.

## Proposed action

Add `noidroid run --verify`: after a successful recording, immediately re-derive the
trajectory in `Mode::Replay` with no live targets, and report the result as a statement
about the *recording*:

```
recorded flight-3 (11 steps)
verify: diverged at step 6 — key mismatch
        recorded  call weather(city="LHR", at=1755600011)
        re-derived call weather(city="LHR", at=1755600042)
        something outside the capture boundary reached this call's arguments.
        this recording cannot be replayed faithfully.
```

Measure on `examples/flight_agent` (should verify clean) and on a deliberately
clock-reading variant (should localise to the step). Land it as a flag first; make it
default only if the false-positive rate on the examples is zero.

Then close the loop with #29: `doctor` says what we *know* we do not cover, `--verify`
says what we did not know. Neither alone earns the claim.

## Confidence

**HIGH** on the mechanism and its applicability — the Hermit behaviour is documented
in its own README, and our side of it is grounded in `engine.rs` which I read.
MEDIUM on the false-positive rate, which is an empirical question the prototype exists
to answer.


## Update 2026-08-21 — madsim does the same check, and its oracle is cheaper than ours

madsim's `Runtime::check_determinism` (`madsim/src/sim/runtime/mod.rs`, lines 178–202)
is the third independent implementation of run-it-twice-and-diff, and the most
instructive because of *what* it diffs. It does not compare program output. It compares
the **sequence of draws from the entropy source**:

- `rand.enable_log()` on the first run appends one byte per draw — `rng.gen::<u8>() ^
  hash_u128(elapsed.as_nanos())`, i.e. a fold of the next RNG byte with the current
  simulated time (`madsim/src/sim/rand.rs`, `GlobalRng::with`, lines 63–86).
- `rand.enable_check(log)` on the second run compares draw *i* against byte *i* and
  panics on the first mismatch: `panic!("non-determinism detected at {time:?}")`.

Two things worth taking. First, the failure is **localised by position and by simulated
time**, which is exactly the ergonomic property `describe_mismatch` in our `engine.rs`
exists to provide — independent confirmation that "where did it first differ" is the
report people need. Second, the log is a *derived digest of a consumption sequence*, not
of state: one byte per event, so the check is nearly free and the artifact is tiny.

The doctest for the API is the honest part, and is worth copying as a test case for
`run --verify`: a future that reads eight bytes from `/dev/urandom` and sleeps for that
many nanoseconds, annotated `should_panic`. That is the shape of test we would want —
a deliberately nondeterministic program that the verifier must catch — and it is
adjacent to TigerBeetle's `canary` fuzzer, which fails on 1% of seeds
(`if seed % 100 == 0`, `src/fuzz_tests.zig`) purely to prove the failure-reporting
pipeline is alive.

Nothing here changes the recommendation. It raises confidence that the mechanism is
standard practice among people who take determinism seriously, and it supplies a
concrete acceptance test.

## Evidence

- Primary: <https://github.com/facebookexperimental/hermit> — `--verify`, three-layer
  architecture, stated compatibility limits.
- Supporting: <https://josnyder.com/blog/2026/deterministic.html> — independent
  implementation, same need for verification.
- Supporting: <https://www.appliedintuition.com/blog/closed-loop-log-replay> — AV
  resimulation validated by re-running segments that should not diverge; independent
  arrival at the same self-check, with a soft threshold where we have an oracle.
- Supporting: <https://github.com/madsim-rs/madsim/blob/main/madsim/src/sim/runtime/mod.rs>
  — `Runtime::check_determinism(seed, config, f)`: runs the same future twice on the same
  seed, logging in run one and comparing in run two. Third independent instance of the
  same self-check, and the only one that names the mechanism as a *positional oracle over
  entropy consumption* rather than an output diff.
- Ours: `crates/noidroid-core/src/engine.rs` (`Mode::Replay`, `DivergenceKind`),
  `crates/noidroid-cli/src/main.rs` (`Command::Verify`, which is a different thing),
  issue #29, `docs/direction.md` § "Where the next release has to get to".

## Changelog

- 2026-08-19 — created.
- 2026-08-20 — updated during the RL/robotics/labs scan. Added cross-domain
  corroboration from AV resimulation (Applied Intuition), which independently
  validates replay machinery by re-running non-diverging segments. Recommendation
  unchanged (PROTOTYPE); confidence in the *shape* of the answer raised, confidence in
  the false-positive rate unchanged.
- 2026-08-21 — updated during the DST scan. madsim's `check_determinism` added as a
  third independent instance, with its entropy-consumption-log mechanism and its
  `should_panic` doctest as a ready-made acceptance test. TigerBeetle's `canary` fuzzer
  noted as the same instinct applied to the reporting pipeline. Recommendation unchanged.
