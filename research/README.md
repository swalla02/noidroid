# Research

The scout's knowledge base. It **accumulates** — nothing here is overwritten wholesale,
and a superseded finding moves to `archive/` with its reason rather than disappearing.

Run a cycle with `/scout <question>`. Close the loop with `/scout-verdict` when a
recommendation gets an answer. Method lives in
`.claude/skills/technical-scouting/SKILL.md`.

**Read before recommending anything:** [`constraints.md`](constraints.md) — settled
decisions with reasons. Re-proposing one without new evidence is the failure mode that
would make all of this noise.

| | |
| --- | --- |
| [`CONTEXT.md`](CONTEXT.md) | The architecture the scout reasons against. Verified against `10c1b64` (0.3.0); the environment model has shipped. |
| [`constraints.md`](constraints.md) | Settled decisions — do not re-propose without new evidence. |
| [`decisions.md`](decisions.md) | Feedback ledger: recommendation → verdict → outcome → lesson. |
| [`taxonomy.md`](taxonomy.md) | The evolving category list. |

---

## Open recommendations

The live ones, highest priority first. Moved to `decisions.md` once they get a verdict.

| Score | Action | Rec | Card | Touches |
| --- | --- | --- | --- | --- |
| 81 | **Close the `--live` irreversible hole** — `engine.rs:693` is the one `execute` path that never consults `may_perform_irreversible()`. Failing test first | PROTOTYPE | [`live-replay-performs-irreversible-effects`](discoveries/2026-08-24-live-replay-performs-irreversible-effects.md) | engine, cli, clients |
| 81 | ~~Make an adopted world observation report as `opaque`, not `witnessed`~~ — **landed as #52** (`Report::served`, `Situation::achieved`). #53 is open on the model underneath. | PROTOTYPE | [`unverified-world-redrive`](discoveries/2026-08-19-unverified-world-redrive.md) | env, engine, cli |
| 81 | Build the engine-issued seed — engine mints, `Action::Genesis` records, client applies. Supersedes the autoseed spike's design | PROTOTYPE | [`engine-issued-seed`](discoveries/2026-08-21-engine-issued-seed.md) | proto, model, engine, clients |
| 81 | Add `noidroid run --verify` — replay the recording you just made and report divergence as a capture gap | PROTOTYPE | [`verify-by-double-execution`](discoveries/2026-08-19-verify-by-double-execution.md) | engine, cli |
| 36 | Write "validated by re-derivation" into the snapshot fast-path as an acceptance criterion, before it is designed | INVESTIGATE | [`snapshot-omits-derived-state`](discoveries/2026-08-19-snapshot-omits-derived-state.md) | engine, tree, store |
| 27 | Spike Landlock enforcement of the egress fence, with the refusal path as the acceptance criterion | INVESTIGATE | [`kernel-enforced-capture-boundary`](discoveries/2026-08-19-kernel-enforced-capture-boundary.md) | engine, cli, clients |
| 36 | Run the five-edit experiment and publish "what you can change and still replay"; add the `is_replaying` warning to `delivery` | INVESTIGATE | [`replay-safe-change-taxonomy`](discoveries/2026-08-21-replay-safe-change-taxonomy.md) | engine, cli, clients, docs |
| 36 | State the branch-point boundary in the README — interventions apply where the program asked | WATCH | [`input-tree-not-state-tree`](discoveries/2026-08-21-input-tree-not-state-tree.md) | docs |
| 24 | ~~Spike PRNG seed-and-record~~ — design answered 2026-08-21; carried forward by `engine-issued-seed` above | INVESTIGATE | [`autoseed-and-record`](discoveries/2026-08-19-autoseed-and-record.md) | clients, model |
| 12 | Name the replay mode (open-loop / closed-loop non-reactive / reactive) in the run report, when #24/#34 are picked up | WATCH | [`log-replay-validity-modes`](discoveries/2026-08-19-log-replay-validity-modes.md) | cli |
| 54 | Emit a fork-point evidence record — branch at N indices and report, per fork, whether the recorded `state_root` was re-derived | PROTOTYPE | [`unverified-fork-in-branching-rl`](discoveries/2026-08-21-unverified-fork-in-branching-rl.md) | engine, checkpoint, env, cli |
| 54 | Settle #53 Q1 (does a pure replay report `opaque`?) using "recomputed vs measured reward" as the forcing case, and name run grip apart from trajectory grip | PROTOTYPE | [`reward-computed-over-an-unaddressed-state`](discoveries/2026-08-21-reward-computed-over-an-unaddressed-state.md) | env, engine, cli |
| 36 | Add the browser adapter's re-drive mute and assert the run report changes (#53 Q5) | PROTOTYPE | [`reproducibility-bought-by-mocking-the-world`](discoveries/2026-08-21-reproducibility-bought-by-mocking-the-world.md) | clients |
| 36 | Ship `noidroid score` — re-run a checker against a step's materialised state, offline | PROTOTYPE | [`reward-computed-over-an-unaddressed-state`](discoveries/2026-08-21-reward-computed-over-an-unaddressed-state.md) | cli, tree |
| 54 | **Measure restore-and-branch at step k and publish the curve** — the band we are compared against is now a distribution (0.1–2 s), not two numbers. Gate on the fork-point record; fourth scan carrying it | INVESTIGATE | [`unverified-fork-in-branching-rl`](discoveries/2026-08-21-unverified-fork-in-branching-rl.md) | engine, cli |
| 36 | Write an OpenEnv adapter: `state()` becomes a declared world, `witnessed` grip for free | INVESTIGATE | [`openenv`](landscape/openenv.md) | clients |
| 36 | Survey the four client paths for server-side session handles, then declare the inference endpoint as a world in §12 and `doctor` | INVESTIGATE | [`attended-state-is-a-world-we-never-declare`](discoveries/2026-08-24-attended-state-is-a-world-we-never-declare.md) | env, engine, cli, clients |
| 24 | Make the irreversible-effect record queryable across a trajectory's branches — a walk and a filter over data we already store | WATCH | [`no-undo-across-the-tool-boundary`](discoveries/2026-08-24-no-undo-across-the-tool-boundary.md) | cli, repo |

