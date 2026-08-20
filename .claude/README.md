# `.claude/` — how Claude works in this repository

Two things live here, and they are checked in so every session gets the same rules:

1. **The working agreement** — how to talk, and the one-issue-one-worktree rule that
   keeps parallel sessions from colliding. `CLAUDE.md`, `settings.json`,
   `hooks/`, `scripts/`, and the `/issue` commands.
2. **The scout** — a technical intelligence system that reads the outside world and
   writes ranked recommendations into `research/`. `agents/`, `skills/`, and the
   `/scout` commands.

## 1. The working agreement

```
.claude/
  CLAUDE.md                      loaded into every session: how to talk, how to work
  settings.json                  the worktree guard, and the permissions it needs
  hooks/worktree-guard.py        PreToolUse: denies Edit/Write in the primary checkout
  scripts/issue-worktree.sh      start | list | done | prune, keyed by issue number
  commands/issue.md              /issue <n> — claim an issue and open its worktree
  commands/issue-done.md         /issue-done [n] — retire a finished one
```

**One issue, one worktree, one session.** Work on issue 46 happens in
`../noidroid-worktrees/46` on branch `fix/46-…`, in a session started there. Nothing
else touches that tree. The primary checkout is read-only — a `PreToolUse` hook denies
`Edit`, `Write` and `NotebookEdit` there and prints the command to run instead.

```bash
/issue 46                                    # or, by hand:
.claude/scripts/issue-worktree.sh start 46   # prints the worktree path
.claude/scripts/issue-worktree.sh list       # who is working on what
/issue-done 46                               # after the PR merged and the issue closed
.claude/scripts/issue-worktree.sh prune      # retire every closed issue at once
```

The worktree is removed when the issue closes, or reused by the next session that runs
`start` on a new issue number. `done` refuses on a dirty tree, so nothing is discarded
by accident. The guard watches the file tools, not shell redirects — it is a tripwire,
not a sandbox. `NOIDROID_MAIN_EDIT=1` is the deliberate exception, for a release commit
or for editing the guard itself.

The rest of the loop — issue first, `Closes #N`, review before merge — is in
[CONTRIBUTING.md](../CONTRIBUTING.md).

## 2. The Paranoid Android scout

> Find ideas the Paranoid Android team would otherwise miss, understand them deeply,
> and turn them into actionable engineering opportunities.

It is **not** a news bot. It reads primary sources, extracts mechanisms, checks them
against our actual code, and produces ranked recommendations with the reasoning shown —
including what we should explicitly *not* do.

### What is here

```
.claude/
  agents/scout.md                       the sub-agent: identity, loop, hard rules
  commands/scout.md                     /scout [question] — run a research cycle
  commands/scout-verdict.md             /scout-verdict — record what happened to a recommendation
  skills/technical-scouting/
    SKILL.md                            the methodology, usable without the sub-agent
    references/sources.md               source mix, vocabulary ladders, query patterns
    references/intelligence-card.md     card schema, template, worked example
    references/prioritisation.md        Impact × Relevance × Feasibility × Novelty
    references/landscape.md             classification of adjacent projects
    references/negative-space.md        failed approaches and unserved problems
```

and the knowledge base it writes to, which is the durable half:

```
research/
  README.md          index of everything discovered, newest first
  CONTEXT.md         the architecture the scout reasons against
  constraints.md     settled decisions — do not re-propose without new evidence
  taxonomy.md        the evolving category list
  decisions.md       feedback ledger: recommendation → verdict → outcome → lesson
  discoveries/       intelligence cards
  landscape/         adjacent and competing projects
  proposals/         build proposals promoted from cards
  scans/             per-run reports, each ending in ranked Recommended Actions
  archive/           superseded cards, with the reason
  templates/
```

### Using it

```
/scout                                        # the broad one: what should we build next?
/scout find new techniques for deterministic replay
/scout what changed in agent observability this month?
/scout look for robotics systems that replay real-world trajectories
/scout find unusual approaches to state snapshotting

/scout-verdict 2026-08-19-some-card — rejected — needs a STEP_VERSION break for a 3% win
```

Or ask for it in prose — "have the scout look into X" — and the sub-agent will be
dispatched.

`/scout-verdict` is the part that makes this a system rather than a search engine. A
recommendation that was rejected, or prototyped and dropped, becomes a constraint the
next run honours. Use it every time a recommendation gets an answer.

### The two rules that keep it honest

1. **The scout writes only to `research/`.** It never touches `crates/`, `clients/`,
   `examples/` or the top-level docs. It produces intelligence; humans and the
   engineering agent decide what becomes code. The only exception is an explicit
   instruction to prototype, in an isolated branch you name.
2. **It reads `research/constraints.md` before recommending anything.** This project
   has settled decisions with reasons behind them; re-proposing one without new
   evidence is the failure mode that would make the whole thing noise.

### Sharing it

The system is self-contained: copy `.claude/` and `research/templates/` into another
repository, then rewrite `research/CONTEXT.md` and `research/constraints.md` for that
project. Those two files are the only place the scout's domain knowledge lives —
everything else is method.

For this repository, keep `research/CONTEXT.md` current. It states the commit it was
last verified against, and it tells the scout how to re-verify itself. A briefing
nobody maintains is worse than none.
