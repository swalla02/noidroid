## What this changes

<!-- One or two sentences. The diff says what; say why. -->

## How it was verified

<!-- Commands you ran and what they printed. "Tests pass" is not evidence; the output
     is. If you added an invariant, name the test that would fail without it. -->

## Checklist

- [ ] `cargo test --all` passes (browser tests skip cleanly without Chromium)
- [ ] `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` pass
- [ ] `CHANGELOG.md` updated under `Unreleased`, if this is user-visible
- [ ] If the on-disk object format changed: `STEP_VERSION` decision made deliberately
      and recorded in the changelog (see CONTRIBUTING.md → Versioning)
- [ ] No capability is claimed that is not actually there; anything simulated says so
