---
description: Run the technical scout — research the outside world for something that should change what Paranoid Android builds next.
argument-hint: [research question, or blank for a full "what should we build next?" sweep]
allowed-tools: Agent, Read, Bash, Glob, Grep
---

Dispatch the `scout` sub-agent (`.claude/agents/scout.md`) on this research question:

**$ARGUMENTS**

If the question is blank, run the broadest form: *"What should Paranoid Android build
next?"* — sweep every active category in `research/taxonomy.md`, weight by the current
roadmap in `README.md` and the open issues, and end with a ranked build recommendation.

Pass the scout these instructions:

- Read `research/CONTEXT.md`, `research/constraints.md`, `research/decisions.md` and
  `research/README.md` before searching, and verify architectural claims against
  `crates/noidroid-core/src/` rather than trusting the summary.
- Deduplicate against existing cards before writing anything new; update rather than
  duplicate.
- Write only inside `research/`. Never touch `crates/`, `clients/`, `examples/` or
  top-level docs.
- Produce intelligence cards in `research/discoveries/`, landscape entries in
  `research/landscape/`, and one scan report in `research/scans/` ending with a ranked
  **Recommended Actions** section and an **Explicitly not recommended** section.
- Update `research/README.md` and, if a new category opened, `research/taxonomy.md`.

When the scout returns, relay its briefing to me directly — what it investigated, what
survived, what it means for our architecture, what it recommends, and what it
recommends against. Do not just tell me which files it wrote. Then stop: do not act on
the recommendations without being asked.
