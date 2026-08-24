---
name: LAP — Lab Agent Protocol
class: RESEARCH
first_seen: 2026-08-19
updated: 2026-08-19
url: https://arxiv.org/abs/2606.03755
licence: unknown (paper; no implementation located)
activity: active (2026 preprint)
---

## What it is

A proposed protocol for the agent-to-instrument edge of autonomous science: how an
autonomous agent addresses a physical laboratory instrument when "operations are
stateful, safety-critical, exclusively owned". It positions itself in the gap left by
agent-interoperability protocols, which model tool calls but not physical consequence.

## How it works

From the paper's structure (abstract and HTML sections read; the full PDF did not render
for me — treat this entry as MEDIUM confidence throughout):

- **InstrumentCard** — a signed capability declaration per instrument: `instrumentClass`
  from an ontology, and per capability an `inputSchema`/`outputSchema` with physical
  typing (QUDT/UCUM), a `safetyClass` S0–S3, a **`reversible` flag**, `physicalLimits`,
  `interlocks`, `estimatedDuration` and `operationalCost`. Plus a calibration block with
  `validUntil`, and a JWS signature from the laboratory authority.
- **Reservation** — exclusive locking of instruments and samples.
- **Safety-fence handshake** — cryptographically binds an operator's confirmation to a
  specific task, gating S2/S3 operations.
- **MeasurementResult** — physically typed, calibration-anchored, with uncertainty.
  "A number without a unit, a calibration, and an uncertainty is not a measurement."
- **Task lifecycle** — the eight standard agent states plus three physical ones:
  `safety-hold`, `paused-fault`, `sample-wait`.

## What it does that we should learn from

Two things.

**It puts reversibility in the capability declaration.** The incumbent standard, SiLA 2,
does not (see `2026-08-19-reversibility-is-not-in-the-instrument-standard`). LAP's
framing sentence — "Some operations are hazardous or irreversible. Autonomous authority
over them must be explicit and bounded" — is our `EffectKind::Irreversible` and our
denial-by-default rule, reached independently for physical rather than epistemic
reasons.

**The card is a pre-flight manifest.** A client can learn what an instrument cannot take
back *before* calling it. Issue #29 (`noidroid doctor`: say what is and is not captured,
before recording) wants the same shape for capture gaps, and LAP's field list is a good
starting point if that issue is picked up — particularly `estimatedDuration` and
`operationalCost`, which issue #36 (per-branch cost accounting) would otherwise have to
measure.

## Where it is weaker, and why that is interesting

`reversible` is declared **per capability, statically**, by the instrument. That cannot
be right in general: the same `dispense` is reversible or not depending on what is in
the tip and what it is going into. Our per-call declaration by the caller is strictly
more expressive and strictly more burdensome, and comparing the two makes the trade-off
concrete — static declarations can be published and audited, dynamic ones cannot.

More importantly for us: LAP records provenance and makes results "reproducible by
construction", but there is no evidence in what I read of *returning to a point* — no
checkpoint, no branch, no re-execution semantics. It is a protocol for doing the
experiment safely and recording it well, not for asking what a different choice would
have produced.

## Overlap with us

Little, and complementary rather than competing. If LAP ever ships, an instrument
speaking it would be an unusually well-declared boundary for us to sit behind: signed
capability descriptions with reversibility already stated is the one thing that would
make a lab adapter tractable rather than a per-vendor maintenance surface.

**Evidence standard: unknown.** Signed provenance and calibration anchoring are audit
mechanisms, not reconstruction verification. I found nothing claiming a re-execution can
be checked.

## Watch triggers

- A reference implementation with a real instrument behind it. Until then this is a
  paper.
- Any adoption signal from a self-driving-lab platform, or convergence with SiLA 2.
- A revision that makes `reversible` per-invocation rather than per-capability — that
  would be someone else discovering the same thing we did, and worth reading properly.

## Changelog

- 2026-08-19 — created. Abstract and HTML sections read; full PDF not successfully read.
  Confidence MEDIUM.
