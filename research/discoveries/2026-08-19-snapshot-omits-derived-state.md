---
id: 2026-08-19-snapshot-omits-derived-state
title: RL's one real state-snapshot API restored the machine and silently lost the observation
discovered: 2026-08-19
updated: 2026-08-19
categories: [negative-signal, checkpointing, snapshotting, model-based RL, state reconstruction]
class: RESEARCH
recommendation: INVESTIGATE
transferability: HIGH
novelty: REFINEMENT
confidence: HIGH
touches: [engine, tree, store]
---

## Discovery

Reinforcement learning has no standard way to save and restore an environment's state,
and the one widely used implementation that does — the Arcade Learning Environment's
`cloneSystemState`/`restoreSystemState` — shipped for years with two defects of exactly
the kind this project exists to refuse. Its docstring claimed the cheap variant excluded
pseudorandomness when in practice the randomness lived in the RAM it copied, and its
"complete" restore put back the console's RAM but not the **screen**, because the Atari
2600 has no framebuffer: the observation is generated during emulation and was never in
the state. So `restoreSystemState()` followed by `getScreen()` returned a stale frame,
and the maintainer's fix was a dirty flag that asserts. The workaround researchers
adopted was to re-execute one recorded action to regenerate what the snapshot could not
hold.

## Source

Primary, read in full:
- <https://github.com/Farama-Foundation/Arcade-Learning-Environment/issues/165> —
  "System state not being restored deterministically". Reproducer in C++; maintainer
  `mgbellemare`: "When `restoreSystemState()` is called the environment's `ALERAM` and
  `ALEScreen` objects are not refreshed. It's possible to restore the first, but not the
  second (because the screen isn't serialized, since it isn't stored anywhere on the
  Atari 2600 hardware)." And: "There's no framebuffer in the Atari — the screen is
  actually generated on the fly, during the emulation step."
- <https://github.com/openai/gym/issues/1017> — "`clone_state()` for Atari games
  actually includes pseudorandomness", with `AdrienLE`'s explanation that the games seed
  themselves from initial RAM and register state, which `clone_state` copies, so the
  documented distinction was largely fictional.
- <https://github.com/Farama-Foundation/Gymnasium/issues/94> and issue #737 — maintainer
  `pseudo-rnd-thoughts`: "Gymnasium environment has no single state variable (some
  environments do but not all). Therefore, the easier way is to make a pickled version
  of the environment"; and `Altriaex`: "if you are using any wrappers, such as
  TimeLimit, then you will need to keep track of status of the wrappers."

## What is interesting

Three separate mechanism findings, and they point the same way.

