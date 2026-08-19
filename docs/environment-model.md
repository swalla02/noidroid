# The environment model

*The contract between Paranoid Android and a world it did not write.*

This document answers one question:

> What is the minimum an environment must satisfy for Paranoid Android to record an
> execution, reconstruct a meaningful point in it, and explore a counterfactual from
> there?

It is a contract, not a tour. Where it says **must**, the engine enforces it or refuses.
Where it says **may decline**, declining is a first-class answer and is represented in
the recording rather than hidden.

Read [direction.md](direction.md) first if you have not. The rule it states —
*do not fake capabilities we do not have* — is the only reason this document is as
complicated as it is, and the reason it is not more complicated than it is.

---

## 1. The loop

Every environment Paranoid Android targets runs the same loop, whatever it is made of:

```
observation → decision → action → effect → observation → …
```

| Environment | observation | decision | action | effect |
|---|---|---|---|---|
| Python application | arguments, returned values | branch taken | function call | return value, files written |
| Browser agent | rendered page | model's next move | click, fill, navigate | new page, network traffic |
| RL environment | `obs` | policy output | `step(a)` | next `obs`, reward |
| Robot | sensor frame | controller output | actuator command | new sensor frame, moved world |
| Autonomous lab | instrument reading | experiment choice | run the protocol | consumed reagents, new reading |
| Production service | request, config | application logic | outbound call | response, other people's state |

These differ in latency, cost and reversibility by many orders of magnitude. They do
not differ in shape. Paranoid Android records the loop, not the machinery.

Two things follow immediately, and they are the whole design:

1. **The recording is of the boundary, not of the machine.** We record what crossed
   between the program and the world. We do not record the program's memory and we do
   not record the world.
2. **Returning to a point is re-execution, not restoration.** A checkpoint is
   re-entered by re-running the prefix with every input served from the recording. The
   program rebuilds its own state, which is the one thing it is guaranteed to be able
   to do.

## 2. Vocabulary

| Term | Meaning |
|---|---|
| **program** | the process under recording. It declares its own boundary. |
| **environment** | everything the program interacts with across that boundary. |
| **world** | environment state that persists *between* steps and is not carried by the recorded effects. Most environments have none. See §6. |
| **trajectory** | the recorded evidence of one execution: an immutable chain of steps. |
| **step** | one node of that chain: `(parent, action, effects, state, provenance)`, addressed by the hash of its content. |
| **checkpoint** | a step, considered as a place you might return to. Not a separate object. |
| **branch** | a step whose parent belongs to another trajectory. |

There is no `Checkpoint` object on disk, and no `Branch` object. Both are *readings* of
the step chain. That is deliberate: a checkpoint you have to create is a checkpoint
somebody forgot to create.

## 3. What a trajectory records

A trajectory is an ordered chain of steps. Each step records:

- **`action`** — what the program said it was about to do. One of `genesis`, `call`,
  `decide`, `finish`.
- **`effects`** — what came back, stored by content address, each carrying its
  `EffectKind` (what re-performing it would do to the world) and its `Provenance`
  (how grounded it is).
- **`state`** — a reference to the situation after the step, plus a **`grip`** saying
  what that reference is worth (§5).
- **`provenance`** — the join of the step's own grounding with its parent's and its
  effects'.
- **`intervention`** — the deliberate change, if this step is where a branch diverged.

And what it does not record, ever:

- the program's memory, stack or heap,
- the world,
- anything not declared at the boundary. Untracked inputs — an unmediated clock, an
  unpatched socket, a subprocess — are *invisible*, and their absence shows up as a
  divergence during reconstruction rather than as a silent wrong answer.

A recording is therefore **evidence**, not a world. Every guarantee below is a
statement about the evidence.

### The step is the only primitive

A trajectory is a chain of steps. A branch is a step whose parent belongs to another
trajectory. Prefix sharing, immutability of history and copy-on-write are consequences
of content addressing, not features layered on top: two runs that did the same thing
produce byte-identical step objects and therefore the same addresses, and a step that
exists cannot be edited because editing it changes its name.

## 4. The environment contract

An environment participates in Paranoid Android by satisfying two obligations. Only the
first is mandatory.

### 4.1 The boundary (mandatory, and it is data, not code)

The program **must** route every interaction that crosses into the environment through
the mediation protocol — `call` / `decide` / `result` / `error` / `finish`, newline
JSON over a Unix socket (see [technical-proposal.md](technical-proposal.md) §protocol).
For each `call` it declares:

- a **target**: a stable name for *what kind of interaction this is* (`flights.search`,
  `browser.click`, `reactor.act`). Position plus target plus arguments is the identity
  of the interaction; reconstruction matches on it.
