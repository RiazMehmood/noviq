# Reviewer Agent + Answer Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a second-pass LLM judge that detects non-responsive answers, retries with progressively richer context (up to 3 times), and improves answer quality for both regular and summary/outline queries.

**Architecture:** A `judge_answer()` function is inserted into `query_router::route()` after grounding validation. It runs the already-loaded Qwen model with a tiny YES/NO classification prompt. Retries escalate context richness (K and token budget). A query-type classifier selects between the regular system prompt and a new summary-mode prompt with higher `max_tokens`.

**Tech Stack:** Rust (llama-cpp-2 via existing `ModelSlot`), fastembed (already loaded), SolidJS frontend (quality notice badge)

---

## Section 1: Reviewer Agent (Retry Loop)

### 1.1 Judge function

`judge_answer()` runs in `query_router.rs` after grounding validation, before citation extraction. It uses the already-warm `ModelSlot` — no model loading cost.

**Trigger condition:** `tier_used != "error"` AND `answer != GROUNDING_FALLBACK`

**Judge prompt (max_tokens = 5, no streaming, no app handle):**
```
<|im_start|>system
You are a relevance judge. Reply only YES or NO.
<|im_end|>
<|im_start|>user
Question: {query}
Answer: {answer}
Does this answer directly address the question? Reply YES or NO only.
<|im_end|>
<|im_start|>assistant
```

**Parse rule:** output contains "YES" (case-insensitive) → pass. Anything else (NO, empty, unclear) → fail → retry.

### 1.2 Retry strategy

Each retry re-retrieves context with escalated parameters and re-runs LLM synthesis + grounding validation + judge check. No re-embedding of the query (query embedding reused).

| Attempt | K (top chunks) | Token budget | Prompt modifier |
|---------|---------------|--------------|-----------------|
| 0 (original) | 8 | 480 | — |
| Retry 1 | 10 | 560 | Adds: "Focus specifically on answering: {query}" prepended to context block |
| Retry 2 | 12 | 640 | Same modifier |
| Retry 3 | 12 | 640 | Same modifier (final attempt) |

**After 3 failed retries:** return error `QueryResponse`:
```
answer: "The model was unable to generate a relevant answer. Please rephrase your question and try again."
tier_used: "error"
quality_notice: None
```
This is stored as an assistant message with `is_incomplete: true`.

### 1.3 Data flow in `route()`

```
embed query
→ cache check (Tier 0)
→ retrieve chunks (Tier 1+2, RRF merge)
→ build context (K=8, budget=480)
→ LLM synthesis (max_tokens per query type)
→ grounding validation (existing)
→ judge_answer()          ← NEW
    PASS  → citation extraction → store cache → return
    FAIL  → retry loop (up to 3x with escalating context)
             after 3 fails → return error response
```

---

## Section 2: Answer Quality — Length + Citation Policy

### 2.1 Query type classifier

`classify_query(query: &str) -> QueryType` — pure function, no LLM call.

```rust
pub(crate) enum QueryType {
    Regular,
    Summary,
}
```

**Summary triggers** (any keyword present, case-insensitive):
`"summarize"`, `"summary"`, `"summarise"`, `"outline"`, `"overview"`, `"what does the book"`, `"what is the book about"`, `"list all chapters"`, `"table of contents"`, `"give me an overview"`, `"what topics"`, `"what subjects"`, `"what does this document cover"`, `"brief me"`, `"brief on"`

All other queries → `QueryType::Regular`.

### 2.2 System prompts

**Regular queries** — updated `GROUNDING_SYSTEM_PROMPT` in `llm.rs`:
- Remove "answer in a few lines" constraint
- Replace with: answer as thoroughly as the context warrants — concise for simple facts, detailed for complex questions
- Citation requirement kept: cite sources as `[filename, page N]`
- `max_tokens = 256` (unchanged — stays within n_ctx budget)

**Summary queries** — new `SUMMARY_SYSTEM_PROMPT` in `llm.rs`:
- Instructs a detailed, structured response (use headings, bullet points if helpful)
- Citations optional — model may cite but is not required to
- Explicitly allows synthesising across multiple passages
- `max_tokens = 512`

### 2.3 Quality notice for summaries

When `QueryType::Summary`, the response always carries:
```rust
QualityNotice {
    constrained: false,
    message: "General summary — content synthesised from document; specific claims may not be individually cited.".to_string(),
}
```

Frontend (`ChatPanel.tsx`) renders this as a distinct badge (not the hardware-constraint warning style).

### 2.4 n_ctx budget note

Current chat n_ctx is set in `llm.rs` `infer_with_cancel`. Summary mode raises `max_tokens` to 512. Implementation sets n_ctx = 2048 for summary queries and n_ctx = 1024 for regular queries in `infer_with_cancel`. This avoids `NoKvCacheSlot` errors when generating up to 512 tokens against a ~1500-token summary prompt.

---

## Non-goals

- No re-embedding of query on retries (reuse existing query embedding)
- No streaming of judge output (judge uses `infer_with_cancel` directly, no app handle)
- No LLM judge on cached responses (Tier 0 cache hits skip the reviewer entirely)
- No change to batch or analyze pipelines — reviewer is chat-only

---

## Files

| File | Change |
|------|--------|
| `src-tauri/src/services/query_router.rs` | `judge_answer()`, `classify_query()`, retry loop, escalating context params |
| `src-tauri/src/services/llm.rs` | Updated `GROUNDING_SYSTEM_PROMPT`, new `SUMMARY_SYSTEM_PROMPT`, n_ctx bump for summary mode |
| `src/components/chat/ChatPanel.tsx` | Summary quality notice badge (distinct from hardware-constraint badge) |

---

## Tests

- `test_classify_query_summary_keywords` — all trigger words → `Summary`
- `test_classify_query_regular` — generic questions → `Regular`
- `test_judge_answer_yes_response` — judge returns answer containing "YES" → pass
- `test_judge_answer_no_response` — judge returns "NO" → fail
- `test_judge_answer_empty_response` — empty output → fail (treated as NO)
- `test_retry_escalation_params` — verify K and budget values at each retry level