**1. "The state" is not one object.** A Gymnasium environment's state is distributed
across the simulator, the wrapper stack (`TimeLimit`'s step counter, frame stacking,
observation normalisation running statistics) and the PRNG. There is no address that
names all of it, which is why the API does not exist and why `deepcopy` is the ecosystem
default — and `deepcopy` is broken too, because `EzPickle` reconstructs the environment
from its constructor arguments and discards the current state (Gymnasium #737).

**2. A snapshot cannot hold derived state, and the omission is invisible.** The screen
case is the cleanest example I have ever read of the failure this project names. The
snapshot was *complete with respect to the hardware*, restored successfully, reported no
error, and produced a wrong observation. Nothing in the API surface distinguished "the
state you asked for" from "the state you needed".

**3. The accepted workaround is re-execution.** From the same thread (`shelhamer`):
"For now I have been either separately saving frames or advancing the emulator one step
by a recorded action (in essence restoring the following state)." Faced with a snapshot
that could not reproduce the observation, a practitioner fell back to *replaying an
action from the recording*. That is our checkpoint model, adopted as a repair for a
snapshot model.

**4. Determinism was a configuration flag nobody knew about.** Issue #165's actual
resolution was `ale.setFloat("repeat_action_probability", 0)` — sticky actions, on by
default, made restore non-reproducible. `mgbellemare`: "the ALE stochasticity option
needs more visibility". A determinism property that depends on a default-on config knob
that is not surfaced at the API is the same class of problem as an ambient environment
we do not capture.

## Why it matters to Paranoid Android

Directly to a roadmap item. `README.md` and `CONTEXT.md` list, in order, "a snapshot
fast-path behind the same checkpoint interface" as the third thing to build, and C2
explicitly leaves the door open for it: "a snapshot *fast-path behind the same
checkpoint interface* is on the roadmap. Proposals must preserve the verification
story."

This card is the specification of what "preserve the verification story" has to mean,
written by somebody else's ten-year-old bug. A snapshot fast-path is a claim that
restoring bytes B at step k is equivalent to re-executing 0..k. ALE's screen shows that
claim can fail for state the snapshot never contained and did not know it was missing —
and fail *quietly*, because the restore succeeds.

We are unusually well placed to refuse this. The equivalence is checkable: restore the
snapshot, take one step, and compare the re-derived step address with the recorded one.
If they differ, the snapshot was incomplete, and `DivergenceKind::StateMismatch` already
localises it. A fast-path that is validated against the slow path on first use is a
fast-path that cannot lie; one that is trusted is ALE's screen.

Bears on: checkpoints, reconstruction fidelity, and the honesty of the `Grip::Captured`
label — because a snapshot fast-path would be the first thing in the system claiming
`captured` on a world we did not build from whole-file blobs.

## Transferability

HIGH as a constraint, LOW as a technique. There is nothing here to copy — it is a
negative result. What transfers is the shape of the failure: **hidden derived state**.
Ask of any snapshot mechanism, "what does the process regenerate rather than store, and
would restoring make it stale?" For us the candidates are obvious and worth naming now:
open file descriptors, memory-mapped regions, an adapter's live browser handle, anything
in `.world/` that a program computed rather than read.

Their assumption we do not share: ALE owns the whole machine, so "restore the hardware"
looked total. We already know we do not own the world, which is why the grip model
exists — this card says the same caution applies to the part we *do* own.

## Novelty

REFINEMENT. C2 already rejects memory snapshots as the checkpoint primitive, and the
existing card `2026-08-19-process-determinism-ceiling` covers a different ceiling. What
is new is a concrete, primary-source instance of a snapshot silently omitting derived
state, which is the specific risk of the roadmap's snapshot fast-path, and which nothing
in `research/` currently records.

Also new, and worth stating: **the RL ecosystem confirms our checkpoint model rather
than challenging it.** The domain the question named as a target has spent a decade
failing to build the snapshot API and settling on "reset with the seed, replay the
actions". That is `Reach::Rebuild`.

## Limitations and negative signal

This whole card is negative signal. The honest counterweight: ALE's problem was a
hardware emulator with no framebuffer, which is a peculiar architecture, and the bug was
fixed a decade ago. Somebody will say it does not generalise. The generalisation is not
"emulators are bad", it is "a restore that succeeds is not evidence that the state is
complete", and Gymnasium #94/#737 show the same shape without any emulator: the wrapper
stack's state is not in the simulator's state, and everyone finds this out by getting
wrong results.

Argument against acting at all: we have not built the fast-path, so this is a constraint
on unwritten code and could simply be recorded. That is why the recommendation is
INVESTIGATE and not PROTOTYPE — the question is whether the validation is cheap enough
to be mandatory, and that is answerable on paper before anyone writes the fast-path.

## Recommendation

INVESTIGATE — answer, before the snapshot fast-path is designed, whether every restore
can be cheaply validated by re-derivation, because if it cannot, the fast-path cannot
claim `captured`.

## Proposed action

Write the acceptance criterion into the snapshot fast-path work before it starts, as a
named invariant test rather than a doc line:

> `a_snapshot_restore_is_validated_against_re_execution` — for a trajectory with a
> snapshot at step k, restoring at k and executing step k+1 must produce the same step
> address as re-executing 0..k+1 from genesis. If it does not, the run is refused and
> the snapshot is reported incomplete, naming the first differing tree path.

The open question to answer first, on paper: what does that validation cost? It requires
one full slow-path reconstruction per snapshot, which is exactly the work the fast-path
exists to avoid. Candidate answers to compare: validate once when the snapshot is
*written* rather than every time it is read; validate on a sampled subset; or make the
fast-path opt-in and mark trajectories that used it with a weaker evidence label. A
negative result — "validation costs as much as the thing it accelerates, so the
fast-path can only ever be an unverified convenience" — is a useful outcome and should
be recorded as a constraint rather than worked around.

## Confidence

HIGH. Both ALE/gym issues were read in full including maintainer replies, via the GitHub
API rather than search snippets. The Gymnasium issues were read the same way. No claim
here rests on a summary.

## Evidence

- Primary: <https://github.com/Farama-Foundation/Arcade-Learning-Environment/issues/165>
  — the screen is not in the serialised state and `restoreSystemState` leaves it stale;
  the fix is an assert, not a restore.
- Primary: <https://github.com/openai/gym/issues/1017> — the documented distinction
  between `clone_state` and `clone_full_state` did not describe what was captured.
- Primary: <https://github.com/Farama-Foundation/Gymnasium/issues/94> — no single state
  variable; wrapper state is separate; `deepcopy` is the workaround.
- Supporting: <https://github.com/Farama-Foundation/Gymnasium/issues/737> — `EzPickle`
  discards live state on deepcopy, so the ecosystem's default workaround is also wrong.
- Counter-evidence: MuJoCo genuinely does expose a complete `qpos`/`qvel`/`act`/`time`
  state (`gymnasium/envs/mujoco/mujoco_env.py:set_state`), so a *simulator* with a
  designed state vector can be snapshotted honestly. The failure is at the wrapper and
  observation layers, not necessarily at the physics layer.

## Changelog

- 2026-08-19 — created.
