---
id: 2026-08-19-unverified-world-redrive
title: Re-driving a world and never checking it landed — the same gap in four unrelated domains
discovered: 2026-08-19
updated: 2026-08-20
categories: [unserved-problem, negative-signal, state reconstruction, robotics / ROS, laboratory automation, simulation]
class: INSPIRATION
recommendation: PROTOTYPE
transferability: HIGH
novelty: MISSING
confidence: HIGH
touches: [engine, model, cli, proto, clients]
---

## Discovery

Every domain that returns a physical or stateful world to an earlier point does it the
same way: **re-perform the recorded actions against a fresh world.** None of the four
I read then does the second half — compare the resulting world against what the
recording says it looked like, and say so when it does not match. LeRobot replays an
episode onto a real arm and never reads the recorded observations back. Bluesky rewinds
a beamline plan by re-sending cached messages and never compares the second reading to
the first. `ros2 bag play` publishes messages at wall rate with no acknowledgement from
the consumer. Autonomous-driving resimulation is the only one that measures divergence
at all, and it does so against a tuned threshold rather than a recorded fingerprint.

The reason this is a card and not a survey line is that **our environment model has the
same hole, and it is now shipped.** `docs/environment-model.md` §7.1 states it
outright: "The engine cannot check that you did it." I verified that the code makes the
skip not merely unchecked but *invisible* — an adapter that never re-drives produces a
run that reports `witnessed` and prints a sentence claiming fingerprints were compared.

## Source

Primary, all read directly:

- LeRobot replay loop — `docs/source/il_robots.mdx` §"Replay an episode",
  <https://github.com/huggingface/lerobot/blob/main/docs/source/il_robots.mdx>. The
  documented API example is 15 lines: iterate `dataset.select_columns("action")`, call
  `robot.send_action(action)`, `precise_sleep`. `dataset` also holds the recorded
  observations; the loop never touches them.
- Bluesky `RunEngine._rewind` —
  <https://github.com/bluesky/bluesky/blob/main/src/bluesky/run_engine.py> lines
  1035–1055 and 1655–1670. Read in full; see the sibling card
  `2026-08-19-checkpoint-as-message-cache`.
- Applied Intuition, "Using re-simulation to verify AV stack",
  <https://www.appliedintuition.com/blog/closed-loop-log-replay>.
- ROS Discourse, "Fast, accurate, robust replay in ROS2",
  <https://discourse.openrobotics.org/t/fast-accurate-robust-replay-in-ros2/30406>.

Our own code, verified this run:

- `crates/noidroid-core/src/env.rs` — `Situation::adopt`, `Situation::fresh`,
  `Reported::grip`.
- `crates/noidroid-core/src/engine.rs` lines 963–981 — the `Phase::Reconstructing`
  branch that calls `adopt` for every world in the recorded tree.
- `crates/noidroid-cli/src/main.rs` lines 512–517 — where `Grip::evidence()` is printed.

## What is interesting

The mechanism is identical everywhere and it is not a snapshot: **the world is put back
by replaying the instructions that built it.** That is our `Reach::Rebuild`, arrived at
independently by robotics, by synchrotron control, and by AV resimulation, because it
is the only method available when you do not own the world's bytes.

What differs is the evidence standard, and the honest ranking is:

| system | re-drives | compares result to recording | reports the comparison |
| --- | --- | --- | --- |
| `lerobot-replay` | yes | no | no — docs say the robot "should replicate movements *similar to* those you recorded" |
| bluesky rewind | yes | no | no — the second reading is emitted as a new event document |
| `ros2 bag play` | yes (publishes) | no | no — no back-pressure; "if your machine can't keep pace you'll start dropping messages" |
| AV resimulation | yes | yes, against a threshold | yes — "monitor the distance … compare to a predetermined threshold to determine whether the log-based simulation is valid or invalid" |
| Paranoid Android | adapter's job | **only if the adapter volunteers** | reports `witnessed` either way |

The last row is the finding. Trace it through the code:

1. During reconstruction, `Engine` calls `Situation::adopt(name, seen)` for every world
   named in the recorded step's tree (`engine.rs` ~line 974).
2. `adopt` is a no-op for a world in `self.fresh` — the set the program has spoken about
   since the last observation — and otherwise sets `Reported.seen` to the **recorded**
   digest.
3. `Reported::grip()` returns `Witnessed` whenever `seen.is_some()`, with no memory of
   where `seen` came from.
4. `Situation::observe` therefore hashes the adopted fingerprint back into the tree, the
   state root equals the recorded one **by construction**, and no `state_mismatch` can
   occur.
