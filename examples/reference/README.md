# The reference environment

A reactor with a temperature and three control rods, and an operator whose policy is
reasonable and wrong.

It exists to be the smallest thing that is genuinely an *environment* rather than a
function, so that the whole lifecycle can be shown end to end in about a hundred lines:

```
record → checkpoint → reconstruct → branch → intervene → execute → compare
```

Read [`docs/environment-model.md`](../../docs/environment-model.md) alongside it. This
directory is that document made runnable.

## Why a reactor

Four properties, and every one of them is load-bearing:

| property | why it matters |
|---|---|
| state persists between steps | tick 4 depends on what tick 3 did, and no recorded return value carries the temperature forward into a counterfactual |
| it is acted on, not just read | `insert` and `withdraw` change it; `read` does not |
| it can be re-driven but not restored | the same moves from the same start reproduce it exactly, and nothing puts a running reactor back to how it was at 14:03 |
| it has one irreversible action | `scram` fires the emergency dump. It happens once, to a real world, and no reconstruction gets to un-fire it |

That is the shape of a browser, an RL environment, a robot and an autonomous
laboratory, minus everything that would make it slow to run in a test. It is
deterministic, has no dependencies and does no I/O, so it produces the same numbers on
every machine.

## Run it

```bash
export PYTHONPATH=$PWD/clients/python
noidroid run --name shift -- python3 examples/reference/agent.py
```

The operator chases output while the core is cool and pulls back once it is nearly too
late. It melts down on tick 4:

```
   0 ● genesis
   1 ● call reactor.read()
   2 ● decide move = "withdraw"
   3 ● call reactor.act({"move":"withdraw"})
   …
  13 ● call reactor.scram()
  14 ✘ finish failure
```

### What is known at a checkpoint

```bash
noidroid show shift@8
```

```
  state        1 file(s) root 0da17bf5b3 · witnessed
               · .world/reactor.json

  WHAT THIS CHECKPOINT GUARANTEES
    reach        rebuild+restore  re-execute the prefix, restoring around effects we will not re-perform
    evidence     witnessed  reported fingerprints are compared; the world cannot be corrected
    grounding    real
```

Three independent questions — can I get back, will I know if I got it wrong, is what I
get back to a claim about reality — and none of them is a percentage.

Ask about a checkpoint past the emergency dump and the answer is different:

```bash
noidroid show shift@14
```

```
    reach        unreachable
                 step 13 performed 'reactor.scram', which is declared irreversible,
                 in a world this run cannot put back.
```

`noidroid branch shift@14 …` is then refused before the program is spawned. Discovering
it halfway through would mean firing the dump a second time to find out we should not
have.

### Reconstruct it

```bash
noidroid replay shift
```

```
  steps re-derived       15/15 identical objects
  faithful: the reconstruction addresses the same objects as the recording
```

Nothing was executed. Every reading, every move and the reactor's own fingerprint came
out of the recording.

### Explore another future

```bash
noidroid branch shift@8 --decide move=insert --label saved
noidroid diff shift saved
```

```
  shared prefix    8 step(s) — the same objects, not copies
  diverged at      @8 replace-decision move = "insert"
  outcome          failure → success
  provenance       real → simulated
  workspace        ~ .world/reactor.json
```

Inserting one tick earlier survives the shift. The first eight steps are not copies of
the original's — they are the *same objects*, found to be identical by re-derivation.

### Ask which decision mattered

```bash
noidroid bisect shift
```

```
  @2 move = "insert"           success  ← flips it
  @2 move = "hold"             failure
  @5 move = "insert"           failure
  @5 move = "hold"             failure
  @8 move = "insert"           success  ← flips it
  @8 move = "withdraw"         failure
  @11 move = "hold"            failure
  @11 move = "withdraw"        failure
```

Two alternatives suffice, at different times, and six do not. Note that this is not
monotone: `@5` fails between two that succeed. Bisect is a **scan**, not a binary
search, because "which decision was sufficient to change the outcome" has no reason to
be a threshold.

## The two layers

`agent.py` has a `Shift` class, and it is the part worth copying. The same shape appears
in the browser adapter and would appear in a robotics one:

1. **Readings, decisions and actions are mediated.** They become recorded, replayable,
   branchable steps.
2. **The reactor is re-driven.** The engine reconstructs the *program* by serving it
   recorded inputs; nothing can put a reactor back, so `_catch_up()` re-performs the
   recorded moves and checks the result against the recorded fingerprint.

The second layer is not optional and it is not the engine's job. That is precisely what
grip `witnessed` means. Delete `_catch_up()` and the branch still runs, still hashes
consistently and still reports `success` — while describing a physics that never
happened. `the_counterfactual_world_is_re_driven_rather_than_assumed` in
`crates/noidroid-core/tests/environment_slice.rs` is the test that catches it, and the
only reason it can is that it knows which alternatives *should* flip.

## Seeing an opaque world

```bash
REFERENCE_BLIND=1 noidroid run --name blind -- python3 examples/reference/agent.py
noidroid show blind@8
```

The agent declares the reactor and tells Paranoid Android it is not looking at it:

```
    evidence     opaque  nothing is compared; a faithful reconstruction cannot be shown to be one
```

The recording still replays and still branches. It simply cannot be shown to have
landed in the same place — which is the truth about a robot, and worth more than a
number invented to fill the gap.

## Files

```
world.py   the environment: 100 lines, deterministic, knows nothing about noidroid
agent.py   the program: the policy, the mediated boundary, and the re-drive layer
```
