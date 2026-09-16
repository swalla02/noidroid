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
| 2026-08-24 | [`reproducibility-bought-by-mocking-the-world`](discoveries/2026-08-21-reproducibility-bought-by-mocking-the-world.md) | PROTOTYPE (the mute + test half) | PROTOTYPED-KEPT | See detail below. |
| 2026-08-24 | [`reward-computed-over-an-unaddressed-state`](discoveries/2026-08-21-reward-computed-over-an-unaddressed-state.md) | PROTOTYPE (the #53 Q1/Q3 half) | DECIDED | See detail below. |

Verdicts: `ADOPTED` · `PROTOTYPED-KEPT` · `PROTOTYPED-DROPPED` · `REJECTED` ·
`DEFERRED` · `SUPERSEDED`.

---

## Detail

Each ledger row gets a section here. The lesson matters more than the outcome.

### 2026-08-24 — add the browser adapter's re-drive mute and assert the run report changes

**Card:** `research/discoveries/2026-08-21-reproducibility-bought-by-mocking-the-world.md`
**Verdict:** PROTOTYPED-KEPT
**Decided by:** agent session, issue #53

**What happened**
Built the half of the card's proposed action that issue #53 Q5 asked for:
`NOIDROID_BROWSER_MUTE=1` skips `Browser._reconstruct`'s re-drive in
`clients/python/noidroid/browser.py`, and
`the_counterfactual_browser_is_re_driven_rather_than_assumed`
(`crates/noidroid-core/tests/browser_slice.rs`) proves, by running the same branch
muted and un-muted, that the report differs — the muted run reads `about:blank`
instead of the recorded page. Did **not** build the card's other half (tabulating the
real-web digest-match rate across recorded sites); that remains open.

**Why**
The mute-and-test half was cheap, mechanical, and directly closed a stated gap
(`docs/environment-model.md` §14.5): the reference environment already had this proof
for its own re-drive (`the_counterfactual_world_is_re_driven_rather_than_assumed`), the
browser adapter did not, and `#52`'s `REFERENCE_MUTE` — cited by issue #53 as already
shipped — turned out not to be on `main` at all (see the `unverified-world-redrive`
entry below). The real-web drift-rate measurement is a different kind of work (a day of
running the adapter against live sites and tabulating results) and doesn't belong in a
design-decision issue.

**Constraint left behind**
A behavioural-flip test (run twice, compare the observable report) is the right shape
for proving a re-drive matters — cheaper and more convincing than deleting code in a
scratch copy, and it's now the pattern in two adapters (reference, browser). The next
adapter should get the same test before it ships, not after.

**What would change this**
Measuring the real-web digest-match rate (the card's other half) if computer-use
credibility becomes a priority — still open, still worth a day.

### 2026-08-24 — settle #53 Q1 using "recomputed vs measured reward" as the forcing case

**Card:** `research/discoveries/2026-08-21-reward-computed-over-an-unaddressed-state.md`
**Verdict:** DECIDED
**Decided by:** agent session, issue #53; revised when #52 and #65 landed

**What happened**
A pure replay keeps the trajectory's evidence and does not degrade to `opaque`: every
replay is served by design, so the downgrade would say nothing about any adapter. What
it may not do is print "faithful" as though that covered the world. A replay of a
trajectory with a declared world now prints a note naming the world as served, not
re-driven (`docs/environment-model.md` §14.2). Run grip and trajectory grip are named
apart (§14.1); run grip is never printed as a bare word.

**Why**
The draft of this decision was written before #52 landed and argued for a blanket
`opaque` on replay. #52 gated run grip on executed steps for a good reason (branches
would otherwise report `opaque` for their whole served prefix). The sentence gives the
reader the true fact without the noise.

**Constraint left behind**
Anything that reports on a replay — `noidroid score`, a future `run --verify` — says what
it did not measure, in words, rather than printing a grip.

**What would change this**
An adapter class for which serving a recorded observation is itself a meaningful check.
None is known.
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
