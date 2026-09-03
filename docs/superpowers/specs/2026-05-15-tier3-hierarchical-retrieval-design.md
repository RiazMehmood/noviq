# Tier 3 — Hierarchical Retrieval & Reranked Grounding

**Date:** 2026-05-15
**Status:** Spec — awaiting user review
**Scope:** Sub-projects #0, #1, #2 (DB wipe + hierarchical schema + reranker + sentence-level grounding)
**Out of scope (separate specs):** OCR pipeline (#3), indexing-mode lock (#4), NER stuck-on-restart fix (#5), query assist UX (#6)

---

## 1. Goals

1. Replace flat 512-token chunks with a **three-level hierarchy** (`document → section → leaf`) that preserves legal/document structure and supports both precise clause QA and section-level summarization.
2. Move from a generic vocabulary that supports legal docs today and other domains later, with domain-specific fields stored in a `domain_metadata` JSON column rather than baked into columns.
3. Add a **cross-encoder reranker** (bge-reranker-base) between RRF retrieval and LLM synthesis to materially improve precision on English text.
4. Replace the current "citation index present" grounding check with **sentence-level grounding validation** (lexical overlap against cited chunks) so unsupported sentences are caught.
5. Wipe the existing database and re-index from scratch under the new schema.

## 2. Non-goals

- OCR. Image and scanned-PDF ingestion still returns the existing errors; the OCR rebuild is sub-project #3.
- Urdu/multilingual support. English-only focus; reranker model is English-tuned.
- Conversation memory / summarized history. Existing behavior preserved.
- Frontend autocomplete / spell correction. Sub-project #6.
- Replacing the existing fine-tuned Qwen2.5-0.5B chat model. Reserved for chat replies only.

## 3. Constraints & invariants

- **Speed budget**: current end-to-end is ~20s with fine-tuned Qwen2.5-0.5B. Reranker may add up to ~1.5s (≤ +7.5%). Anything more is a regression.
- **Hardware**: must run on 8 GB RAM, CPU-only, offline-first.
- **Storage**: bundled SQLite (FTS5 enabled) + sqlite-vec vec0 virtual table — unchanged.
- **No new runtime processes**: reranker is ONNX, loaded in-process via `ort` (already a dep).
- **Re-index required**: schema is incompatible with current chunks; nuclear DB wipe is part of this work.
- **Feature parity**: every existing query type (chat, analysis, knowledge graph) must still function after migration.

## 4. Architecture

### 4.1 Database wipe (sub-project #0)

On first startup after upgrade, the app detects schema version mismatch and:

1. Closes any open DB handles.
2. Deletes `echo.db`, `echo.db-wal`, `echo.db-shm`.
3. Deletes `.fastembed_cache/` (forces clean re-embedding).
4. Deletes all per-folder index state files and trace logs.
5. Preserves `models/` directory (downloaded LLM/embedding/reranker weights).
6. Recreates schema via fresh migration run.

A startup banner explains the one-time wipe and prompts the user to re-add their folders.

### 4.2 Schema (sub-project #1)

New columns on `chunks`:

```sql
ALTER TABLE chunks ADD COLUMN parent_chunk_id  TEXT REFERENCES chunks(id);
ALTER TABLE chunks ADD COLUMN unit_type        TEXT NOT NULL DEFAULT 'leaf';
                                                -- 'document' | 'section' | 'leaf'
ALTER TABLE chunks ADD COLUMN unit_label       TEXT;
                                                -- "Section 8(2)", "Rule 12(2)", "2.1 Methodology"
ALTER TABLE chunks ADD COLUMN unit_path        TEXT;
                                                -- "Chapter III > Section 8 > Clause 8(2)"
ALTER TABLE chunks ADD COLUMN domain_metadata  TEXT;
                                                -- JSON; NULL for generic docs
                                                -- legal example: {"domain":"legal","act_name":"...","jurisdiction":"Sindh","section_number":"8(2)"}

CREATE INDEX idx_chunks_parent     ON chunks(parent_chunk_id);
CREATE INDEX idx_chunks_unit_type  ON chunks(unit_type);
CREATE INDEX idx_chunks_unit_label ON chunks(unit_label);
```

`chunk_embeddings` (vec0 virtual table): unchanged structure, but every `unit_type` level gets its own row. Storage scales ~1.2× vs current (clauses replace fixed-size chunks; sections + document are net-new).

`unit_label` must preserve the **literal citation phrase** (e.g., "Rule 11-A", "SGA&CD Notification dated 27th February 1984", "Schedule II"), because the fine-tuned LLM is trained to echo these verbatim.

### 4.3 Heading detection (hybrid: regex + LLM)

Ingestion pipeline produces a structural tree before chunking:

1. **Pass A — regex**: detect well-known structural markers per pattern set:
   - Legal: `^(Section|Rule|Clause|Article|Chapter|Schedule|Sub-clause|Explanation|Proviso|Definitions?)\s+[\divxlc()A-Z\-\.]+`, `^\d+(\.\d+)*\.?\s+[A-Z]` (numbered sections), Pakistani-specific notification headers ("SGA&CD Notification dated …").
   - Generic: ATX/Setext markdown headings, numbered headings (`1.`, `1.1`, `2.1.3`), all-caps short lines followed by paragraph text.
2. **Pass B — LLM disambiguation**: only for paragraphs where Pass A gave low confidence (e.g., a heading-shaped line that might just be emphasis). Calls Qwen2.5-0.5B with a strict yes/no prompt: "Is this line a structural heading? Reply YES/NO." Capped at ≤ 5% of paragraphs per doc to bound cost.
3. **Tree construction**: produces nested `(unit_type, unit_label, content_range)` nodes. Document is always the root. Sections are determined by detected headings; leaves are the paragraph/clause spans between them.
4. **Fallback**: if Pass A finds zero structure and Pass B confirms, emit `document → leaf` only (two-level), and chunk leaves at the existing 512-token soft cap with paragraph-boundary preference.

### 4.4 Multi-level chunking & embedding

Each detected node becomes a chunk row at its corresponding `unit_type`:

- **Document chunk**: title + first ~1000 chars + auto-generated 200-token summary (LLM-produced once at ingestion; cached). One per source.
- **Section chunk**: heading + full section text. If section > 1500 tokens, content is truncated and a summary is appended. One per section.
- **Leaf chunk**: clause/paragraph as-is. Soft cap 512 tokens; clause-aware splits never break mid-clause. Inherits `parent_chunk_id` pointing to enclosing section.

All three levels are embedded with the existing fastembed model. No reranker call at ingestion.

### 4.5 Retrieval (sub-project #2 — reranker + grounding)

Query path:

```
query
  → embed
  → parallel search:
      vec0 search across ALL unit_types (top 30)
      FTS5 search across leaf + section content (top 30)
  → RRF merge → ~30 candidates with mixed unit_types
  → bge-reranker-base scores all 30 against query
  → take top 8 by reranker score
  → context assembly:
      include each chunk's unit_path as breadcrumb header
      if 3+ leaves share the same parent section, promote the parent section
        instead of stuffing siblings (rollup)
      respect ctx_token_budget (raised from 1200 → 2400)
  → LLM synthesis (unchanged Qwen2.5-0.5B fine-tune)
  → sentence-level grounding validation
  → response
```

**Reranker (bge-reranker-base)**:
- ONNX, ~278 MB, loaded lazily on first query, kept resident.
- Batch size 30 pairs per call; ~1.2s on CPU.
- Bypassed for entity-pattern Tier-1 FTS queries (already exact matches).

**Context budget**: raise `CONTEXT_TOKEN_BUDGET` from 1200 → 2400. With reranker filtering noise, the larger window is high-signal.

### 4.6 Sentence-level grounding validation

Replaces current "is `[N]` index present?" check.

For each generated sentence carrying a citation `[N]`:
1. Tokenize sentence and cited chunk N (lowercase, strip stopwords/punctuation).
2. Compute Jaccard overlap of content tokens (≥ 4 chars).
3. **Pass** if overlap ≥ 0.25 OR ≥ 3 distinct content tokens shared.
4. **Fail** otherwise → mark sentence as ungrounded.

Aggregated outcome:
- 0 ungrounded sentences → response shipped as-is.
- 1–2 ungrounded sentences → strip them, ship rest with a "(some content omitted as unsupported)" note.
- ≥ 3 or > 50% ungrounded → trigger existing retry-escalation path with widened context budget.

Thresholds are tunable constants, not hardcoded literals scattered through the file.

## 5. Data flow summary

**Ingestion**: parser → heading detector (regex + LLM disambiguation) → tree builder → multi-level chunker → multi-level embedder → SQLite (chunks + chunk_embeddings + FTS5 triggers fire automatically).

**Query**: embed → vec+FTS retrieve (mixed levels) → RRF → reranker → context assembly with rollup → LLM → sentence grounding → response + trace.

## 6. Trace logging changes

Trace gains four new steps to keep observability honest:
- `Heading detection` (pass A regex hits, pass B LLM calls, tree depth)
- `Reranker scores` (top-N pairs with scores; bypassed flag)
- `Context rollup` (which sections were promoted from sibling leaves)
- `Grounding (sentence)` (per-sentence overlap scores, pass/fail)

## 7. Migration strategy

This is a one-shot destructive migration. There is no in-place upgrade path.

1. Ship new build with bumped `SCHEMA_VERSION`.
2. On startup, detect mismatch → run nuclear wipe (§4.1) → recreate schema → show banner.
3. User re-adds folders → ingestion runs under new pipeline.
4. Old query traces and benchmark data are lost (acceptable; user confirmed).

## 8. Acceptance criteria

- [ ] First startup after upgrade detects old schema and performs a clean wipe; `models/` is preserved.
- [ ] Re-indexing a known legal PDF produces chunks at three `unit_type` levels with non-null `unit_label` and `unit_path` matching the document's actual structure.
- [ ] Re-indexing a flat plain-text file produces only `document` and `leaf` chunks (no false sections).
- [ ] `chunks.parent_chunk_id` is non-null on every section and leaf; document chunks have `parent_chunk_id IS NULL`.
- [ ] Query "What is Section 8(2)?" returns the matching leaf chunk with `unit_label = "Section 8(2)"` in the top 3 reranked results.
- [ ] Query "Summarize Chapter III" surfaces the corresponding section chunks in the top 5 reranked results, not 20 fragmented leaves.
- [ ] Reranker adds ≤ 1.5s p50 to end-to-end query latency; total stays ≤ 22s p50 on the existing benchmark suite.
- [ ] Sentence-grounding validator flags a deliberately hallucinated test response (sentence with no overlap to cited chunk) as ungrounded.
- [ ] Trace JSON contains the four new step entries on every query.
- [ ] All existing query-router tests pass against the new schema.
- [ ] No regression in the entity-pattern Tier-1 FTS path (reranker bypassed; latency unchanged).

## 9. Risks & open questions

- **Heading-detection LLM cost**: Pass B is bounded to ≤ 5% of paragraphs, but a 1000-page PDF can still mean 50+ LLM calls. Need to confirm Qwen2.5-0.5B can handle this batch in reasonable time during indexing. Mitigation: cap absolute Pass B calls per document at 200; remainder default to "not a heading."
- **Reranker bypass logic**: deciding when to bypass the reranker (Tier-1 entity queries vs. general) is a judgment call. Will be a tunable constant set, not hardcoded conditions.
- **Section chunk size**: a 5000-token "Section 8" section creates a chunk that exceeds embedder context. Truncation + summary is the proposed fallback; quality of that summary matters. May need a separate eval.
- **Sentence grounding thresholds**: 0.25 Jaccard / 3 token overlap is a starting point. Will need tuning against a labeled set of grounded vs. hallucinated responses. Initial set can be hand-built from existing benchmark traces.
- **Rollup heuristic**: "3+ siblings → promote parent" is intuitive but unmeasured. Easy to A/B against keep-leaves once the system is running.

## 10. Follow-up sub-projects (separate specs)

- **#3 — OCR pipeline rebuild** (Tier 2): real OCR for images and scanned PDFs; depends on this schema being in place so OCR'd content lands at the correct `unit_type`.
- **#4 — Indexing-mode lock UX**: while ingestion runs, gate chat/analysis/etc. so LLM and NER get full CPU.
- **#5 — NER stuck-on-restart fix**: small bug; startup must resume `ner_pending` sources.
- **#6 — Query Assist UX**: SymSpell-based "did you mean" + FTS5 prefix autocomplete; no new ML models.
