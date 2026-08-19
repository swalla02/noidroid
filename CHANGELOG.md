# Changelog

All notable changes to this project are documented here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). This project uses
[semantic versioning](https://semver.org/); see [CONTRIBUTING.md](CONTRIBUTING.md) for
how the package version relates to `STEP_VERSION`, the on-disk object format.

## [Unreleased]

### Fixed

- An I/O failure now says what it was doing and on what path. `No such file or
  directory` names none of the six operations that can produce it, which made a
  real CI failure unreadable. (#42)
- **Automatic capture failed open, and said it did not.** The README promised
  "`--auto` prints what it hooked; anything not listed was not recorded" — but the
  print was gated behind an environment variable nothing in the project ever set, so
  the single documented mitigation for fail-open capture never ran. It always prints
  now.
- **The async surface was never patched and never mentioned.** A program using
  `AsyncAnthropic` had its sync calls recorded and its async calls run **live during
  replay**, while the replay reported itself faithful. `--auto` now names every
  surface it could not cover and **refuses to record**, with `--allow-gaps` as a
  deliberate override that is stored on the trajectory so replays make the same
  allowance.
- `auto.install()` silently continued when an SDK's base client was not where it
  expected, contradicting its own docstring; it raises now, because an upstream
  rename that records nothing and exits zero is the failure this module exists to
  prevent.
- `Step.v` was written but never checked on read, so a future format version would
  have produced bogus divergences rather than an honest refusal.
- `_PassThrough.call` did not accept `volatile=`, so a program using it raised
  `TypeError` when run *without* noidroid — defeating the point of the pass-through.

## [0.2.0] - 2026-08-19

### Fixed

- Release checksum files recorded `dist/<name>.tar.gz` as the path, so `sha256sum -c`
  failed for anyone who downloaded them anywhere else. Found by verifying the
  published v0.1.0 artifacts rather than by reading the workflow.
- CI re-downloaded Chromium on every run, which was most of the wall time on the
  slowest job. It is now cached against the Playwright version.

### Added

- **`noidroid export` / `noidroid import`** — a trajectory and everything it reaches
  as one committable JSON file. A recording is only a regression test if it can leave
  the machine it was made on, and `.noidroid/` is gitignored and machine-local. The
  bundle stays readable so a reviewer sees what the agent said in the diff, and every
  address is re-hashed on import: a bundle arrives from elsewhere, so its claim that
  an address holds given bytes is checked rather than believed.

- **`noidroid bisect <trajectory>`** — automatic causal attribution. Every recorded
  decision is re-run from its own checkpoint with a different choice, and the earliest
  one that changes the outcome is reported. A trace cannot answer which step *caused*
  a failure, because that is a question about a world that did not happen; the
  published baseline for judging it from a transcript is around 14% accurate. Each
  probe is a real trajectory that can be opened, diffed and replayed, and the prefix
  of each costs nothing because it is served from the recording. When nothing flips it
  says so and exits non-zero, rather than naming a plausible step and calling it the
  cause.

### Added

- **`noidroid run --watch <dir>`** records a directory you already have — your actual
  project — instead of a sandbox. It is read, never cleared. Snapshots skip `.git`,
  `node_modules`, `target` and the like, extendable with `.noidroidignore`, because
  hashing a real repository after every step is otherwise unaffordable.
- **`noidroid restore <traj>@<step>`** puts the files back as they were at a
  checkpoint. It snapshots what is currently there first and prints its address, so
  **`noidroid checkout-tree <address> <dir>`** is the way back. This is the most
  requested capability on coding-agent issue trackers by an order of magnitude, and it
  is about files rather than conversation.
- Reconstruction never touches a watched directory: replays and branches re-execute
  the program, which writes files, so they always get their own copy.

### Fixed

- Replaying a trajectory whose program is not present reported that the process never
  connected, which is true and unhelpful. It now says that a trajectory records what a
  program did rather than the program itself — the first thing anyone hits after
  importing a bundle.

- `materialize` pruned anything the recorded tree did not contain, without consulting
  the ignore list that had kept it *out* of the recording. Restoring into a real
  project would have deleted `.git`, `node_modules`, and the `.noidroid` directory
  holding the trajectory being restored from — in that order. Found by running it, not
  by reading it.

### Added

- **`noidroid run --auto`: zero-code recording.** A `sitecustomize.py` goes on the
  child's `PYTHONPATH` — the mechanism `opentelemetry-instrument` and `ddtrace-run`
  use — and patches the OpenAI and Anthropic base clients at `request`, below the
  retry loop, so one logical call is one recorded step however many times it retried.
  A program that never mentions Paranoid Android can now be recorded and replayed; the
  SDK's own response type is rebuilt on replay, so `reply.content[0].text` still
  works. Tested against the real Anthropic SDK with the API shut down.
  It cannot make anything *branchable*: no patching can infer that a value was a
  choice among alternatives, so `decide()` stays explicit. The honest shape is
  **zero code to record and replay, two lines to branch**.
- **`volatile=` on `call`** — names arguments that change every run without changing
  what the call means, such as a timestamp or a request id. Without it an argument
  carrying a clock makes every replay diverge, which is true and useless.
- **Divergence reports say what differed**, field by field, and point out when the
  call the run wants is recorded further along — which usually means an interaction
  was inserted or removed, rather than changed.

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
- **`noidroid.llm.Model`** — an adapter for the one input an agent cannot make
  deterministic. Recording model calls means a replay serves them back rather than
  calling the provider, so re-running an agent against a real conversation is free and
  deterministic; and the model's tool choice is declared as a decision on the agent's
  behalf, so branching to a different tool needs no instrumentation from the agent at
  all. Provider-agnostic: it takes a callable, imports no SDK, and understands both
  the Anthropic content-block and OpenAI `tool_calls` response shapes.
- An `examples/llm_agent/` worked example with a deterministic stand-in model, so it
  runs with no API key.

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

[Unreleased]: https://github.com/swalla02/noidroid/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/swalla02/noidroid/releases/tag/v0.2.0
[0.1.0]: https://github.com/swalla02/noidroid/releases/tag/v0.1.0
