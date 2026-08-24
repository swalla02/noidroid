---
date: 2026-08-24
cadence: targeted
question: "For computer-use agents, what happens to state that a rollback does not restore — and what has the field shipped on that since 2026-08-21?"
cards_created:
  - 2026-08-24-live-replay-performs-irreversible-effects
  - 2026-08-24-attended-state-is-a-world-we-never-declare
  - 2026-08-24-no-undo-across-the-tool-boundary
cards_updated:
  - 2026-08-19-kernel-enforced-capture-boundary
  - 2026-08-21-unverified-fork-in-branching-rl
landscape_created:
  - crab-agent-checkpoint-restore
  - openresearch-cli
---

# Scan: computer use, and what a rollback does not take back

> **Framing and dedup.** `scans/2026-08-21-computer-use-gaps-and-rl-post-training.md`
> covered computer-use capability gaps and the RL post-training pipeline. This pass is
> deliberately narrower and on a different axis it did not touch: **the state an agent
> rollback fails to restore.** Nothing here re-treads benchmarks, mocked environments,
> rollout formats or reward integrity. Every paper cited was checked against `research/`
> by id and by name before being opened; all were new. Also a case study for `orx
> discover`/`orx paper` as the literature tool — verdict at the end.
>
> C1, C2, C3, C4 and C9 all bind and nothing below asks to reopen any of them. There is no
> similarity score, no threshold and no fuzzy match in any recommendation — and where a
> source offered one, I say why we should not take it.

## In one paragraph

I started on "computer use for agents" and the sweep pulled up something narrower and more
useful than expected: a **cluster of at least eight independent 2026 systems papers all
building a transaction boundary around agent side effects**, because rolling back an agent
does not roll back what left the process. I read four as full primary sources (ACRFence,
Cordon, Crab, and the KV-cache rollback paper) and screened the rest. Three things
survived. **The headline is a defect in our own code, and it is the run's most important
output:** ACRFence demonstrates at 10/10 that agents re-perform irreversible actions after
a restore, and reading it against `engine.rs` I found that of the three code paths that can
answer `execute`, **one — `Phase::Reconstructing if runs_live(&target)`, line 693 — never
consults `may_perform_irreversible()`**. A `noidroid replay --live world` on a trajectory
containing an irreversible `world.charge` performs it again, `report.denied` stays empty,
and the CLI's warning is gated on `report.denied` being non-empty so nothing is printed.
Our own test `a_replay_never_touches_the_world` passes because it is called with
`live: &[]`. Second, "Aborted but Not Forgotten" (16 Aug 2026) shows that a logical rollback
and a serving session's retained KV cache can disagree invisibly — and the general lesson,
stripped of its security framing, is that **the inference endpoint is a world in our own
environment-model sense and no recording declares it**, which is an unfilled row in a table
we shipped in 0.3.0. Third, the negative half: the eight-paper cluster is inventing, badly
and by inference (one of them uses an LLM to judge whether two tool calls are "the same"),
the seam we already have as `EffectKind`. That is a positioning finding, not a build, and I
have scored it as such. Crab additionally hands us the restore-latency distribution that the
2026-08-21 scan's top recommendation was explicitly blocked on.

## What survived

**`2026-08-24-live-replay-performs-irreversible-effects` — PROTOTYPE. The headline, and it
is about us.**
`may_perform_irreversible()` in `engine.rs:839` returns true only for `Mode::Record`, with
the comment "Only an original recording is allowed to touch the world for real. Every
reconstruction and every branch is denied by default." Three arms of `on_call` reach
`Response::execute()`. Lines 649 and 665 check the guard. Line 693 —
`Phase::Reconstructing if self.runs_live(&target)` — takes `effect` straight from the
client's request and never inspects it. `runs_live` is *prefix* matching, so `--live world`
covers `world.charge` without a glob, while the flag's own doc comment only ever imagines
`--live model`. Narrower than ACRFence's attack (`expect_match` runs first, so the call must
already agree with the recording — this is the plain double-spend, not divergent
re-synthesis, which *is* guarded), but silent in the report and in a mode we actively
recommend. I did not execute the failing case; writing the test that should fail is step one
and could prove this card wrong.