- an **effect kind**, which is a claim about **reversibility under reconstruction**:

  | kind | claim | consequence |
  |---|---|---|
  | `read` | re-performing it changes nothing | freely re-performed |
  | `write` | it mutates a world we can put back — our sandbox, or an environment that re-driving rebuilds | not re-performed during reconstruction; the recorded state is restored instead |
  | `irreversible` | it leaves a mark we cannot take back — a payment, an email, an actuator, a production write | never performed outside an original recording. Denied by default; explorable only with a stated-simulated value |

  `write` is the interesting one, and it is routinely mislabelled. It does **not** mean
  "touches the disk". It means *we have a way to put this back*. A browser navigation
  is a `write` because re-driving the recorded actions rebuilds the page. A robot's
  actuator command is `irreversible` because nothing rebuilds the world.

An environment whose interactions are not declared is not participating. There is no
discovery mechanism and there will not be one: a boundary you inferred is a boundary
you will be wrong about, quietly.

### 4.2 The world (optional, and it is code)

Some environments carry state *between* steps that the recorded effects do not
capture. A browser page is the canonical case: step 3 navigates, step 7 reads — and
what step 7 reads depends on state that lives in the browser, not in the trajectory.

For those, and **only** those, the environment supplies an implementation of:

```rust
pub trait Environment {
    /// Name, and the best grip this environment can ever offer.
    fn manifest(&self) -> Manifest;

    /// Address the world as it is now. `State.grip` says what that address is worth.
    fn observe(&mut self, store: &Store) -> Result<State>;

    /// Put the world back to `state`. Returns the grip actually achieved:
    /// `Captured` if it is genuinely back, `Witnessed` if we can only tell whether it
    /// is, `Opaque` if we cannot even do that.
    fn restore(&mut self, state: &State, store: &Store) -> Result<Grip>;
}
```

Three methods. That is the whole interface, and two of the three are allowed to be
honest about failing. `restore` returning a `Grip` rather than a `Result<()>` is the
load-bearing detail: an environment that cannot put its world back says so in the same
vocabulary everything else uses, and the caller's behaviour changes accordingly — it
stops *asserting* the recorded state and starts *checking* it.

The engine ships three implementations, and there are only three because that is how
many distinct answers there are: `Workspace` (the directory it owns, `captured`),
`Reported` (a world only the program can see, `witnessed` or `opaque`), and `Situation`
(the two together, joined by §5.1). A Python program with no world uses the first and
implements nothing.

In-process environments do not have to be Rust. A world can be declared over the wire
instead — `{"op":"observe","of":"browser","state":{…},"restorable":false}` — which is
how the browser adapter does it and how anything not written in Rust will. The trait
and the protocol message are the same contract from two sides.

**When to declare a world.** Only when state persists inside the environment across
steps *and* is not carried by the mediated effects. Test it with one question: *if the
program branched here, what would the counterfactual inherit that the recording does
not contain?*

- A model provider: **no world.** Every call is independent and its answer is recorded.
- A REST API you read and never mutate: **no world.**
- A browser: **a world.** The page persists and is read later.
- An RL environment or simulator: **a world.** `step()` depends on accumulated state.
- A robot or a laboratory: **a world**, and a very weak grip on it (§5).
- The sandboxed workspace: **a world**, and the strongest possible grip. It is
  supplied by the engine, always present, and needs no adapter.

Over-declaring is not harmless. A world with a weak grip weakens the grip on the whole
situation (§5.1), which weakens the evidence a reconstruction can offer. Declare what
must be true, not everything that is true.

## 5. Grip: what a state reference is worth

The single most important thing this release adds. `state` is not a snapshot; it is an
address, and `grip` says what holding that address entitles you to do.

| grip | we hold | can we detect the world differs? | can we put it back? |
|---|---|---|---|
| `captured` | the bytes | yes | **yes** |
| `witnessed` | a fingerprint | yes | no |
| `opaque` | nothing | no | no |

- **`captured`** — the store holds the content and `restore` reproduces it exactly.
  The sandboxed workspace. This is the only grip that supports restoration, and it is
  the default, so every existing recording keeps its meaning unchanged.
- **`witnessed`** — the recording holds a fingerprint the environment reported: a page
  URL and structure hash, an instrument reading, an observation vector. A
  reconstruction that lands somewhere else is *detected*. It cannot be *repaired*.
- **`opaque`** — nothing was captured. Reconstruction by re-execution is still
  possible; it is simply **unverifiable**, and the system says so rather than implying
  a check it did not perform. A robot arm's position between commands is opaque. So is
  a world an adapter declared and never observed.

