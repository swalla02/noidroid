# Constraints

*Decisions already made, with reasons. **Re-proposing one of these without new evidence
is the scout's worst failure mode.** Reopening is allowed — `docs/direction.md` says
so explicitly — but only with something that was not known when it was closed. Name
it, date it, and spell out the mechanism.*

Seeded from `docs/direction.md` and the issue history on 2026-08-19. Grows from
`decisions.md` as recommendations get their verdicts.

---

## Settled

| # | Constraint | Why | Closed | Reopen if |
| --- | --- | --- | --- | --- |
| C1 | **Zero-code capture is not achievable, and pursuing it produces a worse system.** We capture the boundary, not the process. Low-code, and we say so. | Capturing enough from outside an uninstrumented process to *reconstruct* it is not portably possible. Pretending otherwise produces a system that demos well and lies. | direction.md | A portable, general interposition mechanism appears that yields reconstruction-grade capture — not observation-grade. Evidence must show replay, not tracing. |
| C2 | **A checkpoint is a deterministic prefix, not a memory snapshot.** Returning to step k re-executes 0..k under a recorded-input oracle. | Cheaper, portable, and verifiable in a way an image is not. | direction.md | Never wholesale — but a snapshot *fast-path behind the same checkpoint interface* is on the roadmap. Proposals must preserve the verification story. |
| C3 | **Provenance and delivery are separate axes.** Provenance is content and is hashed; delivery is per-run and is not. | Conflating them means a perfect replay produces different hashes than the run it reproduced. | direction.md, model.rs | Effectively closed. This is load-bearing for prefix sharing. |
| C4 | **Divergence stays fatal, and matching stays positional.** | Four of five comparable systems hold this line; the one that does not ships a documented "unexpected behaviour" mode. This is a verification tool — "record it again" is a fine answer. The real problem was report ergonomics, not strictness. | direction.md | A fuzzy-matching scheme with a *verifiable* evidence standard, not a heuristic one. Report ergonomics work is welcome and is not this. |
| C5 | **Trace import is rejected.** | It would force fuzzy matching on a lossy summary. The recording proxy is the honest version of the same goal. | #39 | A trace format appears that is complete enough to re-derive object addresses. |
| C6 | **"Replay costs zero tokens" is not the pitch.** | It collapses for the change people make most often — swapping the prompt or the model. Lead with divergence localisation. | direction.md | Positioning, not architecture. Not a research question. |
| C7 | **Token-level branching would need a weaker evidence standard than hash equality.** | Rejected on the evidence standard, not on difficulty. | #20 | Someone demonstrates token-level branching with a verifiable reconstruction claim. |
| C8 | **Training runs as the branchable unit — parked.** | Scope; the engine has to earn its claim on ordinary executions first. | #22 | Parked, not rejected. Re-time it, do not re-argue it. |
| C9 | **Not building:** a dashboard, distributed storage, an agent framework, a universal simulator, or speculative infrastructure for a user who has not appeared yet. | Crowded, or premature. The engine is the only thing worth installing. | direction.md | A real user with a real workload, not a market observation. |
| C10 | **Adoption work is parked, not forgotten** (#38 framework answer). The installable artifact half is no longer parked: `install.sh` ships, and #104 covers the rest. | The current priority is the engine. Installation was the exception, because an engine nobody can run is not evidence of anything. | direction.md, #103 | It is a sequencing decision. A finding that makes adoption *cheaper* is still useful — file it, do not campaign. |

## How to use this file

- Score a candidate `Novelty: 0` if it lands on a settled constraint with no new
  evidence (`references/prioritisation.md`).
- A card may still be written for something constrained — record *that we looked again
  and it still does not hold*, so the next run does not repeat the search. Set
  `recommendation: IGNORE` and say which constraint applies.
- When a recommendation gets a verdict in `decisions.md` that hardens into a rule,
  promote it here with a row, a reason and a date.
