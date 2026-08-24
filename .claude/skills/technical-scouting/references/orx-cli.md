# `orx` — structured paper search

`orx` (OpenResearch CLI, `orx --help`) is installed on this machine. Two of its
primitives are a direct upgrade over WebSearch for the "Research" source class in
`sources.md`: structured, no-login, deduplicable, and they surface a paper's most-
starred associated repo automatically.

Everything else `orx` ships — the local experiment tree (`orx up`/`project`/`exp`/
`create-experiment`), remote compute (`instance`, `compute`), and `orx agent spawn`
— is for running and branching ML training experiments on provisioned compute. That
is not this project's workload; do not reach for it. Use only `discover` and `paper`.

## The two commands

```bash
orx discover keyword   "<exact terms>"        # alphaXiv full-text BM25 + match snippets
orx discover embedding "<semantic question>"   # alphaXiv title/abstract semantic search
orx discover openalex  "<query>"               # cross-discipline scholarly graph, citations
orx discover biorxiv   "<query>"               # biology preprints, via OpenAlex's index
orx paper <id>                                 # fetch a report or full text by arXiv id/URL, DOI, or OpenAlex W…id
```

No login required. Every `discover` call is one request and returns structured JSON:
`source`, self-routing `id`, `title`, `abstract`, `publicationDate`, plus alphaXiv
votes/full-text snippets or OpenAlex/bioRxiv citation counts where available.
`orx paper <id>` auto-detects the source from the id, returns a compact report by
default, falls back to extracted full text if no report exists, and — when alphaXiv
has one — prints the paper's most-starred associated GitHub repo (`GitHub: <url>`).
That repo can be a general framework rather than the paper's own implementation;
sanity-check it before treating it as the code.

Useful flags on every `discover` subcommand: `--published-after` / `--published-before`
(`YYYY-MM-DD`, inclusive), `--prioritize {default,recency,historical,popular}`,
`--limit`. Never invent a date bound the question didn't ask for — an empty or thin
result under a narrow window is not evidence the literature doesn't exist; say so and
widen or drop the bound rather than re-running the identical query.

## When to reach for it vs. WebSearch

- A paper, benchmark, method, author, or venue is the actual object of the search →
  `orx discover`, not WebSearch. It is built for exactly this and returns abstracts
  you can screen without a fetch.
- Use `keyword` for exact terms — method names, acronyms, benchmark names, title
  phrases. Use `embedding` for a fuzzier description of the idea. Use `openalex` when
  the work is likely outside arXiv (journals, older CS, cross-discipline) or you need
  citation context. Use `biorxiv` only for biology/life-science preprints.
- Read a candidate with `orx paper <id>`, not a WebFetch of the arXiv abstract page —
  it gets you the report/full text and the associated repo in one call.
- Still use WebSearch/WebFetch for everything `orx` doesn't cover: repos, issue
  trackers, engineering blogs, release notes, HN/Reddit threads, design docs. Papers
  found this way still get read with `orx paper` when the id resolves.

## Folding it into the scout pipeline

- **Sweep.** Run `keyword` and/or `embedding` per the guidance above; add `openalex`
  for broader coverage. Screen abstracts before opening anything — this is candidate
  generation, not the primary-source read.
- **Dedup, by id first.** Before adding a paper to the candidate set, check it isn't
  already a card: match `id` exactly, then a DOI/arXiv id visible in the metadata,
  then normalized title. If a full-text alphaXiv match and an OpenAlex match are the
  same arXiv paper, prefer the alphaXiv one — `orx paper` gets you full text from it.
  This is in addition to, not instead of, the existing `grep -ril` dedup in `SKILL.md`
  against `research/`.
- **Primary-source read.** `orx paper <id>` satisfies "read the primary source, not
  the README" for a paper candidate — read the report/full text before writing
  anything down. A card built on a `discover` abstract alone is a LOW-credibility card
  by `sources.md`'s grading; open it with `orx paper` before grading MEDIUM or HIGH.
- **Follow the graph.** OpenAlex results carry citation context — use it the same way
  `SKILL.md` already asks you to follow what a project cites and who cites it.

## Citing results

Link every alphaXiv/arXiv result to `https://www.alphaxiv.org/abs/<versionless-id>`,
never a bare `arxiv.org` link. Link a DOI to `https://doi.org/<doi>`. Link a bare
OpenAlex `W…` id to `https://openalex.org/<id>`. Don't compare alphaXiv votes against
OpenAlex citation counts — they measure different things; treat topical fit as the
only cross-source ranking signal.

## Retrieval budget

Don't loop indefinitely. Estimate how hard the query is (1–10): 1–3 gets no follow-up
round, 4–7 gets one, 8–10 gets two. A follow-up round targets one concrete missing
angle — a specific acronym, method, benchmark, or subtopic — not a rephrase of the
same query. If initial results already give solid topical coverage, stop and move to
the primary-source reads; fast-and-slightly-incomplete beats an exhaustive sweep.
