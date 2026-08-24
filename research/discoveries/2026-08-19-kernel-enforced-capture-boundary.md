---
id: 2026-08-19-kernel-enforced-capture-boundary
title: Landlock could make the egress fence and the workspace boundary real instead of cooperative
discovered: 2026-08-19
updated: 2026-08-21
categories: [sandboxing, interposition / interception, capture honesty, process isolation]
class: INFRASTRUCTURE
recommendation: INVESTIGATE
transferability: MEDIUM
novelty: MISSING
confidence: MEDIUM
touches: [engine, cli, clients]
---

## Discovery

Landlock is an unprivileged, stackable Linux LSM that lets a process irrevocably
restrict its own filesystem and network access, with no root and no system-wide
policy. Its capabilities are versioned as ABIs: ABI 2 added rename/link control, ABI 4
added TCP connect/bind restriction, ABI 6 added abstract Unix sockets and signals,
ABI 7 added *logging of Landlock audit events*.

## Source

- Primary: <https://docs.kernel.org/userspace-api/landlock.html> — the userspace API,
  ABI history, and the best-effort compatibility guidance.
- Primary: <https://landlock.io/> — project overview.
- Supporting: <https://man7.org/linux/man-pages/man7/landlock.7.html>

## What is interesting

The relevant property is not "sandboxing", which we do not need. It is that Landlock
restrictions are **self-imposed and unremovable, and they are enforced by the kernel
rather than by the cooperation of the program being restricted.** A child process
inherits them. A C extension cannot route around them. Code that never imported our
module is still subject to them.

ABI 7's audit logging matters as much as the enforcement: a denial becomes an
observable event rather than a bare `EACCES` the program may swallow.

## Why it matters to Paranoid Android

Two of our loudest honesty gaps are gaps of *mechanism*, not of intent.

**1. The egress fence is cooperative.** `clients/python/noidroid/fence.py` says so
itself, and the docstring is admirably direct: it "cannot see subprocesses (a child
does not inherit the patch), C extensions that bypass Python's socket module, and
anything already connected before the fence went up". The file also states why this is
the worst gap we have — a replay that reached the network finishes and reports itself
faithful. A Landlock network ruleset (ABI ≥ 4) applied to the child before `exec`
survives all three of those cases, because it is not a patch.

**2. Out-of-workspace writes are neither captured nor detected.** README limitation
three, and roadmap item two ("detecting unmediated effects beyond the workspace"). A
Landlock filesystem ruleset granting write access to the sandboxed workspace and
nothing else converts an undetected write into a refusal at the syscall, which is
precisely the trade this project says it prefers: a loud refusal over a silent gap.

Note the pleasing fit with our architecture: we do not want to *capture* these effects,
we want to *forbid* them and say so. Landlock does exactly that and nothing more. It is
not a step toward whole-process determinism — see
`2026-08-19-process-determinism-ceiling`, which argues that road is closed — it is
kernel enforcement of a boundary we already drew ourselves.

## Transferability

**MEDIUM**, and the reasons it is not HIGH are all real:

- **Linux only.** We support Linux and macOS. macOS has no equivalent with the same
  properties (`sandbox_init` is deprecated and its policy language unsupported). So
  this is a platform-conditional hardening, and the honest framing is "on Linux ≥ X we
  can enforce the fence; elsewhere we cannot" — which is a sentence the tool must
  print, not hide.
- **Kernel-version dependent.** The useful network restriction needs ABI 4; the useful
  logging needs ABI 7. ABI must be probed at runtime and the *actual* level reported.
- **Where to apply it.** Enforcement wants to happen between fork and exec of the
  recorded process, i.e. in `engine.rs`'s `Command` spawn path, not in the Python
  client. That is the right place architecturally — it covers every language client,
  not just Python — but it is a change in the Rust core rather than a client tweak.
- Loopback and our own Unix socket must stay reachable; the fence already carves out
  exactly those, so the policy is known.

## Novelty

**MISSING.** We have a fence; we do not have an enforceable one.
`grep -rn "landlock\|seccomp" crates/ clients/` returns nothing — no kernel
enforcement anywhere in the tree. The fence *mechanism* lives entirely in
`clients/python/noidroid/fence.py`; the Rust side only tests its behaviour across the
protocol (`crates/noidroid-core/tests/fence_slice.rs`), which means a Rust-side
enforcement path would sit under the tests that already exist rather than needing new
ones.

## Limitations and negative signal

Serious, and one of them is disqualifying if handled badly — see the companion card
`2026-08-19-silent-best-effort-sandboxing`, which documents a shipped project whose
Landlock "best effort" mode silently returned success while enforcing nothing. That
failure mode is the exact thing this project exists to not do, and it is the default
shape of every Landlock integration guide.

Others:

- A denial surfaces to the program as `EACCES`/`EPERM`, which a program may mistake
  for an ordinary error. We would need to correlate the denial with our own report —
  hence ABI 7 logging, or a pre-flight probe.
- Landlock cannot restrict UDP, nor connections already established.
- It restricts; it does not record. It closes the honesty gap, it does not close the
  capture gap. Say which one we are claiming.

## Recommendation

**INVESTIGATE** — not PROTOTYPE yet. The open question is not whether Landlock works,
it is whether the platform-conditional story can be told honestly without the tool
appearing to guarantee on macOS what it only enforces on Linux.

## Proposed action

Spike, timeboxed, answering one question: **can a Landlock ruleset be applied in the
child between fork and exec in `engine.rs`'s spawn path such that (a) the recorded
process can still reach our Unix socket and loopback, (b) an outbound connection during
replay is denied and attributable to a step, and (c) on a kernel below the required
ABI, or on macOS, the tool refuses to claim the fence rather than degrading quietly?**