### 5.1 Grip joins like provenance

A situation is usually made of parts — the workspace we own plus the page we can only
look at. The grip on the whole is **the weakest grip of any part**:

```
captured ⊔ witnessed = witnessed
witnessed ⊔ opaque   = opaque
```

Same law as `Provenance::join`, and for the same reason: a claim about a whole cannot
be stronger than the weakest claim it rests on. This is why composition needs no
framework — an environment that owns a page and delegates the files to the workspace
just joins the two.

### 5.2 Where an observation is stored

A reported observation is stored as a blob and folded into the same tree as the files,
at `.world/<name>.json`. Three consequences, all wanted:

- every existing tool keeps working — `checkout`, `diff`, `export` and the state
  comparison need no special case,
- the fingerprint is **inspectable**: `noidroid show run-1@3` prints it,
- the state comparison that verifies a reconstruction covers the reported world for
  free, because it is part of the hashed tree.

`.world/` is never snapshotted from the filesystem and never materialised onto it. It
is evidence about the world, not the world, and it must not be able to become an input.

## 6. What a checkpoint guarantees

A checkpoint is not a saved world. It is the claim:

> **There is a defined procedure that gets back here, and here is what will be checked
> when it does.**

`noidroid show <trajectory>@<k>` answers three independent questions about step *k*:

### 6.1 Reach — can I get back here?

Computed over the prefix `0..k` (exclusive: step *k* itself is the one you are about to
intervene on).

| reach | meaning |
|---|---|
| `rebuild` | re-execute `0..k` with every mediated input served from the recording. Nothing in the prefix has to be re-performed or put back. |
| `rebuild+restore` | as above, and at each step whose `write`/`irreversible` effect we refuse to re-perform, the recorded state is restored as far as the environment can restore it. |
| `unreachable` | the prefix performed an **irreversible** effect at step *j* in a world whose grip at *j* is not `captured`. |

Only `irreversible` blocks, and only outside a `captured` world. The reasoning is worth
stating because it is easy to get backwards:

- A `write` never blocks. That is what `write` *means* — either we hold the bytes and
  restore them, or the environment rebuilds itself by re-driving the recorded actions.
- An `irreversible` effect in a `captured` world does not block either. We do not have
  to un-send it; we have to *not send it again*, and we do not, because the value is
  served from the recording and the workspace it left behind is restored.
- An `irreversible` effect in a `witnessed` or `opaque` world does block, because the
  only route back through it is to re-drive the actions that produced it — which sends
  it again. There is no third option, so the answer is `unreachable`.

That last case is the honest answer for a branch that would have to un-send an email,
un-submit a form, or un-fire an emergency dump. Branching from such a checkpoint is
**refused before the program is spawned**, naming the step and the target. It is not a
warning and it is not best-effort: discovering it halfway through is the version that
performs the irreversible effect a second time in order to find out it should not
have.

### 6.2 Evidence — will I know if I got it wrong?

The join of the grips over `0..=k`.

| evidence | on reconstruction |
|---|---|
| `captured` | the re-derived step addresses equal the recorded ones, or we say exactly where they stopped matching. Byte-level proof. |
| `witnessed` | fingerprints are compared. A divergence is reported; the world cannot be corrected. |
| `none` | nothing is compared. The reconstruction may be perfect and we cannot say so. |

`none` is not an error. It is the truthful description of a robot, and printing it is
worth more than any number we could invent instead.

### 6.3 Grounding — is what I get back to a claim about reality?

Step *k*'s own `provenance`: `real`, `live`, `simulated`, `unknown`. Already joined
along the chain, so a checkpoint downstream of an intervention reports `simulated`
however real the last few steps were.

Three questions, three answers, none of them collapsible into the others. A robot
checkpoint reads `rebuild / none / real` — reachable, unverifiable, and grounded in an
execution that actually happened. A checkpoint inside a branch reads
`rebuild / captured / simulated` — reachable, provable, and counterfactual. Both
statements are useful; neither is a percentage.

## 7. Reconstruction

Reconstruction has two halves, and conflating them is the mistake this project exists
to avoid.

| what | how it comes back | if it cannot |
|---|---|---|
| the program's internal state | **only** by re-executing `0..k` with recorded inputs | there is no fallback. This is why the boundary must be complete. |
| the world's state | `captured` → restored from the store; `witnessed`/`opaque` → **re-driven by the adapter** (§7.1) | if re-driving would re-perform an irreversible effect, the checkpoint is `unreachable` (§6.1) |