**`2026-08-24-attended-state-is-a-world-we-never-declare` — INVESTIGATE.**
arXiv 2608.15939 formalises *rollback consistency*: a believed-complete abort must restore
the state the model attends, not just the transcript. Retained KV alone flipped a protected
effect in 25/63 cells with the attacker tokens provably absent from the served request in
all 63, and it reproduced inside LangGraph time-travel. Their scope note is the load-bearing
sentence for us: a content-addressed prefix cache (vLLM) is **exempt**; the hazard is
retained-*handle* reuse. Our exposure is the mirror image and confined to
`Replay { live: [...] }` — steps 0..k are served from the recording and never sent to the
server, so a client holding a session handle has a verified transcript sitting on an
attended state that was never built. `grep` confirms we declare no world for the model:
`llm.py` has no `observe`, `doctor.rs`'s coverage list has no entry, §12's conformance table
has no row. The likely honest outcome is one sentence saying our clients are stateless — but
right now that guarantee is accidental rather than stated.

**`2026-08-24-no-undo-across-the-tool-boundary` — WATCH, and the run's most useful
negative.** ACRFence, Cordon, DART, Crab, ChronoMem, MemTX, Transactional Continuity Kernel,
AID-Guard, plus maintainer reports from LangGraph, Google ADK, OpenClaw and a HashiCorp
Vault issue where single-use tokens reappeared after a snapshot restore. Every one needs to
know, per call, "is this reversible?", and none can ask — so ACRFence infers it with an
**analyzer LLM**, Cordon with a lineage graph and nine hand-written invariants, Crab with
eBPF syscall tracing. Three inference mechanisms for a fact the caller knows.
`EffectKind { Read, Write, Irreversible }` is declared at the call site, travels in
`proto.rs`, is hashed into the step, and gates `checkpoint.rs`. Novelty **PRESENT** — the
finding is that we should *say* this, plus one small gap: we have no cross-branch query
("has any branch of this trajectory already performed `world.charge`?"), which is the exact
artefact ACRFence had to invent.

## Looked at, not pursued

- **Qwen-CUA (2608.02352)**, 86.2 on OSWorld-Verified, ~100,000 vCPUs of rollout fleet,
  "trajectory slicing". Screened by abstract. Capability scaling; nothing about state
  fidelity, rollback or capture. Confirms the 2026-08-21 picture rather than changing it.
- **Computer-Using World Model (2602.17365, Microsoft)** — read in full and it is the most
  interesting near-miss. Its motivation is verbatim our problem: "real execution does not
  support counterfactual exploration… despite the environment being fully digital and
  deterministic", and it names the reason as latency plus irreversibility ("undo functions
  are often limited and context-dependent"). Their answer is to *generate* the next
  screenshot with a fine-tuned VLM plus a diffusion editor. That is `Provenance::Simulated`
  with a learned generator: useful for action search before acting, useless as evidence, and
  they never claim otherwise. Not a card because there is no mechanism to take — but it is a
  clean statement that the field wants counterfactual exploration of computer use and is
  reaching for a generative substitute because nobody offers a faithful one. Notable detail
  for anyone tempted by multimodal state: combining their text and image predictions
  *degraded* agent performance, which they attribute to cross-modal conflict.
- **DeltaBox (2605.22781)** — millisecond sandbox C/R via delta snapshots. Same unverified-
  restore family as Crab; folded into that card's family rather than opened, since Crab
  covers the mechanism class and has the better evaluation.
- **SynChain (2608.06862)** — agents induced to synthesise poisoned skills that survive
  internal state updates; concludes that "securing CUAs requires provenance-aware reasoning
  over cross-task execution trajectories". Interesting vocabulary collision with ours, but
  the mechanism is fine-tuning-based attack construction. No transferable seam.
- **"What Did It Actually Do?" (2603.28551)** — AgentTrace, an interview study plus a
  traceability UI prototype for what an agent touched and what persists after uninstall.
  Real user need, but it is a visualisation layer and C9 says we are not building a
  dashboard. Recorded so nobody re-opens it.
- **Inducing Task Models from Computer-Use Traces (2608.20319)** — passive screenshot/input
  traces to symbolic task models. Trace *summarisation*, which is C5's territory from the
  other direction; nothing re-derives an address.
