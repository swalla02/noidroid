# Negative space

The things that do not work are cheaper to find than the things that do, and they are
worth more. Most of a scout's unique value is here, because positive findings are
also findable by anyone reading a newsletter.

## Four kinds of negative finding

**1. The failed approach.** Someone tried the obvious thing and it did not work.
Record *why*, in mechanism terms. This is how we avoid spending a release rediscovering
it.

Where to look: `CHANGELOG` entries that remove a feature, "why we moved off X" posts,
closed PRs with long discussions, `git log` for a deleted module, retrospective talks.

**2. The abandoned project.** Last commit two years ago, issues unanswered. Ask what
killed it: no users, an unsolvable technical problem, or a maintainer who left. Only
the middle one is a finding, but it is a big one.

Where to look: last-commit dates, an issue titled "is this project still maintained?",
a fork that became the real one, a final release note.

**3. The recurring complaint.** The same problem reported across unrelated projects.
Five independent teams working around the same missing capability is a specification.

Where to look: search the *complaint* rather than the solution —
`"non-deterministic" flaky replay`, `"we had to" patch monkeypatch intercept`,
`"there is no way to"`, `"gave up on"`, `"this is impossible"`, `"we ended up doing it
by hand"`.

**4. The ugly integration.** A tool that only works via `LD_PRELOAD`, a fork of a
dependency, a regex over a log, a patched vendor SDK. Ugly integrations mark a
**missing seam**. Where a seam is missing, whoever provides one wins the space.

## How to file it

Negative findings are cards like any other. They live in
`research/discoveries/` with `categories: [negative-signal, ...]`.

The sections change meaning slightly:

- `## What is interesting` → what was tried and the mechanism by which it failed.
- `## Why it matters to Paranoid Android` → either "we are about to do this" (a
  warning) or "nobody has solved this" (an opportunity). Say which.
- `## Recommendation` → usually IGNORE (recorded so we do not repeat it) or
  INVESTIGATE (the unserved problem might be ours).

## The unserved-problem card

When several unrelated sources hit the same wall, promote it. Use the same card format
with `categories: [unserved-problem, ...]` and add:

```markdown
## Who hits this
- <project> — <link to the issue/post where they hit it>
- <project> — <link>
- <project> — <link>

At least three independent sources, or it is an anecdote.

## Why it is unsolved
The structural reason, not "nobody got around to it". If the reason is structural, say
whether our architecture escapes it or shares it.

## Would Paranoid Android's model help?
Honestly. Sometimes the answer is no, and that is a finding too — it tells us where
our model's boundary is.
```

## Guard against motivated reasoning

The failure mode of this section is finding an "unserved problem" that flatters the
architecture we already have. Two defences:

1. Require the complaints to come from **projects that have nothing to do with us** —
 if every source is an agent-debugging tool, you have found a crowded market, not a gap.
2. Write the strongest version of "and this is why it will stay unsolved, including for
 us" before writing the recommendation. If that paragraph is convincing, the
 recommendation is WATCH.

## Negative findings about ourselves

The scout is allowed — expected — to bring back evidence that a decision this project
made is wrong. `docs/direction.md` lists settled decisions and explicitly invites
reopening them "with something that was not known at the time". That is the bar:
new evidence, named, dated, with the mechanism spelled out. Meeting it is a valuable
run. Failing to meet it and arguing anyway is noise.
