# Contributing

Read [docs/direction.md](docs/direction.md) first — it says what the project is for
and which decisions are already settled. This file says how to work on it.

Paranoid Android is young enough that the interesting conventions are about *what we
promise*, not about style. Style is enforced by CI; the rest is below.

## The loop

```bash
git switch -c feat/short-description        # or fix/, chore/, docs/, refactor/
cargo test --all                            # 24 tests; browser ones skip without Chromium
cargo fmt --all && cargo clippy --all-targets -- -D warnings
git commit                                  # Conventional Commits, see below
gh pr create --fill
```

`main` is protected: it takes merges through pull requests, and CI has to be green.
Nobody pushes to it directly, including maintainers.

### Branches

`<type>/<short-description>`, where type is one of `feat`, `fix`, `chore`, `docs`,
`refactor`, `test`, `perf`. Keep a branch to one change; two changes are two PRs.

### Commits

[Conventional Commits](https://www.conventionalcommits.org/), with the scope naming
the area: `feat(browser)`, `fix(engine)`, `docs(readme)`. The body should say *why*,
because the diff already says what. Breaking changes get a `!` and a
`BREAKING CHANGE:` footer.

### Pull requests

CI runs fmt, clippy (`-D warnings`), the full suite on Linux with a browser, the suite
without a browser (the browser tests must skip cleanly, not fail), the suite on macOS,
and the CLI example end to end. All of it has to pass.

## Testing

```bash
cargo test --all                                  # everything
cargo test --test vertical_slice                  # the core invariants
cargo test --test browser_slice                   # real Chromium; skips if absent
pip install playwright && playwright install chromium
```

The tests that matter drive real child processes over the real protocol, because the
claims worth testing — *a replay cannot touch the world*, *a branch cannot mutate its
parent* — are claims about what happens between processes. A test that mocks the
protocol proves nothing about them.

If you add an invariant, add a test that would fail without it, and name the test
after the invariant rather than after the function.

## Versioning

Two version numbers matter, and they are not the same thing.

### 1. The package version (semver)

`Cargo.toml` and `clients/python/pyproject.toml` carry the same version. Pre-1.0, a
minor bump may break APIs; we say so in the changelog.

Releases are cut from tags:

```bash
# on main, with CI green
$EDITOR CHANGELOG.md            # move Unreleased into a new version section
$EDITOR Cargo.toml clients/python/pyproject.toml
git commit -m "chore(release): 0.2.0"
git tag -a v0.2.0 -m "Paranoid Android 0.2.0"
git push origin main v0.2.0     # the tag triggers the release workflow
```

The release workflow refuses a tag whose version disagrees with `Cargo.toml`, or that
has no changelog section.

### 2. `STEP_VERSION` — the on-disk object format

This is the one that needs care. Trajectories are content-addressed: an object's name
*is* the hash of its bytes. So a change to how an object serialises changes every
address derived from it, which means:

> **A format change silently invalidates every recording anyone has ever made.**
> A replay of an older trajectory would re-derive different hashes and be reported as
> divergent, with nothing to indicate that the tool changed rather than the program.

Rules:

- **Byte-compatible additions do not bump the version.** A new field that is `default`
  on read and `skip_serializing_if` on write, so that existing objects serialise to
  exactly the bytes they already have, is safe. `Effect::outcome` was added this way.
- **Anything else bumps `STEP_VERSION`** and needs a note in the changelog saying that
  old trajectories cannot be replayed by this version.
- `format_is_pinned` in `crates/noidroid-core/src/model.rs` asserts the exact
  serialised bytes and digest of a known step. If your change fails it, that is the
  test doing its job: decide deliberately which of the two rules above applies, then
  update the fixture in the same commit as the version bump.

Timing, host, pid and per-run delivery deliberately live *outside* hashed content. If
you are tempted to put something run-specific into a hashed object, that is what
`notes/` is for.

## The bar for a change

Two things get refused regardless of how well they are written:

1. **A capability we do not have.** If something is simulated, say so in the output.
   If something cannot be reconstructed, make the boundary explicit. Fidelity theatre
   — a percentage that is not a real measurement, a fuzzy comparison presented as
   verification — is the one failure mode this project cannot survive.
2. **Speculative infrastructure.** A dashboard, a plugin system, an adapter for an
   environment nobody has recorded yet. Build the thing that proves the next claim.

## Reporting problems

Bugs and design discussion go to GitHub Issues. Security reports go to
[SECURITY.md](SECURITY.md) instead.