## Scans

| Date | Question | Cadence |
| --- | --- | --- |
| [2026-08-24](scans/2026-08-24-computer-use-rollback-and-effect-boundaries.md) | For computer-use agents, what happens to state a rollback does not restore? (Answer: eight 2026 systems are inventing the seam we already have — and reading one of them against `engine.rs` found a live replay that performs irreversible effects.) | targeted |
| [2026-08-21](scans/2026-08-21-deterministic-simulation-testing.md) | What does the deterministic simulation testing world know that we do not? (Answer: it confirms our branching model and our checkpoint choice, and hands us the seed mechanism. Its technique needs a rewrite of the world.) | targeted |
| [2026-08-21](scans/2026-08-21-computer-use-gaps-and-rl-post-training.md) | Where is the next capability gap in computer use, and what would make us useful for open-source RL post-training? (Answer: a verified fork — not a rollout format, which is already built.) | open |
| [2026-08-20](scans/2026-08-20-rl-robotics-autonomous-labs.md) | What should we build next to be useful for RL, autonomous labs and robotics? (Answer: nothing domain-specific — but they found a silent pass in our environment model.) | open |
| [2026-08-19](scans/2026-08-19-earning-the-claim.md) | How do other systems find out what they failed to capture — and how do they tell the user? | targeted |

## Discoveries

Newest first. `PRESENT`/`REFINEMENT` cards are here so nobody rediscovers them.

