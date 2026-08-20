---
description: Claim a GitHub issue — create or reuse its worktree, and start work there.
argument-hint: <issue number>
allowed-tools: Bash, Read, Grep, Glob, EnterWorktree
---

Start work on issue **$ARGUMENTS**.

1. `gh issue view $ARGUMENTS` — read the problem and the reasoning in the thread.
   If it is already closed, or someone else's branch is already open against it,
   say so and stop.
2. `.claude/scripts/issue-worktree.sh start $ARGUMENTS` — this prints the worktree
   path. It fetches `origin/main` first, names the branch from the issue's labels
   (`fix/`, `docs/`, `feat/`), and reuses an existing branch for the issue.
3. Move into that worktree — `EnterWorktree`, or tell the user to run `claude` from
   that directory if this session cannot switch. **Everything after this point
   happens in the worktree.** The primary checkout is read-only and the
   `PreToolUse` guard will deny edits there.
4. Work the issue: a test that fails without the fix, then the fix. `cargo test
   --all`, `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`.
5. Commit with Conventional Commits, then `gh pr create` with `Closes
   #$ARGUMENTS` in the body — CI fails the PR without it.

Do not touch anything the issue did not ask for. A second problem found on the way
is a second issue (`gh issue create`), not a second commit here.
