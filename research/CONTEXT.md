# What Paranoid Android is, for the scout

*A briefing, not a spec. It exists so research is grounded in the real system rather
than the pitch. **The code is the truth** — verify anything you are about to build a
recommendation on. Every claim here names where to check it.*

Last verified against `10c1b64` (2026-08-20). The environment model described below is
no longer in flight — it **shipped in 0.3.0**. Check `git log --oneline -10` and
`git status` before trusting the "what exists" claims; this repository usually has a
branch in progress and moved twice during the 2026-08-20 scan.

---

## The one sentence

> A reconstruction is either faithful, or it says exactly why it is not.

Everything below is downstream of that. `docs/direction.md` is the canonical version
and should be read in full before an open-ended scan.

## What it is, and is not

It records an execution as an immutable, content-addressed trajectory, returns to any
checkpoint inside it, and runs branches from there where one thing is different. The
original is never modified.

It is **not** an observability tool, an agent framework, or an eval harness — those
spaces are crowded and served. The unserved thing is: take an execution that already
happened, return to a meaningful point, find out what would have happened instead,
**with evidence rather than a claim**.

The failure this project cannot survive is not a crash. It is *a trajectory that looks
real*. Any change turning a loud failure into a quiet one is wrong even when it is more
convenient.

---

## The object model — `crates/noidroid-core/src/model.rs`

Everything is built from one primitive:

```
Step { parent, index, action, effects, state_root, provenance, intervention }
```

hashed by its canonical bytes (BLAKE3-256, `hash.rs`) and addressed by that hash.

- A **trajectory** is a chain of steps plus a mutable named ref (`Trajectory`).
- A **branch is a step whose parent belongs to another trajectory.** That is the entire
  branching mechanism — immutable history, prefix sharing and copy-on-write fall out of
  the one choice rather than being layered on.
- An **`Action`** is one of `Genesis | Call | Decide | Finish`. The core knows nothing
  about flights, browsers, robots or laboratories.
- An **`Effect`** is `(key, value-address, effect-kind, provenance, outcome)`. The
  `key` is position-and-identity: two executions agree only if their keys agree.
- **`state_root`** is the Merkle root of the sandboxed workspace after the step
  (`tree.rs`).

### Two axes people conflate, kept separate deliberately

| Axis | Values | Hashed? |
| --- | --- | --- |
| `Provenance` — how grounded the *content* is | `real`, `live`, `simulated`, `unknown` | yes, it is content |
| `Delivery` — how *this run* obtained it | `executed`, `replayed`, `intervened`, `denied` | no, per-run (`StepNote`) |

Provenance never improves downstream: `Provenance::join` takes the least grounded of
parent, own, and every effect. A faithful replay of a real value is still `real` — it
was merely *delivered* differently. Conflating them would mean a perfect replay
produced different hashes than the run it reproduced.

`EffectKind` is `read | write | irreversible`. Irreversible effects are never performed
during replay and denied by default during branching.

## Checkpoints — `crates/noidroid-core/src/engine.rs`

**A checkpoint is a deterministic prefix, not a memory snapshot.** Returning to step k
means re-executing 0..k with every mediated input served from the recording, letting
the application rebuild its own internal state. Cheaper, portable, and verifiable in a
way an image is not.

Verification is **hash equality**: a faithful reconstruction re-derives the same object
addresses, or the engine reports exactly where it stopped matching. Divergence is fatal
and matching is positional (`DivergenceKind`: `UnexpectedCall`, `KeyMismatch`,
`StateMismatch`, …).

`Mode` is `Record | Replay { live } | Branch { at, intervention, simulate }` — one code
path. `Replay { live: [...] }` is the hybrid: named targets execute for real while
tools, network and clock still come from the recording, so a changed prompt or model can
be re-run in a controlled way. Everything from the first live call onward is
counterfactual and labelled as such.

`Intervention` is `ReplaceResult | ReplaceDecision | Fail`, plus named `Failure`
injections (timeout, server-error, rate-limited, malformed, empty, unauthorized).

## Storage — `store.rs`, `repo.rs`, `tree.rs`, `bundle.rs`

- Append-only content-addressed store, sharded two hex chars deep, write-temp-then-
  rename. `put` of existing content is a no-op.
- `.noidroid/` holds `objects/ trajectories/ notes/ workspaces/ logs/ tmp/`. Flat
  files, no database, no daemon — deliberately, because the access pattern is "walk a
  chain, read some blobs". The `Store` interface is narrow enough that packing or a
  remote backend could slot in later without touching the model.
- `tree.rs` snapshots the workspace after every step; whole-file blobs, sorted entries,
  only the executable bit of mode survives. `DEFAULT_IGNORES` skips `.git`,
  `node_modules`, `target`, …; `.noidroidignore` extends it.
- `bundle.rs` exports a trajectory and everything it reaches to one committable file.

**On-disk format is a compatibility surface.** `STEP_VERSION` and a byte-pinning
fixture test guard it (`model.rs` tests, rules in `CONTRIBUTING.md`). Any recommendation
that changes step bytes must carry a migration story or it is infeasible.

## The integration boundary — `proto.rs`, `clients/python/`

Newline-delimited JSON over a Unix socket **is the integration contract**, not a
library. Requests: `hello | call | result | error | decide | finish`. Responses carry a
directive: `execute` ("really do it, then tell me"), `use` (here is the recorded
value), `deny` ("no, and here is why"). A client in a new language is an afternoon's
work.

