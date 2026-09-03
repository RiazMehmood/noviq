# Chat architecture: where retrieval is enough vs where an LLM is needed

**Date:** 2026-06-13
**Decision context:** Noviq chat runs on the fine-tuned Sindh Qwen2.5-0.5B (final — see
`feedback_noviq_extractive_05b_final`). The 0.5B fabricates facts even when the correct text is
in its context (eval traces E03/E11/E14). Therefore the answer must come **verbatim from the
documents**; the model must not write facts.

## Guiding principle

> **Use a model for understanding the QUESTION, never for writing the ANSWER.**
> A wrong question-reading only costs search quality (recoverable). A model-written answer can
> fabricate facts (fatal for citation-grade). The answer is always verbatim document text +
> structured citation; the model never touches the output.

## Scenario coverage

| # | Officer query | Pure retrieval? | LLM needed? | Beta plan |
|---|---|---|---|---|
| 1 | Look up a rule ("What does Section 12 say?") | ✅ find chunk, show verbatim | ❌ | Extractive |
| 2 | Definition ("What does 'X' mean?") | ✅ definition is a chunk | ❌ | Extractive |
| 3 | Multiple provisions ("S.22 and Rule 4 each?") | ✅ fetch each, show verbatim (decompose) | ❌ | Extractive (deterministic merge) |
| 4 | Follow-up ("and for BS-22?", "exceptions?") | ✅ re-search with carried context | ❌ (model may rephrase search, low risk) | Extractive |
| 5 | Out-of-scope ("write a poem") | ✅ nothing retrieved → refuse | ❌ | Refuse |
| 6 | Cross-references ("what does this link to?") | ⚠️ only if cross-ref graph built at index | ❌ (stored links, not reasoning) | Show if data exists, else "not available" |
| 7 | Amendments ("was this amended? what changed?") | ⚠️ only if amendment metadata captured | ❌ (stored data: Inserted/Substituted/Repealed) | Show if data exists, else "not available" |
| 8 | Compare two documents ("which is stricter?") | ◑ show both side-by-side; can't judge | ✅ the *verdict* needs reasoning | Extractive: show both, officer judges |
| 9 | Summarize ("what is this about?") | ◑ preamble + chapter/section headings | ✅ *prose* summary needs reasoning | Extractive structural summary |
| 10 | Analyze a document | ◑ structure + key provisions | ✅ deep analysis needs reasoning | Extractive structural overview |

## Conclusions

- **~70% of real queries (rows 1–7) need NO model** — they are retrieval + stored-data
  problems. Pure extraction = 100% fidelity, zero fabrication.
- **Rows 6–7 are INDEXING features, not model features.** Cross-reference links and amendment
  notes must be captured *when the document is indexed* (deep IR). If captured → showing them is
  a lookup. If not → no model can invent them reliably (the 0.5B hallucinates). This is a
  data-pipeline investment.
- **Only rows 8–10 genuinely want abstraction** (comparison verdicts, prose summaries). The
  0.5B can't do these without fabricating. Beta does the **extractive** version (show-both,
  show-structure). A stronger model is the *only* legitimate place for these — and only for
  these, never the lookup path.

## Beta decision (2026-06-13)

1. Lookups / definitions / multi-provision / follow-ups → **fully extractive** (verbatim +
   structured citation). This is the 100%, no-fabrication core.
2. Compare / summarize / analyze → **extractive versions** with honest labels.
3. Cross-refs / amendments → **show only what indexing captured**; else say "not available for
   this document" — never guess.
4. **The model is DISABLED for the first testing phase, not deleted.** Keep a toggle:
   - default (CPU / beta): extractive, model off.
   - optional (GPU available): allow enabling a larger model for rows 8–10 prose answers.
   Decide final hard-coded defaults after the testing phase.

The model becomes a *later, deliberate* enhancement for prose summaries/comparisons — not a
crutch on the factual path.

## Verified status of options 6 & 7 (and the deep-tier intents) — 2026-06-13

Code audit + Feature 007 spec review:

- **Cross-references (6): partially built, not usable for beta docs.** `graph_builder.rs` emits
  `cross_reference`/`non_obstante` edges and `graph::expand` follows them, BUT only for the
  Constitution's "Article N" pattern, only on **deep-IR (Mistral) indexed** docs, and only wired
  into the citation fast-path. The Sindh beta corpus is **fast-indexed** → cross-refs don't fire.
- **Amendments (7): type exists, data does not.** `AmendmentNote`/`AmendmentKind` are defined and
  spec'd (007 FR-010), but `graph_builder.rs` states there is **no amendment node** ("needs a v2
  schema"). Amendment data is never captured; `LegalCitation.amendment` is a placeholder.
- **Difference / CrossCompare / Timeline / Procedure intents:** spec'd in 007 (FR-014/019/020)
  but **gated** — `intent_router::dispatch` short-circuits them with "requires deep-indexed
  instruments — run Deep re-index with Mistral first." The deep pipeline that would populate
  amendments + typed cross-refs (FR-010/011) is incomplete for Section/Rule instruments.

**Conclusion:** options 6–10's "smart" forms are an **indexing investment (deep IR), not a model
feature**, and are not ready for beta. Going extractive loses no working feature — it defers
unbuilt ones. Re-chunk (Phase 2) + deep-IR amendment/cross-ref capture is the post-beta path.

## Toggle design (model disabled for beta, not deleted) — 2026-06-13

User directive: keep both options in hand.
- **Setting:** chat answer mode — `extractive` (default, beta) vs `llm`. Model code stays;
  it is only switched off by default.
- **Hardware-aware:** on a GPU machine, allow enabling a larger model for the synthesis-only
  rows (8–10 prose answers); CPU/beta stays extractive.
- Defaults are configurable now; hard-code the final defaults after the first testing phase.

