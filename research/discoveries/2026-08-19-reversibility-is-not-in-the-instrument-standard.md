---
id: 2026-08-19-reversibility-is-not-in-the-instrument-standard
title: The mature lab-instrument standard has no reversibility attribute; the 2026 agent protocol adds one
discovered: 2026-08-19
updated: 2026-08-19
categories: [laboratory automation, provenance, interposition / interception, capture honesty]
class: RESEARCH
recommendation: WATCH
transferability: MEDIUM
novelty: REFINEMENT
confidence: MEDIUM
touches: [proto, cli, clients]
---

## Discovery

SiLA 2 is the most widely implemented open standard for laboratory instrument
connectivity: gRPC, protobuf, and a Feature Definition Language that gives every command
a machine-readable typed description. I read the FDL XML schema. A `Command` carries
`Identifier`, `DisplayName`, `Description`, `Observable` (Yes/No), `Parameter`,
`Response`, `IntermediateResponse` and `DefinedExecutionErrors` — **and nothing that says
what re-performing the command would do to the world.** Meanwhile LAP, an agent-to-
instrument protocol posted to arXiv in 2026, puts a `reversible` flag and a four-level
`safetyClass` on every capability in its `InstrumentCard`, alongside `interlocks` and
`physicalLimits`, and gates the dangerous classes behind an operator handshake.

## Source

- Primary: the SiLA 2 Feature Definition schema,
  <https://gitlab.com/SiLA2/sila_base/-/raw/master/schema/FeatureDefinition.xsd>,
  downloaded and read in full (6 KB). The `Command` complexType is at lines 16–36.
- Primary (abstract and structured body): "LAP: An Agent-to-Instrument Protocol for
  Autonomous Science", <https://arxiv.org/abs/2606.03755>. I read the abstract and the
  HTML rendering's InstrumentCard and task-lifecycle sections. I did **not** get a clean
  read of the full PDF; see Confidence.
- Supporting: SiLA 2 Part C standard features index, and the Tecan SiLA2 SDK paper in
  SLAS Technology (skimmed for whether an out-of-band reversibility convention exists —
  none found).

## What is interesting

The FDL is a genuinely good declaration language for *what an instrument can do*. It
describes types, units are pushed into the data model, long-running operations are
`Observable` so a client can subscribe to progress and survive a dropped connection, and
`DefinedExecutionErrors` enumerate named failure modes per command. It is a mature,
well-shaped boundary description.

It describes everything about a command except the one property that decides whether an
execution can be returned to. `Observable` is about *watching* a command, not about
*re-performing* it. Dispense 50 µL of a reagent and aspirate it back are, to the schema,
the same kind of thing as reading a temperature.

LAP is the counterexample and it is fifteen months old rather than fifteen years. Its
capability descriptor carries `safetyClass` (S0–S3) and a `reversible` flag, and its
task lifecycle adds three physical states to the eight it inherits: `safety-hold`
(awaiting an operator token for S2/S3), `paused-fault` (a hardware interlock tripped)
and `sample-wait`. The paper's framing sentence is the same one our environment model
opens with: "Some operations are hazardous or irreversible. Autonomous authority over
them must be explicit and bounded."

