---
name: Shepherd
class: DIRECT COMPETITOR
first_seen: 2026-08-21
updated: 2026-08-21
url: https://github.com/shepherd-agents/shepherd
licence: MIT
activity: active — alpha, v0.3.0, ~2.4k stars, last push 2026-08-09, arXiv 2605.10913
---

## What it is

The closest thing to us that exists. Its own description: "A runtime substrate that turns
an agent's execution into a reversible, Git-like trace, so meta-agents can observe, fork,
replay, and revert any run. Couples agent and environments in a copy-on-write fork ~5x
faster than docker commit, with ~95% KV-cache reuse on replay."

Read that sentence next to ours and the overlap is the whole first clause. The difference
is the second: they are building the thing for **meta-agents** — agents that supervise,
optimise and train other agents — and they are building it as an **agent framework**.

## How it works

From the README, the concepts docs and the paper:

- A **task** is a Python function with *no body*; the signature and docstring are the
  contract an agent fulfils at runtime. Everything is declared in the signature, including
  permissions: `repo: sp.GitRepo` is a read-write grant, `May[GitRepo, ReadOnly]` a
  read-only one.
- **Effects**: "Everything a task does to the world crosses one explicit, typed, recorded
  channel." Two events per action — an *intent* when attempted and an *outcome* when the
  world responds. The paper classifies effects as **reversible** (filesystem writes),
  **compensable** (database writes with a rollback handler), and **irreversible** (model
  calls).
- **Trace**: a Git-like commit graph. `scope.emit()` is a commit, `scope.fork()` a branch,
  `scope.merge()` a merge, `scope.discard()` a branch delete. Divergent branches share
  storage by content hashing.
- **Rewind** is copy-on-write layering, not deterministic re-execution: forking creates a
  new filesystem layer over existing state, and reverting checks out a previous commit,
  restoring what the paper calls byte-identical agent-environment state.
- **Enforcement**: grants are compiled to a run's writable roots and enforced at the
  native syscall jail — **macOS Seatbelt and Linux Landlock** (the latter inside a
  privileged container). A write outside a grant is refused at the syscall. Windows is
  unsupported because "enforcement would be advisory-only at best".
- **Settlement**: a run's output is a *retained changeset* — held to one side, inspectable
  and runnable without being applied, then `select` / `apply` / `release` / `discard`.
  `apply` three-way-merges onto a workspace that moved on, when changes are path-disjoint.
- A companion repo (`shepherd-experiments`) bundles a frozen substrate snapshot so the
  paper's numbers stay reproducible.

## What it does that we should learn from

Three things, and the first one stings.

**1. They shipped kernel-enforced grants on two platforms.** Our
`2026-08-19-kernel-enforced-capture-boundary` card proposes spiking Landlock for the
egress fence and is currently an open INVESTIGATE at score 27. Shepherd has it in a
released alpha, on Seatbelt *and* Landlock, with the constraints documented honestly:
Linux enforcement needs a privileged container, grants are whole-profile per binding,
sub-root grants are explicitly out of scope, and Windows is refused rather than faked.
That is a free design document for our spike and it should change how we scope it. It
also makes the surrounding claim — *the boundary is enforced, not merely cooperative* —
one that a competitor can now make and we cannot.

**2. "Compensable" is a third effect kind we do not have.** Our `EffectKind` is
`read | write | irreversible`. Theirs adds an effect that is undoable *if the caller
supplies the undo*, which is a real category: a database write with a rollback handler, a
created resource with a delete. Under our model that is `irreversible` and a checkpoint
past it is `Reach::Unreachable`, which is safe but pessimistic. Worth thinking about
against `checkpoint.rs`'s reach computation before dismissing.

**3. The retained changeset is a good answer to a question we answer differently.** A run
produces a proposal you can execute without applying. We produce a branch you inspect with
`diff` and `checkout-tree`. Theirs is more ergonomic for the "should I keep this?" moment;
ours is more honest about the branch being a separate history rather than a candidate
merge.

## Where it is weaker, and why that is interesting

**Their evidence standard is assertion, not verification.** Rewind is a checkout of a CoW
layer, and correctness rests on the paper's stated **weak-coupling assumption**:
counterfactual replay assumes edits and side effects remain loosely coupled. When that
assumption fails there is no oracle that says so — the same shape as BPO's Assumption 1
(`2026-08-21-unverified-fork-in-branching-rl`) and the same shape as every re-drive in
`2026-08-19-unverified-world-redrive`. They have a Lean-mechanised calculus of the trace
semantics, which proves the *algebra* is right, not that a given rewind landed where it
claims. C2's argument — that a deterministic prefix is verifiable in a way an image is
not — is the argument against their design, and it is the one thing we hold that they do
not.

**It is an agent framework, and it asks for your program.** A task must be written as a
bodyless `@task` function in their type system. That is a much larger ask than routing
side effects through a client, and it means the population it can record is the population
willing to rewrite. Our `--proxy` and `--watch` paths record agents nobody rewrote.

**The model-call surface has not shipped.** Their own concepts index says the Workspaces
and Providers pillars "taught the ambient model-call surface, which has not shipped; they
return when it does." So the LLM-call recording that the paper's counterfactual-replay
claims depend on is roadmap, not release, in v0.3.0. The `~95% KV-cache reuse on replay`
figure in the repo description should be read against that.

## Overlap with us

We make the same top-line claim: record an execution, return to a point inside it, branch,
and never modify the original. We reach it by re-execution under a recorded-input oracle
with hash-equality verification; they reach it by content-hashed CoW layers with an
assumption. They have shipped enforcement and an ergonomic settlement model; we have
shipped the verification and the divergence report.

The uncomfortable summary: **on reach they are ahead, on evidence we are ahead, and
evidence is the only axis we have said we compete on.** If Shepherd grows a verification
story, our differentiation narrows to the capture boundary and the branch semantics.

## Watch triggers

- Any verification of a rewind — a digest comparison, a re-derivation check, a divergence
  report. That is the single most important thing to watch in this landscape.
- The model-call/provider surface shipping, which turns the paper's counterfactual replay
  into released behaviour.
- The `shepherd-experiments` repository, for whether the meta-agent training applications
  touch RL post-training (Tinker is credited in the acknowledgments).
- Their effect taxonomy stabilising — specifically whether "compensable" survives contact
  with real integrations or collapses back into "irreversible".

## Changelog

- 2026-08-21 — created. Read the README in full, `docs/shepherd/concepts/index.md`, the
  repository metadata, and a structured summary of arXiv 2605.10913. **I did not read
  the source and did not run it**, so the mechanism claims are theirs; the paper summary
  was extracted by a fetch tool rather than read section by section, and the effect
  taxonomy and Lean-calculus claims carry that discount.
