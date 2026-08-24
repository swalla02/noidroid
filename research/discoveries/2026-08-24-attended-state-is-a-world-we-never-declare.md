---
id: 2026-08-24-attended-state-is-a-world-we-never-declare
title: The inference session is a stateful world with opaque grip, and no recording declares it
discovered: 2026-08-24
updated: 2026-08-24
categories: [capture honesty, environment reconstruction / hermeticity, state reconstruction, computer-use agents]
class: RESEARCH
recommendation: INVESTIGATE
transferability: MEDIUM
novelty: MISSING
confidence: MEDIUM
touches: [env, engine, cli, clients]
---

## Discovery

"Aborted but Not Forgotten" (arXiv 2608.15939, 16 Aug 2026) formalises **rollback
consistency**: a believed-complete abort must restore the state the model *attends*, not
merely the application's transcript. When a serving session retains its KV cache across a
logical rollback — as it does whenever an application reuses a session handle or a cached
`past_key_values` — the model keeps attending to a branch the application deleted. Across
seven open-weight families the retained KV alone flipped a typed protected effect in
**25 of 63 audited cells while the attacker tokens were provably absent from the served
request in all 63**, and it reproduced *inside LangGraph's first-class time-travel API*,
where a verified logical rollback still left the KV stale.

The consequence for us is not that attack. It is the category: **the inference endpoint is
a world in the sense of `docs/environment-model.md`, and nothing in our tree declares it as
one.**

## Source

- Primary: <https://www.alphaxiv.org/abs/2608.15939> — read the abstract, introduction,
  threat model and the same-token/different-cache audit design (§1–2) in full text via
  `orx paper`. I did not read the appendices or the per-model tables.
- Primary, ours: `crates/noidroid-cli/src/doctor.rs` (the full list of what a recording
  would not cover), `clients/python/noidroid/llm.py`, `crates/noidroid-core/src/env.rs`.

## What is interesting

The methodological core is the part worth stealing, independent of the security framing.
Their **same-token / different-cache audit** holds the decision-step tokens *token-identical*
across two arms and varies only the cached prefix: `stale` (KV retained across the abort)
versus `fresh` (KV rebuilt from the committed transcript). Anything that differs between
the arms is attributable to retained state alone, and they verify per cell that the carrier
token is absent from the fed tokens. That is a controlled-variable experiment over an
invisible state channel — the same shape as our own "change one thing and branch", applied
to a layer nobody instruments.

Their scope note is what makes this precise rather than alarmist, and it is the sentence
that decides whether we are exposed:

