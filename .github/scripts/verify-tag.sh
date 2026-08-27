#!/usr/bin/env bash
# Decide whether a tag is allowed to become a release.
#
#   verify-tag.sh          run from the repository root, on a tag checkout
#
# The release workflow used to answer this by running `cargo test --all` a second
# time. That re-run was weaker than CI -- one Linux runner, no macOS job, no
# no-browser job, no example job -- and it was the flakiest part of the release
# path: v0.2.0 timed out after six hours in the Playwright install, and v0.3.0
# failed on a browser test that CI had already passed on the same commit. Neither
# tag ever produced an artifact, and the commit's real problem (a red macOS job)
# went unexamined.
#
# So this does not re-run anything. It checks the three things the workflow's own
# header comment has always claimed: the tag matches the declared version, the
# commit is on main, and CI was green for that exact commit.
#
# Environment:
#   GITHUB_REF_NAME     the tag being released, `vX.Y.Z`
#   GITHUB_REPOSITORY   `owner/repo`, for the check-runs lookup
set -euo pipefail

# Mirrors the required status checks on main's branch protection. A check added
# there and not here still guards the pull request; it just stops guarding the
# release, which is the drift this list exists to make visible.
REQUIRED_CHECKS=(
  "fmt + clippy"
  "tests (with browser)"
  "tests (no browser installed)"
  "tests (macOS)"
  "the example actually runs"
)

die() { echo "::error::$*" >&2; exit 1; }

: "${GITHUB_REF_NAME:?the tag to verify}"
: "${GITHUB_REPOSITORY:?owner/repo}"

tagged="${GITHUB_REF_NAME#v}"

# 1. The tag must match the version in Cargo.toml.
declared=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
[ "$declared" = "$tagged" ] ||
  die "tag $GITHUB_REF_NAME does not match Cargo.toml version $declared"

# 2. The commit must be on main. A tag cut from a branch that never merged would
#    otherwise publish code nobody reviewed -- the pull request gate applies to
#    main, and this is what carries that gate over to the release.
sha=$(git rev-list -n1 "$GITHUB_REF_NAME^{commit}")
git fetch --quiet origin main
git merge-base --is-ancestor "$sha" origin/main ||
  die "$GITHUB_REF_NAME ($sha) is not an ancestor of origin/main"

# 3. CI must have been green for that commit. Not for main, and not for a fresh
#    run here: for the commit being released. The default `latest` filter folds
#    re-runs, so each check appears once, with the conclusion that stands.
checks=$(gh api --paginate "repos/$GITHUB_REPOSITORY/commits/$sha/check-runs" \
  --jq '.check_runs[] | "\(.name)\t\(.conclusion)"')

for name in "${REQUIRED_CHECKS[@]}"; do
  conclusion=$(printf '%s\n' "$checks" | awk -F'\t' -v n="$name" '$1 == n {print $2}')
  [ -n "$conclusion" ] ||
    die "required check '$name' never ran for $sha"
  [ "$conclusion" = "success" ] ||
    die "required check '$name' concluded '$conclusion' for $sha"
done

echo "$GITHUB_REF_NAME ($sha) is on main, declares $declared, and passed CI."
