---
id: 2026-08-21-reproducibility-bought-by-mocking-the-world
title: Computer-use benchmarks buy reproducibility by replacing the world with a mock
discovered: 2026-08-21
updated: 2026-08-21
categories: [computer-use agents, browser automation, agent evaluation, environment reconstruction / hermeticity, reproducibility, negative-signal]
class: RESEARCH
recommendation: INVESTIGATE
transferability: MEDIUM
novelty: DIFFERENT
confidence: MEDIUM
touches: [clients, engine, cli]
---

## Discovery

Every serious computer-use benchmark and training-environment effort has hit the same
wall — an episode against the real web is not reproducible — and every one of them has
taken the same exit: **stop using the real world**. OSWorld 2.0 ships "hosted mocked
websites" as a managed service pinned to a release tag. CUA-Gym synthesises
"CUA-Gym-Hub, a broad suite of high-fidelity mock web applications" to get deterministic
rewards for RLVR. WebArena was mocked from the start — self-hosted Reddit, GitLab, a
shopping CMS. The mock is not a convenience; it is load-bearing, and it is the field's
entire answer to environment reconstruction.

The cost is stated nowhere and is obvious: the reproducible episode is an episode against
a world that does not exist.

## Source

Primary:
- `xlang-ai/OSWorld-V2` repository front page — release-pinning rules ("Do not mix
  releases or replace a pinned tag with `main` or `latest`"), hosted mocked website
  service at `site.hku.icu` with the previous release still served at `web.hku.icu`,
  self-hostable via `Task-Web/OSWorld-web`.
- `xlang.ai/blog/osworld-verified` — the OSWorld-Verified write-up: 300+ issues over 15
  months, categorised.
- `xlang-ai/OSWorld` issue #382 and the maintainer's reply.
- `web-arena-x/webarena` issue #206.
- arXiv 2605.25624 (CUA-Gym) abstract and method summary.
Secondary (not opened at source): the WebAgent-R1 description of operating four AWS
servers with manual restarts.

## What is interesting

**The 300+ issues have a shape.** OSWorld-Verified's own categorisation is: anti-crawling
and bot detection; network/IP/geo restrictions; dynamic website structure changes; URL
parameter changes; load-time and response-time sensitivity; task ambiguity; and
infrastructure. Strip the ambiguity row and every remaining category is *the world moved
between the day the task was written and the day the agent ran it*. One quoted example:
"speedtest.net CSV export exists before while now be deleted" — a task that was correct
and became infeasible. Another: "lazy loading and encoding changes" on a car-rental site.
This is drift in the sense `research/taxonomy.md` means it, at benchmark scale, and the
mitigation was fifteen months of human labour. The write-up says it plainly: "providing
reliable rewards consumes more human resources than we imagined."

**The headline number is one run.** In issue #382 a user reports being unable to
reproduce the published Claude-4-Sonnet OSWorld score and calls reproducibility "extremely
inconsistent". The maintainer's answer is direct and worth quoting because it is the state
of the art, not a lapse: "I unfortunately did not have enough compute resources to perform
a large number of repeated evaluations, so I cannot provide a statistically tight estimate
of variance across many trials. I can only confirm that the score reported on the
leaderboard corresponds to one full evaluation run without issues." The field's flagship
computer-use metric is a single sample with no variance estimate, and the reason is that
one full evaluation run is expensive enough that nobody does it twice.

**Reset has no cheap form.** WebArena issue #206 asks for a faster way to reset the
websites than stopping and restarting the Docker containers. It has been open with **zero
replies**. The answer is that there is not one, and the downstream consequence appears
whenever anyone tries RL on it: a handful of parallel sessions, manual server restarts,
and reset between tasks to avoid cross-task interference.

**The mock resolves drift and creates a different unknown.** A pinned mock is
reproducible by construction — but nothing then tells you whether the behaviour you
measured transfers, and the benchmark's own reproducibility claim now depends on a hosted
service (`site.hku.icu`) staying up and staying pinned. Reproducibility has been converted
from a property of the recording into a property of someone else's uptime.

## Why it matters to Paranoid Android

Our browser adapter takes the third option, and this is the first evidence I have found
that the third option is unoccupied.

`clients/python/noidroid/browser.py` records the real page's HTTP responses and re-serves
them as the oracle during reconstruction, re-drives recorded actions into a fresh
Chromium, and compares a page digest against the recording. Where the mock says "use a
world I control", we say "use the world as it was on the day, and tell me when I cannot
put it back". Concretely, against the categories above:

- Anti-crawling, geo-blocks, rate limits: irrelevant on reconstruction — the responses
  come from the recording, not the network.
- DOM and URL drift: the recorded episode is unaffected; it re-drives to the same digest
  or reports that it did not.
- Load-time sensitivity: still ours to get wrong, and the honest place to say so.
- A page the recording never visited: refused by default, reported `unknown`, opt-in via
  `allow_network` — which is the `--simulate` discipline applied to a browser.

Two things follow that are worth building on. First, the **unreproducible page is
currently non-fatal by default** (`strict=True` is opt-in), and after #52 the run-level
report can now name a world nobody re-drove. Those two facts belong together: the browser
adapter's `_reconstruct` re-drives and verifies, but issue #53 question 5 notes nothing
tests that *removing* the re-drive changes the report, and the browser has no
`REFERENCE_MUTE` equivalent. Second, a recorded browser episode plus a page digest is a
**reproducible computer-use episode against the real web**, which is the object OSWorld
spent fifteen months failing to manufacture — but it is one episode, not a benchmark, and
we should be careful about how loudly we say this.

## Transferability

MEDIUM. The mechanism transfers to us as validation rather than as code: we already do
the thing, and this is evidence that the alternatives are worse in a specific, cited way.
What does not transfer is scale. OSWorld runs 361 tasks against a full desktop, not a
page; our recorded-response oracle covers the network but not the ambient OS, the clock,
installed package versions, or subprocesses (#30, #31), which are exactly what a desktop
task depends on. Claiming our approach solves OSWorld would be dishonest; claiming it
solves the browser slice is defensible today.

## Novelty

DIFFERENT. The field's approach (freeze a fake world) and ours (record the real one and
serve it back) are not versions of each other, and the comparison is the value. Verified
against our code: the recorded-network oracle exists (`browser.py` `_REWRITTEN_HEADERS`,
the per-`(method,url)` key, the `net/` directory under the workspace), and the page digest
comparison exists in `_reconstruct`. Nothing here is a capability we lack; what we lack is
any evidence that anyone outside would want it, which is what this card is for.

## Limitations and negative signal

- **I could not measure the drift rate.** The OSWorld-Verified categories are the
  authors' own summary; I did not read the 300+ issues, and the blog does not quantify how
  much scores moved. Confidence is MEDIUM for this reason.
- **Our own limitation is the same one they hit, one level down.** A real page carries
  clocks, ads and session tokens, so an exact digest match is rare — which is why the
  adapter's default is to carry on and mark downstream `unknown`. That is the honest
  design, and it also means our reproducibility claim over a real page is weaker in
  practice than the sentence above implies. A mocked world genuinely does hash-match.
- **The mock is winning for a reason.** Deterministic rewards are what RLVR needs, and a
  recording cannot supply a *new* episode — only re-serve an old one. Anywhere the
  requirement is "generate fresh diverse rollouts", the mock beats us outright and we
  should not argue. Our claim is confined to re-entering an episode that already happened.
- WebAgent-R1's four-server, manual-restart account is second-hand; I did not open it.

## Recommendation

INVESTIGATE — one question, answerable in a day: on a recorded real-web browser episode,
what fraction of re-drives reproduce the page digest exactly, and what is in the ones that
do not? That number decides whether "a reproducible computer-use episode against the real
web" is a claim we can make at all.

## Proposed action

Take the existing browser example, record N episodes against a handful of real sites,
reconstruct each, and tabulate: exact digest match / mismatch with the diff / page not in
the recording. Then, separately, add the browser equivalent of `REFERENCE_MUTE` — an env
var that suppresses `Browser._reconstruct`'s re-drive — and assert in a test that the run
report changes. That closes issue #53 question 5 and costs almost nothing.

## Confidence

MEDIUM. Repository front pages, the OSWorld-Verified blog, and two issue threads read
directly; the CUA-Gym and WebAgent-R1 claims come from abstracts and search summaries, not
full texts. The OSWorld #382 quotation is verbatim from the GitHub API. I did not open
WindowsAgentArena, AndroidWorld, or any successor benchmark, so "every serious effort"
is an overreach supported by three instances.

## Evidence

- Primary: `xlang-ai/OSWorld` issue #382, maintainer reply — leaderboard score is one run,
  no variance estimate.
- Primary: `xlang.ai/blog/osworld-verified` — 300+ issues, drift taxonomy, human cost.
- Primary: `xlang-ai/OSWorld-V2` — release pinning and hosted mocked websites.
- Primary: `web-arena-x/webarena` issue #206 — no fast reset, unanswered.
- Supporting: arXiv 2605.25624 (CUA-Gym) — mock web applications for deterministic RLVR.
- Counter-evidence: mocked worlds hash-match exactly and generate fresh episodes; our
  recording does neither.

## Changelog
- 2026-08-21 — created.
