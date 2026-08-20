# Sources, vocabulary and query patterns

## The source mix

Use several. A finding that appears in only one kind of source is usually either very
early or not real.

**Code**
- GitHub / GitLab / Codeberg: repositories, **issues** (where the truth about
  limitations lives), discussions, releases, and specific commits when a mechanism
  changed.
- Package ecosystems: crates.io, PyPI, npm, Go modules — reverse dependencies show
  who actually uses a thing.
- Awesome-lists and curated indexes as *entry points only*, never as evidence.

**Research**
- arXiv (cs.DC, cs.SE, cs.PL, cs.OS, cs.DB, cs.LG, cs.RO), and the venues that matter
  here: OSDI, SOSP, NSDI, EuroSys, ATC, PLDI, OOPSLA, ICSE, FSE, ISSTA, VLDB, SIGMOD,
  CoRL, ICRA, MLSys, TaPP/IPAW (provenance), USENIX FAST (storage).
- Papers *cited by* a project, and papers that cite it. The citation graph is the
  cheapest way out of your own vocabulary.
- Theses and tech reports — often the only place a mechanism is explained fully.

**Engineering writing**
- Company engineering blogs (databases, game engines, CI/build, observability,
  simulation, robotics).
- Architecture decision records and design docs in repositories (`docs/`, `rfcs/`,
  `adr/`, `design/`).
- Post-mortems and "why we removed X" posts. High signal.
- Conference talks: Strange Loop, CppCon, GDC, RustConf, PyCon, Systems Distributed,
  Papers We Love. Slides and transcripts, not the marketing summary.

**Ecosystem**
- Hacker News: search a project name and read the *comments by people who used it*.
- Reddit (r/rust, r/programming, r/robotics, r/MachineLearning, r/devops) where the
  thread is technical.
- Mailing lists, Zulip/Discourse archives (Rust, LLVM, Kubernetes, ROS Discourse).
- Standards bodies: W3C, IETF, OpenTelemetry, CNCF, OCI, RO-Crate / W3C PROV.

**Industry**
- New tools and infrastructure projects, launch posts *read for their architecture*,
  and the docs page that describes how it actually works.

## Vocabulary ladders

Our problems have other names. Search the right-hand column.

| We say | The world says |
| --- | --- |
| trajectory | trace, execution log, journal, event log, tape, recording, lineage, derivation chain |
| checkpoint | snapshot, savepoint, restore point, image, CRIU dump, fork point, epoch |
| deterministic replay | record-and-replay, rr, deterministic execution, reproducible run, hermetic build, deterministic simulation testing (DST), lockstep |
| branching | fork, what-if, alternate timeline, speculative execution, shadow run, A/B execution |
| provenance | lineage, taint, data provenance, W3C PROV, RO-Crate, attestation, audit trail, SLSA |
| content-addressed store | CAS, Merkle DAG, object store, blob store, IPLD, restic/borg chunking, nix store |
| divergence | drift, mismatch, non-determinism, flakiness, differential testing, bisimulation |
| counterfactual | ablation, intervention, do-calculus, fault injection, mutation, chaos experiment |
| capture boundary | interposition, syscall interception, shim, LD_PRELOAD, seccomp filter, ptrace, eBPF uprobe, VFS layer, proxy |
| environment reconstruction | hermetic environment, sandbox, sysroot, container image, nix derivation, lockfile |
| unknown/unavailable | partial observability, best-effort, degraded mode, missing data semantics |
| step | frame, tick, transition, event, span, operation, entry |
| irreversible effect | side effect, non-idempotent operation, external effect, exactly-once, effect system |

## Adjacent fields worth raiding

Each of these has been solving one of our problems for decades:

- **Databases** — MVCC, snapshot isolation, WAL, copy-on-write B-trees, time-travel
  queries, logical vs physical replication.
- **Filesystems / storage** — ZFS and btrfs snapshots, overlayfs, reflinks, dedup and
  chunking (restic, borg, casync), Merkle trees.
- **Distributed systems** — deterministic simulation testing (FoundationDB, TigerBeetle,
  Antithesis), lineage-based recovery (Spark), event sourcing, causal consistency.
- **Debuggers and emulators** — rr, Pernosco, gdb reverse execution, QEMU record/replay,
  Bochs, cycle-accurate console emulators and their savestates, TAS tooling.
- **Build systems** — Bazel/Nix/Guix hermeticity, action caching, remote execution,
  reproducible builds, content-addressed action graphs.
- **Game engines and netcode** — rollback netcode (GGPO), deterministic lockstep,
  replay files, ECS snapshotting, fixed timestep.
- **Observability** — OpenTelemetry, distributed tracing, continuous profiling, and
  crucially the places where a trace is *not enough* to reconstruct behaviour.
- **Testing** — property-based testing and shrinking, fuzzing corpora and coverage,
  deterministic schedulers, model checking (TLA+, Alloy), mutation testing, chaos
  engineering, record/replay HTTP (VCR/cassettes, WireMock, mitmproxy).
- **Robotics** — ROS bags and rosbag2, MCAP, Foxglove, sim-to-real, deterministic
  simulators (MuJoCo, Isaac), teleop trajectory replay.
- **Scientific computing** — workflow engines (Nextflow, Snakemake, CWL, WDL),
  provenance capture (ReproZip, Sciunit, noWorkflow), electronic lab notebooks, lab
  automation and protocol description languages, experiment tracking (W&B, MLflow,
  DVC).
- **RL / agents** — replay buffers, environment seeding, offline RL datasets, world
  models, gym/gymnasium env determinism, agent eval harnesses and their reproducibility
  claims.
- **Virtualisation** — CRIU, gVisor, Firecracker snapshotting, WASM component model,
  microVM restore, unikernel state.

## Query patterns that work

- Mechanism + domain: `"copy-on-write" snapshot process checkpoint site:github.com`
- Failure-first: `"we removed" deterministic replay` / `"why we stopped using" record replay`
- Limitation mining: `record replay "known limitations" async`
- The graph: read a paper's related-work section; search each named system directly.
- Issue mining: `is:issue "non-deterministic" replay` inside a specific repo.
- Version drift: a project's `CHANGELOG` for the release where a mechanism appeared or
  was removed — that is where the design rationale usually is.
- Dates: constrain periodic scans to a window, but never constrain a targeted search —
  the best mechanism for our problem may be from 2005.

## Credibility grading

| Grade | What it takes |
| --- | --- |
| HIGH | You read the implementation or the paper's method section; the mechanism is unambiguous; limitations are stated by the authors. |
| MEDIUM | You read the primary doc but not the code; or the mechanism is described but not evaluated. |
| LOW | You have a strong secondary source and could not reach the primary one. Say so in the card. |

Never grade HIGH on something you did not open.