> A content-addressed automatic prefix cache (e.g. vLLM's) reuses only prefixes actually
> present in the request and never re-injects removed tokens, so it is exempt. Our claims
> are scoped to retained-handle reuse.

So the hazard is **retained-handle** serving — session handles, continued-generation APIs,
`past_key_values` in a local transformers loop — not stateless `messages=[...]` calls.

Their fix is also instructive: a **transaction-local cache restore** (rebuild from the
committed transcript) closes every cell, while a *global flush or full restart does not*
help more and costs more. Rebuilding from the committed history is precisely C2's
deterministic prefix, arrived at independently, for the layer below us.

## Why it matters to Paranoid Android

Our environment model's whole argument is: *name the worlds you cannot capture, and say
what holding a state address entitles you to.* `grip` is `captured` / `witnessed` /
`opaque`; §12 has a six-environment conformance table.

The one world that **every single noidroid recording touches** — the model provider — is
absent from that table and absent from the code. `clients/python/noidroid/llm.py` wraps the
call as an ordinary `nd.call("model.complete", ...)`; it never calls `session.observe(of=...)`,
so no world is declared, no fingerprint is taken, and the run report has no row for it.
`grep -rn "previous_response_id\|past_key_values" clients/` returns nothing — we have no
notion that a model call might carry a handle at all.

For `Mode::Record` and a pure `Mode::Replay` this is genuinely harmless, and I want to be
clear about that: a pure replay serves the recorded completion and never contacts the
server, so there is no attended state to be stale. **The exposure is `Replay { live: [...] }`,**
the flagship hybrid — and it is exposed in the mirror-image direction to the paper's:

- The paper's agent has KV the transcript does not.
- Our live replay has the opposite: steps 0..k are served from the recording and *never
  sent to the server*, so a retained session handle held by the client has KV that reflects
  none of the replayed prefix. The client's transcript is verified by hash; the server's
  attended state is not merely unverified, it was never built.

If the client is doing stateless completions, this is a non-issue and the honest answer is
one sentence saying so. If the client holds a handle, our verified prefix sits on top of an
unverified — possibly empty, possibly stale from a previous branch — attended state, and
the run report calls the reconstruction faithful. That is the `unverified-world-redrive`
shape (2026-08-19) in a new place: we print that the prefix reproduced exactly, and the
sentence is true about the only layer we look at.

`noidroid doctor` is the natural home for the answer. It already enumerates clock,
randomness, subprocesses, the egress fence and platform under "what a recording made now
would and would not cover" (#75). "Whether your model client holds a server-side session
handle" belongs on that list and is not on it.

## Transferability

**MEDIUM.**

What transfers cleanly: the *category*. Adding a row to §12's conformance table for the
inference endpoint, and a `doctor` check for retained-handle usage, is a documentation and
reporting change that needs no new mechanism — `Session.observe` already exists.

What does not transfer: the fix. We cannot rebuild a provider's KV cache, and we should not
try; for a hosted API the honest grip is `opaque` (we hold nothing, we cannot detect a
difference, we cannot put it back) and the only correct action is to say so. For a
self-hosted vLLM the paper says we are exempt, which is a fact worth writing down rather
than a capability to build.

What is uncertain: whether real noidroid users hold handles at all. Our own `llm.py` and
`--proxy` path are message-list shaped. The Responses API's `previous_response_id`, Gemini
context caching, and any local `transformers` loop are the cases where this bites, and I do
not know how common they are among the programs people actually record.

## Novelty

**MISSING**, verified by grep rather than memory. No world is declared for the model
anywhere: `clients/python/noidroid/llm.py` contains no `observe`; `doctor.rs`'s check list
has no entry for serving-session state; `docs/environment-model.md` §12's table has no row
for an inference endpoint. The concept we need (`grip`, `Situation`, `observe`) shipped in
0.3.0 — this is an unfilled row in a table we already built, not a new abstraction.

## Limitations and negative signal

Arguing against myself, hard, because this is the kind of finding that flatters our
architecture:

- **The realistic exposure may be zero.** If every recorded program uses stateless
  completions, the correct output of this card is one sentence in `doctor` and nothing
  else. I have not measured which shape our users use, and there is no user data to
  measure.
- **The paper's harm story is a security one we do not inherit.** Their threat model needs
  an attacker placing content in a branch that gets abandoned. Our value is a truthful
  report, not a defended boundary; the transferable part is the *invisible-state* claim,
  not the exfiltration.
- **This is not a reason to capture the model server.** Capturing provider-side KV is not
  possible for a hosted API and is C1 territory for a self-hosted one. The finding is
  "declare it", not "capture it".
- **It arguably lands near C2 and does not dent it.** If anything the paper corroborates
  C2 — their sufficient fix is "rebuild from the committed transcript", i.e. a deterministic
  prefix, chosen over a snapshot restore *and over a full restart*. Score novelty 1 on that
  reading and 3 on the undeclared-world reading; I claim the latter, and the former is worth
  recording as confirmation.
- I read §1–2 and the abstract, not the full evaluation. The 25/63 figure is theirs,
  unverified by me.

## Recommendation

**INVESTIGATE** — one question, answerable in an afternoon, before any code.

## Proposed action

Answer: **does any supported client path hold a server-side session handle across steps?**
Enumerate the four shapes — `llm.py`'s `model.complete`, `--auto`'s `sitecustomize` hooks,
`--proxy`'s intercepted request bodies, and a hand-written client — and for each say whether
the request is self-contained (`messages=[...]`, exempt per the paper's own scope) or
handle-carrying (`previous_response_id`, cached-content handles, local `past_key_values`).

Then, depending on the answer:

- **All self-contained** → add one line to `doctor` saying so, and a row to
  `docs/environment-model.md` §12 recording the inference endpoint as a world with `opaque`
  grip whose statelessness is what makes reconstruction sound. Cost: a paragraph. This is
  the likely outcome and it is still worth having, because right now the guarantee is
  accidental rather than stated.
- **Any handle-carrying path exists** → `doctor` must warn on it, and `--live` on that
  target should declare the session as an `opaque` world in the run report via
  `Session.observe`, so a live replay stops claiming a clean prefix over a state it never
  built.

## Confidence

**MEDIUM.** HIGH that we declare no world for the model — that is grep. MEDIUM on the
consequence, because it is conditional on a client shape I have not surveyed, and because I
read part of the paper rather than all of it.

## Evidence

- Primary: <https://www.alphaxiv.org/abs/2608.15939> — establishes that logical rollback
  and retained serving state can disagree invisibly, with a controlled audit, and scopes it
  to retained-handle reuse.
- Supporting: `2026-08-19-unverified-world-redrive` — the same failure shape (we print that
  we checked a world we did not) in four other domains.
- Supporting: `crates/noidroid-cli/src/doctor.rs` — the coverage list this belongs on.
- Counter-evidence: the paper's own exemption for content-addressed prefix caches, which
  may cover every client we actually have.

## Changelog

- 2026-08-24 — created, from the computer-use rollback scan.
