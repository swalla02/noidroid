---
description: Retire the worktree for a closed issue, or list what is still claimed.
argument-hint: [issue number, or blank to list and prune everything closed]
allowed-tools: Bash, Read
---

Retire finished work: **$ARGUMENTS**

With an issue number:

1. Confirm the PR merged and the issue is closed — `gh issue view <n> --json state`.
   If it is still open, stop and say what is missing (unmerged PR, failing CI,
   unreviewed).
2. `.claude/scripts/issue-worktree.sh done <n>` — refuses on a dirty tree, then
   removes the directory and deletes the branch.

With no argument:

1. `.claude/scripts/issue-worktree.sh list` — every worktree, its branch, and its
   issue state.
2. `.claude/scripts/issue-worktree.sh prune` — retire every worktree whose issue
   is closed.
3. Report anything left behind: a `[CLOSED]` worktree that would not remove is
   uncommitted work someone abandoned. Name the path and stop; do not discard it.
