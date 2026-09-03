---
id: 0019
title: US8 re-scope (ingestion structural quality) — spec update + parsers/table.rs design
stage: plan
date: 2026-06-17
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: "start US8 — update the spec then design parsers/table.rs"
labels: [us8, ingestion, tabular, extraction, design]
links:
  spec: specs/010-one-ask-drafting/spec-secretariat-summary-refactor.md
  ticket:
  adr:
  pr:
files:
  - docs/post-meeting-roadmap-2026-06-17.md
  - specs/010-one-ask-drafting/spec-secretariat-summary-refactor.md
tests:
  - none (design + spec stage; TDD to follow on implementation)
---

## Prompt

start US8 — update the spec then design parsers/table.rs

(Preceded by a post-meeting brainstorming discussion that settled the roadmap:
US8 = ingestion structural quality, US11 = deterministic-frames drafting,
US12 = KG relational fast-path, US13 deferred. Captured in
docs/post-meeting-roadmap-2026-06-17.md.)

## Response snapshot

Updated the US8 section + FR-013/013a/013b in the secretariat-summary refactor
spec to the re-scoped shape, then presented the `parsers/table.rs` design and
captured three locked decisions via a clarifying question.

US8 re-scope (absorbs the former US11 index-time data-hygiene scope; one re-index):
- **R1 — robust ingestion:** tabular module (`parsers/table.rs`) detecting table
  regions from `pdftotext -bbox` geometry, clustering words into a grid, and
  linearizing each row into one self-describing, citable unit; prose regions
  untouched. The year-rule (`looks_like_year`) folds INTO R1 as parser
  correctness (no longer a separate "guard").
- **R2 — generic boilerplate de-prioritisation** with mandatory safety rules:
  flag-not-delete (exclude from ranking, never remove), provision-label-keep
  invariant, conservative keep-bias threshold, and a dry-run-review gate
  requiring zero false positives against the 101-chunk labeled junk set before
  any re-index.

`parsers/table.rs` design: pure core `reflow_tables(words, layout_text)` (no IO,
unit-tested with canned word-box fixtures) + thin IO shell in `pdf.rs` (runs
`pdftotext -bbox`, parses word boxes, recomputes page_offsets). Algorithm:
lines→detect(≥3 consecutive ≥2-col lines)→logical rows(serial starts a row,
empty col-1 = wrapped continuation)→linearize headers-folded, verbatim cells.
Confident-only fallback: ambiguous region keeps original -layout text.

Locked decisions (2026-06-17):
1. Row format = **headers folded in**.
2. **`ECHO_TABLE_REFLOW` env toggle, default ON** (A/B + revert without rebuild).
3. **R1 + R2 built together** this cycle (one re-index); dry-run gate is the R2
   necessity check.

## Outcome

- ✅ Impact: US8 re-scoped and design-of-record set in spec + roadmap; ready for TDD.
- 🧪 Tests: none yet — implementation will be test-first (pure-core fixtures).
- 📁 Files: roadmap doc + spec updated; this PHR.
- 🔁 Next prompts: implement R1 `parsers/table.rs` via TDD, then R2 boilerplate flag + dry-run.
- 🧠 Reflection: user's "why guards?" challenge correctly collapsed the year-guard into R1 and forced flag-not-delete safety on R2 — a real design improvement from the dialogue.

## Evaluation notes (flywheel)

- Failure modes observed: prior US8 (TAB band-aid in text_normalizer) was insufficient — confirmed live; root cause is lost column geometry, fixable only at extraction time with -bbox.
- Graders run and results (PASS/FAIL): n/a (design stage).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): pure `reflow_tables` against a Delegation-§3 word-box fixture before any pdf.rs wiring.
