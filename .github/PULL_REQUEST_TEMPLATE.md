Closes #

<!-- Every pull request closes an issue. If there isn't one, open it first: the issue
     is where the reasoning lives, and the PR is only where it lands. CI enforces
     this, so a PR without a linked issue will fail. -->

## What this changes

<!-- One or two sentences. The diff says what; say why. -->

## How it was verified

<!-- Commands you ran and what they printed. "Tests pass" is not evidence; the output
     is. If you added an invariant, name the test that would fail without it. -->

## Checklist

- [ ] Linked to an issue above with `Closes #N`
- [ ] `cargo test --all` passes (browser tests skip cleanly without Chromium)
- [ ] `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` pass
- [ ] `CHANGELOG.md` updated under `Unreleased`, if this is user-visible
- [ ] If the on-disk object format changed: `STEP_VERSION` decision made deliberately
      and recorded in the changelog (see CONTRIBUTING.md → Versioning)
- [ ] No capability is claimed that is not actually there; anything simulated says so
- [ ] Reviewed before merge — green CI is not a review
