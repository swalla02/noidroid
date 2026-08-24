---
id: 2026-08-24-live-replay-performs-irreversible-effects
title: A live replay executes irreversible effects for real, and it is the one execute path that never asks
discovered: 2026-08-24
updated: 2026-08-24
categories: [capture honesty, agent effect boundaries, record/replay systems, negative-signal, computer-use agents]
class: RESEARCH
recommendation: PROTOTYPE
transferability: HIGH
novelty: MISSING
confidence: HIGH
touches: [engine, cli, clients]
---

## Discovery

ACRFence (arXiv 2603.20625) demonstrates, at 10/10 in a controlled experiment, that an
LLM agent restored from a checkpoint re-performs an irreversible external action because
it re-synthesises a *different* request and the server's duplicate detection therefore
does not fire. Reading it against our own code, I found that `engine.rs` has the guard
that would prevent this — `may_perform_irreversible()`, which returns true only for
`Mode::Record` — and that **one of the three `Response::execute()` paths does not consult
it**. In `Mode::Replay { live }`, a target named `--live` whose effect is
`EffectKind::Irreversible` is executed against the real world during a replay.

## Source

- Primary: <https://www.alphaxiv.org/abs/2603.20625> — ACRFence, read in full via
  `orx paper`. Threat model, the 12-framework survey, both experiments, the mitigation.
- Primary, ours: `crates/noidroid-core/src/engine.rs` lines 621–709 (`on_call`), 826–843
  (`runs_live`, `may_perform_irreversible`), 845–872 (`deny_irreversible`).
- Primary, ours: `crates/noidroid-core/tests/vertical_slice.rs:226`
  (`a_replay_never_touches_the_world`), `:689`
  (`a_live_replay_reruns_only_what_it_was_asked_to`).

## What is interesting

The engine says the rule out loud, in a comment on the guard itself:

```rust
fn may_perform_irreversible(&self) -> bool {
    // Only an original recording is allowed to touch the world for real. Every
    // reconstruction and every branch is denied by default.
    matches!(self.mode, Mode::Record)
}
```

Three arms of `on_call` reach `Response::execute()`. Two check it:

- line 649 — `Phase::Counterfactual` under a live replay, for a target *not* named live;
- line 665 — `Phase::Fresh | Phase::Counterfactual`, the ordinary execute path.

The third does not:

```rust
Phase::Reconstructing if self.runs_live(&target) => {
    self.gone_live = true;
    self.pending = Some(Pending { action, effect, provenance: Provenance::Live, .. });
    Ok(Response::execute())
}
```

