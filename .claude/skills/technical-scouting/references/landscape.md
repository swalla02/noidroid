# Landscape classification

Track adjacent projects, but do not frame everything as competition. The useful
question is almost always:

> What does this project do that Paranoid Android should learn from?

not

> Is this a competitor?

## Classes

| Class | Definition | What to record |
| --- | --- | --- |
| `DIRECT COMPETITOR` | Solves the same problem for the same user: return to a point in a past execution and explore alternatives with evidence. Rare. | Their evidence standard. Where they are stricter or looser than us and why. |
| `ADJACENT TOOL` | Overlapping user, different problem — eval harnesses, tracing UIs, agent debuggers. | The seam where a user would need both. Where their users complain. |
| `INFRASTRUCTURE` | Something we could build on or steal a mechanism from — stores, sandboxes, proxies, snapshotters. | The interface, the guarantees, the licence, the maintenance signal. |
| `RESEARCH` | Papers and academic systems. | The mechanism and its evaluation. Whether anything shipped. |
| `INSPIRATION` | Different domain, transferable idea. | The idea, stripped of its domain. |
| `POTENTIAL INTEGRATION` | Something a user would plausibly want us to interoperate with. | The integration surface and what it would cost. |
| `IDEAS WORTH TAKING` | We will not use the project, but a specific design decision is worth copying. | The decision, in one paragraph, and where it would land in our code. |
| `IRRELEVANT` | Looked, does not bear on us. | One line, so nobody looks twice. |

## Landscape entry

`research/landscape/<slug>.md`

```yaml
---
name: <project>
class: ADJACENT TOOL
first_seen: 2026-08-19
updated: 2026-08-19
url: <primary>
licence: <spdx or "proprietary">
activity: active | slowing | dormant | abandoned
---
```

Then:

```markdown
## What it is
One paragraph, mechanism first.

## How it works
The architecture, insofar as it is documented or readable. Where the state lives, what
the unit of capture is, what it guarantees.

## What it does that we should learn from
The point of the entry. Be specific and be generous — the best entries make us
uncomfortable.

## Where it is weaker, and why that is interesting
Not scoring points: their weakness usually marks a design trade-off we also face.

## Overlap with us
Honest. Which of our claims they also make, and whether they can back them.

## Watch triggers
What would make this worth re-reading — a release, a rewrite, a paper, a
deprecation.

## Changelog
- 2026-08-19 — created.
```

## Rules

- Update the entry, never duplicate it. Activity and class both change over time; an
  entry that went `active → abandoned` is a finding in itself.
- Record the **evidence standard** of anything in the same space. This project's
  differentiator is that a reconstruction is verified rather than asserted; whether a
  neighbour verifies anything is the single most informative fact about them.
- Abandonment is signal. Note the last commit date and, if you can find it, why.