| Card | Rec | Novelty | Conf | What it is |
| --- | --- | --- | --- | --- |
| [live-replay-performs-irreversible-effects](discoveries/2026-08-24-live-replay-performs-irreversible-effects.md) | PROTOTYPE | MISSING | HIGH | Of three `execute` paths in `on_call`, one never asks `may_perform_irreversible()`. `--live` on an irreversible target charges the card again, silently. |
| [attended-state-is-a-world-we-never-declare](discoveries/2026-08-24-attended-state-is-a-world-we-never-declare.md) | INVESTIGATE | MISSING | MEDIUM | A logical rollback and a serving session's KV disagree invisibly. The inference endpoint is a world in our own §12 sense and has no row. |
| [no-undo-across-the-tool-boundary](discoveries/2026-08-24-no-undo-across-the-tool-boundary.md) | WATCH | PRESENT | MEDIUM | Eight 2026 systems inventing a transaction boundary for agent effects. All three inference mechanisms exist because nobody can declare what `EffectKind` declares. |
| [engine-issued-seed](discoveries/2026-08-21-engine-issued-seed.md) | PROTOTYPE | MISSING | HIGH | Temporal issues the PRNG seed from the orchestrator and records re-seeding as an event. The only shape that keeps a branch honest. |
| [a-simulator-per-dependency](discoveries/2026-08-21-a-simulator-per-dependency.md) | WATCH | DIFFERENT | HIGH | DST's price is a hand-written simulator per dependency. RisingWave got to one; their escape plan was Hermit. The unserved row is ours. |
| [replay-safe-change-taxonomy](discoveries/2026-08-21-replay-safe-change-taxonomy.md) | INVESTIGATE | REFINEMENT | HIGH | Temporal runs our architecture and says code change, not capture, is the top divergence cause. We have no change-safety table. |
| [input-tree-not-state-tree](discoveries/2026-08-21-input-tree-not-state-tree.md) | WATCH | DIFFERENT | HIGH | Antithesis owns the whole machine and still models exploration as a tree of boundary inputs — then added injection time, which we cannot express. |
| [unverified-fork-in-branching-rl](discoveries/2026-08-21-unverified-fork-in-branching-rl.md) | PROTOTYPE | MISSING | HIGH | Branching RL forks a sandbox on an assumption its own algorithm never checks. Tree rollouts avoid it only by being stateless. |
| [rollout-graph-already-exists](discoveries/2026-08-21-rollout-graph-already-exists.md) | WATCH | PRESENT | HIGH | `verifiers` already ships the parent-linked prefix-sharing trajectory graph — and excludes the world from it by one line. |
| [reward-computed-over-an-unaddressed-state](discoveries/2026-08-21-reward-computed-over-an-unaddressed-state.md) | PROTOTYPE | MISSING | MEDIUM | A verifiable reward is a checker run against a final state nobody names. OpenEnv makes it structural. |
| [reproducibility-bought-by-mocking-the-world](discoveries/2026-08-21-reproducibility-bought-by-mocking-the-world.md) | INVESTIGATE | DIFFERENT | MEDIUM | Computer-use benchmarks got reproducibility by replacing the web with a mock. The flagship score is one run. |
| [unverified-world-redrive](discoveries/2026-08-19-unverified-world-redrive.md) | PROTOTYPE | MISSING | HIGH | Four domains re-drive a world they cannot snapshot; none checks it landed. Neither do we, and we print that we did. |
| [snapshot-omits-derived-state](discoveries/2026-08-19-snapshot-omits-derived-state.md) | INVESTIGATE | REFINEMENT | HIGH | ALE's "complete" state restore silently lost the screen. The spec for our snapshot fast-path, written by someone else's bug. |
| [checkpoint-as-message-cache](discoveries/2026-08-19-checkpoint-as-message-cache.md) | WATCH | REFINEMENT | HIGH | Bluesky's checkpoint is a deque of messages replayed at real instruments. C2 is the incumbent design in beamline control. |
| [autoseed-and-record](discoveries/2026-08-19-autoseed-and-record.md) | INVESTIGATE | MISSING | MEDIUM | Minari seeds the run and records the seed instead of capturing randomness. Fail-loud where a clock freeze is fail-open. |
| [reversibility-is-not-in-the-instrument-standard](discoveries/2026-08-19-reversibility-is-not-in-the-instrument-standard.md) | WATCH | REFINEMENT | MEDIUM | SiLA 2's FDL describes every command except what re-performing it would cost. `EffectKind` cannot be inferred. |
| [log-replay-validity-modes](discoveries/2026-08-19-log-replay-validity-modes.md) | WATCH | DIFFERENT | MEDIUM | AV names three replay modes and a validity threshold. Take the names; the threshold is what you use without an oracle. |
| [verify-by-double-execution](discoveries/2026-08-19-verify-by-double-execution.md) | PROTOTYPE | MISSING | HIGH | Hermit checks its own determinism by running twice; our replay oracle makes the same check unambiguous. Corroborated by AV resimulation (upd. 2026-08-20). |
| [kernel-enforced-capture-boundary](discoveries/2026-08-19-kernel-enforced-capture-boundary.md) | INVESTIGATE | MISSING | MEDIUM | Landlock could make the egress fence enforced by the kernel rather than by the program's cooperation. |
| [silent-best-effort-sandboxing](discoveries/2026-08-19-silent-best-effort-sandboxing.md) | IGNORE | DIFFERENT | MEDIUM | A shipped sandbox returned `Ok()` while enforcing nothing. Binding constraint on how we adopt Landlock. |
| [process-determinism-ceiling](discoveries/2026-08-19-process-determinism-ceiling.md) | IGNORE | REFINEMENT | HIGH | Whole-process determinism is capped by the ISA, not by effort. C1 confirmed, with a citation. |

## Landscape

| Project | Class | Activity | Entry |
| --- | --- | --- | --- |
| Shepherd | DIRECT COMPETITOR | active — alpha v0.3.0 | [`shepherd`](landscape/shepherd.md) |
| Crab (HKUST) | INFRASTRUCTURE | active — preprint, no repo found | [`crab-agent-checkpoint-restore`](landscape/crab-agent-checkpoint-restore.md) |
| orx (OpenResearch CLI) | INSPIRATION | active — our own literature tool | [`openresearch-cli`](landscape/openresearch-cli.md) |
| verifiers (Prime Intellect) | POTENTIAL INTEGRATION | active — v1 | [`verifiers-prime-intellect`](landscape/verifiers-prime-intellect.md) |
| OpenEnv | POTENTIAL INTEGRATION | active — nine-org steering committee | [`openenv`](landscape/openenv.md) |
| AgentENV (kvcache-ai / Moonshot) | INFRASTRUCTURE | active | [`agentenv-kvcache`](landscape/agentenv-kvcache.md) |
| Bluesky (NSLS-II) | INFRASTRUCTURE | active | [`bluesky-runengine`](landscape/bluesky-runengine.md) |
| LeRobot (Hugging Face) | ADJACENT TOOL | active | [`lerobot`](landscape/lerobot.md) |
| LAP — Lab Agent Protocol | RESEARCH | active — 2026 preprint, no implementation found | [`lap-agent-instrument-protocol`](landscape/lap-agent-instrument-protocol.md) |
| Antithesis | INSPIRATION | active — commercial | [`antithesis`](landscape/antithesis.md) |
| Hermit (Meta) | INFRASTRUCTURE | dormant — maintenance mode; named by RisingWave as their escape plan before it stopped | [`hermit`](landscape/hermit.md) |

