---
description: Close the research loop — record what actually happened to a scout recommendation.
argument-hint: <card-id or recommendation> — <adopted | prototyped | rejected | deferred> — <what happened and why>
allowed-tools: Read, Edit, Write, Grep, Glob, Bash
---

Record the outcome of a previous scout recommendation so future research learns from
it:

**$ARGUMENTS**

Steps:

1. Find the card and the scan it came from (`grep -ril` in `research/`). If you cannot
   identify which recommendation this refers to, ask rather than guessing.
2. Append an entry to `research/decisions.md` using the table and the detail format
   already in that file: the card id, the verdict, the date, what actually happened,
   and — most important — **the constraint this leaves behind for future research**.
3. Update the card's `## Changelog` and, if the verdict changes its status, its
   `recommendation:` frontmatter.
4. If the outcome hardened into a decision that should not be re-litigated, promote a
   one-line entry to `research/constraints.md` with the reason and the date, matching
   the existing style.
5. If the card is now superseded, move it to `research/archive/` and leave a pointer
   line in `research/README.md`.

Be exact about *why*. A rejection reason of "not a priority" teaches nothing; "rejected
because it requires a `STEP_VERSION` break for a 3% storage win" is a constraint that
saves a future run a day of work.
