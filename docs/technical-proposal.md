# Paranoid Android — Technical Proposal

**Status:** accepted for v0 · **Audience:** engineers building or evaluating the core
**Companion document:** [`manifesto.md`](../manifesto.md) (vision). This document does not restate it.

This proposal turns the vision into an engineering plan: what we build, what we refuse to build,
where the vision is technically naïve, and what the first vertical slice must prove.

The guiding question is narrow and testable:

> Can we take an execution that already happened, return to a meaningful point in that execution,
> and explore what could have happened differently?

Everything below is in service of making that question *answerable by a program*, not by a demo video.

---

## 1. Where the vision is technically naïve

Four assumptions must be corrected before any code is written. Getting these wrong produces a system
that demos well and lies.

### 1.1 "Zero-code integration" is not achievable, and pursuing it produces a worse system

The vision proposes `noidroid run python agent.py` with no changes to the application. Capturing
enough from the outside to *reconstruct state* is not possible portably:

| Outside-in technique | Why it fails for this problem |
|---|---|
| `ptrace` / syscall interception | Captures syscalls, not *semantics*. `write(4, ...)` does not tell you an agent chose a flight. Linux-only, heavy, hostile to debuggers. |
| `LD_PRELOAD` shims | Misses anything not going through libc (Go static binaries, `io_uring`, JIT'd runtimes). |
| CRIU / process checkpointing | Linux-only, privileged, breaks on open sockets, GPU contexts, and most containers. Restores *a process*, not *a world*. |
| VM / container snapshots | Restores a machine, cheap to say and expensive to do, and still cannot replace an external API response. |
| Full-system record-replay (`rr`, Hermit) | Excellent for determinism, x86-only, single-process, huge traces, and gives you no semantic trajectory — only instruction streams. |

The honest reframe: **we do not capture the process, we capture the boundary.** An execution becomes
reconstructible when the *non-deterministic inputs crossing the application/world boundary* are
recorded and can be re-served. This is how `rr`, Temporal, Antithesis and every durable-execution
engine actually work.

That requires the application to route its side effects through a client. So Paranoid Android is
**low-code, not zero-code**, and we say so in the README rather than implying otherwise. The
integration cost we are willing to impose is exactly one line per external interaction, and the
design rule from the vision still binds: every integration requirement must pay for itself in
replay fidelity.

The compensating benefit is large: boundary capture is language-agnostic, OS-portable, cheap,
inspectable, and produces a *semantic* trajectory instead of a syscall stream.

### 1.2 A checkpoint is not a state snapshot

The vision implies a checkpoint is a restorable image of the world. For a general process it is not:
restoring heap, stack, file descriptors, connections and interpreter state is exactly the CRIU
problem above.

**A checkpoint is the deterministic prefix of a trajectory.** Returning to step *k* means
re-executing steps 0..k under an oracle that serves every recorded input back to the application.
The application rebuilds its own internal state, because that is the one thing it is guaranteed to
be able to do.

This is cheaper than it sounds (prefixes are short, and no real I/O happens), and it is *verifiable*
in a way a memory image is not — see §6.

### 1.3 "Determinism where possible" is not a policy — divergence detection is

Any replay of a real program will eventually diverge: the code changed, a value was never recorded,
the agent is sampling from a model. A system that silently papers over this is worse than no system.

So divergence is a **first-class, loud outcome**, not an error path. Every replay either reproduces
the recorded object graph *bit for bit* or reports precisely where and how it stopped matching.
Replay is therefore a hash-equality check, not a vibe.

### 1.4 The four provenance labels conflate two different questions

`REAL / REPLAYED / SIMULATED / UNKNOWN` reads as one axis but is two, and collapsing
them breaks the system in a way that is easy to miss until you try to build it.

Ask what a label is a property *of*:

- `SIMULATED` is a property of **content** — nobody ran this; it was made up.
- `REPLAYED` is a property of **delivery** — we handed you a stored copy.

A faithfully replayed value is *the same real value*. If `REPLAYED` were part of the
content, a perfect replay of a recording would produce different content hashes than
the recording it reproduced — which is absurd, and would destroy prefix sharing,
because a branch's reconstructed prefix would no longer address its parent's objects.

So we split the axes:

**Provenance** — a property of content, part of the hashed object, ordered by
distance from recorded reality and joined along the chain:

```
Real  ⊑  Live  ⊑  Simulated  ⊑  Unknown
```

- **Real** — observed during the original live execution. A replayed value is still
  `Real`; that is the point of replaying it.
- **Live** — really executed, but during a branch: it happened, in a counterfactual
  world. The manifesto's four labels have no way to say this, and it is the most
  common kind of value in a branch. Calling it `Real` would imply it happened in the
  original execution; calling it `Simulated` would imply nobody ran it and would
  understate the risk that it touched something.
- **Simulated** — supplied by an intervention, stub or model.
- **Unknown** — needed and not available. The boundary of what we can say.

**Delivery** — a property of this run, deliberately *not* hashed:
`Executed | Replayed | Intervened | Denied`.

The payoff is concrete: provenance gives us the invariant in §7 (grounding never
improves downstream), delivery gives the operator the audit trail ("4 replayed, 1
intervened, 1 denied"), and content addressing keeps working.

## 2. The smallest useful core abstraction

Not "a trajectory" — that is the vision's abstraction and it is too large to be a *primitive*.

The primitive is:

> **An immutable, content-addressed `Step`: `(parent, action, effects, state_root, provenance)`.**

A trajectory is a chain of steps. A branch is a step whose parent is somebody else's step. The
trajectory graph is a Merkle DAG, and everything the vision asks for falls out of that one choice:

| Vision requirement | How the primitive provides it |
|---|---|
| History is immutable | Mutating a step changes its hash. Parents and siblings are unreachable from a mutation, by construction. |
| Branches share history | A branch's prefix *is* the parent's prefix — the same objects, by hash. Nothing is copied. |
| Copy-on-write | Divergence creates new objects only from the divergence point forward. |
| Content-addressed storage | Identical files, identical tool responses and identical steps are stored once. |
| Provenance is first-class | A field in the hashed object, not metadata attached later. |
| Trajectories are exportable | Objects are canonical JSON. `cat` and `jq` work. |

This is deliberately git's data model. Git solved "immutable history with cheap branching" and we
should not re-solve it; the novelty in Paranoid Android is not the store, it is *what is committed* (world
interactions and their provenance) and *what commit means* (a re-executable checkpoint).

**Consequence for hashing:** the hashed object must contain only semantic content. Wall-clock time,
pid, host and durations are recorded *outside* the hash, in per-run notes. If timing were hashed,
no replay could ever reproduce a hash and the entire verification story would collapse.

---

## 3. Execution and state model

An execution is a mediated dialogue between an application and the Paranoid Android engine, over a
line-delimited JSON protocol on a Unix socket. The engine drives; the application asks permission.

```
application                     engine                        world
     │                            │                             │
     │  call(target, args, kind)  │                             │
     ├───────────────────────────►│                             │
     │                            │ record  → "execute"         │
     │                            │ replay  → "use" + value     │
     │◄───────────────────────────┤ branch  → "use"/"execute"/"deny"
     │                            │                             │
     │  (only if "execute")       │                             │
     ├────────────────────────────┼────────────────────────────►│
     │  result(value)             │                             │
     ├───────────────────────────►│ commit Step(action, effect, state_root)
```

Two properties are structural rather than promised:

1. **A replay cannot touch the world.** The application only executes a side effect when the engine
   returns `execute`. In replay mode the engine never returns `execute`. Safety is enforced at the
   protocol level, not by a policy the application might forget to honor.
2. **Every step boundary is a real interaction.** Steps are implicit: one mediated interaction, one
   step. There is no separate "please checkpoint here" call to forget. Checkpoints are exactly the
   points where the execution met the world, which are exactly the interesting ones.

### State model

State is split by who owns it, because the two halves have very different reconstruction stories:

| State | Owner | How it is captured | How it is restored |
|---|---|---|---|
| Application-internal (heap, control flow, agent memory) | the app | **not captured** | rebuilt by deterministic re-execution |
| World-facing (workspace files, tool responses, clock, randomness) | Paranoid Android | recorded as effects + a Merkle tree of the workspace | re-served from the store |

We never claim to snapshot the first column. We snapshot the second, and we *verify* that the first
one reconstructed correctly by checking that the app produced the same actions and the same
workspace tree hash at every step.

### Declared decision points

Environment interventions ("what if the API had returned X") need no cooperation beyond `call`.
Action interventions ("what if the agent had chosen B") do: an application's choice is internal, so
it must be declared to be overridable.

```python
flight = nd.decide("pick_flight", options=[...], choice=my_ranking[0])
```

This is instrumentation and we charge it honestly: **you can branch environments for free; you can
branch decisions if you declare them.** The by-product is that declared decision points are exactly
the `(state, action, alternatives)` tuples that preference and RL datasets need later.

---

## 4. Trajectory representation

Four object types, canonical JSON (sorted keys, no floats in hashed positions), BLAKE3-256,
hex-addressed.

```jsonc
// step
{ "type":"step", "v":1,
  "parent":   "<step-hash>|null",
  "index":    3,
  "action":   {"kind":"call","target":"flights.seatmap","args":{...}},
  "effects":  [{"key":"3:read:flights.seatmap","value":"<blob-hash>","effect":"read","provenance":"real"}],
  "state_root":"<tree-hash>",
  "provenance":"real",
  "intervention": null }

// tree                                   // blob            // trajectory (a ref, not hashed content)
{ "type":"tree", "entries":[              raw bytes          { "name":"run-1", "head":"<step-hash>",
  {"path":"notes.md","blob":"<h>","mode":420}]}                 "forked_from":{"trajectory":"...","step":2},
                                                                "command":[...], "outcome":{...} }
```

`state_root` is a Merkle tree of the sandboxed workspace directory. It is the part of the world we
can actually snapshot and diff, and it is what makes checkpoint reconstruction *checkable*.

Timing, durations and each step's *delivery* live in `notes/<name>.json`, keyed by step index and
hash — outside the content, for the reasons in §2 and §1.4.

---

## 5. Checkpoint model

A checkpoint is a `(trajectory, step_index)` pair. It denotes:

- the recorded action and effect at that step,
- the workspace tree at that step (materializable: `noidroid checkout run-1@3 ./dir`),
- the provenance of everything used to get there,
- and, crucially, **a re-executable prefix**.

There is no "restore the process" operation, and we do not pretend there is. `noidroid show`
displays exactly what is known at that point and labels the rest `unknown`.

---

## 6. Replay and reconstruction strategy

Reconstruction is **deterministic re-execution under a recorded-input oracle**, verified by hashes.

```
1. materialize the genesis workspace tree into a fresh sandbox directory
2. re-launch the recorded command with NOIDROID_MODE=replay
3. serve every mediated call from the recording, in order, matched by key
4. after each step, re-snapshot the workspace and recompute the step hash
5. assert the recomputed step hash equals the recorded step hash
```

Step 5 is the entire value proposition. If the recomputed chain hashes to the recorded head, the
reconstruction is *provably* faithful with respect to everything we recorded. If it does not, we
report the first mismatching step and stop. There is no partial credit and no fidelity theater.

Divergence has exactly three causes, and we name them:

| Divergence | Meaning |
|---|---|
| `unexpected_call` | the app asked for something the recording does not contain (code changed, or non-determinism we do not mediate) |
| `key_mismatch` | the app made a *different* call than recorded at that position |
| `state_mismatch` | the workspace tree diverged although the calls matched (unmediated side effect) |
| `truncated` | the run stopped before reaching the end of the recording |

`state_mismatch` is the interesting one: it detects side effects that bypassed the client. It is our
built-in audit of our own coverage.

**Fidelity is reported as counts, never as a fabricated percentage bar:** interactions reproduced
*n/m*, state roots matched *n/m*, and a per-provenance census.

### The known limit

Re-execution costs O(prefix). Restoring a memory image would cost O(1). We do not have the second
option and we do not fake it. When prefixes become expensive, the fix is process/container
snapshotting as an *optimization behind the same interface* — the verification story (hash the
resulting state, compare) is unchanged. That is future work, listed as such.

---

## 7. Branching model

```
noidroid branch run-1@3 --decide pick_flight=FL-203
```

1. Re-execute steps 0..k-1 exactly as in replay, **asserting hash equality with the parent**. If the
   prefix cannot be reproduced, the branch is refused. You cannot branch from a checkpoint you
   cannot reach.
2. At step *k*, apply the intervention.
3. Past step *k*, the recording no longer applies. Each call is resolved by policy:

| Call kind | Policy past divergence | Provenance |
|---|---|---|
| `read` (repeatable, no external mutation) | execute for real in the sandbox | `Live` |
| `write` (mutates the sandboxed workspace) | execute for real in the sandbox | `Live` |
| `irreversible` (payments, email, production, physical actuation) | **denied by default** | `Unknown` unless `--simulate-effects` supplies a value → `Simulated` |

Interventions are typed and recorded *inside* the diverging step, so a branch carries the reason it
exists:

- `replace_result(k, value)` — environment / fact / tool-response branching
- `replace_decision(k, choice)` — action branching at a declared decision point
- `fail(k, error)` — adversarial injection

### The invariants worth testing

1. **Prefix sharing:** for `i < k`, `branch.step[i].hash == parent.step[i].hash`. Not "we copied it
   correctly" — literally the same object.
2. **Parent immutability:** after any number of branches, the parent's head hash and every object it
   reaches are byte-identical. Enforced by an append-only store: writing an existing hash is a no-op,
   writing a *different* body at an existing hash is a hard error.
3. **Provenance monotonicity:** `step.provenance = max(own, parent, effects...)`. Provenance never
   decreases along a chain. Once a trajectory diverges, no descendant may claim to be `Real`.

Invariant 3 is the machine-checkable version of "make uncertainty visible."

---

## 8. Persistence and storage

```
.noidroid/
  objects/<aa>/<hash>      append-only, content-addressed (steps, trees, blobs)
  trajectories/<name>.json refs + run metadata + outcome
  notes/<name>.json        timing/host/pid — deliberately outside the hash
  workspaces/<name>/       per-trajectory sandbox
  logs/<name>.{out,err}.log child stdout/stderr, kept out of the content
  tmp/                     scratch space for replay workspaces
```

Flat files, no database, no daemon, no index. Justification: at v0 the access pattern is
"walk a chain, read some blobs," the corpus is small, and a database would be an unfalsifiable bet.
The object store's interface (`put(bytes) -> hash`, `get(hash)`) is narrow enough that packfiles,
mmap, compression or a remote backend slot in later without touching the model. Dedup is real today
(identical blobs stored once); reflink/overlayfs materialization is an optimization we have *not*
done, and the README says so.

---

## 9. Environment adapter strategy

The core knows nothing about flights, browsers, robots or laboratories. It knows `call`, `decide`,
`result`, `finish`.

The adapter surface is **a wire protocol, not an SDK**. That is the anti-lock-in decision: the
Python client is ~200 lines of stdlib and speaks NDJSON over `AF_UNIX`. Reimplementing it in Node,
Go, C++ or inside a ROS node is an afternoon, and requires nothing from us — no bindings, no ABI, no
release coupling.

```
                 ┌──────────────────────────────┐
                 │  noidroid-core (Rust)        │  objects · store · engine · provenance
                 └──────────────┬───────────────┘
                                │  NDJSON / AF_UNIX  (the only integration contract)
        ┌───────────────┬───────┴────────┬─────────────────┐
    Python client    Node client     C++/ROS node     anything else
```

Where an environment *can* be captured from the outside (a browser via CDP, a Gym env via its step
function, a container via its filesystem), that becomes an adapter that speaks the same protocol on
the application's behalf.

### The browser adapter (built)

The browser is the environment where "reconstruct the state" is least fakeable: a DOM, a JavaScript
heap, cookies and live connections cannot be snapshotted and put back. So the adapter applies the
same principle as the core, one level down:

| Level | Not snapshotted | Reconstructed by | Oracle |
|---|---|---|---|
| application | heap, control flow | re-running the program | recorded mediated calls |
| browser | DOM, JS heap, cookies | re-driving recorded actions | recorded HTTP responses |

Two layers, both recorded: browser **actions** (`goto`, `click`, `fill`, `scrape`) become branchable
steps; HTTP **responses** become the oracle that makes re-driving deterministic. Crossing a
divergence point launches a fresh browser, re-drives the recorded prefix into it, and **verifies the
result** by comparing a page digest with the recording — the browser equivalent of the engine's hash
equality check.

Everything the adapter stores — the action log, recorded responses, screenshots — lives in the
workspace, so it is content-addressed, de-duplicated and shared between branches by the mechanisms
that already exist. The adapter has no storage of its own, and required **no change to
`noidroid-core`**: it is an ordinary client of the same protocol, which is the strongest available
evidence that the core really is environment-agnostic.

Its boundary is sharp and worth stating: a branch that navigates somewhere the recording never went
needs the live network. That is refused by default and reported as `unknown` — a counterfactual is
not licence to start browsing the real internet.

Still unbuilt: a plain HTTP adapter, Gym/RL, containers, robotics.

---

## 10. Technology decisions

### Rust for the core — confirmed, with reasons that survive scrutiny

The instruction was to evaluate rather than assume. Rust wins here, but not for the usual reasons:

1. **The core must not appear in the target's dependency graph.** We are recording other people's
   programs. A Python core inside a recorded Python process fights over interpreter version,
   packages, and import side effects — and perturbs the very thing being recorded. A single static
   binary the app talks to over a socket is the only design that keeps the recorder out of the way.
2. **Embeddability across the domains the vision names.** A robotics stack or an RL harness will
   eventually want the trajectory engine in-process. Rust gives a C ABI with no runtime, no GC, no
   GIL. Python cannot be embedded in a C++ real-time loop; Go's runtime and Java's VM are heavy
   guests.
3. **Immutability enforced by the type system.** Committed objects are owned, hashed values with no
   `&mut` path. Provenance is an exhaustive `enum` — adding a variant fails compilation at every
   site that must consider it. For a system whose core claim is "we never mutate history," this is
   worth real money.
4. **The store is the hot path.** Hashing, mmap and zero-copy reads over large trajectory corpora is
   precisely Rust's competence, and it removes the "we'll rewrite it later" tax.

Honest counterweight: Rust slows iteration, and most *adapter* work is glue that belongs in the host
language. Hence the split — Rust core, host-language clients, wire protocol in between. If the core
were only ever going to serve Python agents, Python would be the right call and this would be
over-engineering.

### Other decisions

| Decision | Reasoning |
|---|---|
| **BLAKE3** | Fast, modern, one crate, no hand-rolled crypto. |
| **Canonical JSON** for objects | Human-inspectable and greppable; trajectories are exportable with `cat`. A binary format is a later optimization, not a v0 requirement. |
| **No async, no tokio** | One child process, one socket, one connection, strictly request/response. Blocking I/O is correct here and removes an entire dependency tree. |
| **Child stdout/stderr → files** | Avoids pipe-deadlock and thread management for zero loss of capability. |
| **Unix sockets** | Trivial in every client language. Windows support is deferred, not designed out. |
| **4 direct dependencies** (`blake3`, `serde`, `serde_json`, + `clap` in the CLI only) | The core has three. |

---

## 11. Major technical risks

| # | Risk | Severity | Position at v0 |
|---|---|---|---|
| 1 | **Unmediated side effects.** The app does something behind our back; replay silently lies. | High | Detected as `state_mismatch` for the workspace. Network/DB effects outside the workspace are **not** detected. Named limitation. |
| 2 | **Prefix re-execution cost.** O(k) to reach checkpoint k. | Medium | Acceptable at v0 scale; the fix (snapshot fast-path) is compatible with the model. |
| 3 | **Non-deterministic applications** (threads, async races, model sampling). | High | Single-threaded, sequential apps only. Concurrency is *out of scope*, not "handled". Divergence detection turns this from corruption into a clear error. |
| 4 | **Branch validity.** Post-divergence `Live` calls query a world that has moved on. | High | Provenance labels it. We do not claim a branch is what *would* have happened — only what happens now, from that state, with those inputs. |
| 5 | **Irreversible effects in branches.** | Critical | Denied by default. Opt-in simulation, labeled `Simulated`, provenance poisons the trajectory downstream. |
| 6 | **Instrumentation burden.** Nobody wants to wrap their calls. | Existential (product) | Mitigated by making the wrap trivial and the payoff visible; long-term answer is per-environment adapters (browser, HTTP, Gym) that do the wrapping generically. |
| 7 | **Storage growth** on large artifacts. | Medium | Content addressing dedups; no packing/GC yet. |
| 8 | **Branch explosion.** | Low (today) | Data model is a DAG from day 1; guided search deliberately unbuilt. |

---

## 12. Recommended first vertical slice

One environment, end to end, provable:

```
noidroid run -- python3 examples/flight_agent/agent.py    record  → failure
noidroid log run-1                                        timeline
noidroid show run-1@2                                     checkpoint + how to explore from it
noidroid replay run-1                                     verified reconstruction (hash equality)
noidroid branch run-1@2 --decide pick_flight=FL-203       action intervention
noidroid branch run-1@3 --result '{"seats_left":2}'       environment intervention
noidroid checkout run-1@2 ./state                         materialise a checkpoint's workspace
noidroid tree · diff · verify                             the DAG, a comparison, store integrity
```

The example agent is a real Python program with a real (sandboxed, local) world: it searches
flights, checks a seat map, books, and charges — where "charge" is declared **irreversible** and is
therefore denied in every branch unless explicitly simulated. The original run fails because it
picks the cheapest flight and that flight is sold out.

### What the slice proves

- an execution can be recorded as an immutable, content-addressed trajectory
- a recorded execution can be reconstructed and *verified* by hash equality, not asserted
- a checkpoint can be returned to, inspected, and materialized
- a branch can change either a decision or a fact of the world and reach a different outcome
- branches share their parent's prefix as literally the same objects, and cannot mutate the parent
- every value carries provenance, and provenance never improves downstream
- irreversible effects cannot fire during replay or branching

### What it deliberately does not prove

- nothing about processes that are concurrent, distributed, or non-deterministic internally
- nothing about environments we did not mediate (real networks, databases, browsers, robots)
- nothing about scale — no packing, GC, remote store, or large-artifact handling
- nothing about capturing an *uninstrumented* application
- nothing about physical-world rewind, simulation transfer, or learned models

### Explicitly deferred

UI/dashboard · browser and HTTP adapters · CRIU/container snapshot fast-path · distributed storage ·
model-guided branch search · RL/preference dataset export · Windows support · multi-language clients
beyond Python · concurrency support.

---

## 13. What comes after the slice, in order

1. **HTTP/tool adapter** — remove per-call instrumentation for the most common boundary.
2. **`state_mismatch` coverage beyond the workspace** — make unmediated effects detectable, not just workspace-detectable.
3. **Snapshot fast-path** — container/process snapshot behind the same checkpoint interface.
4. **Comparison and search** — structured trajectory diff, then guided multi-branch exploration.
5. **Dataset export** — declared decision points already carry `(state, action, alternatives, outcome)`.

The order is deliberate: reduce integration cost, then close the honesty gap, then optimize, then
build the products the trajectory graph makes possible.

---

## 14. Implementation status

What follows is what exists and has been run, as opposed to what is planned above.

**Built and verified** (22 tests: 10 unit, 10 end-to-end through a real subprocess, 2 driving real Chromium):

| Claim | How it is checked |
|---|---|
| An execution records as an immutable, content-addressed trajectory | `a_recording_replays_to_the_same_objects` |
| Reconstruction is faithful, and provably so | replay re-derives every step to the same address; divergence otherwise |
| A replay cannot touch the world | `a_replay_never_touches_the_world` — a witness file outside the workspace is unchanged after replay |
| A branch shares its parent's prefix as the same objects | `a_branch_shares_its_parents_prefix_and_cannot_change_it` — step addresses compared, not contents |
| A branch cannot mutate its parent | same test — every object reachable from the parent is byte-identical before and after, and the parent's workspace is untouched |
| Provenance never improves downstream | `provenance_never_improves_downstream` |
| Irreversible effects are denied outside a recording | `an_irreversible_effect_is_denied_outside_a_recording` |
| A changed program is reported, not papered over | `a_changed_program_is_reported_rather_than_papered_over` (key mismatch) and `an_unmediated_state_change_is_detected` (state mismatch) |
| A branch is itself a trajectory | `a_branch_is_itself_a_trajectory_that_can_be_replayed` |
| A replay is bounded by its recording | `an_injected_failure_stops_the_run_and_stays_stopped_on_replay` — found a real bug: a replay that outlived its recording used to start executing calls for real |

**Deviations from the plan above**, all discovered while building:

1. **Provenance was split into two axes** (§1.4). Written after the first
   implementation attempt made replay unable to reproduce its own hashes.
2. **Replay never re-executes anything mediated**, including `write` effects to the
   sandboxed workspace. The alternative — re-running sandbox writes for stronger
   verification — makes safety depend on every effect being labelled correctly. The
   engine instead restores those workspace states from the recording and counts them
   as *restored*, never as *verified*. State verification therefore covers unmediated
   application writes, which is the part that actually evidences internal-state
   reconstruction.
3. **`RunSpec` carries explicit child environment**, because the ambient environment
   is not captured. This is a real limitation, not a convenience: a replay depends on
   the environment being unchanged, and says so when it is not.

### Second slice: the browser adapter

| Claim | How it is checked |
|---|---|
| A browser session reconstructs from recorded responses alone | `a_browser_session_reconstructs_from_recorded_responses_alone` — records against a live site, **shuts the site down**, then branches; the prefix is re-driven into a fresh browser and the page digest matches |
| Running out of recorded knowledge is `unknown`, not `live` | same test — the first unrecorded page ends the branch with `Provenance::Unknown` and outcome `blocked` |
| A browser branch can reach a different outcome | `a_browser_branch_can_reach_a_different_outcome` — the counterfactual books an available flight and succeeds, head provenance `simulated`, parent unchanged |

Two further findings from building it:

4. **`tree::materialize` was unlinking the workspace directory** and recreating it, while the
   recorded process was still running with that directory as its working directory. The child was
   left holding a deleted inode, and every relative path it touched afterwards vanished. The
   adapter surfaced it because a browser run restores state mid-flight far more often than the
   first example did. Now the directory's *contents* are pruned and the directory itself is kept
   (`materialize_keeps_the_directory_itself`, plus an end-to-end regression test). This was a
   correctness bug in the core, found by an adapter that touched it harder.
5. **A client needs a way to say "I could not obtain this."** Without one, an adapter that ran out
   of recorded knowledge had its failure recorded as a `live` value — a real thing that happened —
   when the truth was that nothing happened at all. The protocol now accepts `unknown` on an error,
   the only provenance claim a client may make, because it is the one that can only lose trust.

**Not built, and not pretended:** any adapter beyond the Python client and the browser, snapshot
fast-paths, concurrency support, detection of effects outside the workspace, storage
packing or GC, Windows support, and every product in §13 beyond the first slice.