`effect` is carried straight through from the client's `call` request and never
inspected. So `noidroid replay <t> --live world` on a trajectory containing
`world.charge` declared `irreversible` charges the card again. `runs_live` is *prefix*
matching (`target == p || target.starts_with("{p}."))`, so `--live world` covers
`world.charge` without anyone typing a glob — the flag's own doc comment only ever
imagines `--live model`.

Two details make this quieter than it should be, and one makes it narrower:

- **Narrower:** `expect_match` runs first (line 627), so to reach this arm the call must
  already agree with the recording. This is not the divergent re-synthesis ACRFence
  describes; the divergent case falls through to `Phase::Counterfactual` and *is* guarded.
  What fires here is the plain double-spend: the same irreversible action, performed a
  second time, against a world that already has the first one.
- **Quieter 1:** `report.denied` stays empty, because nothing was denied. The run report
  has no line for "this replay performed an irreversible effect".
- **Quieter 2:** the CLI's denial hint (`main.rs:867`, "irreversible outside a recording")
  is only printed when `report.denied` is non-empty, so the one place a reader would look
  says nothing.

ACRFence's own survey is the reason to treat this as a live hazard rather than a
curiosity. Across 12 frameworks it found the same class of report: LangGraph maintainers
calling re-execution architecturally hard to fix, Google ADK documenting that its rewind
"cannot undo external side effects", a HashiCorp Vault issue where single-use tokens
reappeared after a snapshot restore, and duplicate-charge reports from CrewAI, AutoGen,
OpenHands, n8n and Claude Code. We built the mechanism that answers this. It has a hole
in it.

## Why it matters to Paranoid Android

This is `a_replay_never_touches_the_world` — a named test, one of the project's stated
claims — being true only because the test passes `live: &[]`. The one live-replay test,
`a_live_replay_reruns_only_what_it_was_asked_to`, names `world.read`. No test in the tree
combines `--live` with an irreversible target.

It bears on capture honesty and on the README's promise directly. `Replay { live }` is
the flagship hybrid: the mode we tell people to use when the prompt or the model changed.
It is exactly the mode ACRFence attacks, and it is the mode in which a user is most
likely to name a target broadly (`--live world`, `--live api`) to get a comparison to
work. The failure is silent in the run report and produces a trajectory that looks
faithful.

Note what is *not* wrong: the design. Denying irreversible effects outside a recording is
already the stated rule, `deny_irreversible` already writes a proper `EffectOutcome::Denied`
step with a `Provenance::Unknown` effect, and `--simulate <target>=<json>` is already the
escape hatch. The fix is to consult the guard on the third path, decide what `--live` on
an irreversible target should mean, and test it.

## Transferability

**HIGH** — it is our own code. The only design question is which of three behaviours the
third arm should take:

1. **Deny**, as `Fresh | Counterfactual` does, and let `--simulate` override. Consistent,
   and the strictest reading of the guard's own comment.
2. **Refuse the run up front**: if any `--live` prefix could match an irreversible target
   in the recording, refuse before starting rather than mid-run. Loudest, and checkable
   from the recorded chain without executing anything.
3. **Execute but report it**, adding a `report.performed_irreversible` line. Weakest, and
   it makes a replay's world-touching a footnote rather than a refusal.

(1) or (2). (3) is the shape this project usually calls wrong.

## Novelty

**MISSING.** The check does not exist on that path. `grep -n "Response::execute()"
crates/noidroid-core/src/engine.rs` returns three sites; two are preceded by
`may_perform_irreversible()` and line 693 is not. `noidroid doctor` (`doctor.rs`) covers
clock, randomness, subprocesses, the egress fence, client version and platform — it has
no check for this. Nothing in `crates/noidroid-core/tests/` exercises it.

## Limitations and negative signal

Against my own claim:

- I have **not run** the failing case. This is a read of the control flow, not an
  executed reproduction. That is the first thing the fix should do, and if a test written
  to fail passes instead, this card is wrong and should be struck.
- The severity depends on someone naming an irreversible target `--live`, which the
  documented use (`--live model`) does not. It is a foot-gun rather than a default.
- ACRFence's headline mechanism — divergent re-synthesis bypassing idempotency keys — is
  *guarded* in our engine, via `expect_match` and the `Phase::Counterfactual` arm. The
  transferable part of their paper is the empirical fact that agents do re-perform
  irreversible effects after a restore and that no surveyed framework prevents it; the
  specific hole I found is a plainer one.
- ACRFence itself is a workshop-scale paper: the mitigation is *designed, not evaluated*
  (their own words — "an implementation of ACRFence itself was not evaluated"), and its
  core comparison mechanism is an **analyzer LLM** deciding whether two tool calls are
  semantically equivalent. That is a fuzzy matcher, it is C4 in new clothes, and we should
  not take it. Our version of their idea is `key` equality plus a fatal divergence, which
  is the thing they had to approximate because they had no oracle.

## Recommendation

**PROTOTYPE** — write the failing test first, then pick behaviour (1) or (2).

## Proposed action

1. Add `a_live_replay_still_refuses_an_irreversible_target` to
   `crates/noidroid-core/tests/vertical_slice.rs`, modelled on the existing `irreversible`
   fixture: record, then `Mode::Replay { live: vec!["world".into()] }`, and assert the
   witness file contains `charge` exactly once. Confirm it fails.
2. Consult `may_perform_irreversible()` on the `Phase::Reconstructing if runs_live` arm,
   routing to `simulated_value` then `deny_irreversible`, as line 665 already does.
3. Decide whether `--live <prefix>` matching a recorded irreversible target should refuse
   the run before it starts. The recorded chain is walkable at `cmd_replay` time, so this
   is a pre-flight check, not a runtime one.
4. Add a `doctor` line, or a `replay` warning, naming which recorded targets a given
   `--live` prefix would cover.

## Confidence

**HIGH** on the code path — I read `on_call` in full, enumerated all three `execute()`
sites, and read both relevant tests. **MEDIUM** on the practical severity, because I did
not execute the case and the trigger requires a broad `--live` prefix.

## Evidence

- Primary: <https://www.alphaxiv.org/abs/2603.20625> — establishes that agents re-perform
  irreversible effects after a restore, empirically (10/10), and that none of 12 surveyed
  frameworks prevent it.
- Primary: `crates/noidroid-core/src/engine.rs:684-694` — the unguarded execute path.
- Supporting: `crates/noidroid-core/src/engine.rs:839-843` — the guard, and its comment
  stating the rule the path violates.
- Counter-evidence: `expect_match` at line 627 confines this to non-divergent calls, which
  is narrower than ACRFence's attack; and I did not reproduce it.

## Changelog

- 2026-08-24 — created, from the computer-use rollback scan.
