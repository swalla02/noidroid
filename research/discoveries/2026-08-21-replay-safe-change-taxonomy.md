---
id: 2026-08-21-replay-safe-change-taxonomy
title: Temporal runs our architecture in production and reports that the dominant divergence cause is code change, not capture
discovered: 2026-08-21
updated: 2026-08-21
categories: [deterministic replay, record/replay systems, state reconstruction, divergence reporting, capture honesty]
class: INFRASTRUCTURE
recommendation: INVESTIGATE
transferability: HIGH
novelty: REFINEMENT
confidence: HIGH
touches: [engine, model, cli, docs]
---

## Discovery

Temporal's workflow replay is our architecture: re-execute the program from the start,
serve every boundary interaction from a recorded history, match **positionally**, and
fail the run when the program asks for something other than what the history holds at
that index. They have run it in production for years across seven SDKs. Their docs name
exactly two causes of a non-determinism error, and the first one is not what a capture
tool expects:

> "The following are the two reasons why a Command might be generated out of sequence or
> the wrong Command might be generated altogether:
> 1. **Code changes are made to a Workflow Definition that is in use by a running
>    Workflow Execution.**
> 2. There is intrinsic non-deterministic logic (such as inline random branching)."

Around cause 1 they have built an entire subsystem — `GetVersion` / `patched`, and
Worker Versioning — and, more useful to us, a published **taxonomy of which code changes
are replay-safe and which are not.**

## Source

Primary, read directly:

- <https://docs.temporal.io/workflow-definition.md> (the `.md` form of the docs page),
  sections "Deterministic constraints", "Code changes can cause non-deterministic
  behavior", "Intrinsic non-deterministic logic", "Versioning Workflows".
  - The list of API calls that produce Commands and therefore "must not be reordered,
    added, or removed without proper Versioning techniques".
  - The explicit safe list: changing activity/child-workflow **arguments**, return
    values and timeouts; changing a timer's duration (with named exceptions at 0 and
    -1); adding a signal handler for a signal type not yet sent.
  - The explicit unsafe list: changing the **types or IDs** of activities or child
    workflows; reordering commands.
- <https://docs.temporal.io/develop/go/versioning> — `workflow.GetVersion(ctx,
  "Step1", workflow.DefaultVersion, 1)`, the worked example of migrating
  `ActivityA` → `ActivityC` → `ActivityD`, and the retirement path where `minSupported`
  is raised once old executions have left retention.
- <https://docs.temporal.io/develop/python/workflows/basics.md> — the determinism rules
  ("no threading, no randomness, no external calls to processes, no network I/O, no
  global state mutation, no system date or time") and `workflow.unsafe.is_replaying`
  with its warning.

## What is interesting

**The mechanism.** `GetVersion(changeId, minSupported, maxSupported)` writes a marker
into the event history the first time it is reached. On replay, that marker is served
back, so an execution recorded before the change keeps taking the old branch while new
executions take the new one. Crucially this is **not** fuzzy matching: the history is
still matched exactly, positionally, and divergence is still fatal. What the marker
changes is *which* code path the program deterministically takes, by making the
version an input rather than a property of the binary.

**The taxonomy is the transferable part.** Temporal can say, with precision, what you
may edit and still replay. Arguments: yes. Identity and order: no. That distinction
falls straight out of the matching predicate, and we have the same predicate:

```rust
// crates/noidroid-core/src/engine.rs, actions_agree
(Action::Call { target: a, args: aa, effect: ae },
 Action::Call { target: b, args: bb, effect: be }) => a == b && aa == bb && ae == be,
(Action::Decide { name: a, options: ao, .. },
 Action::Decide { name: b, options: bo, .. })      => a == b && ao == bo,
