# Changelog

All notable changes to this project are documented here, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). This project uses
[semantic versioning](https://semver.org/); see [CONTRIBUTING.md](CONTRIBUTING.md) for
how the package version relates to `STEP_VERSION`, the on-disk object format.

## [Unreleased]

### Added

- **`noidroid doctor` — what a recording would and would not cover, before one is
  made.** Automatic capture fails open by construction: every patching mechanism can
  miss a surface, and a recording that missed one still looks real. `--auto` already
  refused when it found a hole, but only for the holes it knew to look for and only at
  the moment of recording, which is after the decision to trust it has been made. The
  doctor asks first, and it answers with five words that are not interchangeable: `ok`
  (we looked, it is covered), `absent` (we looked, there is nothing here to cover),
  `not captured` (we looked, it is not — with the issue that tracks it), `not
  determined` (we could not look, which is never a pass), and `blocked` (a recording
  made now would be refused or would miss something, and the command exits non-zero).
  Every answer is something the tool did rather than something it meant to do: the SDK
  request surfaces are enumerated out of `_base_client` *after* running the real
  installer, so a client class this build has never heard of appears unhooked rather
  than invisible to both; the egress fence is ticked only once it has actually refused
  a connection to a reserved address. `noidroid doctor -- python3 agent.py` also parses
  the program and names, with file and line, the clock and randomness reads (#30) and
  the subprocess launches (#31) it cannot capture — and because it does not follow
  imports, a file with nothing in it is reported as `not determined` rather than as a
  clean program. The async SDK surface (#33) and the hardcoded `AF_UNIX` (#32) are
  named as the known limitations they are instead of being left to look covered. There
  is no score, no percentage and no readiness grade. (#29)
- **The ways a world fails, named.** `noidroid branch --inject <kind>` writes the
  intervention for you: `timeout`, `server-error`, `rate-limited`, `unauthorized`,
  `malformed`, `empty`. Writing the payload by hand is the difference between a thing
  people do and a thing people mean to. The first four raise where a client would
  raise, and an agent with a `try` around the call survives them; `malformed` and
  `empty` raise nothing at all, which is the case worth branching. An agent that does
  not validate a tool result cannot tell a broken answer from a true one — the flight
  example reports seventeen characters of unterminated JSON as seventeen flights, out
  loud, before anything goes wrong. The presets themselves shipped inside 0.3.0
  unannounced and with no test; they now have both. (#35)
- **`noidroid cost` — what a trajectory bought, and what it did not.** Every recorded
  model call already carried the provider's own token counts, and nothing added them
  up, so the tool's most legible property was invisible: a branch whose model call
  sits in the shared prefix executes nothing and therefore buys nothing. Tokens are
  read back out of the recorded response and split by the step's delivery, which is
  the recorded fact about whether a provider was reached. Money is not: there is no
  built-in price list, `--price 'MODEL=IN/OUT'` is the only source of one, and without
  it the output says which model it could not price rather than printing a figure it
  made up. `$0.00` is the exception, because zero tokens cost nothing at every price
  there is. An imported bundle carries content but not per-run notes, so it cannot say
  how its calls were delivered — and says so instead of totalling to zero. (#36)

### Changed

- **A divergence caused by a clock or a random id now says so.** A timestamp or a UUID
  in a call argument makes every replay diverge, and the report named two long values
  and left the reader to notice that one of them was a clock — then guessed, wrongly,
  that an interaction had been inserted, because nothing in the recording matches a
  value that is new every run. The argument is now named at its full path, the source
  is read off the value (`unix nanoseconds`, `ISO-8601`, a UUID, a random token) and
  the remedy comes with the key already filled in: `volatile=["sent_at"]`, or route
  the value through `nd.call()`. The same value written into the watched workspace
  produces a `state_mismatch` that `volatile=` cannot reach at all, so that report now
  names the file, names the value inside it, and says plainly that `volatile=` is not
  the fix and why — the workspace is hashed whole. Detection only: the clock is not
  frozen and nothing is suppressed. A freeze covers most clocks and not all, which
  trades a loud mismatch for a quietly wrong value, and every claim here is worded as
  a reading of the evidence because that is all it is. (#30)
- **A divergence report now names an insertion, not just the fields that differ.** It
  could already say "this call is recorded at step N, it looks like interactions were
  removed" and had no symmetric sentence: add a call and the reader got a target and an
  argument diff and had to work out what moved. When what the run wants appears nowhere
  in the recording, the report says it looks like an interaction was added there — and
  says in the next breath that a rewritten interaction looks identical from one
  mismatch, because it does. Matching is still positional and divergence is still
  fatal; only the sentence changed. (#34)

### Fixed

- **The proxy test suite bound fixed ports, so a stale provider could be recorded
  instead of the one under test.** `Provider::start` polled `TcpStream::connect` and
  returned as soon as *something* answered on `8791`-`8794` — never checking that the
  answerer was the process it had just spawned. A provider left behind by a killed
  run, or a second worktree of this repository running its suite at the same time,
  meant the next test recorded against someone else's server. For
  `a_streamed_response_reaches_the_client_before_it_ends` that produced the wrong
  verdict rather than a loud failure: a stale non-streaming provider on the port made
  the arrival-spread assertion fail on a proxy fix that was fine, which is exactly
  what happened once during #30. Two worktrees running `cargo test --all` at once is
  now the ordinary case, not the exotic one, so every stand-in provider in
  `proxy_slice.rs` and `auto_slice.rs`, and the example site `browser_slice.rs`
  drives, now binds port 0 and reports back the port the OS actually gave it, the
  same remedy `unique_socket_path` has always used and #44 already applied to
  `Store::put`'s scratch name and `watch_slice`'s fixture directory. A new test,
  `a_provider_never_adopts_a_server_it_did_not_start`, starts two providers back to
  back and proves that killing the second one only takes down its own port. (#74)
- **A streamed call had no single JSON object for `cost` to read, so it vanished from
  the total instead of being reported unaccounted for.** #45 made streaming pass
  through as it arrives; a provider that streams reports usage split across events —
  Anthropic's input count on `message_start`, its output count on `message_delta`;
  OpenAI's whole `usage` block once, on the final chunk, only with
  `stream_options.include_usage`. None of that is the one `usage` object `cost`
  already knew how to read, so the call was silently skipped: not zero, not named,
  just gone, and the total it fed into looked complete. `cost` now reads a recorded
  event stream the same way it reads a plain response — recognising the events by the
  shape a model's own stream actually uses, not by the request's target name — and
  takes the last value reported for each field, which is correct for both providers'
  streaming conventions. A stream that never reports usage at all is still counted,
  named `unaccounted`, and kept out of every `$0.00` the tool would otherwise print:
  a call nothing could read is not a call that cost nothing. (#62)
- **A branch whose program died printed a timeline that just stopped.** `noidroid log`
  called it `aborted` and `noidroid branch` never did, so the operator had to infer the
  crash from a missing `finish` row — the inference this tool exists to remove, and the
  whole result when the intervention was a failure the program was supposed to survive.
  `branch` now states the outcome the way `log` does, and the child's stderr is printed
  alongside its stdout, so the traceback that says *why* is on the screen instead of in
  a log file nobody was told about. When the program said nothing on the way out, that
  is what it says; the reason is never guessed at. This was never specific to
  `--inject` — `--decide`, `--result` and `--fail` were all equally silent. (#58)
- **The proxy recorded a gzipped reply as a wall of replacement characters.** It
  forwarded the agent's `accept-encoding` upstream, so a real provider compressed;
  it then stripped `Content-Encoding` and passed the compressed bytes through, so the
  agent could not parse them — and `decode("utf-8", "replace")` wrote them into the
  trajectory with every unreadable byte swapped for U+FFFD. The recording was lossy,
  irreversible, and said nothing about it, which is the one failure this project
  cannot survive: not a crash, a trajectory that looks real. The proxy now asks
  upstream only for `identity`, inflates gzip or deflate if a provider compresses
  anyway, and records the plain body — so the stripped header is finally an honest
  description of the bytes under it. Anything it still cannot read back, another
  content coding or a body that is not UTF-8, fails the call with the reason, in the
  agent's error and in the trajectory, instead of being written down wrong. The
  stand-in provider that catches this compresses whether or not it was asked to,
  because a recorder that only copes when it opted in is one that lies the first time
  a provider changes its mind. (#56)
- **A trajectory the pre-#56 proxy already mangled still read back as faithful.** #56
  stopped new recordings holding U+FFFD; it did nothing for the ones already on disk,
  and a replay of one re-derives the same addresses because the mangled body is
  exactly what was recorded — hash equality proving nothing about content nobody can
  read. `noidroid verify` now also reads every recorded value back and reports one of
  three: `intact`, `suspect` (a replacement character is present, which a provider can
  legitimately send — never called corrupt on that alone), or `lost` (a run of
  replacement characters, a body that is mostly one, a body opening with what a gzip
  header becomes under a lossy decode, or a declared `application/json` body that does
  not parse — evidence weighed together, not a single trigger). `noidroid replay`
  carries the same reading: a trajectory whose source recording is `lost` now reports
  `unverifiable`, never `faithful`, because the claim is not available rather than
  false. Nothing is repaired — the original bytes are gone — so the only outcome is
  that the recording stops looking real. (#70)
- **`--inject all` was in the help and never in the binary.** It promised to branch
  every failure in turn and refused the moment anyone tried it. The help now names
  only what works, and a test through the real binary keeps it that way — a tool whose
  own `--help` overstates it is the same failure as a trajectory that looks real,
  scaled down. (#35)
- **Two writers of the same object raced, and the loser said only `No such file or
  directory`.** `Store::put` wrote through a scratch file named after the object, so
  concurrent writers of identical bytes — which is the normal case in a
  content-addressed store, not the exotic one — shared one scratch path. Whichever
  renamed first moved the file out from under the other. The scratch name is now
  unique to the writer. (#44)
- **The object store and the tree walker never got the context #42 introduced.** A
  failure reading, writing, listing or pruning now names the operation and the path,
  so a `NotFound` in CI says which file it could not find. This is what made the macOS
  failure in #44 unreadable for two runs. (#44)
- **An agent behaved differently under recording than outside it.** The proxy read a
  whole response before writing a byte back, so a streamed completion arrived as one
  block at the end instead of as tokens. The bytes were identical and the recording
  faithful, which is what made it easy to miss: an agent that only times out when
  recorded is not the agent you set out to record. A `text/event-stream` response is
  now relayed chunk by chunk while the concatenation is kept for the trajectory.
  Everything else still buffers, because a JSON body is one value that exists all at
  once. The engine hears about a call only when it completes, so a passed-through
  stream is recorded after its last byte has reached the agent — fine for a recording,
  and to be reconsidered if a replay ever streams. (#45)
- **The fence blocked calls the engine had authorised.** Egress is now permitted for
  the body of a call the engine answered `execute` to — and nowhere else, on the
  thread that call runs on. A plain replay authorises nothing, so the window never
  opens; a branch performs its post-fork calls, which are recorded and were never the
  silent egress the fence exists to catch. Without this, `--live` would have been
  fenced out of the one call it exists to make, and CI would not have noticed:
  its stand-in is on loopback, which was allowed all along. (#46)

## [0.3.0] - 2026-08-19

*The laws of the Stand.* This release adds almost no functionality. It makes the
execution model coherent, so that adding robotics, RL environments or a laboratory
later is an adapter rather than a redesign. The contract is
[`docs/environment-model.md`](docs/environment-model.md).

### Added

- **The environment contract.** An environment is not something we can save and load;
  it is something that can be asked, told, and asked what it knows about its own state.
  Three methods — `manifest`, `observe`, `restore` — and two of them are allowed to say
  no. Three implementations, because there are three distinct answers: `Workspace` (the
  directory we own), `Reported` (a world only the program can see), and `Situation`
  (the two together). Anything not written in Rust declares its world over the wire
  instead, with the new `observe` protocol message.
- **Grip: what a state reference is worth.** `state_root` used to be the Merkle root of
  a directory, full stop — one answer to "what is the state here", and the wrong one for
  every environment this project is aimed at. A step now records a `grip` beside it:
  `captured` (we hold the bytes and can put the world back), `witnessed` (we hold a
  fingerprint, so a reconstruction that lands elsewhere is detected and can never be
  repaired), `opaque` (nothing was captured, so re-execution still works and cannot be
  shown to have worked). Grip joins like provenance: the weakest part decides.
- **A checkpoint answers three questions.** `noidroid show` now prints *reach* (can I
  get back here: `rebuild`, `rebuild+restore`, `unreachable`), *evidence* (will I know
  if I got it wrong) and *grounding* (is what I get back to a claim about reality). None
  collapses into another, and none of them is a percentage. A robot checkpoint reads
  `rebuild / none / real`; a checkpoint inside a branch reads
  `rebuild / captured / simulated`.
- **Branching from an unreachable checkpoint is refused before the program is spawned.**
  If the prefix performed an irreversible effect in a world we cannot put back, the only
  route back through it is to perform it again. The refusal names the step and the
  target. Discovering this halfway through is the version that re-submits the form in
  order to find out it should not have.
- **The reference environment** (`examples/reference/`): a deterministic reactor and an
  operator whose policy is reasonable and wrong. About a hundred lines that record,
  reconstruct, branch, intervene and compare — original melts down, branch survives —
  in a world that can be re-driven and can never be put back. It is the environment
  contract made runnable, and it is what `environment_slice` tests against.
- `Trajectory.worlds` records which worlds a run declared and how well it could see
  them, so `noidroid log` can say what it would take to return to a recording.

### Changed

- **The browser adapter declares its page as a world.** It was already re-driving and
  comparing page digests, in its own private vocabulary, with nothing in the core aware
  that a page existed. The page is now a `witnessed` world like any other: the
  fingerprint lands in the recorded state at `.world/browser.json`, `noidroid show`
  prints it, `noidroid diff` shows when it changed, and the checkpoint analysis knows
  the difference between the workspace and the page.
- **`bisect` no longer counts a probe that established nothing as a flip.** A branch
  that could not be re-entered, could not be reconstructed, or died without reaching a
  verdict reads as `unknown`. Reporting it as "changed the outcome" was inventing the
  one answer the command exists to find.
- `EffectKind::Write` is documented as what it always meant: *reversible under
  reconstruction*, not "touches a disk". A browser navigation is a `write` because
  re-driving rebuilds the page; an actuator command is `irreversible` because nothing
  rebuilds the world.

### Fixed

- **A reconstruction claimed to have restored a world it had not.** When a
  `write`/`irreversible` effect was deliberately not re-executed, the engine restored
  the recorded tree and counted the step as `state_restored`. For a directory that is
  true. For a browser it filed a page nobody restored under the address of the page the
  recording saw. Restoration now reports the grip it actually achieved, and anything
  short of `captured` is *checked* instead of asserted.
- **The browser tests could not tell a missing browser from a broken one.** The guard
  looked for a Chromium on disk, so a host with the download but not its shared
  libraries failed three tests with an agent that "aborted" for no stated reason. It
  launches a browser now — and the first version of that probe was itself the same bug
  one level down: `cargo fmt` joined its multi-line string literal and kept the source
  indentation, turning the script into an `IndentationError` that exited in thirteen
  milliseconds and read as "no browser here", on the CI job whose entire purpose is to
  run the browser tests. One-line script now, and the skip says which shared library is
  missing.

### Compatibility

`STEP_VERSION` stays at **1**. `grip` defaults to `captured` and is skipped when
serialising, so a step recorded before this release reads back as exactly what it was,
and a workspace-only step recorded after it serialises to the same bytes and the same
address as before. Existing trajectories replay, branch, export and import unchanged.
No migration.

### Added

- **The egress fence.** During a replay or a branch, an outbound socket to anything
  but loopback is refused. A reconstruction is supposed to serve every mediated input from the
  recording and touch nothing, but that was only enforced for calls that went
  *through* the protocol — anything a program did behind our back still reached the
  network, and nothing said so. The replay finished and reported itself faithful. That
  was the worst failure mode here, because it was the silent one. Loopback stays open
  for our own socket, the proxy and local stand-ins; recordings are never fenced,
  since reaching the world is what they are recording.
  Blind spots, stated rather than hidden: subprocesses do not inherit the patch, C
  extensions can bypass Python's socket module, and connections opened before the
  client loads escape it.
- A run that dies mid-way now reports **what the program said on the way out**.
  "Truncated" is true and explains nothing; the traceback usually is the explanation.

### Fixed

- An I/O failure now says what it was doing and on what path. `No such file or
  directory` names none of the six operations that can produce it, which made a
  real CI failure unreadable. (#42)
- **Automatic capture failed open, and said it did not.** The README promised
  "`--auto` prints what it hooked; anything not listed was not recorded" — but the
  print was gated behind an environment variable nothing in the project ever set, so
  the single documented mitigation for fail-open capture never ran. It always prints
  now.
- **The async surface was never patched and never mentioned.** A program using
  `AsyncAnthropic` had its sync calls recorded and its async calls run **live during
  replay**, while the replay reported itself faithful. `--auto` now names every
  surface it could not cover and **refuses to record**, with `--allow-gaps` as a
  deliberate override that is stored on the trajectory so replays make the same
  allowance.
- `auto.install()` silently continued when an SDK's base client was not where it
  expected, contradicting its own docstring; it raises now, because an upstream
  rename that records nothing and exits zero is the failure this module exists to
  prevent.
- `Step.v` was written but never checked on read, so a future format version would
  have produced bogus divergences rather than an honest refusal.
- `_PassThrough.call` did not accept `volatile=`, so a program using it raised
  `TypeError` when run *without* noidroid — defeating the point of the pass-through.
- **`noidroid replay <traj> --live <target>`** — re-run part of a recording for real.
  A plain replay is the wrong instrument when the thing you changed is upstream of the
  recording, such as a prompt or a model: a published study forking live trajectories
  after a model swap found only about 3% of replayed states remained valid. So the
  operator names what should be new — `--live model` covers every `model.*` call — and
  the tools, network and clock still come from the recording, so exactly one thing
  changed and the comparison means something.
  Everything before the first live call still reproduces and is verified. Everything
  after is `live`, and a call the run still makes in the same order is still served
  from the recording, so it keeps its grip for as long as it tracks and only executes
  what the recording genuinely cannot answer. The result is kept as its own trajectory,
  because comparing it against its recording is the whole point, and it is never
  called faithful — part of it was asked to be new.

## [0.2.0] - 2026-08-19

### Fixed

- Release checksum files recorded `dist/<name>.tar.gz` as the path, so `sha256sum -c`
  failed for anyone who downloaded them anywhere else. Found by verifying the
  published v0.1.0 artifacts rather than by reading the workflow.
- CI re-downloaded Chromium on every run, which was most of the wall time on the
  slowest job. It is now cached against the Playwright version.

### Added

- **`noidroid run --proxy -- <command>`** records an agent you did not write. Both the
  Anthropic and OpenAI clients read their endpoint from the environment, so the proxy
  stands between the agent and the provider and records what crosses the wire — no
  patching, no TLS interception, and no requirement that the agent be Python or that
  you have its source. Tested with an agent that has neither a noidroid import nor a
  configured base URL, recorded and then replayed with the provider shut down.
  Unlike a trace exported from an observability tool, what is recorded is the request
  itself, which is what makes matching on replay meaningful rather than approximate.
  It captures provider traffic only; pair it with `--watch` to record the files. A
  streamed response is buffered rather than passed through: same content, different
  timing (#45).

### Fixed

- The check that catches a missing program treated URLs and flag values as paths,
  because both contain slashes. Introduced with the check itself and caught by using
  the proxy, whose upstream is a URL.

### Added

- **`noidroid export` / `noidroid import`** — a trajectory and everything it reaches
  as one committable JSON file. A recording is only a regression test if it can leave
  the machine it was made on, and `.noidroid/` is gitignored and machine-local. The
  bundle stays readable so a reviewer sees what the agent said in the diff, and every
  address is re-hashed on import: a bundle arrives from elsewhere, so its claim that
  an address holds given bytes is checked rather than believed.

- **`noidroid bisect <trajectory>`** — automatic causal attribution. Every recorded
  decision is re-run from its own checkpoint with a different choice, and the earliest
  one that changes the outcome is reported. A trace cannot answer which step *caused*
  a failure, because that is a question about a world that did not happen; the
  published baseline for judging it from a transcript is around 14% accurate. Each
  probe is a real trajectory that can be opened, diffed and replayed, and the prefix
  of each costs nothing because it is served from the recording. When nothing flips it
  says so and exits non-zero, rather than naming a plausible step and calling it the
  cause.

### Added

- **`noidroid run --watch <dir>`** records a directory you already have — your actual
  project — instead of a sandbox. It is read, never cleared. Snapshots skip `.git`,
  `node_modules`, `target` and the like, extendable with `.noidroidignore`, because
  hashing a real repository after every step is otherwise unaffordable.
- **`noidroid restore <traj>@<step>`** puts the files back as they were at a
  checkpoint. It snapshots what is currently there first and prints its address, so
  **`noidroid checkout-tree <address> <dir>`** is the way back. This is the most
  requested capability on coding-agent issue trackers by an order of magnitude, and it
  is about files rather than conversation.
- Reconstruction never touches a watched directory: replays and branches re-execute
  the program, which writes files, so they always get their own copy.

### Fixed

- Replaying a trajectory whose program is not present reported that the process never
  connected, which is true and unhelpful. It now says that a trajectory records what a
  program did rather than the program itself — the first thing anyone hits after
  importing a bundle.

- `materialize` pruned anything the recorded tree did not contain, without consulting
  the ignore list that had kept it *out* of the recording. Restoring into a real
  project would have deleted `.git`, `node_modules`, and the `.noidroid` directory
  holding the trajectory being restored from — in that order. Found by running it, not
  by reading it.

### Added

- **`noidroid run --auto`: zero-code recording.** A `sitecustomize.py` goes on the
  child's `PYTHONPATH` — the mechanism `opentelemetry-instrument` and `ddtrace-run`
  use — and patches the OpenAI and Anthropic base clients at `request`, below the
  retry loop, so one logical call is one recorded step however many times it retried.
  A program that never mentions Paranoid Android can now be recorded and replayed; the
  SDK's own response type is rebuilt on replay, so `reply.content[0].text` still
  works. Tested against the real Anthropic SDK with the API shut down.
  It cannot make anything *branchable*: no patching can infer that a value was a
  choice among alternatives, so `decide()` stays explicit. The honest shape is
  **zero code to record and replay, two lines to branch**.
- **`volatile=` on `call`** — names arguments that change every run without changing
  what the call means, such as a timestamp or a request id. Without it an argument
  carrying a clock makes every replay diverge, which is true and useless.
- **Divergence reports say what differed**, field by field, and point out when the
  call the run wants is recorded further along — which usually means an interaction
  was inserted or removed, rather than changed.

### Added

- **`noidroid tui`** — the viewer the manifesto calls V0.1, built with
  [ratatui](https://ratatui.rs). Three panes and one verb: the timeline is coloured by
  provenance, and pressing `e` on a recorded decision reconstructs the prefix,
  diverges, and returns a new trajectory without leaving the screen. `--plain` drops
  the flourishes; `NO_COLOR` drops the colour; neither removes information, because
  nothing is said in colour that is not also said in words.
- **The Stand's colourway**, replacing the four-colour ANSI styling. Each colour is
  bound to a meaning the tool already had — phosphor green `real`, chrome `live`,
  violet `simulated`, amber `unknown`, cyan `replayed`, crimson for divergence — so
  provenance is legible at a glance. Truecolor where the terminal admits to it, ANSI
  where it does not, nothing when piped.
- **`noidroid stand`**. Araki names Stands after music, so a Stand called PARANOID
  ANDROID is built to the rule and fans will know it on sight. The six parameters are
  graded honestly — Destructive Power **E**, because it can never change what
  happened — which makes the stat block an accurate capability summary as well as a
  joke. Nothing in the workflow goes through it.
- **`noidroid.llm.Model`** — an adapter for the one input an agent cannot make
  deterministic. Recording model calls means a replay serves them back rather than
  calling the provider, so re-running an agent against a real conversation is free and
  deterministic; and the model's tool choice is declared as a decision on the agent's
  behalf, so branching to a different tool needs no instrumentation from the agent at
  all. Provider-agnostic: it takes a callable, imports no SDK, and understands both
  the Anthropic content-block and OpenAI `tool_calls` response shapes.
- An `examples/llm_agent/` worked example with a deterministic stand-in model, so it
  runs with no API key.

### Fixed

- A branch whose checkpoint could not be reached was refused, and then written to disk
  anyway. The caller was told the branch failed while a trajectory sat in
  `noidroid log` claiming an ancestry it did not have. The engine now declines to
  persist it, and removes its workspace, because "you cannot branch from a checkpoint
  you cannot reach" is an invariant of the engine rather than advice to the CLI.
- A browser branch whose starting state could not be reproduced said so in the
  terminal but recorded its observations as `live`, i.e. as things that really
  happened. An unreproducible reconstruction now marks everything after it `unknown`,
  which propagates to the head of the trajectory.

### Added

- `noidroid.Ungrounded`: a wrapper an adapter returns to say "here is a real value,
  but it is not evidence about the original execution". The protocol gained `unknown`
  on a result to carry it, which — like `unknown` on an error — is the only kind of
  provenance claim a client may make, because it can only lose trust.
- `Browser(strict=True)` refuses to continue from a state it could not reproduce,
  instead of continuing and marking it unknown. Off by default: a page digest is an
  exact comparison and real pages carry clocks, so a fatal default would refuse most
  real branches.
- A `/volatile` page in the example site whose rendered text comes from the clock, so
  the boundary can be demonstrated rather than described.

## [0.1.0] - 2026-08-18

First working prototype: an execution can be recorded, returned to, and branched.

### Added

- **Trajectory engine.** An execution records as an immutable, content-addressed
  Merkle DAG of steps — `(parent, action, effects, state_root, provenance)`. A branch
  is a step whose parent belongs to another trajectory, so immutable history, prefix
  sharing and copy-on-write follow from the data model rather than being layered on.
- **Verified reconstruction.** `noidroid replay` re-derives a trajectory and checks it
  addresses the same objects, reporting `key_mismatch`, `state_mismatch`,
  `unexpected_call` or `truncated` instead of papering over a divergence.
- **Branching with typed interventions:** `--decide` (choose differently at a declared
  decision point), `--result` (answer differently from the world), `--fail` (inject a
  failure), `--simulate` (supply a stated-simulated value for an irreversible effect).
- **Provenance and delivery as separate axes.** Provenance — `real` ⊑ `live` ⊑
  `simulated` ⊑ `unknown` — is content, is hashed, and never improves along a chain.
  Delivery — `executed`, `replayed`, `intervened`, `denied` — is per-run and is not
  hashed, which is what lets a branch share its parent's objects exactly.
- **Irreversible effects fail safe.** Performed only during an original recording;
  every replay and branch refuses them unless a simulated value is supplied.
- **CLI:** `run`, `log`, `show`, `replay`, `branch`, `checkout`, `tree`, `diff`,
  `verify`.
- **Python client** (standard library only) and a **browser adapter** that drives
  Chromium, records HTTP responses, and reconstructs a browser session by re-driving
  recorded actions — verified against a recorded page digest, and demonstrated with
  the website switched off.
- Two worked examples and 24 tests, two of which drive real Chromium.
- **Engineering practice.** Continuous integration on every pull request: fmt, clippy
  (`-D warnings`), the full suite on Linux with a browser, the suite *without* a
  browser (the browser tests must skip cleanly, or contributors cannot run the suite),
  the suite on macOS because the Unix-socket transport is a portability claim, and the
  CLI example end to end including a check that branching did not move its parent's
  head. Releases come from `v*` tags only, and are refused if the tag disagrees with
  `Cargo.toml` or has no changelog section.
- `format_is_pinned`, which asserts the exact serialised bytes and address of a known
  step. Object names *are* the hash of their bytes, so a silent format change would
  invalidate every recording ever made; see [CONTRIBUTING.md](CONTRIBUTING.md) for when
  a change needs a `STEP_VERSION` bump.

### Known limitations

Documented in the README, and deliberate: not zero-code, sequential programs only,
only the sandboxed workspace is captured, the ambient environment is not captured, a
branch is not a prediction, browser reconstruction is bounded by the recorded page
set, and no scale work.

[Unreleased]: https://github.com/swalla02/noidroid/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/swalla02/noidroid/releases/tag/v0.3.0
[0.2.0]: https://github.com/swalla02/noidroid/releases/tag/v0.2.0
[0.1.0]: https://github.com/swalla02/noidroid/releases/tag/v0.1.0