Condition (c) is the acceptance criterion, not (a) or (b). If the spike cannot make (c)
clean, the finding is that Landlock is not for us yet.

Report the enforcement level in `noidroid doctor` (#29) as a hard fact — "egress fence:
kernel-enforced (Landlock ABI 7)" versus "egress fence: cooperative, Python sockets
only; subprocesses and C extensions are not covered".

## Confidence

**MEDIUM.** The Landlock API and ABI history are from kernel documentation and are
reliable. What I have *not* verified is the interaction with our spawn path and with
the Unix-socket carve-out under ABI 6+ — that is what the spike is for. No claim here
rests on a source I did not open, but the applicability claim is reasoned rather than
tested.

## Update — 2026-08-21: someone shipped it

Shepherd (`shepherd-agents/shepherd`, MIT, v0.3.0 alpha) enforces per-run writable roots
at the native syscall jail on **both** macOS Seatbelt and Linux Landlock, in a released
package. Its README states the constraints plainly, and they are the answers our spike was
going to have to find out for itself:

- Linux Landlock enforcement runs "in a privileged container" — so the unprivileged story
  is weaker in practice than the ABI docs suggest for their shape of deployment.
- Grants are **whole-profile per binding**: a bound repository is entirely writable or
  entirely read-only. Sub-root / per-path grants are explicitly out of the shipped cut.
- **Windows is refused, not faked**: "enforcement would be advisory-only at best" — use
  WSL. That is exactly the posture `2026-08-19-silent-best-effort-sandboxing` says to
  take, taken by someone else.

Consequence for us: the spike is smaller than scoped, because the coarse-grained
whole-root policy is evidently enough to ship on, and the hard part (per-path grants) is
one a serious project chose to defer. It also raises the stakes — a direct competitor can
now say its boundary is kernel-enforced while `clients/python/noidroid/fence.py` remains
cooperative. See `research/landscape/shepherd.md`.

Not verified: I read their README and concepts docs, not their enforcement code, so
"enforced at the syscall" is their claim. `2026-08-19-silent-best-effort-sandboxing` is
the reason to check it rather than assume it.

## Update — 2026-08-24: the other verb, with accuracy numbers

Crab (HKUST, <https://www.alphaxiv.org/abs/2604.28138>) uses the same kernel surface for
the *other* half of this card. Landlock **forbids**; Crab **detects**, and it publishes the
numbers this card has always lacked.

Their Inspector combines an in-kernel eBPF monitor with a user-space daemon:
`sys_enter`/`sys_exit` tracepoints on filesystem-affecting syscalls made by sandbox
processes, cgroups to delineate which processes are the sandbox's, and kernel soft-dirty
page tracking (`/proc/PID/pagemap`, `/proc/PID/clear_refs`) for in-memory change. It
computes **net** change between checkpoints, so a temp file created and deleted inside one
turn registers as nothing.

Measured, on Claude-code / iFlow-cli / SWE-agent over Terminal-Bench and SWE-Bench:

- Process change detection: **100% accurate, zero false positives, zero false negatives.**
- Filesystem change detection: **98.3% accurate, zero false negatives, 2.3% false positives.**
- Asynchronous, off the critical path; median per-turn Inspector latency 31–72 ms, p95
  under 200 ms.

Two things matter to us. First, the **error direction is the one we require**: zero false
negatives, and a false positive costs an unnecessary checkpoint rather than a missed
effect. That is the acceptance criterion for roadmap item 2 ("detecting unmediated effects
beyond the workspace") written by someone else's evaluation — a detector for us is only
admissible if a miss is impossible and the cost of a spurious hit is bounded.

Second, it reframes the Landlock spike. The two verbs are separable and can ship
separately: *forbid* the egress and out-of-workspace writes (Landlock, this card's original
proposal), or *observe* them and name the step that did it (eBPF, Crab's mechanism). Forbid
is a smaller change and fits our posture better — a loud refusal over a silent gap — but
observe is the only one that works for the `--watch <dir>` mode, where the whole point is
that the program writes to a real project directory we do not own.

Caveat, and it is the same one as for Shepherd: I read Crab's report, not its code. The
accuracy figures are theirs. eBPF also carries a privilege cost that Landlock does not —
Landlock is unprivileged by design, tracepoint-attaching eBPF generally is not — so
"observe" is the option with the worse deployment story even though it has the better
numbers.

## Evidence

- Primary: <https://docs.kernel.org/userspace-api/landlock.html>
- Primary: <https://landlock.io/>
- Supporting: <https://www.alphaxiv.org/abs/2604.28138> — Crab; eBPF change detection with
  a zero-false-negative result, and the fail-safe error direction we would need.
- Supporting: <https://man7.org/linux/man-pages/man7/landlock.7.html>
- Counter-evidence: <https://github.com/NVIDIA/OpenShell/issues/803> — how this goes
  wrong in practice; see `2026-08-19-silent-best-effort-sandboxing`.
- Ours: `clients/python/noidroid/fence.py` (the stated gaps), `README.md` § Limitations
  (items 3 and 4), roadmap item 2, issue #29.

## Changelog

- 2026-08-19 — created.
- 2026-08-21 — updated: Shepherd ships Seatbelt + Landlock enforcement with documented
  scope limits; spike is smaller than originally scoped. Landscape entry added.
- 2026-08-24 — updated: Crab's eBPF Inspector supplies the *detection* half of this card
  with published accuracy (zero false negatives on both filesystem and process change) and
  the fail-safe error direction roadmap item 2 needs. Forbid and observe separated as two
  shippable halves.