```

Ours is **stricter than Temporal's on exactly the axis they found had to be loosened**.
Temporal permits changing an activity's arguments; we compare `args` for equality, so
any change to a call's arguments is a `KeyMismatch`. For a `Decide`, we compare
`options` — so if a program passes its available tools as the option set, adding one
tool diverges every decision point in every recorded trajectory, not only the one whose
behaviour changed.

**And the reason the difference is correct.** Temporal loosens because their goal is
*resumption*: a running workflow must survive a deploy. Ours is *localisation*: the
whole pitch (C6 — "lead with divergence localisation") is telling you where your change
first mattered. A divergence caused by a code change is our output, not our failure.
`noidroid replay` already exits 1 and prints `@index kind — detail`
(`crates/noidroid-cli/src/main.rs`, `cmd_replay`), so the CI regression-test workflow
Temporal ships as `WorkflowReplayer` is mechanically already available to us.

**One footgun we ship and they document.** `Response` carries `delivery` to the program
(`proto.rs`), so a noidroid client can see whether it is being replayed. Temporal
exposes the same thing as `workflow.unsafe.is_replaying` and attaches a hard warning:
"Never use this to affect Workflow business logic — branching on replay status breaks
determinism." A program that branches on our `delivery` field produces a prefix that is
not the prefix it recorded, and nothing in our engine would catch it — the calls would
differ and we would report a `KeyMismatch` at the wrong place, blaming the program's
logic rather than its introspection. We ship the affordance and no warning.

## Why it matters to Paranoid Android

Three specific things.

1. **We have no answer to the first question a user will ask.** "I changed my agent —
   will my recordings still replay?" Temporal answers it with a table. We answer it with
   "try it". Writing our version of that table is documentation work over
   `actions_agree`, costs a day, and is the difference between a tool people trust with
   week-old recordings and one they re-record from every time they touch the code.

2. **The `Decide.options` comparison deserves a deliberate decision, not an inherited
   one.** It may well be right — a decision among a different option set arguably *is* a
   different decision. But it is currently a consequence of how `actions_agree` was
   written rather than a position anyone argued, and it interacts badly with the
   workflow the roadmap is aiming at (`Replay { live }` for a changed prompt or model).
   I have not tested whether real programs put volatile content in `options`; that is the
   question the investigation answers.

3. **The `delivery` footgun is cheap to close.** A sentence in the client docstring and
   the protocol doc, matching Temporal's wording. Or, more strongly, stop sending
   `delivery` to the program at all and keep it in the run report — it is a per-run axis
   (C3) and arguably has no business crossing into the program's logic.

Bears on: replay, divergence reporting, capture honesty, and the integration boundary.

## Transferability

HIGH for the taxonomy and the warning; **LOW for `GetVersion` itself**, and that is the
finding. The version marker exists to make replay survive a code change. We do not want
replay to survive a code change — we want it to tell you where the change bit. Adopting
patching would be adopting a mechanism whose purpose is to suppress our product's
output. Recording this explicitly so nobody proposes it later.

The one context where a marker-like mechanism would earn its place is the branch: if we
ever want "replay this trajectory against v2 of the agent, and tell me the *first*
place they differ, then keep going" — a continue-past-divergence mode. That is squarely
against C4 and I am not proposing it.

## Novelty

REFINEMENT. The matching predicate, positional keys and fatal divergence are all already
in `engine.rs`. What is new to us is (a) that the largest production deployment of this
exact design reports code drift, not capture gaps, as the dominant divergence source,
and (b) that they answered it with a published change-safety taxonomy we could write for
ourselves in an afternoon.

## Limitations and negative signal

- **Their environment is not ours.** Temporal workflows run for months; ours run for
  minutes. Code drift dominates their divergences partly because their executions
  outlive deploys. Do not import the conclusion wholesale — import the taxonomy.
- **Their loosened comparisons cost them something.** Because argument changes are
  permitted, a workflow can be replayed against code that passes different arguments and
  Temporal will not tell you. That is exactly the silent-difference class this project
  refuses. Our stricter predicate is the better one *for us*, and this card should not
  be read as an argument to relax it.
- **`GetVersion` is famously awkward in practice** — the docs' own worked example ends
  with dead branches that can only be removed once old executions have "left retention",
  which is a retention-policy dependency inside application code. Their newer answer
  (Worker Versioning) exists because of it, and the legacy version of *that* is being
  removed from the server in March 2026. A mechanism on its third iteration is a
  mechanism whose problem is hard.
- I did not open the `WorkflowReplayer` implementation in any SDK; the claim that the CI
  replay-test workflow is standard practice there rests on the docs, not on source.

## Recommendation

INVESTIGATE — answer one question: **does `actions_agree`'s equality on `Call.args` and
`Decide.options` make ordinary agent edits unreplayable in a way that damages the
localisation story, or is it exactly the strictness the product needs?**

## Proposed action

Half a day, no production code, then a docs PR.

1. Take `examples/reference/agent.py`. Record a trajectory. Then make, one at a time,
   the five edits a user would actually make: add a tool to the option set, rename a
   tool, reorder two independent calls, change a prompt string inside a call's args, add
   a call that was not there before.
2. For each, record which `DivergenceKind` fires, at which index, and whether the report
   points at the edit or somewhere unrelated.
3. Write the result up as a **"what you can change and still replay"** table in
   `README.md` or `docs/`, in the shape of Temporal's. It is generated by the
   experiment, not by reading `actions_agree`, so it is true rather than intended.
4. Separately and independently: add the `is_replaying` warning to the client docstring
   in `clients/python/noidroid/__init__.py` and to `proto.rs`'s `Response` docs, or
   decide to stop sending `delivery` to the program.

**How we would know it failed.** If every edit produces a divergence at the exact index
of the edit with a report that names it, the concern is unfounded, the table is still
worth publishing, and this card closes as documentation. If a single-tool addition
diverges at index 3 when the edit's first effect is at index 40, that is a localisation
bug and it becomes an issue.

## Confidence

HIGH on Temporal's mechanism and taxonomy — the docs pages were read in their markdown
form and the lists are verbatim. HIGH on our matching predicate — read at
`engine.rs::actions_agree`. MEDIUM on the practical consequence: whether real programs
put volatile content in `Decide.options` is exactly what step 1 above tests, and I am
not asserting it.

## Evidence

- Primary: <https://docs.temporal.io/workflow-definition.md> — the two causes of
  non-determinism errors; the safe/unsafe change lists.
- Primary: <https://docs.temporal.io/develop/go/versioning> — `GetVersion`, the marker
  in the event history, the version-retirement path.
- Primary: <https://docs.temporal.io/develop/python/workflows/basics.md> —
  `workflow.unsafe.is_replaying` and its warning.
- Ours: `crates/noidroid-core/src/engine.rs` (`actions_agree`, `expect_match`,
  `describe_mismatch`), `crates/noidroid-cli/src/main.rs` (`cmd_replay` exits 1 on
  divergence), `crates/noidroid-core/src/proto.rs` (`Response.delivery`), C4 and C6 in
  `research/constraints.md`.

## Changelog

- 2026-08-21 — created.