We never snapshot memory. Not as an optimisation — a memory image is not portable, not
comparable, not inspectable and not verifiable, and the moment a trajectory contains
one, "the original is immutable" stops being checkable.

**The check.** A reconstruction re-derives each step from scratch rather than copying
it. If the re-derived object addresses equal the recorded ones, the reconstruction is
faithful *with respect to everything captured* — hash equality is evidence, not
bookkeeping. If they differ, the engine reports the first index where they stopped
matching, the kind (`unexpected_call`, `key_mismatch`, `state_mismatch`, `truncated`)
and the field-by-field difference. It never repairs and never rounds.

### 7.1 Re-driving a world the engine does not own

This obligation belongs to the adapter and cannot be moved. During a reconstruction the
engine serves the program every value it asks for, so the program never touches its
world — which means that when a branch crosses the divergence point, the world is
sitting wherever it started, not where the recording left it.

An adapter for a `witnessed` or `opaque` world must therefore, before the first action
it really performs:

1. re-perform the recorded actions of the prefix against a fresh world,
2. compare the result with the fingerprint the recording holds,
3. and if it does not match, mark what follows `Ungrounded` rather than continue as
   though it did.

Both in-tree adapters do exactly this: `Browser._reconstruct` re-drives the recorded
actions into a fresh browser with the recorded network responses re-served, and
`Shift._catch_up` in the reference environment re-drives the recorded moves. It is
about fifteen lines in each case, and it is the difference between a counterfactual and
a plausible-looking fiction.

