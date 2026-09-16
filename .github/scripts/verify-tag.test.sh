#!/usr/bin/env bash
# What verify-tag.sh must refuse.
#
#   verify-tag.test.sh
#
# Every case builds a throwaway repository with a real `origin/main`, so the
# ancestry check runs for real. Only `gh` is stubbed: it stands in for
# `gh api ... --jq`, and prints the tab-separated `name<TAB>conclusion` lines
# that call produces.
#
# Nothing here runs git in the checkout it was invoked from. Every git command
# is confined to $repo, under a temporary directory, and refuse_outside_tmp
# enforces that rather than trusting it -- an earlier version of this file did
# not, and committed to the branch it was testing.
set -euo pipefail

script=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/verify-tag.sh
passed=0
failed=0
repo=""

# The five required checks, all green -- the shape of a commit that may ship.
ALL_GREEN='fmt + clippy	success
tests (with browser)	success
tests (no browser installed)	success
tests (macOS)	success
the example actually runs	success'

refuse_outside_tmp() {
  case "$repo" in
    /tmp/*|/var/folders/*) ;;
    *) echo "refusing to run git in '$repo'" >&2; exit 1 ;;
  esac
}

# Build a repository whose HEAD is on origin/main, tagged, declaring $1.
fixture() {
  local version="$1" work
  work=$(mktemp -d)
  repo="$work/repo"
  refuse_outside_tmp
  git init -q --bare "$work/origin.git"
  git clone -q "$work/origin.git" "$repo" 2>/dev/null
  (
    cd "$repo"
    git config user.email test@example.com
    git config user.name test
    printf '[workspace.package]\nversion = "%s"\n' "$version" > Cargo.toml
    git add Cargo.toml
    git commit -qm "release $version"
    git push -q origin HEAD:main
    git tag "v$version"
  )
}

# Add a commit the fixture never pushed, and move the tag onto it.
tag_off_main() {
  refuse_outside_tmp
  (
    cd "$repo"
    git commit -q --allow-empty -m "not pushed to main"
    git tag -f "$GITHUB_REF_NAME" >/dev/null 2>&1
  )
}

check() {
  local name="$1" want="$2" checks="$3" bin out status
  refuse_outside_tmp
  bin=$(mktemp -d)
  cat > "$bin/gh" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$STUB_CHECKS"
STUB
  chmod +x "$bin/gh"

  out=$(
    cd "$repo"
    STUB_CHECKS="$checks" PATH="$bin:$PATH" \
      GITHUB_REPOSITORY=swalla02/noidroid \
      bash "$script" 2>&1
  ) && status=0 || status=$?

  if [ "$want" = "$status" ]; then
    passed=$((passed + 1))
    echo "ok   - $name"
  else
    failed=$((failed + 1))
    echo "FAIL - $name (wanted exit $want, got $status)"
    printf '       %s\n' "$out"
  fi
}

# --- a tag that should ship -------------------------------------------------

export GITHUB_REF_NAME=v0.4.0
fixture 0.4.0
check "a green commit on main is released" 0 "$ALL_GREEN"

# --- the v0.3.0 regression --------------------------------------------------
#
# The old gate re-ran the suite on its own runner and failed on a browser test
# that CI had passed. What was actually wrong with that commit was the macOS
# job, which the re-run never covered. This is the case that fails without the
# fix, because the old gate had no way to see it.

check "a red macOS job stops the release" 1 \
  "${ALL_GREEN/tests (macOS)	success/tests (macOS)	failure}"

check "a cancelled job stops the release" 1 \
  "${ALL_GREEN/fmt + clippy	success/fmt + clippy	cancelled}"

check "a required check that never ran stops the release" 1 \
  "$(printf '%s\n' "$ALL_GREEN" | grep -v '^tests (macOS)')"

# The release workflow's own failed attempt stays attached to the commit. It is
# not a required check, so a re-run after a fixed gate must not deadlock on it.
check "the release job's own earlier failure is ignored" 0 \
  "$ALL_GREEN
gate the tag on the full suite	failure"

# --- the two checks that predate this script --------------------------------

export GITHUB_REF_NAME=v0.9.9
check "a tag that disagrees with Cargo.toml stops the release" 1 "$ALL_GREEN"

export GITHUB_REF_NAME=v0.5.0
fixture 0.5.0
tag_off_main
check "a tag off main stops the release" 1 "$ALL_GREEN"

echo
echo "$passed passed, $failed failed"
[ "$failed" -eq 0 ]
