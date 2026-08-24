---
date: 2026-08-19
cadence: targeted
question: "How do other systems find out what they failed to capture — and how do they tell the user? (The 'earn the claim' milestone.)"
cards_created:
  - 2026-08-19-verify-by-double-execution
  - 2026-08-19-process-determinism-ceiling
  - 2026-08-19-kernel-enforced-capture-boundary
  - 2026-08-19-silent-best-effort-sandboxing
cards_updated: []
landscape_created:
  - hermit
---

# Scan: how do other systems find out what they failed to capture?

*Seed run — the first scan, executed to prove the pipeline produces something worth
reading rather than to survey the field. Scope was deliberately narrow: the "earn the
claim" milestone in `docs/direction.md`, and specifically its hardest sentence — "for
each one the honest answer is currently 'we do not look'."*

*Checked against the environment-model work in flight (#48, uncommitted). Neither
recommendation below duplicates it; recommendation 1 gives its `evidence` axis
something that exercises it.*

## In one paragraph

I looked at how three serious attempts at the same honesty problem handle it, and the
single most useful thing found is not a capture technique — it is an epistemic move.
A capture layer cannot enumerate the holes it failed to plug, because enumerating a
hole is most of plugging it. Meta's Hermit answers this with `hermit run --verify`:
run twice, diff, and report that determinism was *not* achieved. **We can do a
strictly better version of that, almost for free, because replaying under our
recorded-input oracle removes the world drift that makes Hermit's live double-run
ambiguous. Record, then immediately replay: any divergence is a capture gap, localised
to a step by machinery we already have.** That is the top recommendation. Second,
Landlock could make the egress fence enforceable by the kernel rather than by the
cooperation of the program — worth a spike, but it arrives with a warning attached, in
the form of a shipped sandbox that reported success while enforcing nothing. Third,
this scan looked for evidence against constraint C1 (zero-code capture) and found the
opposite, sharpened: the ceiling is an x86 instruction, not an effort budget.

## What survived

**`2026-08-19-verify-by-double-execution` — PROTOTYPE.** Hermit does not trust its own
interception layer and ships a self-check. We have the same problem and better
machinery for it: `Mode::Replay` already serves mediated inputs from the recording and
`DivergenceKind` already localises a mismatch. What is missing is running that check at
the one moment its result is evidence about the *recording* — immediately after
recording, before anyone trusts it — and printing a sentence that says so. This is
complementary to issue #29, not a substitute: `doctor` finds the holes we thought of,
this finds the ones we did not.

**`2026-08-19-kernel-enforced-capture-boundary` — INVESTIGATE.** Our egress fence is a
monkeypatch and its own docstring lists what it cannot see: subprocesses, C extensions,
connections established before it went up. Landlock is unprivileged, inherited across
`exec`, and unremovable, which closes all three. It could also turn out-of-workspace
writes from an undetected event into a syscall-level refusal — roadmap item two. The
blocker is not the mechanism, it is platform honesty: Linux-only, kernel-version
dependent, and macOS has no equivalent.

**`2026-08-19-silent-best-effort-sandboxing` — IGNORE, but binding.** NVIDIA's
OpenShell shipped Landlock confinement that silently enforced nothing because its
`BestEffort` mode converted every setup failure into `Ok(())`. Landlock's own
documentation is what recommends best-effort handling — sound advice for a hardening
layer, catastrophic for one someone relies on. This becomes an acceptance criterion on
the spike above, not a separate piece of work.

**`2026-08-19-process-determinism-ceiling` — IGNORE.** Filed because a confirmed
decision with a citation is worth more than a confirmed decision without one.

## Looked at, not pursued

- **Antithesis / "demonic nondeterminism" (Cockroach Labs).** Surfaced, not opened.
  Deterministic simulation testing is adjacent and probably relevant to branching
  strategy rather than to capture. Next pass.
- **ML reproducibility survey (arXiv 2302.12691).** Surfaced, not opened. Likely
  terminology and taxonomy value rather than mechanism.
- **NixOS packaging request for Hermit.** Only an activity signal; folded into the
  landscape entry.
- **`stillness`** — read, but it is a personal project without a public repo link in
  the material I read; its value here is the writeup, which is cited in
  `2026-08-19-process-determinism-ceiling`.

## Negative findings

- **Hermit is in maintenance mode** and no longer actively developed within Meta.
  The best-resourced attempt at whole-process determinism stopped. *Opportunity for
  us? No — warning.* It confirms the road we did not take.
- **Everyone who tries this serialises or disables threads.** Hermit serialises;
  `stillness` disables threading outright. Our "sequential programs only" limitation is
  not a shortfall, it is where honest systems land. Worth saying in those terms.
- **`rdrand` cannot be trapped** without virtualisation or binary rewriting. There is
  no `prctl`, no control-register bit. This is the concrete form of "not portably
  possible" in C1.
- **Best-effort sandboxing is the industry default and it fails silently.** Two shipped
  "fixes" (OpenShell #599, #677) did not fix the bug, which is what happens when the
  failure state is invisible from outside.

## What we now know that we did not

1. There is a cheap, mechanism-free way to detect capture gaps we did not anticipate,
   and we are one flag away from it.
2. Our replay oracle is not just a reconstruction device — it is what makes a
   differential capture-gap test *unambiguous*, which Hermit's cannot be.
3. C1's real reason is instruction-level and citable, not a judgement call.
4. The cooperative fence has a kernel-enforced alternative on Linux, and the standard
   way of adopting it would violate our own first rule.

## Still unknown

- What is the false-positive rate of an immediate post-record replay on real programs?
  Only the prototype answers this.
- Can a Landlock ruleset be applied between fork and exec in `engine.rs`'s spawn path
  while keeping our Unix socket and loopback reachable? Unverified.
- Is there any macOS mechanism with comparable properties, or is platform-conditional
  enforcement the only honest option?
- Deterministic simulation testing (Antithesis, TigerBeetle, FoundationDB) is
  unexamined and is the most likely source of ideas for *branching* rather than
  capture. That is the next scan.

# Recommended Actions

### 1. Add `noidroid run --verify`: replay the recording you just made, and report divergence as a capture gap

**Why now:** the milestone is "earn the claim", and every current mechanism for it is
enumerative — `--auto` refuses on holes it knows about, and #29 probes surfaces we
listed. Neither can find an unknown hole. This can, and the machinery already exists;
what is missing is calling it at the right moment and changing the sentence it prints.

**Impact:** 3 — turns "the first replay, weeks later, tells you the recording was never
faithful" into "the recording tells you before you trust it". That is the difference
between a loud failure and a silent one, on the project's own terms.
**Relevance:** 3 — directly reconstruction fidelity and capture honesty.
**Feasibility:** 3 — `Mode::Replay` and `DivergenceKind` exist; no on-disk format
change, so no `STEP_VERSION` question. A flag plus a report path.
**Novelty:** 3 — no equivalent capability today. `Command::Verify` is a different
thing (it re-hashes stored objects to detect tampering on disk).
**Score:** 81

**Cost:** one PR. The risk that would blow it up is the false-positive rate on real
programs — if ordinary recordings routinely fail to re-derive, the check is noise and
must stay opt-in. Measure before deciding the default.

**What we would learn:** whether our own examples record faithfully. The answer could
genuinely be "no", and that would be the most valuable result available from a single
PR. A deliberately clock-reading variant of `examples/flight_agent` should localise to
the offending step; if it does not, we have learned where the check is blind.

**Touches:** `crates/noidroid-core/src/engine.rs`, `crates/noidroid-cli/src/main.rs`,
the run report. Complements issue #29, and gives the in-flight environment model (#48)
the operation that actually exercises its `evidence` axis — under `witnessed` grip the
check compares fingerprints, under `none` it honestly reports that nothing can be
verified.

**Evidence:** `2026-08-19-verify-by-double-execution`.

---

### 2. Spike Landlock enforcement of the egress fence, with the refusal path as the acceptance criterion

**Why now:** the fence's own docstring calls its gap "the worst failure this project
has, because it is the silent one", and lists three ways past it. All three are closed
by a mechanism that is stable, unprivileged, and already in every supported Linux
kernel. It is also a prerequisite for roadmap item two.

**Impact:** 3 — makes an existing claim true rather than cooperative, and opens the
path to detecting out-of-workspace writes.
**Relevance:** 3 — capture honesty; roadmap item two.
**Feasibility:** 1 — Linux-only, ABI-version dependent, no macOS equivalent, and the
change belongs in the Rust spawn path rather than the Python client. Cross-cutting.
**Novelty:** 3 — nothing in `crates/` touches Landlock, seccomp or namespaces.
**Score:** 27

**Cost:** a timeboxed spike, not a PR. The risk is scope: it must not turn into general
sandboxing, which is not our business.

**What we would learn:** whether platform-conditional enforcement can be reported
honestly. The acceptance criterion is **not** that the fence works on Linux — it is
that on a kernel below the required ABI, or on macOS, the tool *refuses to claim the
fence* rather than degrading quietly. If that path is not clean, the answer is "not
yet", and that is a fine outcome.

**Touches:** `crates/noidroid-core/src/engine.rs` (spawn path),
`clients/python/noidroid/fence.py` (becomes the fallback, not the mechanism),
`noidroid doctor` (#29) as the reporting surface.

**Evidence:** `2026-08-19-kernel-enforced-capture-boundary`, constrained by
`2026-08-19-silent-best-effort-sandboxing`.

---

## Explicitly not recommended

- **Do not pursue process-level determinism, in any form** — ptrace/seccomp
  interception, deterministic scheduling, whole-process record/replay. C1 is not just
  intact, it now has a mechanism-level citation: Meta built the best version of this,
  reached 3–6× overhead with a per-program compatibility matrix, and put it in
  maintenance mode; the residual hole is `rdrand`, which cannot be trapped without
  virtualisation or binary rewriting. If this is proposed again, it must clear that
  bar. *(`2026-08-19-process-determinism-ceiling`)*
- **Do not add a `best_effort` mode to any capability probe**, however strongly the
  upstream documentation recommends one. A fence that reports itself installed while
  enforcing nothing is worse than the cooperative fence we have, which never claimed
  more than Python sockets. Probes report an achieved *level*, never a boolean, and the
  level travels with the trajectory — the pattern `Trajectory::allow_gaps` already
  uses. *(`2026-08-19-silent-best-effort-sandboxing`)*
- **Do not make `--verify` the default until the false-positive rate is measured.**
  A check that cries wolf on ordinary recordings would train users to pass
  `--no-verify`, which is worse than not having it.
- **Do not treat `--verify` as a substitute for `noidroid doctor` (#29).** They find
  disjoint classes of hole — enumerated versus unanticipated. Shipping one and closing
  the other would be a mistake.
