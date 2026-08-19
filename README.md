<div align="center">

# Paranoid Android

**Record an execution. Return to a point inside it. Explore what could have happened instead.**

`noidroid` — the command you type

</div>

---

An autonomous system runs, does something wrong, and ends. What survives is a pile of
logs: evidence that something happened, with no way to stand inside the moment it
happened and try again.

Paranoid Android records an execution as an immutable, content-addressed **trajectory**,
can return to any checkpoint inside it, and can run a **branch** from there where one
thing is different. The original is never modified — a branch is a new experiment
that shares its parent's history object-for-object.

The project is **Paranoid Android**. Its command-line interface, and the crates and
packages that implement it, are `noidroid` — the contraction you actually type.

<details>
<summary><b>「 ゴ ゴ ゴ ゴ 」</b> — on the name</summary>

Araki names Stands after the music he likes: Killer Queen, Echoes, Crazy Diamond,
Highway Star, and Radiohead's own Creep. **PARANOID ANDROID** is built to that rule.

It is a Stand whose ability is that an execution it has witnessed can be returned to
and continued differently, and whose Destructive Power is rated **E**, because it can
never change what happened. `noidroid stand` prints the parameters. They are graded
honestly, so the stat block doubles as an accurate summary of what this thing can and
cannot do.

Nothing in the workflow goes through any of it. If none of the above meant anything
to you, the tool is unaffected.

</details>

```
        record              checkpoint                branch
  ┌──────────────┐      ┌──────────────┐      ┌──────────────────┐
  │  execution   │ ───▶ │  trajectory  │ ───▶ │   alternative    │
  │  (happened)  │      │  (immutable) │      │  (counterfactual)│
  └──────────────┘      └──────────────┘      └──────────────────┘
```

