# Prioritisation

Research is the input. **Prioritisation is the product.** A scan that ends without a
ranked, arguable list of what to do has not finished.

## The score

```
priority = Impact × Relevance × Feasibility × Novelty
```

Multiplicative on purpose: a zero anywhere kills it. Something brilliant, highly
relevant and completely infeasible is a WATCH, not a plan.

Each factor is 0–3. Show the number *and* the sentence that justifies it.

### Impact — if we did this, how much better is the system?

| | |
| --- | --- |
| 3 | Changes what the tool can honestly claim. Removes a limitation from the README, or closes a gap in capture honesty. |
| 2 | Makes an existing capability materially better: faster, more faithful, easier to integrate. |
| 1 | Nice quality-of-life or ergonomics improvement. |
| 0 | No user-visible or invariant-level consequence. |

### Relevance — does it bear on what this project actually is?

Weigh against `docs/direction.md` and `research/CONTEXT.md`.

| | |
| --- | --- |
| 3 | Directly serves reconstruction fidelity, checkpointing, branching, or honest capture. |
| 2 | Serves a supporting subsystem: storage, adapters, reporting, CLI. |
| 1 | Adjacent; would matter to some future version of the project. |
| 0 | Belongs to something we have said we are not building (dashboard, distributed store, agent framework, universal simulator). |

### Feasibility — can we do it, here, soon?

| | |
| --- | --- |
| 3 | Fits behind an existing interface (`Store`, `tree`, the wire protocol) and could be a single PR. |
| 2 | A contained change to one subsystem plus tests. |
| 1 | Cross-cutting, or needs a dependency or platform capability we do not have. |
| 0 | Requires abandoning a settled decision, or an on-disk format break with no migration story (`STEP_VERSION` — see CONTRIBUTING.md). |

### Novelty — is it new *to us*?

| | |
| --- | --- |
| 3 | A capability we lack, or a fundamentally different approach we have never evaluated. |
| 2 | A named improvement on something we already do. |
| 1 | Confirms an approach we already chose. Worth recording, rarely worth building. |
| 0 | Already implemented, or already rejected in `research/constraints.md` with no new evidence. |

## Tie-breakers, in order

1. **Does it make a silent failure loud?** This project's stated worst outcome is a
   trajectory that looks real. Anything converting a silent gap into a stated one wins.
2. **Does it reduce what we cannot say?** Shrinking the unknown surface beats adding
   a feature.
3. **Is it testable as an invariant?** If it can be defended by a named test
   (`a_replay_never_touches_the_world`), it is worth more than something only
   observable in a demo.
4. **Cheapest disproof first.** Prefer the item whose *failure* we would learn from
   quickest.

## Writing a recommendation

Every entry in `# Recommended Actions` states all five:

```markdown
### 1. <Imperative sentence — what to build or do>

**Why now:** the trigger. What changed in the world or in our code that makes this
the moment. "It is interesting" is not a why-now.

**Impact:** 3 — <one line>
**Relevance:** 3 — <one line>
**Feasibility:** 2 — <one line>
**Novelty:** 2 — <one line>
**Score:** 36

**Cost:** an honest estimate in the units we work in — a spike, a PR, a release.
Name the risk that would blow it up.

**What we would learn:** the question it answers, phrased so the answer could be
"no". If a negative result teaches nothing, it is not an experiment.

**Touches:** the subsystems and files. `crates/noidroid-core/src/engine.rs`, the
wire protocol, the Python client.

**Evidence:** the card ids this rests on.
```

## The other half: what not to do

Every scan report ends with **Explicitly not recommended**. List the plausible-looking
things you considered and rejected, with the reason. This is the section that stops the
same idea being re-proposed in three months, and it is the section that feeds
`research/constraints.md`.

A ranked list without a rejection list is only half a recommendation.