## Proposals

None yet. A card at `ADOPT`, or a `PROTOTYPE` that survived its spike, gets worked up
into `proposals/` before it becomes an issue.

## Archive

Empty. Superseded cards move here with a pointer to what replaced them.

---

## Standing questions

The things the next scans should chip at, kept here so they are not lost between runs:

- **How long does our checkpoint actually take to restore and branch, as a function of
  k?** Nothing has ever measured it, through four scans. The comparison band resolved on
  2026-08-24: Crab reports checkpoint p50/p95/p99 of 0.1/0.7/1.0 s and restore median 0.71 s
  on commodity backends (OpenZFS + runc-CRIU), bracketed by 1,920 ms (BPO Docker snapshot)
  and sub-50 ms (AgentENV microVM resume). So the question is now "are we inside 0.1–2 s at
  realistic k", which is sharper and still a day's work.
- **Do any of our client paths hold a server-side inference session handle?** Opened
  2026-08-24. Decides whether `2026-08-24-attended-state-is-a-world-we-never-declare` is one
  sentence of documentation or a `doctor` warning plus a declared world. A survey of four
  code paths, not research.
- **Does anyone besides ACRFence maintain a cross-branch log of irreversible effects, and
  in what shape?** Opened 2026-08-24. We found the need and not the prior art.
- **How often does a real-web browser re-drive reproduce the page digest exactly?** Decides
  whether "a reproducible computer-use episode against the real web" is a claim we may make.
- ~~**Deterministic simulation testing**~~ — **done 2026-08-21**, own targeted scan.
  Three leads it did not reach and which are worth a short follow-up, in order:
  **Shadow** (syscall-interposing network simulator — the one system sitting between DST
  and interposition, and its determinism caveats are the point), **shuttle** (a failing
  concurrency *schedule* serialised as a compact replayable artifact — possibly not a
  duplicate of the seed finding), and the **deterministic-OS line** (dOS, Determinator,
  CoreDet, Dthreads), which I expect to be confirmatory of the parallelism table but have
  not verified.
- **How does anyone choose which branch to explore?** Roadmap item 4 (guided
  multi-branch exploration) has no source anywhere. Antithesis explicitly withholds its
  guidance component; nobody else publishes one. Opened by the DST scan and unanswered.
- **The rest of the trainer landscape** — TRL, OpenRLHF, SkyRL, AReaL, slime, ROLL, rLLM,
  Tinker. The 2026-08-21 claim that persisted rollouts are "a transcript plus a scalar"
  rests on `verl` and `verifiers` only.
- **Computer-use trajectory datasets** — OpenCUA/AgentNet, AgentTrek, OS-Genesis,
  GUI-Odyssey, AndroidControl. An explicit sub-question of the 2026-08-21 scan that it did
  not reach, and which the 2026-08-24 scan also did not reach. **It has now lost twice.**
  Commission it as its own targeted scan or strike it — leaving it here is how it stays
  permanently second to whatever the run's headline question is.
- **rosbag2's replay implementation and the MCAP container format** — the 2026-08-20 scan
  reached the ROS *complaint* but not the source. The one weak line in
  `unverified-world-redrive`'s comparison table.
- **Do seeded simulators genuinely re-derive identical state?** (Isaac Lab, MuJoCo across
  versions, Gazebo.) This decides whether the RL row of the environment model's §12 table
  can honestly claim `captured` grip with save/load.
- Scientific workflow provenance (CWL, Nextflow, ReproZip, W3C PROV, RO-Crate) —
  anything there sharper than our two-axis provenance/delivery model? Partly addressed
  from the instrument-protocol side (SiLA 2, LAP) on 2026-08-20; the workflow-engine side
  is still untouched.
- Content-defined chunking and near-duplicate storage: `tree.rs` snapshots whole files
  after every step. What does that cost on a real project, and is there a cheaper
  representation that does not break "the workspace at step k has address T"?
- Structured trajectory comparison (roadmap item 4) — what do diffing tools for
  execution traces actually do well?