**Status: working prototype.** It does what this page says it does — for
instrumented Python programs and for real browser sessions — and it is explicit
about where its knowledge stops. See [Limitations](#limitations).

---

## The example, end to end

An agent books the cheapest flight under €800. The cheapest flight has no seats. It
gives up.

```console
$ noidroid run -- python3 examples/flight_agent/agent.py
│ found 3 flights under 800
│ chose FL-101
│ FL-101 has no seats left; giving up

recorded run-1
    0 ● genesis                                     real      executed   2219c2d195
    1 ● call flights.search({"max_price":800})      real      executed   4ffc976f16
    2 ● decide pick_flight = "FL-101"               real      executed   733b08d1b9
    3 ● call flights.seatmap({"flight":"FL-101"})   real      executed   95a6d43d5b
    4 ✘ finish failure                              real      executed   c2781fcaa9
```

Step 2 is where it chose. Stand there and look:

```console
$ noidroid show run-1@2
CHECKPOINT run-1@2
  action       decide pick_flight = "FL-101"
  options      ["FL-101","FL-203","FL-311"]
  provenance   real
  state        1 file(s) root 367c26441d
               · notes/candidates.json

  EXPLORE FROM HERE
    noidroid branch run-1@2 --decide pick_flight=FL-203
    → what if it had chosen differently?
```

Take the other path:

```console
$ noidroid branch run-1@2 --decide pick_flight=FL-203 \
      --simulate 'payments.charge={"status":"charged"}'

branched alt-1 from run-1@2
  intervention replace-decision pick_flight = "FL-203"
  prefix       2 step(s) shared with run-1 — identical objects, stored once

    0 ● genesis                                     real      replayed   2219c2d195
    1 ● call flights.search({"max_price":800})      real      replayed   4ffc976f16
    2 ◆ decide pick_flight = "FL-203"               simulated intervened 3a39a969ff
    3 ● call flights.seatmap({"flight":"FL-203"})   simulated executed   55e9522ae0
    4 ● call flights.book({"flight":"FL-203",…})    simulated executed   45b45b70ce
    5 ● call payments.charge({"amount":680,…})      simulated intervened 3e75ce19c9
    6 ✔ finish success                              simulated executed   fecf1e4b34

  values by provenance   1 real, 2 live, 2 simulated
```

Two things to notice, because they are the whole point:

- **Steps 0 and 1 have the same addresses in both trajectories.** The prefix is not
  copied, it *is* the parent's prefix. `run-1` cannot be altered by anything that
  happens in a branch.
- **Nothing claims to be real that isn't.** The branch reached `success`, and it says
  plainly that the success rests on two simulated values. Without `--simulate`,
  `payments.charge` is refused outright — Paranoid Android will not spend money on a
  counterfactual's behalf.

```console
$ noidroid tree
run-1 failure 5 steps
  ├─ @2 alt-fl203 success  replace-decision pick_flight = "FL-203"
  └─ @3 alt-seats success  replace-result {"flight":"FL-101","seats_left":2}

$ noidroid diff run-1 alt-fl203
  shared prefix    2 step(s) — the same objects, not copies
  diverged at      @2 replace-decision pick_flight = "FL-203"
  outcome          failure → success
  provenance       real → simulated
  workspace        + booking.json
```

---

## The same thing in a real browser

The adapter in `noidroid.browser` drives Chromium through Playwright. It applies the
core's principle one level down: a browser's DOM and JavaScript heap cannot be
snapshotted, so they are **re-derived** — recorded actions are re-driven into a fresh
browser while every HTTP response is served from the recording.

Which means a branch can reconstruct a browser session with the website switched off:

```console
$ kill %1                                    # take the site away
$ noidroid branch web-1@3 --decide pick_flight=FL-203

│ [noidroid.browser] reconstructed 2 browser action(s), 2 recorded response(s)
│                    re-served; page state verified
│ stopped: browser.goto needed http://…/flight/FL-203, which this recording does
│          not contain; allow_network=True would fetch it live

    3 ◆ decide pick_flight = "FL-203"                simulated intervened
    4 ● call browser.goto({"url":"http://…/FL-203"}) unknown   executed
    5 ✘ finish blocked                               unknown   executed
```

The reconstruction is verified by comparing the page digest with the recording. Then
the branch reaches a page the recording never visited and stops, marking it `unknown`
instead of inventing what it would have said. Allowing the live network
(`NOIDROID_BROWSER_ALLOW_NETWORK=1`) lets the counterfactual go further and reach
`success` — labelled `2 real, 5 live, 1 simulated`, because that is what it rests on.

See [`examples/browser_agent/`](examples/browser_agent/README.md). The adapter needed
**no changes to `noidroid-core`**: it is an ordinary client of the same protocol.

---

## Replay is already a regression test

`noidroid replay` re-executes the *current* program against a recorded trajectory. If
the program changed, it says where its behaviour stopped matching, field by field, and
exits non-zero:

```console
$ noidroid replay run-1
  steps re-derived       2/4
  divergences:
    @2 key_mismatch —
      target: recorded "flights.seatmap", got "flights.availability"
      args.flight: recorded "FL-101", absent now
      args.id: not recorded, got "FL-101"
$ echo $?
1
```

Export the trajectory, commit it, and a production failure becomes a test that fails
the day somebody changes the decision that caused it.

The boundary is the same as everywhere else: this checks the program still behaves as
recorded given the inputs that were recorded. It is not a claim that the world stayed
the same, and it is not a substitute for running against a live model when what you
changed is the prompt or the model.

---

## Committing a failure

A recording is only a regression test if it can leave the machine it was made on.
`.noidroid/` is gitignored and full of sharded object files; `export` puts the
trajectory and everything it reaches into one file you can commit.

```console
$ noidroid export run-1
exported run-1 → run-1.noidroid.json (9 object(s), 4.6 KB)

$ noidroid import run-1.noidroid.json     # anywhere, empty store
imported run-1 (5 step(s), 9 object(s), every address re-checked)
```

The file is readable JSON, so a reviewer can see what the agent actually said in the
diff. Every address is re-hashed on the way in — a bundle arrives from somewhere
else, so its claim that an address holds given bytes is checked, not believed, and a
tampered one is refused.

What a bundle carries is the recording, not the program. Replaying it needs the
checkout it was recorded from, and says so plainly if it is missing.

---

## Which step actually caused it

A trace tells you what happened. It cannot tell you which step *caused* it, because
that is a question about a world that did not occur — and judging it from the
transcript is close to guessing: the published baseline for attributing an agent
failure to a step is around 14% accurate.

A branchable trajectory can settle it by experiment.

```console
$ noidroid bisect run-1
BISECT run-1 (ended failure)
  probing 1 alternative(s) across 1 decision(s)

  @2 tool_choice_1 = "lookup_charges"   success  ← flips it

  earliest flip: run-1@2, choosing "lookup_charges" for tool_choice_1
    noidroid diff run-1 run-1~2~lookup_charges
  everything after this step is downstream of a choice already made
```

Every recorded decision is re-run from its own checkpoint with a different choice, and
the earliest one that changes the outcome is the one worth looking at. In this example
the decision was the model's choice of tool, which the agent never declared — the model
adapter did, so this needed no instrumentation at all.

Each probe is a real trajectory you can open, diff and replay. Up to the divergence
point they cost nothing, because that part is served from the recording.

When nothing flips, it says so and exits non-zero, rather than picking a plausible
step and calling it the cause.

---

## Rewinding the files, not the conversation

The most-requested thing on coding-agent trackers is not undoing the chat — it is
undoing what the agent did to your files. Point a recording at your actual project:

```console
$ noidroid run --watch . -- python3 my_agent.py
    1 ● call edit.bump()      real  executed
    2 ● call edit.break()     real  executed
    3 ✘ finish failure        real  executed

$ noidroid restore run-1@1
restored /home/you/project to run-1@1
  ~ src/app.py

  the files that were here are saved; to put them back:
    noidroid checkout-tree 18bb309cbb… /home/you/project
```

The watched directory is read, never cleared. `.git`, `node_modules`, `target` and
friends are skipped — extend the list with a `.noidroidignore` — and, crucially, they
are skipped when *restoring* too: what was never recorded is never removed. Restoring
snapshots what is currently there first and prints its address, so the way back is one
command and nothing is destroyed.

Reconstructing is different from restoring. A replay or a branch re-executes your
program, which writes files, so those always run in their own copy — never in the
directory you are sitting in front of.

---

## The viewer

```bash
noidroid tui
```

Three panes and one verb. The timeline is coloured by provenance, so a trajectory's
honesty is legible before you have read a word of it; pressing `e` on a recorded
decision reconstructs the prefix, diverges, and comes back with a new trajectory
without leaving the screen.

```text
┌ explore from here ───────────────────────────────────────────────────────┐
│「PARANOID ANDROID」  ゴ ゴ ゴ ゴ                                          │
└──────────────────────────────────────────────────────────────────────────┘
┌ TRAJECTORIES ──┐┌ TIMELINE ─────────────────────┐┌ CHECKPOINT ───────────┐
│● run-1  failure││  0 ● genesis         replayed ││step        84c592e78c │
│└ alt-1  success││  1 ● call flights.se…replayed ││action      decide …   │
│                ││  2 ◆ decide pick_fli…intervene││provenance  simulated  │
│                ││  3 ● call flights.bo…executed ││                       │
│                ││  4 ✔ finish success  executed ││it could have chosen   │
│                ││                               ││   FL-203              │
└────────────────┘└───────────────────────────────┘└───────────────────────┘
┌──────────────────────────────────────────────────────────────────────────┐
│ e  explore from here    r  replay    tab  pane    ↑↓  move    q  quit     │
└──────────────────────────────────────────────────────────────────────────┘
```

The colours are the Stand's, and they carry meaning rather than mood:

| | |
|---|---|
| **phosphor green** | `real` — observed in the execution that actually happened |
| **chrome** | `live` — really executed, but in a counterfactual world |
| **violet** | `simulated` — supplied by an intervention; nobody ran it |
| **amber** | `unknown` — needed, and not available |
| **cyan** | `replayed` — served from the recording |
| **crimson** | divergence, refusal, an ability that did not work |

`--plain` drops the flourishes; `NO_COLOR` drops the colour. Neither removes anything
you need, because nothing is said in colour that is not also said in words.

---

## Quickstart

Requires Rust ≥ 1.74, Python ≥ 3.9, Linux or macOS.

```bash
git clone https://github.com/swalla02/noidroid && cd noidroid
cargo build --release
export PATH=$PWD/target/release:$PATH
export PYTHONPATH=$PWD/clients/python        # or: pip install -e clients/python

noidroid run -- python3 examples/flight_agent/agent.py
noidroid show run-1@2
noidroid branch run-1@2 --decide pick_flight=FL-203 --simulate 'payments.charge={"ok":true}'
noidroid diff run-1 alt-1
```

| Command | |
|---|---|
| `noidroid run -- <cmd>` | run a program and record its trajectory |
| `noidroid log [<traj>]` | list trajectories, or show one as a timeline |
| `noidroid show <traj>@<step>` | inspect a checkpoint and how to explore from it |
| `noidroid replay <traj>` | re-derive a trajectory and check it still hashes the same |
| `noidroid branch <traj>@<step>` | diverge: `--decide`, `--result` or `--fail` |
| `noidroid checkout <traj>@<step> <dir>` | write out the workspace as it was |
| `noidroid run --proxy -- <cmd>` | record an agent you did not write, in any language |
| `noidroid bisect <traj>` | find which decision, changed, would have flipped the outcome |
| `noidroid restore <traj>@<step>` | put the files back as they were, keeping a way out |
| `noidroid export` · `import` | move a trajectory between machines, as one committable file |
| `noidroid tree` · `diff` · `verify` | the branch graph, a comparison, a store integrity check |
| `noidroid tui` | browse trajectories and explore from a checkpoint, interactively |
| `noidroid stand` | 「 ゴ ゴ ゴ ゴ 」 |

---

## Integrating your own program

**Zero code to record and replay. Two lines to branch.**

```bash
pip install -e clients/python
noidroid run --auto -- python3 your_agent.py
```

`--auto` puts a `sitecustomize.py` on the child's `PYTHONPATH` — the same mechanism
`opentelemetry-instrument` and `ddtrace-run` use — and patches the OpenAI and
Anthropic base clients at `request`, below the retry loop, so a call that retried
three times is recorded once. Your program does not mention Paranoid Android at all:

```python
import anthropic                                   # no noidroid import

client = anthropic.Anthropic()
reply = client.messages.create(model=…, messages=…)
print(reply.content[0].text)                       # a real Message, replayed
```

Record it once, then replay it with the network unplugged: the recorded response is
served back and the SDK's own type is rebuilt, so `reply.content[0].text` still works.

### Agents you did not write

`--auto` patches SDKs inside a Python process, which is no help for a coding agent you
installed or a service in another language. Both common clients read their endpoint
from the environment, so there is a second way in:

```bash
noidroid run --proxy -- claude --print "fix the failing test"
```

The proxy stands between the agent and the provider and records what actually crosses
the wire. No patching, no TLS interception, no language requirement — and unlike a
trace exported from an observability tool, what gets recorded is the request itself,
so a replay matches on what was sent rather than on a lossy summary of it.

It captures the provider traffic and nothing else: files the agent writes, commands it
runs and other services it calls are invisible to it. Pair it with `--watch` to record
the files too. A streamed response is buffered rather than passed through — the content
is identical, the timing is not.

### What neither can do

What automatic capture **cannot** do is branch. No amount of patching can infer that a
value was a *choice among alternatives*, and that is what an intervention needs — so
`decide()` stays explicit, and it is the only thing that does:

```python
pick = nd.decide("route", options=candidates, choice=candidates[0])
```

It also does not capture async clients, streaming responses, non-SDK HTTP, the clock,
or randomness — and it will not quietly record around them. `--auto` prints what it
hooked *and* what it could not, and **refuses to record** when it finds a surface it
cannot cover:

```console
$ noidroid run --auto -- python3 agent.py
[noidroid.auto] hooked: anthropic._base_client.SyncAPIClient.request
[noidroid.auto] NOT hooked: anthropic._base_client.AsyncAPIClient.request — calls
                through it are not recorded
[noidroid.auto] refusing to record: the surfaces above are not captured, so this
                recording would be incomplete without saying so.
  Record it anyway with --allow-gaps if you know your program does not use them.
```

And during a replay the network is fenced: an outbound socket to anything but
loopback is refused, because a reconstruction is supposed to serve every input from
the recording and touch nothing. A blocked connection is not an inconvenience — it is
proof that something was never recorded, and the report says which address tried to
leave. It cannot see subprocesses, C extensions that bypass Python's socket module, or
connections opened before the client loaded.

`--allow-gaps` is the way past it, and the allowance is stored on the trajectory so
replaying it makes the same one. Refusing costs you a run; recording anyway costs the
trust in every run, because a trajectory that missed the model calls still looks real
and still claims to replay faithfully.

## Integrating it by hand

Three declarations. Everything else is your code, unchanged.

```python
import noidroid

nd = noidroid.connect()      # a pass-through when not running under `noidroid run`

data = nd.call("api.search", lambda: requests.get(url).json(), args={"url": url})
pick = nd.decide("choice", options=candidates, choice=candidates[0])
nd.call("payments.charge", lambda: charge(pick), effect=noidroid.IRREVERSIBLE)
nd.finish("success", {"picked": pick})
```

| Declaration | What it buys you |
|---|---|
| `call(target, run, effect=…)` | recorded, replayed instead of re-executed, and branchable |
| `decide(name, options, choice)` | the *choice* becomes branchable |
| `finish(status, result)` | the trajectory has an outcome worth comparing |

`run` is invoked **only** when the engine says so, and during replay it never does.
That is why a replay cannot touch the world: the guarantee lives in the protocol, not
in anyone's discipline.

The client is one dependency-free file speaking newline-delimited JSON over a Unix
socket. That protocol, not the Python package, is the integration contract — a client
for another language is an afternoon's work.

For agents, `noidroid.llm.Model` wraps the model call — the one input an agent cannot
make deterministic:

```python
from noidroid.llm import Model

model = Model(noidroid.connect())
response = model.complete(
    lambda: client.messages.create(model=name, max_tokens=1024, messages=messages),
    request={"model": name, "messages": messages, "temperature": 0},
    tools=list(REGISTRY),
)
```

Two things follow without any further instrumentation. **Replay costs nothing** —
every recorded response is served back, so you can iterate on your agent's code
against a real conversation, deterministically, for free. And **the model's tool
choice becomes a branchable decision**, so "what if it had reached for the other tool"
is `noidroid branch run-1@2 --decide tool_choice_1=lookup_charges` rather than a
prompt-engineering session. See [`examples/llm_agent/`](examples/llm_agent/README.md).

For browsers, `noidroid.browser.Browser` wraps a Playwright page and does the
mediating for you:

```python
from noidroid.browser import Browser

browser = Browser(noidroid.connect())
browser.goto(url, wait_for="#results")
rows = browser.scrape("tr.flight", ["data-id", "data-price"])["data"]
pick = browser.decide("pick_flight", options=[r["data-id"] for r in rows], choice=rows[0]["data-id"])
```

---

## How it works

**A checkpoint is not a snapshot of memory.** Returning to step *k* means re-executing
steps 0..*k* with every mediated input served from the recording, and letting the
program rebuild its own internal state. Restoring an arbitrary process image portably
is not something anyone can do honestly; re-execution is.

**Reconstruction is verified, not asserted.** Steps are content-addressed, so a
faithful replay re-derives *the same objects*. `noidroid replay` reports
`5/5 identical objects` or tells you exactly which step stopped matching and why
(`key_mismatch`, `state_mismatch`, `unexpected_call`, `truncated`).

**Branching is the data model, not a feature.** A step is
`(parent, action, effects, state_root, provenance)`, addressed by its hash. A branch
is a step whose parent belongs to another trajectory. Immutable history, shared
prefixes and copy-on-write all fall out of that; identical files and tool responses
are stored once.

**Two kinds of honesty are tracked separately.** *Provenance* is a property of
content and is part of the hash — `real` ⊑ `live` ⊑ `simulated` ⊑ `unknown`, joined
along the chain so it can never improve downstream. *Delivery* is how this run got a
value — `executed`, `replayed`, `intervened`, `denied` — and is deliberately not
hashed, so a faithful replay produces the same objects as the run it reproduces.

**Irreversible effects fail safe.** Declaring an effect `irreversible` means it is
performed only during an original recording. Every replay and every branch refuses it
unless you explicitly supply a stated-simulated value, which then poisons the
provenance of everything downstream.

Design reasoning, and where this disagrees with the manifesto, is in
[`docs/technical-proposal.md`](docs/technical-proposal.md).

---

## Limitations

Stated plainly, because a system whose whole point is honesty about reconstruction
cannot be vague about its own boundaries.

- **Not zero-code.** Your program must route its side effects through the client.
  Capturing enough from *outside* an uninstrumented process to reconstruct it is not
  portably possible; pretending otherwise would produce a system that demos well and
  lies.
- **Sequential programs only.** Threads, async races and concurrent interleavings are
  out of scope. A non-deterministic program will be reported as divergent, not
  silently mis-replayed.
- **Only the sandboxed workspace is captured.** Unmediated writes *inside* it are
  detected by hash. Writes outside it — networks, databases, other directories — are
  neither captured nor detected.
- **The ambient environment is not captured.** Environment variables, installed
  packages and the program's own source are assumed unchanged; a replay from a
  different directory or a modified script will diverge (loudly).
- **A branch is not a prediction.** Past the divergence point, `live` calls query a
  world that has moved on. Paranoid Android tells you what happens now from that state — not
  what would have happened then.
- **Browser reconstruction is bounded by the recorded page set.** A branch that
  navigates somewhere the recording never went needs the live network, which is
  refused by default and labelled `live` when allowed. Reaching the site again is not
  the same as the site being unchanged.
- **A `--result` intervention on a browser observation desynchronises belief from
  page.** The agent is told something the page does not say; everything after is
  `simulated`.
- **A page that cannot be reproduced is reported, not refused.** Page digests are
  exact, and real pages carry clocks and session tokens, so a branch whose
  reconstruction did not match carries on with everything after it marked `unknown`.
  `Browser(strict=True)` turns that into a refusal instead.
- **A replay reproduces that a call failed and with what message, not the original
  exception type.** A program that branches on exception class rather than on the
  failure itself will be reported as divergent.
- **No scale work.** No packing, no garbage collection, no remote store, no large
  artifact handling. Unix sockets only, so Linux and macOS but not Windows.

---

## Roadmap

1. **An HTTP/tool adapter**, so common boundaries need no per-call instrumentation.
   (The browser adapter is built; a plain HTTP one is the next-largest boundary.)
2. **Detecting unmediated effects beyond the workspace** — closing the honesty gap
   the third limitation names.
3. **A snapshot fast-path** (container or process image) behind the same checkpoint
   interface, for prefixes too expensive to re-execute.
4. **Structured trajectory comparison**, then guided multi-branch exploration.
5. **Dataset export** — declared decision points already carry
   `(state, action, alternatives, outcome)`.

Deliberately unbuilt for now: a dashboard, distributed storage, an agent framework,
and anything resembling a universal simulator.

---

## Development

```bash
cargo test --all                # 33 tests: unit, end-to-end, and real-browser
cargo clippy --all-targets

# the browser tests need Chromium; they print SKIP and pass without it
pip install playwright && playwright install chromium
```

The end-to-end tests drive a real Python child process through the real protocol,
because the claims worth testing — *a replay cannot touch the world*, *a branch cannot
mutate its parent* — are claims about what happens between processes.

[docs/direction.md](docs/direction.md) says where this is going and which decisions
are settled. See [CONTRIBUTING.md](CONTRIBUTING.md) for the branch and release workflow, and for the
rules about `STEP_VERSION` — the on-disk object format is a compatibility surface,
because an object's name *is* the hash of its bytes.

```
crates/noidroid-core/   objects, store, workspace trees, the record/replay/branch engine
crates/noidroid-cli/    the `noidroid` binary, the palette, the viewer, the Stand
clients/python/         the client (stdlib only) and the browser adapter
examples/flight_agent/  the example above
examples/browser_agent/ the same idea driving real Chromium
docs/                   technical proposal
manifesto.md            the product vision this is built toward
```

## License

Apache-2.0. See [LICENSE](LICENSE).
