# Changelog

All notable changes to this project are documented here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). This project uses
[semantic versioning](https://semver.org/); see [CONTRIBUTING.md](CONTRIBUTING.md) for
how the package version relates to `STEP_VERSION`, the on-disk object format.

## [Unreleased]

### Added

- **`noidroid tui`** — the viewer the manifesto calls V0.1, built with
  [ratatui](https://ratatui.rs). Three panes and one verb: the timeline is coloured by
  provenance, and pressing `e` on a recorded decision reconstructs the prefix,
  diverges, and returns a new trajectory without leaving the screen. `--plain` drops
  the flourishes; `NO_COLOR` drops the colour; neither removes information, because
  nothing is said in colour that is not also said in words.
- **The Stand's colourway**, replacing the four-colour ANSI styling. Each colour is
  bound to a meaning the tool already had — phosphor green `real`, chrome `live`,
  violet `simulated`, amber `unknown`, cyan `replayed`, crimson for divergence — so
  provenance is legible at a glance. Truecolor where the terminal admits to it, ANSI
  where it does not, nothing when piped.
- **`noidroid stand`**. Araki names Stands after music, so a Stand called PARANOID
  ANDROID is built to the rule and fans will know it on sight. The six parameters are
  graded honestly — Destructive Power **E**, because it can never change what
  happened — which makes the stat block an accurate capability summary as well as a
  joke. Nothing in the workflow goes through it.

### Fixed

- A branch whose checkpoint could not be reached was refused, and then written to disk
  anyway. The caller was told the branch failed while a trajectory sat in
  `noidroid log` claiming an ancestry it did not have. The engine now declines to
  persist it, and removes its workspace, because "you cannot branch from a checkpoint
  you cannot reach" is an invariant of the engine rather than advice to the CLI.
- A browser branch whose starting state could not be reproduced said so in the
  terminal but recorded its observations as `live`, i.e. as things that really
  happened. An unreproducible reconstruction now marks everything after it `unknown`,
  which propagates to the head of the trajectory.

### Added

- `noidroid.Ungrounded`: a wrapper an adapter returns to say "here is a real value,
  but it is not evidence about the original execution". The protocol gained `unknown`
  on a result to carry it, which — like `unknown` on an error — is the only kind of
  provenance claim a client may make, because it can only lose trust.
- `Browser(strict=True)` refuses to continue from a state it could not reproduce,
  instead of continuing and marking it unknown. Off by default: a page digest is an
  exact comparison and real pages carry clocks, so a fatal default would refuse most
  real branches.
- A `/volatile` page in the example site whose rendered text comes from the clock, so
  the boundary can be demonstrated rather than described.

## [0.1.0] - 2026-08-18

First working prototype: an execution can be recorded, returned to, and branched.

### Added

- **Trajectory engine.** An execution records as an immutable, content-addressed
  Merkle DAG of steps — `(parent, action, effects, state_root, provenance)`. A branch
  is a step whose parent belongs to another trajectory, so immutable history, prefix
  sharing and copy-on-write follow from the data model rather than being layered on.
- **Verified reconstruction.** `noidroid replay` re-derives a trajectory and checks it
  addresses the same objects, reporting `key_mismatch`, `state_mismatch`,
  `unexpected_call` or `truncated` instead of papering over a divergence.
- **Branching with typed interventions:** `--decide` (choose differently at a declared
  decision point), `--result` (answer differently from the world), `--fail` (inject a
  failure), `--simulate` (supply a stated-simulated value for an irreversible effect).
- **Provenance and delivery as separate axes.** Provenance — `real` ⊑ `live` ⊑
  `simulated` ⊑ `unknown` — is content, is hashed, and never improves along a chain.
  Delivery — `executed`, `replayed`, `intervened`, `denied` — is per-run and is not
  hashed, which is what lets a branch share its parent's objects exactly.
- **Irreversible effects fail safe.** Performed only during an original recording;
  every replay and branch refuses them unless a simulated value is supplied.
- **CLI:** `run`, `log`, `show`, `replay`, `branch`, `checkout`, `tree`, `diff`,
  `verify`.
- **Python client** (standard library only) and a **browser adapter** that drives
  Chromium, records HTTP responses, and reconstructs a browser session by re-driving
  recorded actions — verified against a recorded page digest, and demonstrated with
  the website switched off.
- Two worked examples and 24 tests, two of which drive real Chromium.
- **Engineering practice.** Continuous integration on every pull request: fmt, clippy
  (`-D warnings`), the full suite on Linux with a browser, the suite *without* a
  browser (the browser tests must skip cleanly, or contributors cannot run the suite),
  the suite on macOS because the Unix-socket transport is a portability claim, and the
  CLI example end to end including a check that branching did not move its parent's
  head. Releases come from `v*` tags only, and are refused if the tag disagrees with
  `Cargo.toml` or has no changelog section.
- `format_is_pinned`, which asserts the exact serialised bytes and address of a known
  step. Object names *are* the hash of their bytes, so a silent format change would
  invalidate every recording ever made; see [CONTRIBUTING.md](CONTRIBUTING.md) for when
  a change needs a `STEP_VERSION` bump.

### Known limitations

Documented in the README, and deliberate: not zero-code, sequential programs only,
only the sandboxed workspace is captured, the ambient environment is not captured, a
branch is not a prediction, browser reconstruction is bounded by the recorded page
set, and no scale work.

[Unreleased]: https://github.com/swalla02/noidroid/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/swalla02/noidroid/releases/tag/v0.1.0
