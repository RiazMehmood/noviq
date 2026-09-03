---
id: 0006
title: Implement P1 D5 D6 zoning
stage: green
date: 2026-06-19
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: Riaz Mehmood
command: /sp.implement
labels: [ingestion, P1, D5, D6, footnotes, page-numbers, chrome]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/parsers/chrome.rs
  - src-tauri/src/services/structured_importer.rs
  - specs/013-ingestion-hardening/tasks.md
tests:
  - cargo test --lib chrome
---

## Prompt

/sp.implement — continue into Phase 3 (P1 geometry zoning, D5/D6). User directed: "u directly
run the phase 3" (proceed without further questions) and reaffirmed build only at FEATURE END.

## Response snapshot

Discovered the fast pipeline carries NO geometry (`Block.y_band` is always `None`; bbox isn't
threaded into `strip_chrome`), so the plan's "geometry zoning" can't use margins on this path.
Chose Approach B (text patterns, fast-path friendly) — recommended — and implemented it edit-only:

**chrome.rs**
- `BlockClass::Footnote` (+ `as_str` "footnote") and `ChromeReason::PageNumber`.
- `is_bare_integer` + `page_number_blocks`: a line that is ENTIRELY a bare integer, recurring across
  ≥2 distinct pages (a footer run) → `Chrome(PageNumber)`. A lone bare integer survives (keep-bias);
  numbers *inside* legal text are never matched (D6 fix; spec edge case honoured).
- `is_footnote_apparatus`: amendment-history lines (amendment verb + instrument/effect token, or
  `w.e.f`) → `BlockClass::Footnote` (D5). Conservative co-occurrence guard so prose saying
  "substituted" alone is untouched.
- `classify_blocks`: footnote + page-number checks added after the keep invariant.
- `FootnoteBlock` struct + `StripResult.footnotes`; `strip_chrome` now removes BOTH chrome and
  footnote lines from `cleaned` (shared `removed_lines`, offsets realigned) and returns footnotes.
- 6 new unit tests (page-number run, lone-integer survival, legal-phrase-with-number, footnote
  classification, footnote isolation-but-kept, prose-"substituted" guard).

**structured_importer.rs**
- `import()` captures `r.footnotes`; `ImportOutcome.footnote_blocks`; `persist()` calls new
  `persist_footnote_chunks` → `block_class='footnote'`, `page_number`, KEPT retrievable (not
  boilerplate), `label_status='unresolved'` (marker anchor deferred — never same-page guess).

Grep-verified: both `StripResult` literals carry 4 fields; no external exhaustive `BlockClass`
match to break (the one internal match has a `_` arm); `persist_footnote_chunks` defined+called;
existing chrome tests use no bare-integer/amendment inputs so they shouldn't reclassify. NOT compiled.

## Outcome

- ✅ Impact: D6 (page-number leakage) and D5 (footnotes captured as provision text) addressed on the
  digital docs without geometry; footnotes kept as a separate retrievable class (enables US5).
- 🧪 Tests: 6 new chrome unit tests added; NOT run (build at feature end).
- 📁 Files: chrome.rs, structured_importer.rs, tasks.md.
- 🔁 Next prompts: Phase 4 (P2 structural tree, D2/D3/D4/D10) — articles incl. alpha-suffix, Chapter
  nodes, catchline merge, unit_type, 25/25A flag.
- 🧠 Reflection: Plan assumed geometry; fast path has none. Surfaced the A/B fork, took the safer
  text-pattern route per user's "just proceed". Footnote over-capture on continuation lines is the
  main risk to watch at the end-of-feature re-index.

## Evaluation notes (flywheel)

- Failure modes observed: potential footnote over-capture (mid-provision continuation line with
  amendment wording); page-number rule needs ≥2-page run (single-page docs unaffected).
- Graders run and results (PASS/FAIL): grep arity/consistency PASS; not compiled.
- Prompt variant (if applicable): n/a
- Next experiment: at feature-end build + re-index law doc; check footnote count + that no provision
  contains "Subs. by"/page integers; tune `is_footnote_apparatus` if over-capturing.