**The engine cannot check that you did it.** If an adapter skips the re-drive, the run
still completes, still hashes consistently and still reports a clean outcome — while
describing a physics that never happened. This is a real, structural limit: the only
source of truth about a world the engine cannot see is the program that can. What the
model does is make the obligation *explicit* (grip `witnessed` means "must be
re-driven") and give the adapter the fingerprint to check itself against. A recording
that fails to declare its world at all does not get even that.

**Observations obey the recorded-input oracle.** During reconstruction the program is
not touching the world, so it has nothing new to say about it, and the engine serves the
recorded observation in its place — testimony is an input like any other. A program that
*did* re-drive reports, its report wins, and the difference surfaces as a state
mismatch. That is the only case in which comparing fingerprints tells you anything, and
it is exactly the case worth comparing.

**Reconstruction is bounded by capture.** A prefix containing an effect with provenance
`unknown` re-derives correctly — the recording holds what the program saw — but every
step downstream inherits `unknown`. The reconstruction is exact and the *claim* is
weak, and those are different things, so they are recorded on different axes.

## 8. Replay

Replay is reconstruction of a whole trajectory, run for its own sake: *does this still
do what it did?*

The invariant, and it is structural rather than remembered: **during reconstruction the
engine never issues `execute`.** The program cannot perform a mediated interaction it
was not told to perform, so a replay cannot touch the world — not because the adapter
is careful but because the directive never arrives. A replay that reaches the network
anyway (an unmediated socket) is caught by the egress fence and reported, because a
reconstruction that touched the world is not a reconstruction.

The one deliberate exception is `--live <target>`: named targets are executed for real
while everything else is served from the recording. This is how a recording stays
useful when what changed is the prompt or the model. The engine does not pretend it is
still a reproduction — from the first live call onward the run is marked
counterfactual, its steps are `live`, and they are not compared against the recording.

`replayed` is therefore a **delivery**, not a provenance. Serving a recorded value back
does not make it less real; it makes it differently delivered. Keeping the two axes
apart is what lets a branch share its parent's prefix object-for-object — if replay
changed the content, a perfect reconstruction would produce different hashes than the
run it reproduced, which is absurd.

## 9. Intervention

An intervention is the single deliberate difference that makes a branch a branch:

| intervention | question |
|---|---|
| `replace_result` | what if the world had answered differently? |
| `replace_decision` | what if it had chosen differently? (requires a declared `decide`) |
| `fail` | what if this had failed? |

Rules:

1. **Exactly one, at exactly one step.** A branch with two differences explains
   nothing.
2. **An intervened step is `simulated`, always.** Nobody ran it. Provenance never
   improves, so everything downstream is `simulated` or worse, permanently.
3. **Past the divergence point the program really executes** — `read` and `write`
   effects are performed for real and recorded as `live`: they happened, in a
   counterfactual world. `irreversible` effects are **denied** unless the operator
   supplies a stated-simulated value with `--simulate target=<json>`, which is recorded
   as `simulated` and poisons everything after it.
4. **A branch never writes back.** It gets its own workspace and its own trajectory.

## 10. Branching

```
run-1   S0 → S1 → S2 → S3 → FAILURE
                   │
                   └── branch at @2
                       ├── run-1~a  A → S4 → FAILURE
                       └── run-1~b  B → S5 → SUCCESS
```

The invariants, each of which has a test:

- **The parent is immutable.** Branching writes no object that the parent references
  and never rewrites the parent's ref. Enforced by construction: objects are named by
  their content, so a modified step is a different step.
- **The branch shares historical identity with its parent.** For every `j < k` the
  branch's step *j* has the *same address* as the parent's step *j*. Not a copy — the
  same object, re-derived and found to be identical. This is what makes prefix sharing
  free and what makes "same history" checkable rather than asserted.
- **Divergence occurs only after the branch point.** Step *k* is the first step whose
  address differs, and it is the step carrying the intervention.
- **Provenance is preserved and joined.** The branch's prefix is `real` — it is the
  parent's evidence. From *k* onward nothing may claim to be real.
- **A refused branch leaves nothing behind.** If the prefix does not reconstruct, no
  trajectory is written: a trajectory on disk claiming an ancestry it does not have is
  worse than no trajectory.
- **A branch is an ordinary trajectory.** It can be replayed, branched again,
  exported, and compared with its parent.

The branch's `forked_from` records `(trajectory, step, step_hash)`. The step hash is
what makes the claim falsifiable: it names the exact object the branch says it grew
from.

## 11. Unknown

`unknown` is a result, not an error path.

It appears when:

- an adapter could not obtain information at all (`Unavailable`) — the effect's outcome
  is `unavailable` and its provenance is `unknown`,
- an adapter produced a real value against an environment it could not put back into
  the recorded state (`Ungrounded`) — the value is usable, its provenance is not,
- an irreversible effect was **denied** — no value exists, and the program is told so,
- a reported world was never observed — grip `opaque`, evidence `none`.

In every case the join makes the rest of the trajectory `unknown` too. There is no
mechanism to repair it, on purpose. The alternative — filling the hole with a plausible
value — produces a trajectory that looks real, which is the one failure this project
cannot survive.

## 12. Conformance

What an environment must do, in order:

1. **Route its interactions through the protocol**, with a stable target and an honest
   `EffectKind`. *Mandatory.* An environment that stops here gets recording, replay,
   branching, diffing and bisect, with `captured` grip on the workspace.
2. **Declare a world**, if and only if state persists inside the environment across
   steps and is not carried by the effects (§4.2). *Optional.*
3. **Observe it**, reporting whatever fingerprint must be true for a reconstruction to
   count. *Optional; without it the world is `opaque` and evidence is `none`.*
4. **Restore it**, if it genuinely can. *Optional; almost nothing can.*

Against the six environments:

| environment | boundary | world | best grip | restore | notes |
|---|---|---|---|---|---|
| Python application | function calls | workspace | `captured` | yes | the default. Nothing to implement. |
| Browser agent | actions + network | the page | `witnessed` | no | reconstructed by re-driving; verified by fingerprint |
| RL environment | `step`/`reset` | simulator state | `witnessed`, `captured` with save/load | sometimes | a seeded env with `save_state` is `captured` |
| Robot | sensor/actuator | physical | `opaque` | no | reachable, unverifiable; actuation is `irreversible` |
| Autonomous lab | instruments/protocols | physical + consumables | `opaque` | no | most checkpoints are `unreachable`: reagents do not come back |
| Production service | outbound calls | other people's state | `opaque` | no | branch with `--simulate`; irreversible writes denied |

Note what the table does **not** say: that these are the same. A laboratory
checkpoint is usually unreachable and a Python one usually is not. The contract's job
is to make that difference *explicit and computable*, not to hide it behind a uniform
interface that lies about three of the six rows.

## 13. Format and compatibility

`STEP_VERSION` stays at **1**. `grip` is `captured` by default and is skipped when
serialising, and `Trajectory.worlds` is omitted when empty, so:

- a step recorded before this release deserialises as `captured`, which is exactly what
  it was,
- a workspace-only step recorded after this release serialises to the same bytes and
  the same address as before,
- only a step with a declared non-restorable world carries the field at all.

Existing trajectories replay, branch, export and import unchanged. No migration.

---

## Appendix: the laws

1. A trajectory is evidence, not a world.
2. The boundary is declared, never inferred.
3. A checkpoint is a claim about reachability plus evidence, not a snapshot.
4. Grip and provenance only ever get weaker downstream.
5. What cannot be grounded is `unknown`, and `unknown` is never repaired.
6. The past is immutable; a branch is a new step whose parent is somebody else's.
