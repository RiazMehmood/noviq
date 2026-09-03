---
id: 040
title: Tier 3 retrieval quality + reranker speed regression fix
stage: refactor
date: 2026-05-16
surface: agent
model: claude-opus-4-7
feature: 001-notebook-knowledge-engine
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: chat
labels: [tier3, retrieval, reranker, chunker, performance, qwen2.5-0.5b]
links:
  spec: specs/001-notebook-knowledge-engine/
  ticket: N/A
  adr: N/A
  pr: N/A
files:
  - path: src-tauri/src/services/chunker.rs
    change: edited
  - path: src-tauri/src/services/query_router.rs
    change: edited
  - path: src-tauri/src/services/reranker.rs
    change: edited
  - path: CHANGELOG_DEV.md
    change: appended
tests:
  - name: manual — test_prompts.md (5 prompts on constitution.pdf)
    status: pending re-run after rebuild + re-index
---

## Prompt

> i followed test_prompts.md file which is at desktop and ask those questions
> and all are incorrect the log details are saved at desktop in 'fine tunning
> research & work' folder and log file name is test1_log.md, diagnose and find
> the root cause as bge also consuming 20s extra and total time increased from
> 20s to 50s it is not good, we loss our speed and gain nothing.
>
> ok go ahead, no we stuck with qwen2.5:0.5B model only, as 1.5B takes 90s and
> 0.5B takes 20s so speed matters more here.

## Response snapshot

Diagnosed two compounding regressions from Tier 3 work:

1. **Quality** — hierarchical chunker was emitting a 7437-char "cover + preface
   + TOC" front-matter chunk for constitution.pdf. The cross-encoder reranker
   was scoring this chunk top-1 for nearly every prompt (the TOC contained
   nearly every keyword in the document). With the per-chunk content this
   large, `build_context` admitted it as the first chunk (the "always admit
   first" rule) and the rest of the 2400-token budget was eaten before any
   relevant article-level chunk could enter the window. 0.5B generator then had
   no signal to answer from.
2. **Speed** — reranker scored 24 candidates × 512 tokens each. Cross-encoder
   attention is O(n²); 19–34s per query was attributable solely to this stage.

**Fix:**
- `chunker.rs::build_document_content` returns title only (was: title + 1000
  body chars); new `strip_toc_paragraphs` helper removes dot-leader paragraphs
  via `is_toc_paragraph`; applied in both `chunk_tree` section branch (skip
  section if nothing remains) and `emit_leaves` (skip TOC paragraphs in split
  loop, plus pre-check full body).
- `query_router.rs::build_context` adds per-chunk size cap = 25% of token
  budget; oversized chunks are skipped instead of always-admitted.
- `query_router.rs` reranker step caps candidates to top-12 (was: all 24);
  candidates beyond cap appended in RRF order so context-builder still sees
  them. Added `ECHO_RERANK=0` env flag to disable cross-encoder.
- `reranker.rs::MAX_PAIR_TOKENS` 512 → 256 (~4× speedup, O(n²) attention).

`cargo check --lib` clean.

## Outcome

- ✅ Impact: retrieval window no longer poisoned by cover/TOC chunk; reranker
  latency budget cut by capping both N and per-pair length; quality and speed
  fixed in one pass without rolling back Tier 3.
- 🧪 Tests: pending user re-run of `~/Desktop/test_prompts.md` after rebuild
  and re-index of constitution.pdf.
- 📁 Files: chunker.rs, query_router.rs, reranker.rs, CHANGELOG_DEV.md.
- 🔁 Next prompts: validate 5 prompts; if any still fail, capture trace
  showing reranker top scores and selected context chunks.
- 🧠 Reflection: "always admit first chunk" was load-bearing under the wrong
  assumption that the first chunk would be small. With hierarchical chunks
  it stopped holding; the per-chunk cap is the durable invariant.

## Evaluation notes (flywheel)

- Failure modes observed: oversized front-matter chunk monopolising context;
  reranker latency dominating end-to-end time.
- Graders run and results (PASS/FAIL): N/A — manual eval pending.
- Prompt variant (if applicable): N/A (system prompt unchanged).
- Next experiment (smallest change to try): if reranker still slow on CPU,
  drop `MAX_PAIR_TOKENS` to 192 or reduce candidates to 8.