- **The CUA safety line** — BraveGuard, StepJack, SeerGuard, CORA, Safety Sentry, Visual
  Confused Deputy, ROGUE. Screened. All are guard models, risk classifiers or conformal
  abstention over proposed actions. Crowded, actively researched, and squarely inside "do
  not build a judge" from the 2026-08-21 scan. One line each and no more.
- **CLI-Anything (2606.03854)**, **Multi-Agent Computer Use (2606.01533)**,
  **AOI (2606.29472)**, **CUADebug (2608.02643)** — agent architecture and interface papers.
  CUADebug is the closest to us in spirit (root-cause localisation over failed OSWorld
  trajectories, 11.2%→19.6% joint subtype-and-step diagnosis with Gemini 2.5 Pro) and is
  worth one sentence: it is `bisect`'s problem solved by prompting a debugger over
  screenshots instead of by re-running the execution, and its numbers are the reason we
  answer it by experiment. Not a card; it corroborates a claim the README already makes.
- **`orx`'s experiment tree** — examined at the task's suggestion, and the checkpoint-model
  resemblance mostly **did not survive**. See the landscape entry for why; short version, the
  freeze is a discipline rule over a git branch rather than an invariant of an addressed
  structure, and the genuinely sharp half of it ("a run that broke is not a run that
  answered") we shipped ourselves in #58. Kept for one narrow reason only.

## Negative findings

**Opportunities.**

1. *Nobody can declare an effect's reversibility, so everybody infers it.* Eight systems,
   three different inference mechanisms, none of them sound. We are told. (Card 3.)
2. *No system in this cluster verifies its restore.* Crab claims "100% recovery correctness"
   and means benchmark pass rate under one injected crash. Its Inspector already computes
   the net-change data that would let it check, and it uses that data only to decide whether
   to checkpoint. Fourth instance of the pattern in
   `2026-08-21-unverified-fork-in-branching-rl`.
3. *Recovering an agent from its transcript alone does not work, and there is now a number.*
   Crab's baselines: chat-only recovery 8–28% correct, chat+filesystem 28–42% on
   Terminal-Bench. That is the measured cost of the thing most frameworks do.
4. *Counterfactual exploration of computer use is wanted and unserved.* CUWM says so in its
   motivation and answers it with a generative model because no faithful option exists.

**Warnings.**

5. *Our own irreversible guard has a hole, in the mode we recommend most.* Card 1. This is
   the project's stated worst failure mode — a trajectory that looks real — in our own
   engine, and it was found by reading a paper against our code rather than by any test.
6. *The fashionable fix in this cluster is a fuzzy matcher and we must not take it.*
   ACRFence's replay-or-fork decision is made by "a lightweight analyzer LLM" comparing
   whether two tool calls are semantically equivalent, precisely so it can ignore differing
   request ids and timestamps. That is C4 wearing a new hat. Our version is `key` equality
   and a fatal divergence; they approximate it because they have no oracle.
7. *The whole cluster is about prevention, which is not our business.* Outboxes, shadow
   state, policy engines, guard models. C9. The temptation to add "and it can block the bad
   call too" should be refused — we report, we do not police.
8. *We would be measured against 0.1–2 s per fork, not 50 ms.* Crab's commodity-backend
   distribution reframes the AgentENV comparison in our favour, and our own number is still
   unmeasured after three scans.

## What we now know that we did not

1. **`engine.rs:693` executes irreversible effects during a replay**, and it is the only one
   of three `execute` paths that does not consult the guard. Read directly from source.
2. **A logical rollback and a serving session's KV can disagree invisibly**, causally
   demonstrated with a same-token/different-cache audit, reproducing inside a first-class
   rollback API — and **content-addressed prefix caches are exempt**, which is the sentence
   that scopes our exposure.
3. **We declare no world for the model provider anywhere in the tree.** grep-verified across
   `llm.py`, `doctor.rs` and §12's conformance table.
4. **At least eight independent 2026 systems are building an agent effect-transaction
   boundary**, and none of them can ask the caller what we ask the caller.
5. **Checkpoint/restore cost on commodity backends**: p50/p95/p99 = 0.1/0.7/1.0 s,
   bimodal — filesystem-only 20–100 ms, process 700–1000 ms; restore median 0.71 s.
6. **Up to 87% of agent turns change no recovery-relevant state**, and detecting that with
   eBPF is 100% accurate on process change and 98.3% on filesystem change **with zero false
   negatives** — the fail-safe direction roadmap item 2 requires.
7. **The deterministic prefix keeps being re-derived by people solving other problems.**
   Crab's fast-forward replays cached LLM responses to realign an agent with a restored
   checkpoint; the KV paper's sufficient fix is "rebuild from the committed transcript",
   chosen over both a global flush and a full restart. C2 gains two more independent
   corroborations.
8. **`--proxy` is now the third-party architecture three times over**: `verifiers`,
   our `af81680`, and Crab's Coordinator.

## Still unknown

- **Our own restore-and-branch cost as a function of k.** Fourth consecutive scan carrying
  this. We now have a proper band to be measured in (item 5 above), which makes the
  measurement more valuable, not less. It remains a day's work.
- **Do any of our client paths hold a server-side session handle?** The whole consequence of
  card 2 turns on this and it is a survey of four code paths, not research.
- **Does the `--live` irreversible case actually fire?** Card 1 is a control-flow read, not
  an execution. One test settles it.
- **Crab's source.** No repository located; `orx paper` surfaced no associated GitHub. Every
  number in that landscape entry is theirs.
- **Cross-branch effect queries** — whether anyone besides ACRFence has built one, and what
  shape. I found the need, not the prior art.
- **The computer-use trajectory datasets** — OpenCUA/AgentNet, AgentTrek, OS-Genesis,
  GUI-Odyssey, AndroidControl. Carried from 2026-08-21 and *still* not reached; this pass
  went sideways into rollback semantics instead. It should be its own scan or be dropped
  from the standing list, because it has now lost twice.
- **Whether `EffectKind` survives contact with a real adapter author.** Card 3's whole
  argument is that declaring reversibility is better than inferring it. We have no evidence
  that people declare it *correctly*, and a mislabelled charge is indistinguishable from a
  correct one to us by construction (C1).

# Recommended Actions

Ranked by Impact × Relevance × Feasibility × Novelty.

### 1. Close the `--live` irreversible hole, failing test first

**What to build.** Add `a_live_replay_still_refuses_an_irreversible_target` to
`crates/noidroid-core/tests/vertical_slice.rs`, modelled on the existing `irreversible`
fixture: record, then run `Mode::Replay { live: vec!["world".into()] }`, and assert the
witness file contains `charge` exactly once. Confirm it fails. Then consult
`may_perform_irreversible()` on the `Phase::Reconstructing if runs_live` arm, routing to
`simulated_value` then `deny_irreversible` exactly as line 665 already does. Then decide
whether `--live <prefix>` that matches a recorded irreversible target should refuse the run
*before it starts* — the chain is walkable at `cmd_replay` time, so this is a pre-flight
check.

**Why now:** ACRFence is the trigger — it establishes empirically (10/10, and a survey
finding that none of 12 frameworks prevent it) that re-performing irreversible effects after
a restore is a real production failure rather than a hypothetical. We built the mechanism
that answers it and left one path unguarded. Every day it stays open, `--live` is the mode
we recommend for the most common change people make.

**Impact:** 3 — it restores a claim the README makes and a named test asserts. A replay that
touches the world and reports itself faithful is this project's defining failure.
**Relevance:** 3 — capture honesty and replay, dead centre.
**Feasibility:** 3 — one guard call, one test, optionally one pre-flight check. Nothing
hashed moves; `STEP_VERSION` unaffected. Single PR.
**Novelty:** 3 — the check does not exist on that path and no test covers it.
**Score:** 81

**Cost:** one PR. The risk that blows it up: the test written to fail passes instead, meaning
I misread the control flow and the card is wrong. That is a cheap failure and it is why the
test comes first.
**What we would learn:** whether `Phase::Reconstructing` + `runs_live` + `Irreversible` is
actually reachable end-to-end through the client, or whether something upstream in
`proto.rs` or the Python client already prevents it. A "no, it is already safe" is a real
answer and would downgrade this to a comment in the code explaining why.
**Touches:** `crates/noidroid-core/src/engine.rs` (`on_call` line 684–694),
`crates/noidroid-core/tests/vertical_slice.rs`, `crates/noidroid-cli/src/main.rs`
(`cmd_replay` pre-flight and the denial hint at line 867).
**Evidence:** `2026-08-24-live-replay-performs-irreversible-effects`.

---

### 2. Measure restore-and-branch at step k, and publish the curve

**What to build.** The step 0 the 2026-08-21 scan named and nobody has run: wall-clock
restore-and-branch at step k, as a function of k, on the reference environment and on the
browser example. Report the curve, not a single number.

**Why now:** this is the fourth consecutive scan to carry it, and it is now *more*
decision-relevant rather than less, because the comparison band has resolved. It was
"1,920 ms or 50 ms, take your pick"; Crab makes it a distribution on commodity hardware —
p50/p95/p99 of 0.1/0.7/1.0 s to checkpoint, 0.71 s median to restore. If our re-execution at
k=25 lands inside 0.1–2 s we are competitive on a claim nobody else can make; if it is tens
of seconds, several open PROTOTYPE recommendations are mis-scoped and should be re-pitched as
offline analysis rather than as rollout primitives.

**Impact:** 2 — it changes no capability, but it decides the framing of at least three open
recommendations and one competitive claim.
**Relevance:** 3 — checkpoints and branching, directly.
**Feasibility:** 3 — no new code; a script over existing CLI commands and the two examples
already in the tree.
**Novelty:** 3 — never measured, and named as the single most decision-relevant unknown by
the 2026-08-21 scan.
**Score:** 54

**Cost:** a day. The risk: the reference environment is small enough that the curve is
flattering and tells us nothing about a k=200 agent run. Mitigate by reporting k on both
examples and stating the shape rather than a headline figure.
**What we would learn:** whether verified forking is a rollout-collection primitive or an
offline-analysis tool. "It is tens of seconds at k=25" is a perfectly good answer and it
changes the pitch rather than killing the work.
**Touches:** nothing in `crates/` — a benchmark script, `examples/`, and a paragraph in
`README.md` if the number is good.
**Evidence:** `2026-08-21-unverified-fork-in-branching-rl` (updated),
`research/landscape/crab-agent-checkpoint-restore.md`.

---

### 3. Survey the four client paths for session handles, then write one honest sentence

**What to build.** No code first. Enumerate `llm.py`'s `model.complete`, `--auto`'s
`sitecustomize` hooks, `--proxy`'s intercepted request bodies, and the hand-written-client
case, and for each record whether the request is self-contained (`messages=[...]`, exempt by
the KV paper's own scope note) or handle-carrying (`previous_response_id`, cached-content
handles, local `past_key_values`). Then: if all self-contained, add one line to
`noidroid doctor` and one row to `docs/environment-model.md` §12 recording the inference
endpoint as a world with `opaque` grip whose statelessness is what makes reconstruction
sound. If any path carries a handle, `doctor` must warn on it and `--live` on that target
should declare the session as an `opaque` world via `Session.observe`.

**Why now:** 2608.15939 is eight days old and is the first work to state the cross-layer
rollback-consistency contract precisely enough to check ourselves against. §12's conformance
table is the artefact that is supposed to answer "which environments get which grip", and the
one environment every recording touches is missing from it. Doing this before the table
acquires more rows is cheaper than retrofitting it.

**Impact:** 2 — converts an accidental guarantee into a stated one. It does not add a
capability, and if the survey comes back "all stateless" the output is a paragraph.
**Relevance:** 3 — capture honesty and the environment contract.
**Feasibility:** 3 — a survey plus a `doctor` line plus a table row. No hashed bytes move.
**Novelty:** 2 — the mechanism (`grip`, `observe`, `Situation`) shipped in 0.3.0; this is an
unfilled row, not a new abstraction.
**Score:** 36

**Cost:** an afternoon for the survey, then a small PR. The risk: over-reach. If this turns
into "capture the model's serving state" it has gone wrong — that is not possible for a
hosted API and is C1 for a self-hosted one. The deliverable is a sentence, not a mechanism.
**What we would learn:** whether any supported path holds server-side state across steps.
"No, all four are self-contained" is the likely answer and it is worth having written down,
because right now nobody can tell whether that is a design guarantee or a coincidence.
**Touches:** `clients/python/noidroid/llm.py`, `auto.py`, `proxy.py`,
`crates/noidroid-cli/src/doctor.rs`, `docs/environment-model.md` §12.
**Evidence:** `2026-08-24-attended-state-is-a-world-we-never-declare`.

---

### 4. Make the irreversible-effect record queryable across a trajectory's branches

**What to build.** `noidroid log --irreversible <trajectory>`, or an equivalent column in
`noidroid diff`: walk the trajectory and its branches and answer "which irreversible effects
exist anywhere in this family, on which branch, with what outcome (`Value` / `Denied`)?" A
walk plus a filter over data already in the chain. Stores nothing new, judges nothing,
blocks nothing.

**Why now:** ACRFence's central artefact is an effect log keyed by thread and branch id,
which they had to build from scratch and which they populate by interposing with eBPF. We
already store strictly better data — hashed, immutable, per-step, with a declared
`EffectKind` and an `EffectOutcome` — and no command surfaces it. Making it queryable is the
cheapest way to turn a design advantage into something a user can see.

**Impact:** 2 — exposes something we already hold; no change to what a reconstruction can
claim.
**Relevance:** 2 — CLI and reporting.
**Feasibility:** 3 — a chain walk and a filter, single PR.
**Novelty:** 2 — the data exists and is not reachable; this is a named improvement to
reporting rather than a new capability.
**Score:** 24

**Cost:** one PR. The risk: it drifts toward "and it can warn you before you re-run one",
which is the prevention business (C9) that the entire eight-paper cluster is in and we are
not. The discipline is that it reports history and offers no opinion about the future.
**What we would learn:** whether "what irreversible things did this family of runs do?" is a
question anyone asks. If nobody uses it, that is evidence that our `EffectKind` advantage is
architecturally real but practically invisible, which is itself worth knowing before we lead
with it in positioning.
**Touches:** `crates/noidroid-cli/src/main.rs`, `crates/noidroid-core/src/repo.rs`
(read-only walk).
**Evidence:** `2026-08-24-no-undo-across-the-tool-boundary`.

## Explicitly not recommended

**Do not adopt ACRFence's analyzer-LLM comparison, in any form.** Their replay-or-fork
decision is made by an LLM judging whether two tool calls are "semantically equivalent", so
that differing request ids and timestamps can be ignored. It is a fuzzy matcher with a
learned oracle, it is C4 exactly, and the reason they need it is that they have no way to
identify a call positionally. We do: `key` is position-and-identity, and divergence is fatal.
The correct reading of ACRFence is that it validates our evidence standard by demonstrating
what you have to resort to without one.

**Do not build an effect outbox, shadow state, or a staged-commit boundary.** Cordon is very
good at this — 45/45 risk-bearing workflows intercepted before commit versus 14/45 for
adapted existing defences — and it is an agent runtime that interposes on tool dispatch,
holds writes in a shadow filesystem, and decides policy. That is C9's agent framework and
C1's "own the program" trade in one package. Our answer to the same problem is that we record
what happened and refuse to re-perform it, which is a different and smaller job.

**Do not build a guard model, a risk classifier, or an action-abstention mechanism.** The
computer-use safety literature is large, well-funded and moving fast — BraveGuard, StepJack,
SeerGuard, CORA, Safety Sentry, the visual-confused-deputy line. The 2026-08-21 scan already
ruled out judging trajectories; this rules out judging actions, for the same reason and with
more sources.

**Do not build a generative world model for counterfactual computer use.** CUWM is the
tempting one because its motivation sentence is ours almost verbatim. But a predicted next
screenshot is `Provenance::Simulated` produced by a diffusion model, and our whole claim is
that a reconstruction is faithful or says why not. Their own result that adding text to image
predictions *degrades* agent performance is a hint at how load-bearing the fidelity is.
Where we and they overlap is a customer question, not an engineering one: they serve "search
before you act", we serve "find out what would have happened".

**Do not treat orx's experiment tree as a peer checkpoint model.** I tested the resemblance
because the task asked me to, and it does not hold: the freeze is a rule an agent obeys, over
a git branch, with no content addressing and no re-derivation check. The one part of it worth
having — distinguishing "the run answered" from "the run broke" — we shipped in #58. The
entry is kept only for its published branch-selection policy, which is the first data point
of any kind on a standing question, and even that assumes a scalar winner per round that
counterfactual debugging does not have.

**Do not re-open computer-use benchmarks or datasets on the strength of this run.** The
standing-questions list has carried "computer-use trajectory datasets" for two scans and it
has now lost twice to a more urgent question. Either commission it as its own targeted scan
or strike it; leaving it on the list is how it stays permanently second.
