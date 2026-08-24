---
name: orx (OpenResearch CLI) — experiment tree
class: INSPIRATION
first_seen: 2026-08-24
updated: 2026-08-24
url: https://github.com/openresearch (CLI; `orx skill orx-experiment-tree`)
licence: unknown — distributed as a binary, source not located
activity: active — the tool this project now uses for literature search
---

## Why this entry exists, and what it is *not*

`orx` is the paper-search tool the scout now uses (`orx discover` / `orx paper`, see
`.claude/skills/technical-scouting/references/orx-cli.md`). Separately, it ships a local
**experiment tree** for ML training runs whose two cardinal rules — *never edit a node a
run has answered*, and *branch a child to try a variant* — look, at a glance, like this
project's checkpoint model.

**I tested that resemblance and most of it does not survive.** The entry exists for a
narrower reason, given at the end: orx publishes a written branch-*selection* policy, and
that is the first answer of any kind to a standing question in `research/README.md` that
has had no source at all.

## What it is

A project is a tree of experiment nodes. The root (baseline) holds starting code and a
**run command** — one shell command that trains or evaluates the node and prints results to
a run log. Every other node is a child branched off a parent, inheriting its code (as a git
branch) and its run command. Runs are launched against the node; results attach to it.

## How it works

Two rules carry the model:

- **Provisional until it answers.** A run that dies on an error establishes nothing and
  tests nothing, so the node is still provisional: fix it in place and re-run the same node.
  There is a repair cap of two.
- **Frozen once it answers.** A run that produced the result the node was after — "good, bad
  or `nan`" — freezes the node permanently. Never edit it again; branch a child. Explicitly:
  "a disappointing number is a result, not a reason to repair," and, on the other side,
  "unintended behaviour is not an answer" — an OOM, a timeout, a missing dependency leaves
  the node provisional, *unless the node's hypothesis was about memory or runtime*.

State lives in a local store plus a git worktree per project; the unit of capture is a git
branch plus a run log.

## What it does that we should learn from

**The one thing worth taking, and it is not the freeze rule.** orx publishes an explicit
policy for *which* branch to create next, which is roadmap item 4's problem and which the
2026-08-21 DST scan recorded as having no source anywhere ("Antithesis explicitly withholds
its guidance component; nobody else publishes one"). Theirs:

> **width = the open options of one decision** (fan freely — a 3-way LR sweep *should* be
> three siblings under a common head); **depth = decisions already resolved, stacked** (one
> level down per winner kept). A new round never hangs off the root — it hangs off the
> previous round's winner.

with two named anti-patterns: the **flat fan** (whole sweep off the root, so "every result
is measured against the *start*, so wins never accumulate"), and the **noodle** (a long
single-child chain, "depth manufactured for its own sake"). The operational test for
whether X should be a child of Y or its sibling is stated as a question: *name what Y
established that X builds on.* If you can name it, descend; if X and Y are co-equal options
of the same decision, fan.

That is a real, checkable heuristic for shaping a branch tree, published rather than
withheld. It is one data point, from a domain (hyperparameter search) with a property we
lack — a scalar per round that identifies a "winner". A counterfactual debugging session has
no winner, so the rule cannot be lifted. But "measure this round against the last round's
winner, not against the root" is a shape our `bisect` already implicitly has and our
multi-branch exploration does not yet have a story for.

## Where it is weaker, and why that is interesting

**The freeze is a discipline, not an invariant — this is where the analogy fails.** orx's
immutability is a rule written in a skill document and obeyed by an agent that reads it.
Nothing content-addresses a node; a child inherits code through a git branch and the run
command is a "fixed contract" by convention. You *can* edit a frozen node; you are told not
to. In our model you cannot edit a step without changing its address, and prefix sharing,
copy-on-write and immutable history are consequences of hashing rather than of compliance.
Two systems can share a shape without sharing an evidence standard, and the evidence
standard is the whole of our differentiation. Treating this as a peer model would be exactly
the surface analogy the scout is supposed to reject.

**And the interesting half of the freeze rule, we already have.** orx's sharpest distinction
is between "the run answered the question" and "the run broke for an unrelated reason". That
is `ceb2fd4` / #58 — "'It ended here' and 'it broke here' are different facts. A timeline
that stops does not distinguish them, which is this project's stated failure mode in
miniature." We shipped that argument on 2026-08-21 in `noidroid branch`'s outcome reporting.
Novelty PRESENT; no action.

## Overlap with us

Essentially none as products. orx branches *code* to answer research questions; we branch
*executions* to answer counterfactual ones. It makes no reconstruction claim, no fidelity
claim, and no verification claim, so there is nothing to compare on the axis we compete on.

## Watch triggers

- If orx (or anything like it) publishes a branch-selection policy that does **not** assume a
  scalar winner per round, that is directly relevant to roadmap item 4 — reopen.
- If the experiment tree gains content addressing or any re-derivation check on a node, the
  comparison becomes real and this entry should be rewritten as `IDEAS WORTH TAKING`.

## Changelog

- 2026-08-24 — created. Read `orx skill orx-experiment-tree` and `orx --help`. The
  checkpoint-model resemblance was examined and largely rejected; the entry is kept for the
  published branch-selection policy only.