The design difference from us is worth stating precisely. LAP declares reversibility
**per capability, statically, in the instrument's card, signed by the laboratory
authority**. We declare it **per call, dynamically, by the program making it**
(`proto.rs`, the `call` request's effect kind). Static is weaker as a truth claim — the
same `dispense` is reversible or not depending on what is in the tip and what it is
dispensing into — but it has one property we lack: a client can find out *before* it
calls, and before anything has been recorded.

## Why it matters to Paranoid Android

Two things, one confirming and one that points at an existing issue.

**Confirming.** Law 2 of the environment model — "the boundary is declared, never
inferred" — is usually argued from the impossibility of inferring it. This gives the
sharper version: even where a domain has spent fifteen years building a rigorous,
machine-readable, typed description of every instrument command, **the reversibility
bit is not in it**. An adapter sitting on top of SiLA 2 could not derive `EffectKind`
from the standard; someone would have to state it per call. That is not a limitation of
our design, it is the shape of the problem, and it closes off "auto-derive effect kinds
from the instrument schema" as a future shortcut before anyone proposes it.

**Pointing.** Issue #29 — `noidroid doctor`: say what is and is not captured, *before*
recording — is the same move LAP makes with `InstrumentCard`. Ours is about capture
gaps; theirs is about physical hazard; both are "declare the properties of the boundary
before you cross it, so the operator is not surprised afterwards". If #29 grows a
manifest, LAP's card is the closest existing shape and worth borrowing from: capability
id, what it can do, what it cannot take back, and what it costs.

Bears on: the wire protocol (`proto.rs`), the pre-flight story (#29), and the honesty of
`Reach::Unreachable`, which is only as good as the `EffectKind` it reads.

## Transferability

MEDIUM. Nothing to port. The FDL finding is a constraint on a future adapter and is
solid. The LAP finding is a design echo rather than a mechanism — an early-stage
protocol proposal with, as far as I could establish, no implementations. What would have
to be true for LAP to matter to us: an autonomous-lab user who has already adopted it,
which is a C9 condition and is not met.

One idea genuinely worth taking, cheaply: LAP's `estimatedDuration` and
`operationalCost` per capability. Issue #36 (per-branch cost accounting) wants the same
number, and a declaration-time cost per target is a cleaner source than measuring it.

## Novelty

REFINEMENT. `EffectKind` already exists and already carries exactly this bit at a finer
granularity than either standard. What is new to `research/` is the evidence that the
bit is *absent* from the incumbent lab standard, which is the fact an adapter proposal
would need and which nobody would guess.

## Limitations and negative signal

The negative signal is the FDL gap itself, and it is a warning about the size of a lab
integration rather than an opportunity. Any SiLA-based adapter would need a
hand-maintained table mapping features and commands to effect kinds, per instrument
vendor, and getting one row wrong means a checkpoint reported reachable that is not —
the exact failure the model is built to refuse. That is a real, unbounded maintenance
surface and it argues against a lab adapter until somebody is asking for one.

On LAP: I could not read the full paper cleanly and cannot say whether `reversible` is
load-bearing in the protocol or a field in a diagram. Treat every LAP claim here as
"the paper says", not "the system does". There is no evidence of an implementation.

## Recommendation

WATCH — records why a lab adapter cannot infer effect kinds from the incumbent standard,
and names the one design (LAP) that would change that. No build.

## Proposed action

None. Two triggers to re-read on, below. If #29 (`doctor`) is picked up, read LAP's
InstrumentCard section properly first and steal the field list; that is thirty minutes,
not a project.

**Watch triggers:** (a) a SiLA 2 revision or standard feature that adds a reversibility,
idempotency or "undo" attribute to `Command` — that would make an inferring adapter
possible and is worth knowing immediately; (b) a LAP reference implementation with a
real instrument behind it.

## Confidence

MEDIUM. The SiLA claim is HIGH on its own — I read the schema and the absence is not a
matter of interpretation. The LAP claims are MEDIUM at best: abstract and HTML sections
read, full PDF not, no implementation seen. The card is graded to the weaker half.

## Evidence

- Primary: <https://gitlab.com/SiLA2/sila_base/-/raw/master/schema/FeatureDefinition.xsd>
  — the `Command` element's full child list contains no reversibility or side-effect
  attribute.
- Primary: <https://arxiv.org/abs/2606.03755> — LAP's InstrumentCard capabilities carry
  `safetyClass` (S0–S3) and a `reversible` flag; task states add `safety-hold`,
  `paused-fault`, `sample-wait`; "Some operations are hazardous or irreversible.
  Autonomous authority over them must be explicit and bounded."
- Counter-evidence: SiLA 2 does ship a standard feature for pausing, resuming and
  stopping an observable command, so the standard is not indifferent to control — it
  simply does not model what a repeat would cost.

## Changelog

- 2026-08-19 — created.
