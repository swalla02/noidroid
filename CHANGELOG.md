# Changelog

All notable changes to this project are documented here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). This project uses
[semantic versioning](https://semver.org/); see [CONTRIBUTING.md](CONTRIBUTING.md) for
how the package version relates to `STEP_VERSION`, the on-disk object format.

## [Unreleased]

### Added

- Continuous integration: fmt, clippy (`-D warnings`), the full suite on Linux with a
  browser, the suite without a browser (the browser tests must skip cleanly), the
  suite on macOS, and the CLI example end to end.
- A release workflow triggered by `v*` tags, which refuses a tag that disagrees with
  `Cargo.toml` or has no changelog section, and publishes Linux and macOS binaries.
- `format_is_pinned`: a test asserting the exact serialised bytes and digest of a
  known step, so a change to the on-disk format cannot pass unnoticed.
- `CONTRIBUTING.md`, `SECURITY.md`, issue and pull-request templates, Dependabot, and
  a pinned toolchain.

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
- Two worked examples and 23 tests, two of which drive real Chromium.

### Known limitations

Documented in the README, and deliberate: not zero-code, sequential programs only,
only the sandboxed workspace is captured, the ambient environment is not captured, a
branch is not a prediction, browser reconstruction is bounded by the recorded page
set, and no scale work.

[Unreleased]: https://github.com/swalla02/noidroid/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/swalla02/noidroid/releases/tag/v0.1.0
