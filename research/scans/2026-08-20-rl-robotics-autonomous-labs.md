---
date: 2026-08-20
cadence: open
question: "What should Paranoid Android build next to make it useful for RL, autonomous labs, and robotics?"
cards_created:
  - 2026-08-19-unverified-world-redrive
  - 2026-08-19-checkpoint-as-message-cache
  - 2026-08-19-snapshot-omits-derived-state
  - 2026-08-19-reversibility-is-not-in-the-instrument-standard
  - 2026-08-19-log-replay-validity-modes
  - 2026-08-19-autoseed-and-record
cards_updated:
  - 2026-08-19-verify-by-double-execution
landscape_created:
  - bluesky-runengine
  - lerobot
  - lap-agent-instrument-protocol
---

# Scan: RL, autonomous labs, robotics

*The run started on 2026-08-19 and finished on 2026-08-20; the cards carry their
discovery date (19th) and are not renamed. This report carries the completion date.*

> **The repository moved under this scan.** Analysis began against `af81680` with the
> environment model (#48) uncommitted in the tree. While the run was paused, HEAD
> advanced to `10c1b64` and the environment model **shipped in 0.3.0** (`eb497cf`),
> together with a browser adapter that declares the page as a world (`c1fb622`) and a
> runnable reference environment (`56227ac`). I re-read the relevant code at the new
> HEAD before finishing: `Situation::adopt`, the `Phase::Reconstructing` branch in
> `engine.rs` and the printed evidence sentence are unchanged, so the findings hold —
> but recommendation 1 is now a fix to released behaviour rather than a design objection
> to a branch. `research/CONTEXT.md` has been updated accordingly; it described #48 as
> work in flight.

## In one paragraph

I went looking in three domains' own vocabularies — gym/env determinism, seeds, rollout
datasets, ROS bags, resimulation, beamline plans, instrument protocols — for a capability
we should build to serve them. **I did not find one, and I recommend against building
for these domains.** What I found instead is more useful: all three domains, plus
autonomous driving, return a world to an earlier point by exactly our method
(re-perform the recorded instructions, because nobody can snapshot a physical world),
and **none of them checks that it worked.** LeRobot replays recorded actions onto a real
arm and never reads the recorded observations back. Bluesky rewinds a synchrotron plan
by re-sending a cached message deque and never compares the second reading to the first.
`ros2 bag play` publishes with no back-pressure. AV resimulation is the only one that
measures divergence, and it does so against a tuned threshold. Then I checked our own
in-flight environment model against the code and found the same gap, one level up: an
adapter that skips the re-drive obligation in `docs/environment-model.md` §7.1 produces a
run that reports `witnessed` and prints "reported fingerprints are compared" — because
`Situation::adopt` fills the fingerprint in from the recording, so the state root matches
by construction. That is a silent pass in the newest subsystem, it is cheap to close on
the delivery axis we already have, and closing it is the one thing this scan says to
build. Everything else on the RL/robotics/labs axis is either already settled (C2,
confirmed twice over), already parked (C8), or has no user (C9).

## What survived

**`2026-08-19-unverified-world-redrive` — PROTOTYPE.** The headline. Four unrelated
domains re-drive a world they cannot snapshot and none verifies the result; our engine
makes the unverified case indistinguishable from the verified one. Verified in our
source: `env.rs` (`Situation::adopt`, `Situation::fresh`, `Reported::grip`),
`engine.rs:963-981`, `noidroid-cli/src/main.rs:512-517`. `Situation` already tracks
exactly the distinction needed (`fresh`) and throws it away in `settle()`. Fix is a field
plus a join plus a printed sentence, on the per-run delivery axis, so no step bytes move.

**`2026-08-19-checkpoint-as-message-cache` — WATCH.** Bluesky's `RunEngine._rewind()`
rebuilds a plan from `self._msg_cache` — a deque of the messages processed since the last
`checkpoint` — and re-executes them against live instruments. Constraint C2 is not our
design position; it is the incumbent implementation in beamline control, shipped since
2015. `_UNCACHEABLE_COMMANDS` is a static per-verb `EffectKind` table; `clear_checkpoint`
is `Reach::Unreachable` declared forward by the plan author. Our retrospective
computation from `EffectKind` + grip is strictly more capable, because it works on
recordings whose author never considered the question. Nothing to build; a citation to
stop re-arguing C2, and a named adapter seam (`RunEngine.msg_hook`) for a user who does
not exist.

**`2026-08-19-snapshot-omits-derived-state` — INVESTIGATE.** RL has no standard env
state API, and the one real implementation — ALE's `cloneSystemState`/`restoreSystemState`
— restored the console RAM but not the **screen**, because the Atari 2600 has no
framebuffer and the observation is generated during emulation. Restore succeeded,
returned no error, and produced a stale frame. Its docstrings also mis-stated which
variant captured the RNG. The practitioner workaround was to advance the emulator one
recorded action to regenerate what the snapshot could not hold. This is the specification
for roadmap item 3 (a snapshot fast-path behind the checkpoint interface): a restore that
succeeds is not evidence that the state was complete, and ours must be validated by
re-derivation or it cannot claim `captured`.

**`2026-08-19-reversibility-is-not-in-the-instrument-standard` — WATCH.** I read the
SiLA 2 Feature Definition Language schema. A `Command` carries `Identifier`,
`DisplayName`, `Description`, `Observable`, `Parameter`, `Response`,
`IntermediateResponse`, `DefinedExecutionErrors` — and nothing about what re-performing
it would do. Fifteen years of rigorous machine-readable instrument description, and the
one bit that decides checkpoint reachability is absent. So a lab adapter could never
infer `EffectKind` from the standard; it would need a hand-maintained per-vendor table,
where one wrong row means a checkpoint reported reachable that is not. That closes off
"auto-derive effect kinds from the instrument schema" before anyone proposes it. LAP, a
2026 preprint, does put a `reversible` flag and a `safetyClass` on every capability —
independent convergence on our rule, from physical rather than epistemic motives.

**`2026-08-19-log-replay-validity-modes` — WATCH.** AV evaluation names three replay
modes (open-loop / closed-loop non-reactive / closed-loop reactive) and reports which one
produced every number, because the same planner scores differently in each. We have all
three situations and no names for them; the taxonomy is derivable from the chain we
already store. Their other answer — validity by divergence threshold — is the
fuzzy-matching evidence standard C4 closed, and is what you reach for when you have no
oracle. We have one.

**`2026-08-19-autoseed-and-record` — INVESTIGATE.** Minari does not capture randomness;
it *generates* a seed when the caller did not supply one (`secrets.randbits`), passes it
to `env.reset(seed=...)`, records it, and makes the opt-out a recorded flag. We seed
nothing and record no seed anywhere (`grep -rn "seed"` across `crates/` and `clients/` is
empty). Issue #30 rejected clock freezing because it is fail-open; PRNG seeding is
arguably fail-loud, because an unseeded source still diverges exactly as loudly as today.
That asymmetry is worth a half-day spike, and a negative result closes the question.

**`2026-08-19-verify-by-double-execution` — updated, recommendation unchanged.** Applied
Intuition validate resimulation by re-running log sections *without a disengagement* and
confirming divergence is small. That is Hermit's `--verify` argument from a fourth
domain and an unrelated tradition. Both are stuck with a soft criterion because neither
has an oracle; under `Mode::Replay` ours is hash equality and the threshold disappears.

## Looked at, not pursued

- **Gymnasium `FuncEnv` / functional API, Gymnax** — a functional env whose state is an
  explicit value would be `captured` grip for free, but it is a rewrite of the
  environment, not something we can offer. Noted inside the snapshot card.
- **MuJoCo `set_state` (`qpos`/`qvel`/`act`/`time`)** — a genuine complete state vector,
  and the honest counter-evidence to the snapshot card. Recorded there rather than
  separately.
- **RLDS / TFDS** — same "store every field, do not summarise" argument we make; no
  mechanism to take.
- **Google Research Football `get_state`/`set_state`** — a third instance of the
  ad-hoc-per-env state API. Confirms the Gymnasium finding, adds nothing.
- **MCAP and Foxglove** — a well-designed indexed container format with a schema
  registry. Not opened properly this run (see *Still unknown*); on its face it is a
  logging/interchange concern, and `bundle.rs` already covers our export need.
- **SiLA 2 command control (pause/resume/stop)** — real, but about controlling a running
  command, not about what a repeat costs. Folded into the FDL card.
- **ELN / LIMS provenance and the self-driving-lab literature** — the recurring complaint
  ("execution records and measurement outputs captured in separate systems without
  reliable linkage") is real but is a data-integration problem, not a reconstruction
  problem. I could not reach the primary article (403) and did not build on it.
- **Isaac Lab / Gazebo / Drake determinism claims** — not examined. Named as a gap below.

## Negative findings

- **The re-drive is never verified** (four domains). Opportunity, and specifically an
  opportunity to close the same hole in ourselves before adapters exist that rely on the
  current lenience. This is the run's main output.
- **The one shipped RL state-snapshot API silently omitted derived state.** Warning: it
  is aimed straight at roadmap item 3.
- **The mature lab standard has no reversibility attribute.** Warning: it sizes a lab
  adapter as an unbounded per-vendor maintenance surface, not a protocol shim.
- **Gymnasium's missing state API is a decade-old recurring complaint** (issues #94,
  #737, plus every MCTS wrapper in the ecosystem), and the reason it stays unsolved is
  structural: env state is distributed across the simulator, the wrapper stack and the
  PRNG, so there is no address that names it. `EzPickle` makes `deepcopy` — the accepted
  workaround — silently wrong. This is a *confirmation* of C2, not an opening for us: the
  fix people actually use is reset-with-seed plus action replay, which is our checkpoint.
- **Minari repairs a stored environment spec with a string replacement**
  (`env_spec.replace('"order_enforce": true,', "")`, "for gymnasium 1.0.0
  compatibility"). Ugly integration: environment identity is recorded but not versioned.
  Our ambient-environment gap is the same gap; nobody has solved it.

## What we now know that we did not

1. **The deterministic-prefix checkpoint (C2) is the incumbent design in two of the three
   named domains**, not a contrarian choice. Bluesky ships it in production beamline
   control; RL practice converges on reset-with-seed plus action replay because the
   snapshot API cannot be built.
2. **Our environment model has a reachable silent-pass path**, and it is in `env.rs` /
   `engine.rs` today: an adapter that never re-drives gets a `witnessed` badge and a
   printed claim that fingerprints were compared.
3. **`EffectKind` cannot be inferred from the incumbent lab instrument standard.** SiLA 2
   FDL has no reversibility attribute. Verified against the schema.
4. **A "complete" state snapshot can restore successfully and still be wrong**, with a
   named, primary-source instance (ALE's screen).
5. **We record no seed and seed nothing**, so in-process randomness has no honest story
   at all today — not even the cheap one the RL ecosystem uses.
6. **These domains do not need a new subsystem from us.** Robotics and labs sit in the
   `opaque`-grip rows of the §12 conformance table; what they would stress is the
   evidence reporting, not the object model. There is no user asking, so C9 applies.

## Still unknown

Named honestly — the run was cut short by a session limit and some of this was on the
list rather than deliberately dropped:

- **rosbag2's actual replay implementation.** The ROS row in the unverified-re-drive card
  rests on a Discourse thread and issue titles, not on source. It is corroborating, not
  load-bearing, but it is the weakest line in that table.
- **MCAP** — the format, its index and chunking design, and whether anything in it bears
  on `store.rs`/`bundle.rs`. Not opened.
- **LAP** — I read the abstract and HTML sections; the full PDF did not render. Every LAP
  claim is "the paper says", and no implementation was found.
- **Deterministic simulation testing** (Antithesis, TigerBeetle, FoundationDB) — still
  the highest-value unexamined area, still carried over from the previous run's standing
  questions. It bears on *branching* and state-space exploration, which nothing in this
  scan touched.
- **Simulator determinism claims** (Isaac Lab, MuJoCo across versions, Gazebo) — whether
  a seeded simulator genuinely re-derives identical state, which decides whether the RL
  row of §12 can honestly say `captured`.
- **What a real user in any of these domains would actually ask for.** Everything here is
  inferred from artifacts. No user contact, so every "they would want this" is inference.

# Recommended Actions

Ranked by Impact × Relevance × Feasibility × Novelty
(`.claude/skills/technical-scouting/references/prioritisation.md`).

### 1. Make an adopted world observation report as `opaque`, not `witnessed`

**Why now:** it shipped. When I started this scan the argument was "fix it before #48
lands"; #48 landed in 0.3.0 mid-run, along with two adapters that declare worlds (the
browser page and the reference environment). So the lenient path is now released,
exercised behaviour, and every additional adapter written against it makes the change
more of a behaviour break and less of a fix. The domains this scan examined say the
skipped re-drive is not a hypothetical failure but the domain default: LeRobot's
documented replay loop does exactly it, and bluesky's rewind does exactly it.

**Impact:** 3 — converts a silent pass into a stated one on the project's own worst
failure mode, and it is the environment model's one acknowledged blind spot.
**Relevance:** 3 — capture honesty and reconstruction fidelity, the core claim.
**Feasibility:** 3 — a field on `Reported`, a second grip accessor, one join changed in
`engine.rs`, one printed line in the CLI. Per-run delivery axis, so `STEP_VERSION` does
not move and step bytes are unchanged. One PR.
**Novelty:** 3 — nothing in the codebase records or prints where an observation came
from; `Situation::fresh` holds the distinction and discards it.
**Score:** 81

**Cost:** one PR with three invariant-named tests, now against released code rather
than a branch, so it needs a line in the changelog saying what the report used to say.
The risk that would blow it up:
over-refusal. A `witnessed` world whose adapter genuinely cannot re-drive would report
`opaque` on every reconstruction, and the run's achieved grip would start to differ from
the trajectory's recorded grip. If the report cannot make that distinction legible in one
line, the change makes the output worse, not better, and should be dropped.

**What we would learn:** whether the run report today is byte-identical with and without
the browser adapter's `_reconstruct` re-drive. If it already differs, this card is wrong
and should be downgraded to IGNORE with that note recorded.

**Touches:** `crates/noidroid-core/src/env.rs` (`Reported`, `Situation::adopt`,
`Situation::fresh`), `crates/noidroid-core/src/engine.rs:963-981` (`report.grip` join),
`crates/noidroid-cli/src/main.rs:512-517` (the evidence sentence),
`docs/environment-model.md` §7.1.

**Evidence:** `2026-08-19-unverified-world-redrive`, and the landscape entries
`lerobot`, `bluesky-runengine`.

### 2. Ship `noidroid run --verify` (unchanged from the previous scan; now better evidenced)

**Why now:** unchanged as a priority, but the case is stronger. A fourth domain, with no
connection to syscall interception, independently validates its reconstruction machinery
by re-running segments that should not diverge. Two industries reaching the same
self-check makes this the shape of the answer rather than a Hermit idiosyncrasy — and
both of them are stuck with a soft threshold that our recorded-input oracle removes.

**Impact:** 3 — finds capture gaps we did not think to enumerate, at the moment the
result is evidence about the recording.
**Relevance:** 3 — the "earn the claim" milestone directly.
**Feasibility:** 3 — wires `replay` to `run` behind existing machinery.
**Novelty:** 3 — no facility today tells a user a recording is incomplete.
**Score:** 81

**Cost, what we would learn, touches:** unchanged — see the card. Ranked below item 1
only on the tie-breaker "cheapest disproof first" and because item 1's window closes when
#48 lands, not because it matters less.

**Evidence:** `2026-08-19-verify-by-double-execution` (updated 2026-08-20).

### 3. Write the validation criterion into the snapshot fast-path before it is designed

**Why now:** roadmap item 3 has not started, so the criterion is free to add and
expensive to retrofit. And we now have a named, primary-source instance of the exact
failure: a "complete" snapshot that restored without error and returned a stale
observation, because the observation was derived rather than stored.

**Impact:** 2 — no user-visible change now; prevents a future capability from being able
to lie about `captured` grip.
**Relevance:** 3 — C2's explicit reopening clause says a fast-path must "preserve the
verification story". This is what that sentence has to mean.
**Feasibility:** 3 — a paragraph and a named test on an existing issue. Nothing to build.
**Novelty:** 2 — a named constraint on a decision already taken.
**Score:** 36

**Cost:** an hour on the issue. Then one open question to answer on paper before any
code: validation by re-derivation costs exactly the slow path the fast-path exists to
avoid, so decide whether it runs on snapshot *write*, on a sample, or opt-in with a
weaker evidence label.

**What we would learn:** whether a snapshot fast-path can ever be verified cheaply enough
to be mandatory. "No — so a fast-path can only be an unverified convenience" is a good
answer and belongs in `constraints.md`.

**Touches:** the snapshot fast-path issue, `crates/noidroid-core/src/engine.rs`,
`tree.rs`, `store.rs`.

**Evidence:** `2026-08-19-snapshot-omits-derived-state`.

### 4. Spike PRNG seed-and-record, and try hard to break it

**Why now:** we have no story at all for in-process randomness — we neither capture it,
control it, nor detect it — and the RL ecosystem's answer has been sitting in plain sight
for a decade: set the seed, record the seed, make the opt-out data. Issue #30 rejected
*clock freezing* for being fail-open; the asymmetry with a PRNG is that an unseeded source
still diverges exactly as loudly as today.

**Impact:** 2 — removes one class of unexplained replay divergence.
**Relevance:** 3 — capture honesty; issue #30 directly.
**Feasibility:** 2 — the bootstrap change is small, but *where the seed lives* in the
model is unresolved (hashed genesis effect vs per-run note vs environment manifest), and
the wrong answer makes a bundle replay differently on another machine.
**Novelty:** 2 — a capability we lack, adjacent to a decision partly taken in #30.
**Score:** 24

**Cost:** half a day in a scratch branch. The risk: partial seeding creates a false sense
of coverage — a user concludes their run is deterministic and a dependency's own
`Random()` instance proves otherwise.

**What we would learn:** whether seeding is genuinely fail-loud. Step 3 of the spike is
the experiment: introduce `os.urandom`/`uuid4` and confirm the divergence is still loud
and still localised. If seeding ever converts a divergence into a silent wrong value, the
answer is no, #30 wins, and the card becomes an IGNORE.

**Touches:** `clients/python/` (the `sitecustomize.py` auto-capture path),
`crates/noidroid-core/src/model.rs` if the seed becomes content. Report to issue #30.

**Evidence:** `2026-08-19-autoseed-and-record`.

### 5. Name the replay mode in the run report, when the comparison work is picked up

**Why now:** not now — this is queued behind issues #24 and #34. Recorded so the naming
is taken from an established taxonomy rather than invented.

**Impact:** 1 — ergonomics.
**Relevance:** 2 — divergence reporting.
**Feasibility:** 3 — one derived sentence from the chain we already store; nothing new on
disk.
**Novelty:** 2 — a named improvement on prose we currently keep in a limitations list.
**Score:** 12

**Cost:** part of a PR on #24/#34. Risk: it becomes a taxonomy nobody maintains. One
sentence per run, derived, or not at all.

**What we would learn:** whether users reading a branch report can tell, unprompted,
which parts of it met a world that had moved on.

**Touches:** `crates/noidroid-cli/src/main.rs`, `docs/`.

**Evidence:** `2026-08-19-log-replay-validity-modes`.

## Explicitly not recommended

**Do not expand into RL, robotics or autonomous labs.** This is the direct answer to the
question asked. All three sit in the weak-grip rows of the environment model's §12 table,
which the model already handles correctly; none of them exposes a mechanism we lack; and
no user from any of them has appeared. C9 applies exactly as written. What these domains
gave us is not a market, it is a stress test that found a real defect in our newest
subsystem — which is a better outcome and is recommendation 1.

**Do not build a bluesky, SiLA 2 or ROS adapter.** The bluesky seam is genuinely small
(`RunEngine.msg_hook` plus a verb-to-`EffectKind` table seeded from
`_UNCACHEABLE_COMMANDS`) and is recorded so it can be built in half a day if a user
appears. The SiLA seam is *not* small: the FDL has no reversibility attribute, so the
table would be hand-maintained per vendor and one wrong row means a checkpoint reported
reachable that is not. C10 parks adoption work; this scan adds that one of these adapters
is cheap and one is a maintenance liability, so if the parking ever lifts they should not
be treated as the same size of job.

**Do not try to infer `EffectKind` from an instrument or tool schema.** Verified against
the SiLA 2 FDL: the bit is not there, in the most mature standard the field has. Law 2 of
the environment model holds — the boundary is declared, never inferred — and this is now
evidence rather than assertion.

**Do not add a divergence threshold, a similarity score, or a branch-validity number.**
The AV industry has one because it has no oracle and a metric world where "nearly the
same" means something. We have an oracle, and the distance between two BLAKE3 digests is
not a quantity. This is C4 and nothing in this scan reopens it; the finding is that the
threshold is what the alternative looks like.

**Do not reopen C8 (training runs as the branchable unit).** Nothing found this run
changes the timing argument. Issue #21 already holds the reasoning, including the point
that hash-equality verification does not survive the move to GPU and that storage
inverts because branches diverge immediately. This scan adds one small corroboration —
the RL ecosystem's reproducibility primitive is the seed, not the state — which supports
recommendation 4 and does not bear on C8.

**Do not build a snapshot fast-path that is trusted rather than validated.** See
recommendation 3. If validation turns out to cost as much as the slow path, the honest
outcome is a fast-path that reports a weaker evidence label, not one that reports
`captured` because the restore returned `Ok`.

**Do not treat MCAP or rosbag2 as an export target.** Not examined properly this run, so
this is a "not now, and not on the basis of this scan" rather than a reasoned rejection —
but on its face it is interchange for a viewer, which is adjacent to the dashboard we
have said we are not building, and `bundle.rs` already covers handing a trajectory to a
colleague.
