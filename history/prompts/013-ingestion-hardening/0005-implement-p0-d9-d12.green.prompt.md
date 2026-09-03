---
id: 0005
title: Implement P0 D9 D12
stage: green
date: 2026-06-19
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: Riaz Mehmood
command: /sp.implement
labels: [ingestion, P0, D9, D12, page-number, normalizer]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/text_normalizer.rs
  - src-tauri/src/services/parsers/chrome.rs
  - src-tauri/src/services/structure_parser.rs
  - src-tauri/src/services/structured_importer.rs
  - specs/013-ingestion-hardening/tasks.md
tests:
  - cargo test --lib text_normalizer
  - cargo test --lib structure_parser
  - cargo test --lib chrome
---

## Prompt

/sp.implement (no additional user input)

Execute the Feature 013 task plan. Mid-run the user corrected scope twice:
(1) clarified ingestion stays deterministic/no-LLM, LLM is only used at the retrieval→response
stage (so FR-013 stands; reworded task T041a). (2) chose Way B for D12 (path→offset map, no DocNode
struct change), page numbers now, EDIT ONLY — no cargo this session, user builds once at the end.

## Response snapshot

Implemented Phase 2 (P0) of the locked plan — D9 + D12 — edit-only (no compile, per user):

**D9 (T004/T005)** — `text_normalizer.rs`: removed the lone two-word entry `"shallnot"` from
`JOIN_TARGETS` (it fused the verbatim phrase "shall not"); added a comment invariant + a new guard
test `never_fuses_two_word_legal_phrase_into_one_token` that asserts "shall not" survives AND that no
JOIN_TARGETS entry is a concatenation of two other listed single words.

**D12 (T006/T007)** — page_number for provisions, via Approach B (minimal-ripple, no DocNode field):
- `chrome.rs`: `StripResult` gains `cleaned_page_offsets` (page offsets re-aligned to the
  chrome-stripped body, since stripping shifts byte positions); `strip_chrome` builds them from a new
  per-line offset table; `page_for` made `pub`.
- `structure_parser.rs`: `build_tree_from_headings` now returns `(DocNode, HashMap<unit_path, offset>)`
  recorded during the heading walk (+ Preamble). Added `persist_tree_with_pages` (sibling to
  `persist_tree`, which now delegates with an empty map so all other callers are untouched).
  `collect_chunks` takes `parent_page` + `page_by_path`, stamps `page_number` from the map keyed by
  `unit_path`, and inherits the nearest ancestor's page for finalize-tree-created nodes (`/pN` splits,
  `/mN` merges, root) — NULL only when no mapped ancestor.
- `structured_importer.rs`: `import()` captures `cleaned_page_offsets`, builds `page_by_path`
  (offset→page via `chrome::page_for`), stores it on `ImportOutcome`; `persist()` calls
  `persist_tree_with_pages`.

Verified call-site consistency by grep (1 build_tree caller destructured; both collect_chunks calls
8-arg; both StripResult literals updated; page_by_path threaded end-to-end). NOT compiled.

T008 (page_number acceptance test) left pending the fixture harness + the end-of-session build.

## Outcome

- ✅ Impact: D9 (quote corruption) fixed + guarded; D12 (NULL provision page_number) implemented with
  citation-safe page mapping (chrome-aligned offsets; inherit-or-NULL, never a wrong page).
- 🧪 Tests: new D9 unit test added; not run this session (edit-only per user). Verify at end:
  `cargo test --lib`, then re-index the law doc + check DB page_number is non-null/monotonic/in-range.
- 📁 Files: text_normalizer.rs, chrome.rs, structure_parser.rs, structured_importer.rs, tasks.md.
- 🔁 Next prompts: user runs the single build+test; then re-index control set + fixture diff (P0
  checkpoint); then Phase 3 (P1 geometry zoning, D5/D6).
- 🧠 Reflection: The audit under-scoped D12 ("just thread page_offsets"); reality needed offset
  propagation + chrome-aligned page offsets because DocNode carries no source offset and content is
  reconstructed. Surfaced the scope + verification tradeoffs rather than blasting a 59-literal change.

## Evaluation notes (flywheel)

- Failure modes observed: scope creep risk on D12; mitigated by Approach B (no struct churn).
- Graders run and results (PASS/FAIL): none run (edit-only); grep arity check PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): build + re-index law doc; confirm page_number column
  populated and monotonic; if a page is off-by-one at a page boundary, inspect cleaned_page_offsets.
