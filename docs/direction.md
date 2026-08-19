# Direction

*For whoever picks this up next — a person, a session, or an agent. Read this before
the backlog. The issues say what to build; this says what the project is for, and what
would make it worthless.*

---

## The one sentence

> A reconstruction is either faithful, or it says exactly why it is not.

Everything else is downstream of that. When a decision is hard, this is the tiebreaker.

## What the project is

Paranoid Android records an execution as an immutable, content-addressed trajectory,
returns to any checkpoint inside it, and runs branches from there where one thing is
different. The original is never modified.

It is **not** an observability tool, an agent framework, or an eval harness. Those are
crowded and well served. What is not served is the ability to take an execution that
already happened, return to a meaningful point in it, and find out what would have
happened instead — with evidence rather than a claim.

## The rule that governs everything

**Do not fake capabilities we do not have.**

That sounds like a slogan until it starts refusing things. In practice it has meant:

- Replay is verified by hash equality, not asserted. A faithful reconstruction
  re-derives the same object addresses, or it reports where it stopped matching.
- Provenance never improves downstream. Once a trajectory turns on an intervention,
  nothing after it may claim to be real.
- Irreversible effects are refused outside an original recording, and a simulated
  value poisons everything downstream of it.
- Automatic capture refuses to record around a hole rather than recording through it.
- A replay that reaches the network is stopped, because a reconstruction that touched
  the world is not a reconstruction.

The failure mode this project cannot survive is not a crash. It is **a trajectory that
looks real**. Every silent gap is worse than a loud refusal, and any change that turns
a loud failure into a quiet one is wrong even when it is more convenient.

## Where the next release has to get to

The milestone is called *earn the claim*, and that is the whole brief. Today the
sentence at the top is true only for the surfaces we happen to mediate. Between the
program and the world there are still openings — the clock, randomness, subprocesses,
async SDK paths — and for each one the honest answer is currently "we do not look".

Getting there does not mean capturing everything. It means **knowing and saying what
is not captured**, before a recording is made rather than after it has been trusted.
A tool that records half a program and says so is useful. One that records half a
program and reports faithfully is dangerous.

## Decisions already made — do not re-litigate without new evidence

Each of these was researched and settled. Reopen one only with something that was not
known at the time.

- **Zero-code capture is not achievable, and pursuing it produces a worse system.** We
  capture the boundary, not the process. Low-code, and we say so.
- **A checkpoint is a deterministic prefix, not a memory snapshot.** Returning to step
  k re-executes 0..k under a recorded-input oracle. Cheaper, portable, and verifiable
  in a way an image is not.
- **Provenance and delivery are separate axes.** Provenance is content and is hashed;
  delivery is per-run and is not. Conflating them means a perfect replay produces
  different hashes than the run it reproduced.
- **Divergence stays fatal, and matching stays positional.** Four of five comparable
  systems hold this line; the one that does not ships a documented "unexpected
  behaviour" mode. This is a verification tool — "record it again" is a fine answer.
  The real problem was report ergonomics, not strictness.
- **Trace import is rejected** (#39). It would force fuzzy matching on a lossy summary.
  The recording proxy is the honest version of the same goal.
- **"Replay costs zero tokens" is not the pitch.** It collapses for the change people
  make most often — swapping the prompt or the model. Lead with divergence
  localisation instead.

## What we are deliberately not building

A dashboard. A distributed store. An agent framework. A universal simulator. Anything
resembling speculative infrastructure for a user who has not appeared yet.

Adoption work — packaging, framework adapters, launch — is real and is *parked*, not
forgotten (#37, #38). The current priority is the engine, because an artifact that
records honestly is the only thing worth installing.

## How to work here

- **Every bug and every feature becomes an issue first**, then a branch, then a pull
  request that closes it, then a review. Green CI is not a review. See CONTRIBUTING.
- **Research before building when the direction is uncertain**, and file what comes
  back as issues — including the negative results. Half the value in this repository's
  issue list is the things decided against, with reasons.
- **Verify by running, not by reading.** Every serious bug in this project so far was
  found by using it: the workspace restore that deleted `.git`, the replay that ran
  past the end of its recording, the capture path that claimed a mitigation which
  never executed. Read the output; do not trust the assertion.
- **Write tests for the invariant, not the function**, and name them after the claim
  they defend. `a_replay_never_touches_the_world` is worth more than `test_replay_3`.
- **When something cannot be done, say so in the product**, not only in the docs. A
  refusal with a reason is a feature.

## How you will know it is working

Not by stars. By whether these stay true as the project grows:

1. Someone can hand a colleague a bundle, and the colleague can replay it and get the
   same answer or a precise explanation of why not.
2. Every claim the tool prints can be traced to something it actually checked.
3. The list of things it cannot do is easy to find, and nobody is surprised by an item
   on it after the fact.

If a change makes one of those harder, it is the wrong change however good it looks.
