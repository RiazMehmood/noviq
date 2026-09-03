---
id: 0021
title: T028 / A4 — F1–F5 in-app re-run
stage: green
date: 2026-05-23
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: Riaz Mehmood
command: (conversation)
labels: [constitution, acceptance, A4, retrieval, generation, eval]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: null
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: null
files:
  - specs/003-constitution-hierarchical-graph/tasks.md
  - CHANGELOG_DEV.md
  - /home/riaz/Desktop/test 5 prompts/T028-A4-results.md
tests:
  - In-app F1–F5 (5 facts × 3 register variants = 15 prompts, local-0_5b)
  - Score: 3 PASS / 4 PARTIAL / 8 FAIL end-to-end
  - Citation fast-path: 4/4 fired, correct sub-tree, <500 ms retrieval
  - Avg total time: citation 12.4 s, hybrid 29.2 s (~2.4× speedup)
---

## Prompt

User: "for T028 do i need to run the app and ask the five prompts? do i need to
ask these questions again?" (then pasted the v2 validation doc with 5 facts ×
3 variants). Confirmed yes — same questions, but expect citation fast-path on
v1 prompts and verify only the IR is indexed. User ran all 15 prompts in-app on
`local-0_5b` and pasted the responses; trace log at
`~/Desktop/test 5 prompts/echoai-traces-2026-05-22T20-13-34.md`.

## Response snapshot

Located the final 15-prompt block in the trace log (lines 128311–131774).
Extracted per-prompt timings, model tier, retrieval strategy, FTS5 returns, and
citation-fast-path firings. Compared each response against the ground-truth
answer in the validation doc. Compiled a per-fact pass/fail grid and a
retrieval-vs-generation diagnosis. Saved a full report to
`~/Desktop/test 5 prompts/T028-A4-results.md`. Updated `tasks.md` (T028 → [x]),
appended a dated `CHANGELOG_DEV.md` entry, refreshed the
`project_echoai_constitution_v1` memory and `MEMORY.md` index line.

## Outcome

- ✅ **Retrieval objective of A4 — PASS.** Citation fast-path fired on all 4
  article-citing prompts (Art 10, 1, 6, 11), returned the correct Article
  sub-tree in <500 ms, ~2.4× faster than hybrid (12.4 s vs 29.2 s avg). FTS5
  hybrid recall healthy (24 hits, `hybrid(rrf)`); old Tier-3 returned 0.
- ⚠️ **End-to-end accuracy 3/15 PASS** — gated by the 0.5B generator. Even when
  retrieval handed the correct chunk, the model inverted a negation (Fact 3
  v2 "SC *can* validate"), ignored context and hallucinated (Fact 4 v3 voting
  age), truncated an enumerated list (Fact 2 drops territory category d), and
  hallucinated extras (Fact 1 "Review Board" / missed journey-time exclusion).
- 🐛 **2 retrieval gaps surfaced:**
  - **Preamble not indexed** (Fact 5, all 3 variants miss "Quaid-i-Azam Mohammad
    Ali Jinnah") — IR import emits no retrievable chunk for the Preamble text.
    Real bug, real fix needed before S7.
  - **Art 204 contaminates Art 6** results on Fact 3 v3.
  - Semantic cache propagated a wrong answer (Fact 5 v2) — consider not caching
    low-confidence / failed-grounding answers.
- 📁 Files: tasks.md (T028 → [x]), CHANGELOG_DEV.md (T028 entry on top),
  Desktop results doc, memory updates.
- 🔁 Next prompts: fix the Preamble IR-import gap, then S7 (T029–T031
  acceptance harness + full A1–A4 gate). 28/34 tasks done.
- 🧠 Reflection: T028 split the Feature-003 verdict cleanly — the hierarchical
  rebuild + citation fast-path delivered the retrieval improvements they were
  designed for; the 0.5B generator is now the dominant accuracy ceiling. That
  reframes the v2 priorities: a stronger / more constrained generator matters
  more than further retrieval tuning.

## Evaluation notes (flywheel)

- Failure modes observed:
  - **Generation (dominant):** negation inversion, list truncation, ignoring
    correct context, hallucinated citations ("Article 107", "Rule 2").
  - **Retrieval (gaps):** Preamble missing from corpus; Art 204 contamination
    on Fact 3 v3.
  - **Cache:** propagates wrong cached answers.
- Graders run and results (PASS/FAIL): A4 retrieval **PASS**; A4 end-to-end
  **3/15 PASS** (gated by 0.5B). A1/A2/A3 already PASS.
- Prompt variant (if applicable): v1 technical (citation fast-path), v2
  professional (hybrid), v3 plain (hybrid).
- Next experiment (smallest change to try): fix the IR import to emit a
  Preamble chunk; re-run Fact-5 to confirm retrieval reaches it.
