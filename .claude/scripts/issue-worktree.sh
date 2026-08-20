#!/usr/bin/env bash
# One issue, one worktree, one session.
#
#   issue-worktree.sh start <issue>   create (or reuse) the worktree for an issue
#   issue-worktree.sh list            what is checked out where, and its issue state
#   issue-worktree.sh done <issue>    remove the worktree once the issue is closed
#   issue-worktree.sh prune           remove every worktree whose issue is closed
#
# Worktrees live beside the repository, never inside it:
#   /path/to/noidroid            the primary checkout — read-only, main only
#   /path/to/noidroid-worktrees/ one directory per open issue
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
common=$(cd "$repo_root" && cd "$(git rev-parse --git-common-dir)" && pwd)
main_root=$(dirname "$common")
trees="$(dirname "$main_root")/$(basename "$main_root")-worktrees"

die() { echo "$*" >&2; exit 1; }

# fix/ for bugs, docs/ for documentation, feat/ for everything else.
type_for() {
  case " $1 " in
    *" bug "*)           echo fix ;;
    *" documentation "*) echo docs ;;
    *)                   echo feat ;;
  esac
}

slugify() {
  echo "$1" | tr '[:upper:]' '[:lower:]' \
    | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//' \
    | cut -c1-40 | sed -E 's/-+$//'
}

branch_for() {  # an existing branch for this issue wins, so a resumed session lands on its own work
  local n=$1
  local existing
  existing=$(git for-each-ref --format='%(refname:short)' refs/heads refs/remotes/origin \
    | sed 's|^origin/||' | sort -u | grep -E "^[a-z]+/${n}-" | head -1 || true)
  if [ -n "$existing" ]; then echo "$existing"; return; fi

  local title labels
  title=$(gh issue view "$n" --json title -q .title) || die "no issue #$n"
  labels=$(gh issue view "$n" --json labels -q '[.labels[].name] | join(" ")')
  echo "$(type_for " $labels ")/${n}-$(slugify "$title")"
}

path_for() { echo "$trees/$1"; }

cmd_start() {
  local n=${1:?usage: issue-worktree.sh start <issue>}
  local state
  state=$(gh issue view "$n" --json state -q .state) || die "no issue #$n"
  [ "$state" = OPEN ] || die "issue #$n is $state — nothing to work on"

  local branch dir
  branch=$(branch_for "$n")
  dir=$(path_for "$n")

  if [ -d "$dir" ]; then
    echo "$dir"   # already claimed; another session may be in it
    return
  fi

  git -C "$main_root" fetch --quiet origin main
  mkdir -p "$trees"
  if git -C "$main_root" show-ref --verify --quiet "refs/heads/$branch"; then
    git -C "$main_root" worktree add "$dir" "$branch" >&2
  elif git -C "$main_root" show-ref --verify --quiet "refs/remotes/origin/$branch"; then
    git -C "$main_root" worktree add --track -b "$branch" "$dir" "origin/$branch" >&2
  else
    git -C "$main_root" worktree add -b "$branch" "$dir" origin/main >&2
  fi
  echo "$dir"
}

cmd_list() {
  git -C "$main_root" worktree list --porcelain | awk '/^worktree /{print $2}' | while read -r dir; do
    local_branch=$(git -C "$dir" symbolic-ref --quiet --short HEAD 2>/dev/null || echo DETACHED)
    n=$(basename "$dir")
    if [ "$dir" = "$main_root" ]; then
      printf '%-6s %-44s %s\n' '-' "$local_branch" "$dir (primary, read-only)"
    else
      state=$(gh issue view "$n" --json state -q .state 2>/dev/null || echo '?')
      printf '#%-5s %-44s %s\n' "$n" "$local_branch" "$dir [$state]"
    fi
  done
}

cmd_done() {
  local n=${1:?usage: issue-worktree.sh done <issue>}
  local dir state
  dir=$(path_for "$n")
  [ -d "$dir" ] || die "no worktree for #$n"
  state=$(gh issue view "$n" --json state -q .state)
  [ "$state" = CLOSED ] || die "issue #$n is still open — close it through its PR first"

  git -C "$dir" diff --quiet && git -C "$dir" diff --cached --quiet \
    || die "$dir has uncommitted changes"
  branch=$(git -C "$dir" symbolic-ref --quiet --short HEAD || true)

  git -C "$main_root" worktree remove "$dir"
  [ -n "$branch" ] && git -C "$main_root" branch -D "$branch" >/dev/null 2>&1 || true
  echo "removed $dir"
}

cmd_prune() {
  git -C "$main_root" worktree prune
  [ -d "$trees" ] || return 0
  for dir in "$trees"/*; do
    [ -d "$dir" ] || continue
    n=$(basename "$dir")
    [ "$(gh issue view "$n" --json state -q .state 2>/dev/null || echo OPEN)" = CLOSED ] || continue
    cmd_done "$n" || true
  done
}

case "${1:-}" in
  start) shift; cmd_start "$@" ;;
  list)  shift; cmd_list "$@" ;;
  done)  shift; cmd_done "$@" ;;
  prune) shift; cmd_prune "$@" ;;
  *) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 1 ;;
esac