5. `self.report.grip` joins to `Witnessed`, and the CLI prints
   `"reported fingerprints are compared; the world cannot be corrected"`.

The design intent is defensible and the comment in `engine.rs` argues it well —
testimony obeys the recorded-input oracle like any other input, and a program that is
not touching its world has nothing new to say. The problem is not the substitution. It
is that **the substitution is not reported**, so "the adapter re-drove the browser and
the page hashed identically" and "the adapter did nothing and we filled the answer in
from the recording" produce the same run report and the same word, `witnessed`.

`Situation` already knows which case it is in: `fresh` distinguishes them exactly, per
step, and is discarded by `settle()` without ever being read for this purpose.

## Why it matters to Paranoid Android

This is the project's stated worst outcome — a trajectory that looks real — reachable
through the newest subsystem, without a bug. It is `capture honesty` and
`reconstruction fidelity`, not adapters: the misreport comes from `Reported::grip`
collapsing two different provenances of the fingerprint into one value.

It also decides the answer to the question this scan was asked. Robotics and autonomous
labs are the two rows of the §12 conformance table with the weakest grip, which means
they are the two rows where *almost the entire evidence claim rests on the adapter
having done something the engine cannot see*. Shipping the environment model without
this distinction would mean the first robot adapter anyone writes produces clean-looking
reconstructions of a physics that never happened — and the LeRobot evidence says the
domain's default replay loop is precisely the one that skips the check.

The fix is small and lands on the axis the architecture already has. The observation was
**delivered** from the recording rather than **executed** against the world. That is
`Delivery`, which is per-run and unhashed (C3, `model.rs`), so nothing about step bytes
or `STEP_VERSION` moves.

## Transferability

HIGH — but the transfer is the *problem*, not a solution. Nobody in these four domains
has a mechanism we can copy; three of them have the gap and the fourth papers it with a
tuned threshold we should not adopt (see
`2026-08-19-log-replay-validity-modes`). What transfers is the evidence that the failure
is systemic rather than sloppy: when the only way back is to re-drive, checking the
re-drive is a separate discipline that everyone skips because skipping it costs nothing
and looks identical.

Our position is genuinely better on one point and we should keep it: the recording
already holds the fingerprint to check against, in the same hashed tree as everything
else (`WORLD_DIR`), so the comparison costs a digest equality. LeRobot has the recorded
observations too and does not use them; the difference is that ours is already wired
into the state-root comparison, if we stop pre-filling the answer.

## Novelty

MISSING. Grepped: nothing in `env.rs`, `engine.rs` or `main.rs` records or prints the
provenance of an adopted observation. `Situation::fresh` is the only place the
distinction exists and it is cleared by `settle()` without being surfaced. There is no
test named for this invariant in `env.rs` or `engine.rs`.

## Limitations and negative signal

The strongest argument against acting is in the environment-model document itself: the
engine *cannot* verify the world, only the report about it, so any signal we add is
still a statement about adapter behaviour rather than about physics. An adapter could
re-observe without re-driving and get a `witnessed` badge it has not earned. True — but
that is a lie the adapter had to construct, whereas today it gets the same badge for
doing nothing at all. Moving from "silence passes" to "silence is recorded as silence"
is the whole of the improvement being claimed, and it should be claimed as no more.

Second: for the workspace-only case (the overwhelming majority of runs today) this
changes nothing, because there are no declared worlds. The value is entirely in the rows
of §12 that have not shipped yet, which would make it speculative in the sense C9 warns
about — except that as of 0.3.0 there is an in-tree adapter with a declared `witnessed`
world (`c1fb622`, "declare the page as a world the core understands") plus a reference
environment (`56227ac`), so the lenient path is exercised, not hypothetical.

Third: there is a real risk of over-refusal. A `witnessed` world whose adapter cannot
re-drive at all — an instrument reading, an RL env without `save_state` — would end up
reporting `opaque` on every reconstruction. That is arguably correct, but it makes the
grip a property of the *run* rather than of the trajectory, and readers will conflate
them. The report needs to say which one it is describing.

## Recommendation

PROTOTYPE — the environment model's one acknowledged blind spot is observable from
inside the engine, cheaply, and closing it converts a silent pass into a stated one.

## Proposed action

In `crates/noidroid-core/src/env.rs`, make `Reported` remember how it got its
observation — an enum alongside `seen` (`Reported` vs `Adopted`), set by `report()` and
`adopt()` respectively. Leave `Reported::grip()` — the grip *of the recording* — alone,
and add a second accessor for the grip **this run achieved**, which is `Opaque` for an
adopted world. Join that into `Report.grip` in `engine.rs` instead of `observed.grip`,
and in `crates/noidroid-cli/src/main.rs` print, per declared world:

```
world browser   not re-driven — state served from the recording; nothing was checked
```

Acceptance criteria, as invariant-named tests:

- `a_world_the_adapter_never_redrove_is_not_reported_as_witnessed`
- `an_adapter_that_redrives_and_matches_still_reports_witnessed`
- `a_workspace_only_run_reports_exactly_what_it_did_before`

Measure on the in-tree browser adapter by deleting the `Browser._reconstruct` re-drive
in a scratch copy: the hypothesis is that today's run report is byte-identical with and
without it, and after the change it is not. If the run report already differs, this card
is wrong and should be downgraded to IGNORE with that note.

## Confidence

HIGH. Every claim about our own behaviour was read in the source this run and the code
path is short. The LeRobot and bluesky mechanisms were read as code/docs, not summaries.
The `ros2 bag play` claim rests on a Discourse thread and issue titles rather than the
rosbag2 source, and is the weakest line in the table — it is corroborating, not
load-bearing.

## Evidence

- Primary: <https://github.com/huggingface/lerobot/blob/main/docs/source/il_robots.mdx>
  — the documented replay loop sends recorded actions to a real robot and reads no
  recorded observation back; the doc's own success criterion is "similar to".
- Primary: <https://github.com/bluesky/bluesky/blob/main/src/bluesky/run_engine.py>
  — `_rewind()` returns "a new plan made from the messages in the message cache"; there
  is no comparison step anywhere in the rewind path.
- Primary: `crates/noidroid-core/src/env.rs`, `engine.rs:963-981`,
  `noidroid-cli/src/main.rs:512-517` — the adopted-fingerprint path and the sentence it
  causes to be printed.
- Supporting: <https://www.appliedintuition.com/blog/closed-loop-log-replay> — the one
  domain that does check, and it checks against a threshold, not a recording.
- Supporting: <https://discourse.openrobotics.org/t/fast-accurate-robust-replay-in-ros2/30406>
  — `ros2 bag play` "doesn't close the loop"; consumers silently drop messages.
- Counter-evidence: `docs/environment-model.md` §7.1 — the authors already know and
  argue the engine structurally cannot verify a world it does not own. This card does
  not dispute that; it disputes that the *report* should be identical either way.

## Who hits this

- LeRobot (Hugging Face) — replay onto real hardware, no observation comparison:
  <https://github.com/huggingface/lerobot/blob/main/docs/source/il_robots.mdx>
- Bluesky / NSLS-II — rewind by message replay, no comparison of the re-read values:
  <https://blueskyproject.io/bluesky/main/state-machine.html>
- ROS 2 / rosbag2 — replay with no back-pressure or consumer acknowledgement:
  <https://discourse.openrobotics.org/t/fast-accurate-robust-replay-in-ros2/30406>
- Applied Intuition / nuPlan — divergence acknowledged, measured against a threshold:
  <https://www.appliedintuition.com/blog/closed-loop-log-replay>

Four independent sources, none of them agent tooling, none of which knows we exist.

## Why it is unsolved

Structural, and we share most of it: the only party that can observe a world the tool
does not own is the program, so any check is a check on testimony. What is *not*
structural is the reporting. Every one of these systems could record "the re-drive was
not verified" and none does, because the natural implementation makes the unverified
path indistinguishable from the verified one — exactly as ours does today.

## Would Paranoid Android's model help?

For the physics: no, and the environment model already says so. For the *claim*: yes,
and uniquely — we are the only one of the five that already stores the fingerprint in
the same content-addressed tree as everything else, so the comparison is a digest
equality rather than a new subsystem. That is a narrow win and should be sold as one.

## Changelog

- 2026-08-19 — created against `env.rs` and `checkpoint.rs` as uncommitted work in flight
  (#48).
- 2026-08-20 — **the environment model landed.** HEAD moved from `af81680` to `10c1b64`
  via `chore(release): 0.3.0` (`eb497cf`) while this run was paused. I re-read the code
  at the new HEAD: `Situation::adopt` (`env.rs:335`), the `Phase::Reconstructing` branch
  (`engine.rs:971-979`) and the printed evidence sentence
  (`noidroid-cli/src/main.rs:512-517`) are **unchanged**, so every claim in this card
  still holds. What changed is the framing: this is no longer a design objection to a
  branch, it is a defect in released behaviour, and there are now two in-tree adapters
  (browser, reference environment) that declare worlds. The only uncommitted diff in
  `crates/` is unrelated dead-code removal (`Checkpoint.branchable`, `checkpoint::all`).
