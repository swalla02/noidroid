---
id: 2026-08-19-log-replay-validity-modes
title: Autonomous driving names three replay modes and a validity threshold; we should take the names, not the threshold
discovered: 2026-08-19
updated: 2026-08-19
categories: [deterministic replay, counterfactual reasoning, simulation, divergence reporting, robotics / ROS]
class: INSPIRATION
recommendation: WATCH
transferability: MEDIUM
novelty: DIFFERENT
confidence: MEDIUM
touches: [cli, engine, docs]
---

## Discovery

Autonomous-driving evaluation has a standard three-way classification of what a replay
of a recorded drive actually is, and it is exactly the distinction our README currently
makes in a limitation paragraph. nuPlan's benchmark defines **open-loop** (the recorded
log drives everything, the planner's output is only scored), **closed-loop
non-reactive** (the planner's output drives the ego, the other agents are still played
back from the log) and **closed-loop reactive** (the other agents are simulated so they
respond to what the ego actually did). The industry's practical answer to the resulting
problem — "closed-loop simulation quickly diverges if the planner decides to take
different actions from what is recorded" — is to transform the logged data into the new
reference frame and to declare a run **invalid** once divergence exceeds a threshold.

## Source

- Primary: "Towards learning-based planning: the nuPlan benchmark for real-world
  autonomous driving", <https://arxiv.org/pdf/2403.04133> — the three challenge modes
  and their definitions.
- Primary (vendor engineering writing, read in full): Applied Intuition, "Using
  re-simulation to verify AV stack",
  <https://www.appliedintuition.com/blog/closed-loop-log-replay>.
- Supporting: nuPlan-R, <https://arxiv.org/pdf/2511.10403> — "Non-reactive log replay or
  rule-based models sever the causal link between the ego's actions and the
  environment's response."

## What is interesting

Three mechanisms, of decreasing usefulness to us.

**1. The mode is a first-class property of the experiment, reported with the result.**
No nuPlan number is quoted without saying which of the three modes produced it, because
the same planner scores differently in each and the modes measure different things.
Nobody writes "we replayed it" and leaves the reader to guess whether the world was
responding.

**2. Coordinate transformation as a partial repair.** Applied Intuition: "In a drive
log, detected actors may be reported relatively to the ego. To get from the open-loop
reference frame to the re-simulation reference frame, different actor positions thus need
to be adjusted to align with the simulated ego pose (coordinate transform)." This is a
domain-specific trick for making recorded observations usable after the actor diverged —
it works because the recorded data has a known geometric relationship to the ego. There
is no general version of it and we should not look for one.

**3. Validity by threshold.** The recorded data is treated as valid for the
counterfactual until the divergence between simulated and logged ego pose crosses a
tuned bound, at which point the run is marked invalid. Applied Intuition also gives the
self-check: "triage and engineering teams need to be able to trust that a re-simulation
is accurate and reproducible. This can be validated by running re-simulations on log
sections without a disengagement and confirming that the ego divergence is small."

## Why it matters to Paranoid Android

**The names.** `README.md` documents "a branch is not a prediction: past the divergence
point, `live` calls query a world that has moved on" as a limitation in prose. Our
`Mode` enum has `Record | Replay { live } | Branch { at, intervention, simulate }`, and
those names describe *what the engine did*, not *what the resulting trajectory is
evidence of*. The AV taxonomy is the missing second axis and it maps cleanly onto
machinery we already have:

| AV term | our situation | what we already record |
| --- | --- | --- |
| open-loop | `Mode::Replay` with no live targets | every effect `delivery: replayed`, provenance unchanged |
| closed-loop non-reactive | a branch whose post-divergence effects are still served from the recording | `simulated` provenance, effects delivered from the recording |
| closed-loop reactive | a branch whose post-divergence `read`/`write` effects really execute | `live` provenance on the executed effects |

We can already tell these apart from the trajectory; we do not *say* it. A one-line
statement in the run report and in `show` — "this trajectory is a closed-loop reactive
branch from run-1@3: everything from step 3 met a world that had moved on" — is worth
more than a paragraph in a limitations list, and it feeds issue #24 ("replay is the
wrong instrument when the prompt or model changed") and #34 (divergence report
ergonomics).

