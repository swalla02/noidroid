---
date: YYYY-MM-DD
cadence:            # targeted | periodic | open
question:
cards_created: []
cards_updated: []
---

# Scan: <question>

## In one paragraph

What was investigated, roughly how much, what survived, and the single most important
thing the team should take away. Written so someone who reads nothing else is not
misled.

## What survived

For each: one paragraph, the card id, and the reason it matters *here*.

## Looked at, not pursued

One line each, with the reason. This section is how the next run avoids repeating this
one.

## Negative findings

Failed approaches, abandoned projects, recurring complaints, ugly integrations. Say
whether each is a warning to us or an opportunity for us.

## What we now know that we did not

The unknowns this run closed, stated as facts.

## Still unknown

The questions this run raised and could not answer, phrased so the next run can pick
them up.

# Recommended Actions

Ranked by Impact × Relevance × Feasibility × Novelty. Full rubric in
`.claude/skills/technical-scouting/references/prioritisation.md`.

### 1. <imperative sentence>

**Why now:** <the trigger — what changed>

**Impact:** n — <why>
**Relevance:** n — <why>
**Feasibility:** n — <why>
**Novelty:** n — <why>
**Score:** n

**Cost:** <a spike / a PR / a release — and the risk that would blow it up>
**What we would learn:** <a question whose answer could be "no">
**Touches:** <subsystems and files>
**Evidence:** <card ids>

## Explicitly not recommended

The plausible-looking things considered and rejected, with reasons. Feeds
`constraints.md`.
