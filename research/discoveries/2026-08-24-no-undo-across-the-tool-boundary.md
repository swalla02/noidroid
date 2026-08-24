---
id: 2026-08-24-no-undo-across-the-tool-boundary
title: Eight independent 2026 systems are inventing a transaction boundary for agent side effects
discovered: 2026-08-24
updated: 2026-08-24
categories: [unserved-problem, negative-signal, agent effect boundaries, checkpointing, computer-use agents]
class: RESEARCH
recommendation: WATCH
transferability: MEDIUM
novelty: PRESENT
confidence: MEDIUM
touches: [model, engine, proto]
---

## Discovery

Over 2026 at least eight unrelated groups have independently built the same missing
primitive: a **task-scoped commit/rollback boundary around an agent's external effects**,
because rolling back an agent's transcript does not roll back what left the process. They
disagree on the layer — security fence, transaction runtime, OS checkpointer, memory
version-control — and agree on the diagnosis. Every one of them ends up needing a
per-effect declaration of reversibility, and every one of them has to *infer* it, because
no tool interface carries it.

## Who hits this

- **ACRFence** (UCSC/UConn) — <https://www.alphaxiv.org/abs/2603.20625> — surveyed 12
  agent frameworks; none enforce exactly-once semantics at the tool boundary. Proposes an
  effect log plus "replay-or-fork" semantics.
- **Cordon** (Tsinghua/SJTU/RUC) — <https://www.alphaxiv.org/abs/2606.17573> — "today's
  agent runtimes still expose tools as isolated RPCs… it lacks a task-scoped execution
  boundary for commit, rollback, recovery." Stages external effects in an *effect outbox*
  and local mutations in a *shadow state*.