**The threshold, we should refuse.** "Valid until divergence exceeds X" is precisely the
fuzzy-matching evidence standard closed under C4. It is defensible for them because
their divergence has a metric with physical meaning (metres) and a tolerance derived
from vehicle dynamics. Ours does not: the distance between two `state_root` digests is
not a number, and inventing one would be the "heuristic rather than verifiable" scheme
C4 names. Their approach is a good reminder that a threshold is what you reach for when
you have no oracle — and that our recorded-input oracle is the thing that lets us skip
it.

**The self-check corroborates our top open recommendation.** Validating resimulation by
re-running sections that should not diverge and confirming divergence is small is
Hermit's `--verify` argument arriving from a fourth domain. See
`2026-08-19-verify-by-double-execution`, which I have updated with this.

## Transferability

MEDIUM. The vocabulary transfers completely and costs nothing but a rename. The
coordinate transform does not transfer at all — it depends on the recorded observation
having a computable relationship to the counterfactual state, which is true of actor
poses relative to an ego and false of an API response relative to a changed prompt. The
threshold transfers technically and should not be taken.

Their assumption we do not share: an AV log's world is *continuous and metric*, so
"nearly the same" is meaningful. Ours is content-addressed and discrete, where "nearly
the same hash" means nothing. That difference is the whole reason we can hold C4 and
they cannot.

## Novelty

DIFFERENT. We solve the same problem — describing what a counterfactual over a recording
is worth — with a categorical answer (provenance and delivery, joined, never repaired)
where they use a continuous one (divergence magnitude against a bound). Comparing them
is the point: it shows our answer is the stronger one *given an oracle*, and shows what
the fallback looks like without one.

## Limitations and negative signal

The negative signal is that after a decade and enormous budgets, the AV industry's
answer to "when does a replayed world stop being valid for a counterfactual" is still a
tuned constant chosen per application. Applied Intuition's own framing admits it: there
is "a limit to the amount of divergence that could be sustained before the data is no
longer reliable", with no principled way to find it. If anyone proposes we compute a
"branch confidence" number, this is the citation for why not.

Against acting: this is report ergonomics, which is real but is not the milestone. It is
also the sort of change that can quietly become a taxonomy nobody maintains. Keep it to
one sentence per run, generated from data the trajectory already carries, or do not do
it.

## Recommendation

WATCH — take the vocabulary when the structured-comparison work (roadmap item 4, issues
#24/#34) is picked up; take nothing else.

## Proposed action

No build now. When #24 or #34 is scheduled, add one derived sentence to the run report
and to `show`, computed from the existing chain rather than stored: which of the three
modes this trajectory is, and from which step onward. Nothing new on disk, no
`STEP_VERSION` implication.

Explicitly do **not** add a divergence magnitude, a similarity score, or a validity
threshold.

## Confidence

MEDIUM. The Applied Intuition post was fetched and read; the nuPlan definitions come
from the paper's abstract and challenge descriptions plus two citing papers, not from a
full read of the benchmark paper or its code. The taxonomy itself is not in dispute — it
is stated consistently across four independent sources — but I have not verified how
nuPlan's simulator implements the reactive mode.

## Evidence

- Primary: <https://www.appliedintuition.com/blog/closed-loop-log-replay> — coordinate
  transform for ego divergence; validity established by re-simulating sections that
  should not diverge; timing non-determinism if the stack falls behind.
- Primary: <https://arxiv.org/pdf/2403.04133> — the three nuPlan challenge modes.
- Supporting: <https://arxiv.org/pdf/2511.10403> — non-reactive replay "severs the
  causal link between the ego's actions and the environment's response".
- Counter-evidence: the reason they need reactive agents at all is that the recorded
  world is *not* usable past divergence — which is an argument that our post-divergence
  `live` execution against the real world is itself a mode with no established name and
  possibly no established validity. We should not assume our version escapes the problem
  just because we label it.

## Changelog

- 2026-08-19 — created.
