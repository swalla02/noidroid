# Working in this repository

## How to talk

Write like a staff engineer talking to another engineer. Simple and direct.

- Short sentences. Plain words. Default to a few lines, not a page.
- Lead with the answer. Context after, only if it changes what they do.
- Say what you did and what broke. Skip the summary of what you were asked to do.
- Name things precisely — file, function, line. Drop the adjectives around them.
- No preamble ("Great question", "I'll now proceed to"), no closing pitch, no
  restating the change you just made in prose the diff already shows.
- Uncertainty is stated once, plainly: "I didn't check X." Not hedged through
  a paragraph.
- Don't explain concepts the reader obviously knows. If you're unsure whether
  they know it, one clause, not a section.
- Disagree when you disagree, with the reason and a recommendation. Don't
  present a survey of options and make them choose.
- Prose over bullet lists unless the content is genuinely a list.
- Long only when asked for long. "Explain", "walk me through", "write the doc"
  are the ask; everything else gets the short version.

## One issue, one worktree, one session

Every issue is worked in its own git worktree, by its own Claude session. Two
sessions never share a working tree, so parallel work on two issues cannot
collide in the index, in `target/`, or in a half-finished edit.

```bash
.claude/scripts/issue-worktree.sh start 46   # prints ../noidroid-worktrees/46
.claude/scripts/issue-worktree.sh list       # who is working on what
.claude/scripts/issue-worktree.sh done 46    # after the issue is closed
```

`start` derives the branch from the issue — `fix/46-slug` for a `bug` label,
`docs/` for `documentation`, `feat/` otherwise — branches it from a freshly
fetched `origin/main`, and reuses an existing branch for that issue if one is
already there. Run it, then open the session **in the directory it prints**:
`EnterWorktree`, or `claude` started from that path.

The primary checkout is read-only. A `PreToolUse` hook denies `Edit`, `Write`
and `NotebookEdit` there and tells you what to run instead. It is a tripwire on
the file tools, not a sandbox — it does not watch shell redirects, so don't
route around it. The deliberate exception is `NOIDROID_MAIN_EDIT=1` (a release
commit, or editing the guard itself).

When the issue is closed, the worktree goes. `done <issue>` refuses while the
issue is open or the tree is dirty, then removes the directory and deletes the
merged branch. `prune` does that for every closed issue at once. A worktree
whose issue closed and which nobody removed is a stale claim on a branch name —
`list` shows it as `[CLOSED]`.

The rest of the loop — issue first, `Closes #N` in the PR body, review before
merge — is in [CONTRIBUTING.md](../CONTRIBUTING.md). CI enforces the `Closes #N`.

## Where the rules live

- [CONTRIBUTING.md](../CONTRIBUTING.md) — the loop, branch and commit names,
  testing, `STEP_VERSION`, and the two kinds of change that get refused.
- [docs/direction.md](../docs/direction.md) — what the project is for, and which
  decisions are already settled.
- [research/constraints.md](../research/constraints.md) — settled decisions with
  their reasons. Read before proposing something the project already rejected.
