---
name: AgentENV (kvcache-ai / Moonshot)
class: INFRASTRUCTURE
first_seen: 2026-08-21
updated: 2026-08-21
url: https://github.com/kvcache-ai/AgentENV
licence: MIT (per launch coverage; not verified against the LICENSE file this run)
activity: active — open-sourced 2026-07, powers Kimi K3 agentic RL training
---

## What it is

A distributed platform for running agent environments at RL-training scale. Each
environment is a Firecracker microVM rather than a container, so isolation is
kernel-level, and the platform's selling point is state transition speed: snapshot,
resume, pause and **fork**.

## How it works

From the README and launch material:

- Snapshots capture memory and filesystem changes **incrementally** rather than writing a
  full image, and complete "in under 100 ms even under heavy disk modification".
- Boot or resume from a snapshot in under 50 ms; pause in under 100 ms.
- A running environment can fork into up to **16 independent children on the same node**.
- Snapshots go to S3-compatible object storage or a shared distributed filesystem.
- OCI-compatible images are loaded on demand via overlaybd, scaling to 1.5 million images
  in production.
- The HTTP API is E2B-compatible, so existing E2B SDK code runs unchanged. The CLI is
  `start / pause / resume / delete / exec` plus timeout management.

## What it does that we should learn from

Fork is a first-class verb on the environment, not something bolted on for debugging, and
the performance target is set by the RL rollout loop rather than by a human's patience.
That is a useful corrective: our checkpoint's cost model is "re-execute the prefix", and
the number it will be measured against by anyone in this space is 50 ms.

The E2B-compatible API is also worth noting as a strategy — they made the substrate
swappable by adopting somebody else's interface rather than publishing a new one.

## Where it is weaker, and why that is interesting

The README describes what a snapshot **captures** and never describes what it **loses**.
There is no statement about open network connections, the guest clock, entropy sources,
or state held in services outside the VM; no statement about determinism; and no
procedure for checking that a resumed or forked VM is the state it claims to be. A
microVM fork carries a great deal, which is exactly why the residue is easy to stop
thinking about — and the residue is where a forked RL rollout would silently diverge from
its sibling.

This is the same shape as `2026-08-19-unverified-world-redrive`: the mechanism is
excellent and the verification is absent.

## Overlap with us

Almost none in method, and directly competing in purpose. They snapshot the machine; we
re-execute the prefix (C2). They serve an agent handed a raw VM, where nothing routes
through a mediation layer, so C1 puts that population out of our reach entirely. Where we
meet is the claim: both of us offer "return to an earlier point in an execution and
branch". Theirs is faster by two orders of magnitude and unverified; ours is verified by
hash equality and confined to mediated environments.

The useful framing for us is not competition but division: AgentENV is what "reach"
looks like when it is bought with infrastructure, and it has no answer on "evidence" — the
second of the three questions `checkpoint.rs` makes a checkpoint answer.

## Watch triggers

- Any documentation of fork residue: what does not survive, and how a caller finds out.
- A determinism or verification feature. That would be the strongest possible evidence
  that verified forking has a real user, and also the moment our angle narrows.
- Adoption by `verifiers`/`prime-rl` as a runtime, which would make it the default
  substrate for open agentic RL and set the performance bar in public.

## Changelog

- 2026-08-21 — created. Read the README and the launch write-ups; **did not read the
  source**, so every mechanism claim here is theirs, not verified. Confidence in the
  timing numbers is low; confidence in the *absence* of a verification story is higher,
  because absence in a README is directly observable.
