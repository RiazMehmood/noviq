# Design draft — Issue-driven facet expansion (deep search)

**Status:** DRAFT (not built). 2026-07-18. Feature 025.
**Motivation:** the SLGA-2013 "omnibus resolution" case-study audit — normal search pulled 3 wrong
chunks and hallucinated rule numbers; deep search pulled 22 real citations and fixed items 1–4, **but
both missed the two highest-value issues** (conflict of interest — the bidder is the Chairman's
relative; and quorum not maintained — members left before the vote). See
`[[project_deep-search]]` (2026-07-18 section).

## Root cause
`services/deep_search/expand.rs::expand_query` decomposes only the parts the user **explicitly** asked
— by design. That prompt is the `3c9037ad` *faithful-decomposition* fix: it FORBIDS inventing topics
("no 'appeal', 'exceptions', 'definitions' … unless the user wrote them"), because the old "always
produce 4–6 facets" prompt padded questions with manufactured topics that pulled off-domain passages.

So decomposition is **item-driven, not issue-driven**: it split the five enumerated disposal items and
retrieved their property rules, but never generated a facet for the legal issues **implied by facts
buried in the prompt** ("members left before the vote", "a bidder is a relative of the Chairman").
Issue-spotting requires *recognising those facts as issues* — exactly what faithful decomposition
forbids.

## Non-negotiable constraint
**Do NOT fix this by loosening faithful decomposition** — that reintroduces the padding regression
(off-domain retrieval, the food-safety-in-E&D contamination). The fix must add issues **only when a
concrete trigger fact is present in the prompt**, never speculatively.

## Approach — a gated, deterministic issue-spotting stage (recommended)
A new stage that runs AFTER `expand_query` and APPENDS issue sub-questions. Each issue facet requires a
**trigger phrase actually present in the prompt** — the exact inverse of the padding bug (which invented
topics with no anchor). Deterministic-first (a regex trigger scan → templated, instrument-anchored issue
sub-questions); escalate to an LLM issue-spotter only if templates prove too rigid.

### Placement
`expand.rs`: new `spot_issues(query_text, instrument) -> Vec<String>`, appended in `expand_query` after
the existing facets, deduped, capped by `ISSUE_MAX` (=3). Gated behind `ECHO_DEEP_ISSUE_SPOTTING`
(default OFF for rollout) via a `deep_search::issue_spotting_enabled()` flag mirroring `agentic_enabled()`.

### Trigger → issue table (the heart of it)
`{instrument}` = the named Act/Rules if detected (reuse the instrument-anchoring already in the expand
prompt), else "the applicable local-government rules".

| Trigger (case-insensitive regex over the prompt) | Issue sub-question template |
|---|---|
| `quorum`, `members? (present\|left)`, `attendance`, `meeting`, `before the vote`, `resolution (passed\|was passed)` | "Under {instrument}, what is the required quorum for a valid meeting and must it be maintained at the time of voting?" |
| `relative`, `brother`, `son`, `spouse`, `father`, `related to`, `interested`, `his own`, `conflict of interest` | "Under {instrument}, what are the restrictions on a member/officer (or their relative) being interested in a council contract, and the effect on the contract's validity?" |
| `write[- ]?off`, `book value`, `Rs\.?\s*\d`, `condemn`, `unserviceable`, `sanction limit`, `competent authority` | "Under {instrument}, what are the monetary limits and the competent authority for write-off/condemnation of stores and for sanction of contracts?" |
| `omnibus`, `single resolution`, `in one go`, `validly passed`, `onward sanction of Government` | "Under {instrument}, may several distinct matters be sanctioned in one (omnibus) resolution, and which matters require separate Government (not Council) sanction?" |

The enlarged sub-question set then flows through the existing loop unchanged: per-sub-question retrieval
(`retrieve_for_query`, already concurrent via `join_all`), the coverage check (which will drop any issue
facet that genuinely retrieved nothing — bias-to-refetch, safe), and grounded synthesis.

### Code sketch
```rust
// expand.rs
const ISSUE_MAX: usize = 3;

/// Deterministic issue-spotting: append a sub-question for each LATENT legal issue
/// whose trigger fact is ACTUALLY present in the prompt. Never speculative — an
/// issue with no matching trigger yields nothing (inverse of the padding bug).
fn spot_issues(query_text: &str, instrument: &str) -> Vec<String> {
    static RULES: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let rules = RULES.get_or_init(build_issue_rules); // (trigger regex, template)
    let mut out = Vec::new();
    for (re, template) in rules {
        if re.is_match(query_text) {
            out.push(template.replace("{instrument}", instrument));
            if out.len() >= ISSUE_MAX { break; }
        }
    }
    out
}
```
Wire into `expand_query` (only when `issue_spotting_enabled()`), after the item facets, respecting the
overall `MAX_FACETS` budget and dedup.

## Guardrails against the padding regression
1. **Trigger-gated:** no trigger phrase in the prompt ⇒ no issue facet. A simple single-topic question
   spawns none.
2. **Instrument-anchored:** every issue facet names the same instrument (reuses existing rule 3).
3. **Capped** at `ISSUE_MAX = 3`; deduped against item facets.
4. **Flag-gated** (`ECHO_DEEP_ISSUE_SPOTTING`, default off) for a measured rollout.
5. **Coverage check is the backstop:** an issue facet that retrieves nothing is judged not-answered and
   simply doesn't contribute — no off-domain text is forced into synthesis.

## Trade-offs
- **Latency:** up to +3 round-1 retrievals. Bounded by `ISSUE_MAX` + existing concurrency; deep is
  already ~135 s and users accept it for thoroughness.
- **False triggers:** a casual "meeting" could spawn a quorum facet that finds nothing — low cost, the
  coverage check discards it, and it's instrument-anchored.
- **Coverage of issues** is only as good as the trigger table — start with the four highest-value legal
  issues (quorum, conflict of interest, sanction/write-off limits, omnibus/Government sanction) and grow
  it from real misses, the same evidence-driven way the enum keyword list grew.

## Tests
- Case study → issue facets for **quorum** AND **conflict of interest** generated (regex + template).
- A single-topic question ("what are the grounds for penalty under the E&D Rules?") → **zero** issue
  facets (anti-regression: no triggers).
- The E&D 4-part question → its four item facets, no invented issue facets (triggers absent).
- `spot_issues` respects `ISSUE_MAX` and instrument substitution.

## Verification (end-to-end)
Flag on in a VM worktree; run the case study through deep; confirm the trace's sub-question list now
contains the quorum + conflict-of-interest facets, that round-1 retrieves the corresponding rules, and
that synthesis addresses both issues (which both current runs omit). A/B the answer completeness vs the
current deep run.