Capture modes that exist today:
- explicit client calls (the honest baseline),
- `--auto`: SDK-level capture via a `sitecustomize.py` on `PYTHONPATH`, which **refuses
  to record around a hole** rather than recording through it (`--allow-gaps` overrides,
  and is carried on the trajectory so replay makes the same allowance),
- `--proxy`: stands between an agent and a provider API, for agents we did not write,
- `--watch <dir>`: records a real project directory instead of a sandbox,
- a browser adapter (real Chromium, page-digest reproduction).

## CLI — `crates/noidroid-cli/src/main.rs`

`run · log · show · replay · branch · restore · checkout · checkout-tree · bisect ·
export · import · graph · diff · verify · tui · stand`

`bisect` is the flagship inference: re-run from each recorded decision with a different
choice and find the earliest one that flips the outcome. The published baseline for
attributing an agent failure to a step from a transcript is ~14% accurate; this answers
it by experiment instead.

---

## Known limitations — the honest list (README "Limitations")

These are where research is most likely to be *useful*, so read them as an agenda:

- Not zero-code; the program must route side effects through the client.
- Sequential programs only. Threads, async races, concurrent interleavings out of scope.
- Only the sandboxed workspace is captured. Writes outside it are neither captured nor
  detected.
- The ambient environment is not captured — env vars, installed packages, the program's
  own source.
- The clock and randomness are not captured (#30). Subprocesses are not captured (#31).
  Async SDK surfaces are refused rather than captured (#33).
- A branch is not a prediction: past the divergence point, `live` calls query a world
  that has moved on.
- Browser reconstruction is bounded by the recorded page set; an unreproducible page is
  reported and downstream marked `unknown`, not refused (unless `strict=True`).
- Replay reproduces *that* a call failed and with what message, not the exception type.
- No scale work: no packing, no GC, no remote store, no large-artifact handling. Unix
  sockets only — Linux and macOS, not Windows (#32).

## Shipped in 0.3.0 — read this before recommending anything

**The environment model (#48) landed on 2026-08-20** (`c92f416` … `eb497cf`), together
with a browser adapter that declares the page as a world (`c1fb622`) and a runnable
reference environment (`56227ac`).
`docs/environment-model.md` plus `crates/noidroid-core/src/env.rs` and
`checkpoint.rs`, with edits to `model.rs`, `tree.rs`, `engine.rs` and `proto.rs`. It is
the contract between Paranoid Android and a world it did not write, and it adds one
concept that changes how everything downstream should be reasoned about:

**`grip`** — what holding a state address entitles you to:

| grip | we hold | can we detect the world differs? | can we put it back? |
| --- | --- | --- | --- |
| `captured` | the bytes | yes | yes |
| `witnessed` | a fingerprint | yes | no |
| `opaque` | nothing | no | no |

Grip joins like provenance — the weakest part wins — and a checkpoint now answers three
non-collapsible questions: **reach** (can I get back here?), **evidence** (will I know
if I got it wrong: `captured` / `witnessed` / `none`), and **grounding** (`real` /
`live` / `simulated` / `unknown`). A robot checkpoint reads `rebuild / none / real`;
a checkpoint inside a branch reads `rebuild / captured / simulated`. `STEP_VERSION`
stays at 1 — `grip` defaults to `captured` and is skipped when serialising, so existing
trajectories are unchanged.

Consequences for research:

- **Do not propose an environment-manifest or environment-contract abstraction.** It is
  built and released. Read the document first; propose *against* it if you disagree —
  and note that the 2026-08-20 scan did exactly that and found something
  (`discoveries/2026-08-19-unverified-world-redrive.md`): an adapter that skips the
  §7.1 re-drive obligation is indistinguishable, in the run report, from one that
  performed it, because `Situation::adopt` supplies the fingerprint from the recording.
- The six-environment conformance table (§12) is the current statement of which
  environments get which grip. Robotics and laboratory findings should be checked
  against those rows before being written up as new.
- The appendix's six laws are the compact version of the whole design and are the
  cheapest way to test whether a discovery fits.

## Where the next release has to get to

The milestone is **"earn the claim"**. Today the one-sentence promise holds only for the
surfaces the tool happens to mediate; between the program and the world there are still
openings — clock, randomness, subprocesses, async SDK paths — and the current honest
answer for each is "we do not look". Getting there does not mean capturing everything.
It means **knowing and saying what is not captured, before a recording is made.**

Roadmap in `README.md`, in order: an HTTP/tool adapter · detecting unmediated effects
beyond the workspace · a snapshot fast-path behind the same checkpoint interface ·
structured trajectory comparison then guided multi-branch exploration · dataset export
from declared decision points.

Deliberately unbuilt: a dashboard, distributed storage, an agent framework, a universal
simulator, and anything resembling speculative infrastructure for a user who has not
appeared yet.

---

## How to verify this briefing before you rely on it

```bash
sed -n 1,60p crates/noidroid-core/src/lib.rs        # the model, from the source
sed -n 1,60p crates/noidroid-core/src/engine.rs     # reconstruction semantics
grep -n "pub enum\|pub struct" crates/noidroid-core/src/model.rs
grep -n "^## \|^### " README.md                     # capabilities and limitations
cat docs/direction.md                               # what would make it worthless
gh issue list --state open --limit 50               # what is already known-broken
git log --oneline -20                               # what changed since this was written
```

If something here is stale, fix this file as part of your run and note it in the scan
report. A briefing nobody maintains is worse than none.