- **DART** (<https://www.alphaxiv.org/abs/2605.23311>) — "replaying the entire task is safe
  but wasteful, while restoring from a local checkpoint is efficient but can leave
  committed downstream work tied to an upstream history that no longer exists."
- **Crab** (HKUST) — <https://www.alphaxiv.org/abs/2604.28138> — the "agent–OS semantic
  gap": frameworks see tool calls but not their OS effects; the OS sees state changes but
  not turn-level context.
- **ChronoMem** (<https://www.alphaxiv.org/abs/2607.27773>) — agent memory is
  "forward-only… with no principled mechanism to inspect, version, or revert prior states."
- **MemTX** (<https://www.alphaxiv.org/abs/2607.23929>) and **Beyond Memory: A Transactional
  Continuity Kernel** (<https://www.alphaxiv.org/abs/2608.11632>) — the same commit
  discipline applied to belief/memory state.
- **AID-Guard** (<https://www.alphaxiv.org/abs/2608.21159>, 21 Aug 2026) — stateful
  authorization for delegated agent effects; three days old at the time of this scan.
- **The framework maintainers themselves**, via ACRFence's survey: LangGraph
  (re-execution acknowledged as architecturally hard), Google ADK (documentation states
  rewind "cannot undo external side effects"), OpenClaw (webhook replay advisory), and a
  HashiCorp Vault issue where single-use tokens reappeared after a snapshot restore.

Far more than the three independent sources `negative-space.md` requires, and — importantly
— they are not all agent-debugging tools. Crab is a systems paper, Cordon is a security
runtime, ChronoMem is a memory system, Vault is a secrets manager.

## Why it is unsolved

Structural, and worth stating precisely: **a checkpoint restores the process, and the
process is not where the effect went.** Local state (files, memory, transcript) is
restorable by copying bytes. External state (a payment, a sent message, a consumed token,
a provisioned VM) is restorable only by a *compensating action* that the tool interface
does not describe and often does not offer. Cordon says this outright — once an effect is
released, it relies on "audit or compensation, as physical undo is not possible."

So every one of these systems is forced to answer, per call, "is this reversible?", and
none of them can ask. They infer it instead:

- ACRFence: an **analyzer LLM** decides whether two calls are semantically equivalent.
- Cordon: a lineage graph plus nine hand-written invariants and path/fan-out heuristics.
- Crab: eBPF syscall tracing to infer which turns changed OS state at all.

Three different inference mechanisms for a fact the caller knows and has no way to state.
That is the signature of a missing seam (`negative-space.md`, kind 4).

## Would Paranoid Android's model help?

**We already have the seam, and this is the card's main point: novelty here is PRESENT,
not MISSING.** `EffectKind { Read, Write, Irreversible }` is declared by the caller at the
call site and travels in the wire protocol (`proto.rs`), is hashed into the step
(`model.rs`), is refused outside a recording (`engine.rs::may_perform_irreversible`), and
determines checkpoint reachability (`checkpoint.rs:101`, an irreversible effect is
survivable only in a world we hold). We did not have to infer anything.

That is a genuinely strong position and it is worth *saying* rather than building toward.
But three honest deductions:

1. **Our declaration is only as good as the caller.** A client that labels a charge `write`
   gets it replayed for free. We have no detection, by design (C1). Crab's eBPF Inspector
   is the one mechanism in this cluster that *detects* rather than trusts — see the update
   to `2026-08-19-kernel-enforced-capture-boundary`.
2. **We have no cross-branch effect log.** ACRFence's core artefact is a log of irreversible
   effects keyed by *thread and branch id*, so a restored run can be told "this already
   happened on another branch." We store effects per step in an immutable chain, which is
   strictly better data — but nothing queries *across* siblings. The question "has any
   branch of this trajectory already performed `world.charge`?" is answerable from the
   store and is not answerable from the CLI.
3. **`2026-08-24-live-replay-performs-irreversible-effects` shows our own guard has a hole
   in it.** Having the right primitive is not the same as applying it on every path.

## Limitations and negative signal

- **This is mostly a positioning finding, and positioning is C6-adjacent.** It does not
  change what the tool can claim; it tells us what to say. Score it accordingly.
- Most of these systems are 2026 preprints with prototype implementations; ACRFence's
  mitigation is explicitly unevaluated. Cordon and Crab have real evaluations; the rest I
  screened by abstract only.
- The cluster's centre of gravity is *prevention* (block the bad effect), which is not our
  business and is squarely inside C9's "not an agent framework". We should not follow them
  into policy engines, outboxes or guard models.
- The convergence could reflect a fashionable framing rather than a real gap — "transactions
  for agents" is an easy paper to write. The evidence that it is real is the non-academic
  half: the LangGraph, ADK, OpenClaw and Vault reports, which are maintainers describing
  production bugs.

## Recommendation

**WATCH**, with one cheap concrete action. The mechanism is built; what is missing is a
query over it and a sentence in the README. Re-check on the trigger below.

## Proposed action

Two things, both small, neither a new abstraction:

1. **Say it.** One line in the README's positioning: an effect's reversibility is declared
   by the caller and carried in the step, which is why a replay can refuse it. Eight 2026
   systems are inferring what we are told. This is free and it is the only place in the
   competitive picture where we are unambiguously ahead of the field rather than level.
2. **Make the effect log queryable across branches.** `noidroid log --irreversible <traj>`,
   or a column in `noidroid diff`, answering "which irreversible effects exist anywhere in
   this trajectory's family, on which branch, with what outcome (`Value` / `Denied`)?" The
   data is already in the chain; this is a walk plus a filter, and it is the artefact
   ACRFence had to invent from scratch.

**Watch trigger:** if any of MCP, OpenEnv or the OpenAI/Anthropic tool schemas adds a
reversibility or idempotency field to its tool declaration, that is the ecosystem adopting
our seam, and the right response is an adapter that maps it onto `EffectKind` — reopen this
card then. `2026-08-19-reversibility-is-not-in-the-instrument-standard` is the same
observation from the laboratory-instrument side (SiLA 2's FDL describes every command
except what re-performing it costs); that this recurs in an unrelated standards family
strengthens both.

## Confidence

**MEDIUM.** HIGH that the cluster exists and that our primitive is the one they lack —
ACRFence, Cordon and Crab were read as full reports, and our own code was read directly.
MEDIUM overall because five of the eight sources are abstract-only screening, and because
the "we are ahead" conclusion is the kind that deserves suspicion.

## Evidence

- Primary: <https://www.alphaxiv.org/abs/2606.17573> — Cordon, read in full; the clearest
  statement of the missing boundary and the best evaluation (45/45 intercepted pre-commit
  versus 14/45 for adapted existing defences).
- Primary: <https://www.alphaxiv.org/abs/2603.20625> — ACRFence, read in full; the
  12-framework survey and the maintainer reports.
- Primary: <https://www.alphaxiv.org/abs/2604.28138> — Crab, read in full; the agent–OS
  semantic gap.
- Supporting: five further 2026 preprints screened by abstract (DART, ChronoMem, MemTX,
  Transactional Continuity Kernel, AID-Guard).
- Ours: `crates/noidroid-core/src/model.rs:104-126`, `engine.rs:839-872`,
  `checkpoint.rs:95-110`.
- Counter-evidence: the whole cluster is about *preventing* effects, which is not our
  problem; and our declaration is unverified by construction (C1).

## Changelog

- 2026-08-24 — created, from the computer-use rollback scan.
