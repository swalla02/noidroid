# Decisions ledger

*The feedback loop. Research is only a system if it learns what happened to its own
recommendations.*

```
research → discovery → recommendation → engineering decision → implementation
   ↑                                                                  ↓
   └──────────────────── recorded here, as a constraint ──────────────┘
```

Read this at the **start** of every run. A rejected recommendation carries a reason,
and that reason is now a constraint on what you recommend next. A prototype that failed
is worth more than one that was never tried — cite it.

Append with `/scout-verdict`, or by hand in the same shape.

---

## Ledger

| Date | Card | Recommendation | Verdict | Lesson |
| --- | --- | --- | --- | --- |
| — | — | — | — | *(empty — this system was created 2026-08-19)* |

Verdicts: `ADOPTED` · `PROTOTYPED-KEPT` · `PROTOTYPED-DROPPED` · `REJECTED` ·
`DEFERRED` · `SUPERSEDED`.

---

## Detail

Each ledger row gets a section here. The lesson matters more than the outcome.

<!--
### YYYY-MM-DD — <recommendation>

**Card:** `research/discoveries/<id>.md`
**Verdict:** REJECTED
**Decided by:** <human or agent>

**What happened**
What was actually built or tried, and what it showed.

**Why**
The real reason, in mechanism terms. "Not a priority" teaches nothing. "Requires a
STEP_VERSION break for a 3% storage win" is a constraint that saves the next run a day.

**Constraint left behind**
One sentence a future scan must honour. Promote to `constraints.md` if it is permanent.

**What would change this**
The evidence that would justify reopening it.
-->
