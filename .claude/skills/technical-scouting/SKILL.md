---
name: technical-scouting
description: Use when researching the outside technical landscape for Paranoid Android — deterministic replay, checkpointing, provenance, storage, agent infrastructure, simulation, testing, robotics, scientific workflows — or when asked what the project should build next. Turns discoveries into intelligence cards and ranked recommendations in research/, with deduplication against what is already known.
---

# Technical scouting

The methodology behind the `scout` sub-agent. Read this when running a scan yourself,
when reviewing what the scout produced, or when extending the system.

The scout's purpose in one line:

> Find ideas the Paranoid Android team would otherwise miss, understand them deeply,
> and turn them into actionable engineering opportunities.

## Ground rules

| Rule | Why |
| --- | --- |
| Primary sources only | A summary of a summary is how wrong architecture gets adopted. |
| Mechanism over claim | "Uses CoW snapshots" is a headline. *How* they handle open file descriptors is a finding. |
| Volume is a smell | One discovery that changes the architecture beats thirty links. |
| Write only to `research/` | The scout produces intelligence. Humans decide what becomes code. |
| Dedup before writing | Same project twice is noise; update the card instead. |
| Negative findings count | "Five projects gave up on X" is a product opportunity. |
| Check the code before recommending | Half the obvious ideas are already built. |

## The pipeline

```
question → broad sweep → candidate set → primary-source read → mechanism extraction
  → relevance test → dedup → intelligence card → scoring → recommended actions
  → engineering decision → outcome recorded → constraint for the next run
```

The last two arrows are what make this a system rather than a search. They live in
`research/decisions.md` and `research/constraints.md`.

### 1. Frame the question

Three cadences, and they behave differently:

- **Targeted** — "find new techniques for deterministic replay". Narrow sweep, deep
  reads, expect 1–3 cards.
- **Periodic scan** — "what changed in agent observability this month?". Wide sweep,
  shallow reads, expect mostly landscape updates and few cards.
- **Open** — "what should Paranoid Android build next?". The broadest form: sweep
  every active category in `research/taxonomy.md`, weight by our current roadmap and
  open issues, and end with a ranked build recommendation. This is the expensive one;
  budget for it.

### 2. Sweep

Use `references/sources.md`: the source mix, the vocabulary ladders (how to search
for our problems in other fields' words), and the query patterns that work per source.

Cast wider than the obvious domain. The connection you are looking for is usually in
a field that has had the problem for twenty years and does not know we exist.

### 3. Read the primary source

Do not stop at the README. Where it matters:

- the data structure and the invariant it protects
- the code path for the mechanism you care about (find it, name the file)
- the limitations / future work / "known issues" section
- the issue tracker for where it breaks in practice
- the design discussion where an alternative was rejected
- what it cites, and who cites it
- benchmarks, and whether they measure the thing that would matter to us

### 4. Test relevance

Ask, in order:

1. Could this make Paranoid Android better?
2. How — which subsystem, which file, which invariant?
3. Should we actually build something because of it?

Anything that fails (1) gets a single line in the scan report and no card.

### 5. Dedup

```bash
grep -ril "github.com/<org>/<repo>" research/
grep -ril "arxiv.org/abs/<id>" research/
grep -ril "<project name>" research/
```

Existing card → update it, append to its `## Changelog`, bump `updated:`.

### 6. Card

`research/discoveries/YYYY-MM-DD-<slug>.md`, schema in
`references/intelligence-card.md`. Stable id, never renamed. Superseded cards move to
`research/archive/` with a pointer, they are not deleted — the reason something was
dropped is part of the knowledge base.

Adjacent projects get a landscape entry instead or as well:
`research/landscape/<slug>.md`, taxonomy in `references/landscape.md`.

### 7. Score and rank

`references/prioritisation.md`. Impact × Relevance × Feasibility × Novelty, each on a
defined scale, with the reasoning shown. The ranking is the deliverable; the cards are
the evidence for it.

### 8. Report

`research/scans/YYYY-MM-DD-<topic>.md` from `research/templates/scan.md`. Then update
`research/README.md`.

## Knowledge base layout

```
research/
  README.md          index of everything, newest first
  CONTEXT.md         the architecture the scout reasons against (verify against code)
  constraints.md     settled decisions — do not re-propose without new evidence
  taxonomy.md        the evolving category list
  decisions.md       feedback ledger: recommendation → verdict → outcome → lesson
  discoveries/       intelligence cards
  landscape/         adjacent and competing projects
  proposals/         worked-up build proposals promoted from cards
  scans/             per-run reports
  archive/           superseded cards, with the reason
  templates/         card, scan and landscape templates
```

It **accumulates**. Nothing is overwritten wholesale, ever.

## Anti-patterns

- A card whose "why it matters" could be pasted onto any project. Be specific to our
  code or do not write it.
- A recommendation that says "consider looking into X". Say what to build, where, and
  what it would prove.
- A "competitor" framing. The useful question is *what does this do that we should
  learn from*, not *are they beating us*.
- Reopening a settled decision because a search result made it sound new.
- Confidence HIGH on a source you did not open.

## References

- `references/sources.md` — source mix, vocabulary ladders, query patterns
- `references/intelligence-card.md` — card schema, template, worked example
- `references/prioritisation.md` — scoring rubric and ranking method
- `references/landscape.md` — classification taxonomy for adjacent projects
- `references/negative-space.md` — hunting failed approaches and unserved problems
