---
id: 0007
title: Grounding prompt v5.2 — instruct-only, [N] citations, Summary budget fix
stage: refactor
date: 2026-05-12
surface: agent
model: claude-opus-4-7
feature: grounding-prompt
branch: master
user: Riaz Mehmood
command: discussion + implementation
labels: [llm, prompt-engineering, grounding, summary, citations]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: src-tauri/src/services/llm.rs
    change: prompt v5.2 body + VERSION bump (5.1.0 → 5.2.0) + doc-comment
  - path: src-tauri/src/services/query_router.rs
    change: |
      clean_llm_output simplified (PART 1 anchor removed, residual header
      stripper added); extract_indexed_citations + count_valid_index_markers
      added; validate_grounding teaches about [N] markers; Summary budgets
      bumped 512→1280 max_tokens / 2048→3584 n_ctx; 5 tests updated/added.
  - path: CHANGELOG_DEV.md
    change: top entry documenting v5.2 changes, rationale, caveats
tests:
  - cargo test --lib --features llm services::query_router::tests → 39/39 passed
  - cargo check --lib --features llm → clean (2 pre-existing warnings)
---

## Prompt

User reported (via Qwen2.5-1.5B trace bundle at `~/Desktop/New Folder/
Qwen2.5:1.5BV3/`) three symptoms: system prompt too long, output truncation,
and apparently-empty answers despite visible streaming. Asked to discuss root
cause, then evolved into a decision to: (a) drop inline citations from
prose and surface only model-cited chunks in the bottom references panel,
(b) keep a single prompt (instruct-only — thinking models retired), (c) bump
Summary budgets which were backwards from need, (d) use strict `[N]` index
markers tied to context chunk numbers.

Verbatim user messages (excerpted, latest-first):
- "strict form and continue"
- "Page 1▼ expand … these are citations in the frontend. and we are not
  going to use thinking model anymore so we will only use instruct model so
  one prompt is ok."
- "clear inline citations, and show only those traces which llm uses in the
  bottom, need summary prompt also if user ask for summerization. and what
  is the purpose of classify query type? and how it is deciding?"
- "no create only one prompt"
- "and show references not in output, just parse top 3 or 4 references or
  selected references and directly show in the bottom of response as already
  showing, dont show in the output which is consuming output context"
- "/home/riaz/Desktop/New Folder/Qwen2.5:1.5BV3 its responses and log file …
  system prompt is too long, and it truncate the output, and some responses
  shows streaming but at the end it shows nothing."

## Response snapshot

Diagnosed the three symptoms (empty-final = parser sentinel miss; truncation
= 1024 cap tight for PART 1 + PART 2; long-prompt = ~275 token v5.1 has
redundancy on 1.5B). Surfaced two blockers before coding: v5.1 PART 1 anchor
doubles as CoT-leak shield (regression risk for thinking models — accepted
since they're retired) and frontend already renders citation chips via
CitationChip.tsx (no new UI work needed). Designed `[N]` index markers that
piggyback on existing chunk numbering in `build_context_string`, added two
new helpers to keep the grounding validator and citation-filter pipeline
working unchanged at the (filename, page) layer. Bumped Summary budgets to
1280/3584 to fix the backwards-from-need allocation.

## Outcome

- ✅ Impact: v5.2 prompt 35% smaller, eliminates parser-sentinel failure mode,
  fixes Summary truncation, removes inline citation hallucination surface.
- 🧪 Tests: 39/39 query_router unit tests pass (3 new index-marker tests +
  rewrites of 2 anchor tests; deleted 3 anchor tests for removed behavior).
- 📁 Files: 2 source + CHANGELOG + this PHR.
- 🔁 Next prompts: re-baseline Qwen2.5-1.5B sweep against v5.2; optionally
  wire llama.cpp prompt-cache for the now-stable system prefix.
- 🧠 Reflection: discussion-first paid off — surfacing the CoT-shield
  conflict before coding let the user explicitly retire thinking models,
  which collapsed a tier-split design back to a single prompt.

## Evaluation notes (flywheel)

- Failure modes observed: v5.1 sentinel miss → empty final answer despite
  streamed tokens; PART 2 hallucinated filenames on 1.5B; Summary mode
  truncated mid-sentence due to 512-token cap.
- Graders run and results (PASS/FAIL): unit test suite PASS (39/39).
  Real-corpus benchmark sweep pending re-baseline (paused at v5.1).
- Prompt variant (if applicable): v5.2.0 (~180 tokens, indexed citations,
  no section framing).
- Next experiment (smallest change to try): re-run top-10 model sweep with
  v5.2 and compare answer-length, citation-validity, and total-time deltas
  against v5.1 baselines in memory/project_echoai_offline_model_benchmark.md.
