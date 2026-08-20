# The intelligence card

One card per discovery. A card is a claim with evidence attached and a decision at the
end. If you cannot fill in "Why it matters to Paranoid Android" with something specific
to our code, you do not have a card — you have a link.

## File

`research/discoveries/YYYY-MM-DD-<slug>.md`

The date is the date of **first** discovery and never changes; the id is stable and
gets cited from scans, proposals and `decisions.md`. Slug is lowercase, hyphenated,
about the *mechanism* rather than the company — `cow-process-snapshots` ages better
than `acme-labs-launch`.

## Frontmatter

```yaml
---
id: 2026-08-19-cow-process-snapshots
title: <short mechanism-first title>
discovered: 2026-08-19
updated: 2026-08-19
categories: [checkpointing, copy-on-write]
class: RESEARCH            # see references/landscape.md
recommendation: PROTOTYPE  # IGNORE | WATCH | INVESTIGATE | PROTOTYPE | ADOPT
transferability: HIGH      # LOW | MEDIUM | HIGH
novelty: MISSING           # PRESENT | REFINEMENT | MISSING | DIFFERENT | NEW-DIRECTION
confidence: MEDIUM         # LOW | MEDIUM | HIGH
touches: [engine, store]   # core subsystems: model, store, tree, engine, repo, bundle, proto, cli, clients
---
```

## Sections

```markdown
## Discovery
What it is, in two or three sentences, mechanism first.

## Source
Primary source link, then what you actually read (file, section, issue number).

## What is interesting
The technical explanation. The data structure, the invariant, the trick. Enough that a
reader who never opens the link understands the mechanism and could argue with it.

## Why it matters to Paranoid Android
The concrete connection. Name the subsystem and, where you can, the file and type.
Which of these does it bear on: trajectories, checkpoints, branching, replay,
reconstruction fidelity, environments/adapters, provenance, storage, counterfactual
exploration, divergence reporting, capture honesty?

## Transferability
LOW / MEDIUM / HIGH — and *why*. What would have to be true for it to transfer, and
what part of their design depends on assumptions we do not share.

## Novelty
One of: already present in Paranoid Android / a refinement of what we have / a missing
capability / a fundamentally different approach / a potentially new direction.
Justify it against the code, not against memory.

## Limitations and negative signal
What they say does not work, what the issue tracker says does not work, what they
tried and removed. Often the most valuable section.

## Recommendation
IGNORE | WATCH | INVESTIGATE | PROTOTYPE | ADOPT — one word, then one line of reason.

## Proposed action
Concrete. "Prototype content-addressed checkpoint storage using the chunking scheme in
X, behind the existing `Store` interface, measured on the browser example."
Not "consider looking into snapshotting."

## Confidence
LOW / MEDIUM / HIGH, with the reason — grading rules in `references/sources.md`.

## Evidence
- Primary: <link> — what it establishes
- Supporting: <link> — what it adds
- Counter-evidence: <link> — anything that argues against the above

## Changelog
- 2026-08-19 — created.
```

## Recommendation vocabulary

| Word | Means |
| --- | --- |
| IGNORE | Looked at it, it does not bear on us. Recorded so nobody looks again. |
| WATCH | Not actionable now; re-check on a trigger you must name (a release, a paper, an adoption threshold). |
| INVESTIGATE | Worth a deeper read or a spike to answer a specific question. Name the question. |
| PROTOTYPE | Worth building a throwaway to test a specific hypothesis. Name the hypothesis and the measurement. |
| ADOPT | Strong enough to become a design change. Requires a proposal in `research/proposals/`. |

Most cards are WATCH or IGNORE. A run where everything is PROTOTYPE is a run that did
not think.

## Novelty vocabulary

| Value | Means |
| --- | --- |
| PRESENT | We already do this. Card exists so we stop rediscovering it. Grep the code to prove it. |
| REFINEMENT | We do something like it; theirs is better in a specific, named way. |
| MISSING | A capability we do not have and plausibly should. |
| DIFFERENT | Solves the same problem in a way incompatible with our model. Interesting precisely because it forces a comparison. |
| NEW-DIRECTION | Opens a problem we had not considered being in. Rare; be sceptical. |

## Worked example (shape only)

```markdown
---
id: 2026-08-19-example-chunked-cas
title: Content-defined chunking for near-duplicate blob storage
discovered: 2026-08-19
updated: 2026-08-19
categories: [content-addressed storage, checkpointing]
class: INFRASTRUCTURE
recommendation: WATCH
transferability: MEDIUM
novelty: REFINEMENT
confidence: HIGH
touches: [store, tree]
---

## Discovery
<system> stores backups as variable-sized chunks split by a rolling hash, so a small
edit inside a large file rewrites one chunk rather than the whole object.

## Why it matters to Paranoid Android
`tree::snapshot` hashes whole files after every step (`crates/noidroid-core/src/tree.rs`).
A recorded run that appends to one large log file stores a full copy per step. Chunking
would bound that at the edit size, behind the existing `Store::put` interface — the
object model does not change, only what an object *is*.

## Limitations and negative signal
Chunking costs CPU per snapshot and makes an object's identity no longer equal to the
file's own hash, which would complicate the "the workspace at step k has address T"
story we print to users.
...
```

Note what the example does: it names our file, it names the invariant at risk, and its
negative section argues against itself. That is the standard.
