# What can you change and still replay?

Temporal runs the same architecture we do — re-execute a program from the start,
serve every boundary interaction from a recorded history, match positionally, fail
loudly on the first mismatch — at a larger scale than anyone else, and their own
data says the dominant cause of a replay failure is an ordinary code change, not a
capture gap. They answer the question "I changed my code, will my history still
replay?" with a published taxonomy of safe and unsafe edits. We had never answered it
for ourselves; `actions_agree` in `crates/noidroid-core/src/engine.rs` compares
`Call.args` and `Decide.options` for equality, which is *stricter* than Temporal on
exactly the axis they were forced to loosen, but that strictness was inherited from
how the comparison was written, not argued from evidence.

This page is that evidence. It answers one question: when you make the kind of edit
an agent developer actually makes, does `noidroid replay` diverge exactly where the
edit happened, or somewhere else? See
[`research/discoveries/2026-08-21-replay-safe-change-taxonomy.md`](../research/discoveries/2026-08-21-replay-safe-change-taxonomy.md)
for the full background and Temporal's sources.

## Method

1. Record a trajectory from the unmodified [`examples/reference/agent.py`](../examples/reference/agent.py)
   — a reactor operator that reads a temperature, decides a move among
   `["insert", "hold", "withdraw"]`, and acts, six ticks in a row, melting down on
   tick 4. `noidroid run --name shift -- python examples/reference/agent.py` produces
   a 15-step trajectory: `0` genesis, then per tick `read → decide → act`
   (`1,2,3`, `4,5,6`, `7,8,9`, `10,11,12`), then `13` the emergency scram and `14`
   the `failure` verdict.
2. Apply one edit to a *copy* of the agent, run `noidroid replay shift`, record what
   fired, then `git checkout` the edit back out before making the next one — every
   edit is tested independently against the same original recording, never stacked.
3. Read the divergence the CLI printed (`@<index> <kind> — <detail>`) and judge
   whether the detail names the actual edit, or something else.

## Results

Measured against the current engine. Each edit was applied to its own copy of the agent
and replayed against the same original recording.

| # | Edit | Kind | Step | What the report says |
|---|------|------|------|----------------------|
| 1 | Add an option to a `Decide` (`MOVES` gains `"scram_now"`) | `key_mismatch` | 2, the first `decide` | `options: recorded [...], got [...]` — the exact field. |
| 2 | Rename a call (`reactor.read` → `reactor.sense`) | `key_mismatch` | 1, the first `read` | `target: recorded "reactor.read", got "reactor.sense"`. |
| 3 | Reorder two independent calls (tick 0's `act` swapped with tick 1's `read`) | `key_mismatch` | 3, the swap boundary | *"this call is recorded at step 4; 1 interaction(s) before it were removed, or moved later"*. Before #78 this said only "removed". |
| 4 | Change an argument (`read()` gains `args={"channel": "primary control room"}`) | `key_mismatch` | 1, the first `read` | `args.channel: not recorded, got "primary control room"`. |
| 5 | Add a call (`shift.audit` before tick 3's `read`) | `key_mismatch` | 10, the insertion point | `target: recorded "reactor.read", got "shift.audit"` and *"it looks like it was added here"*. |

Every run reports the mismatch and then `truncated`: the engine fails the request, the
client raises `Divergence`, and the program stops. The `truncated` line is a consequence
of the first divergence, not a second finding.

For edits 1, 2 and 4 the report also carries the insertion guess — *"this interaction
appears nowhere in the recording; it looks like it was added here"* — followed by its own
hedge: *"a rewritten interaction looks the same from here — one mismatch cannot tell the
two apart"*. The field diff above it is the part that names the edit.

## What this means

**The strictness is earning its keep, not just inherited.** All five edits are things
an agent developer does routinely — extend a tool list, rename a function, reorder
independent work, tweak a prompt, add a step — and all five are caught immediately,
at the right place, with an explanation that survives reading. Temporal's decision to
loosen argument-equality was made for their goal (a workflow must survive its own
redeploy); ours is localisation, and for localisation the strict comparison is
correct, not merely convenient. `research/constraints.md`'s existing position —
divergence stays fatal, matching stays positional — holds up.

**One real limitation, not a bug.** `Decide` matching ignores `choice`, and `read()`'s
identity here never varies (target and args are constant across every tick). Between
two ticks whose recorded action is *identical* in target/args/options, a reorder of
those specific calls is invisible to positional matching — there is nothing to
disagree about. That is not a defect in `actions_agree`; it is the necessary
consequence of matching on identity rather than content, and it only bites when two
positions genuinely have the same identity. Edit 3 above deliberately swapped calls
whose identities *do* differ (`act`'s args carry the move) precisely so the swap would
be visible; a swap of two same-shaped `read()` calls would not have been.

**Localisation holds, and the one cause-naming gap is fixed.** Every edit diverged at its
own first point of effect, and every field diff named the edit. Per the discovery card's
falsification condition, localisation is not the problem this page was looking for.

Edit 3 turned up the one wording error: a reorder was described as a removal. A single
mismatch cannot tell the two apart, because the run stops before the displaced call
would reappear, so the message now names both readings. The step was always right; only
the wording was wrong.
