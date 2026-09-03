---
id: 132
title: Split a compendium into one source per legal instrument — detect, review, execute
stage: green
date: 2026-07-30
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: "now split the compendium"
labels: [ingestion, compendium, instrument-split, admin, migration, e2e]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/services/instrument_detector.rs
  - src-tauri/src/services/instrument_split.rs
  - src-tauri/src/services/indexer.rs
  - src-tauri/src/api/admin.rs
  - src-tauri/src/api/uploads.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/db/pg/migrations/018_graph_delete_cascade.sql
  - src-tauri/src/db/pg/migrations/019_split_provenance.sql
  - src/pages/Admin.tsx
  - src/components/IndexedFolders.tsx
  - src/api/transport.ts
  - scripts/detect_instruments.py
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib --no-default-features --features server: 873 passed, 0 failed"
  - "instrument_detector: 13 unit tests + #[ignore] corpus_audit + page_count_agrees_with_pdfinfo"
  - "instrument_split: 9 unit tests + #[ignore] cuts_a_real_compendium"
  - "chunks::deleting_a_source_directly_cascades_through_the_graph (control-tested: fails with PG 23503 without migration 018)"
  - "end-to-end on a shadow server (isolated NOVIQ_DATA_DIR + own PostgreSQL): upload -> detect -> inspect -> split -> DB/disk verified"
  - "tsc --noEmit clean; vite build clean"
---

## Prompt

now split the compendium

(Preceded in the same session by: fix all three [extraction defects] · run the
detector on all 19 documents · fix the TOC reconciliation · fix the FK migration ·
build the admin review flow · build the split execution · add editing ranges ·
wire detection into the upload path · test the split end to end on a copy.)

## Response snapshot

Built detect → review → execute for multi-instrument documents, then cut the real
Compendium into 12 verified pieces WITHOUT applying it, because prod does not
have this code and applying it would require a deploy that auto-ships every
commit of the day at once.

The unit of indexing should be the INSTRUMENT, not the file. A corpus audit put
the scope beyond argument: 1 of 19 documents is a container, and over-splitting
is the real danger — four documents restart their numbering for Schedules and
must stay whole.

## Outcome

- ✅ Impact: the Compendium resolves to 12 instruments with derived page offsets
  and citable names; migration 018 makes deleting a source cascade through the
  legal graph (two call sites were failing silently); the extraction fixes
  recover 13 provisions. All held for post-beta deployment.
- 🧪 Tests: 873 lib tests green, plus two ignored real-corpus audits and a
  full end-to-end run against an isolated shadow server.
- 📁 Files: 12 source + migrations + changelog.
- 🔁 Next prompts: deploy after beta testing, then re-index; apply the split
  through the reviewed UI; run Build-Summaries + Rebuild-graph afterwards.
- 🧠 Reflection: **every serious defect this session was found by running the
  real thing, never by reasoning.** Four in the split alone — lopdf cannot open
  the target document, the detector proposed a plan its own executor refused, the
  split named nothing because `index_sources` returns an empty id, and the
  indexer overwrote the citation string. All four passed unit tests and review.

## Evaluation notes (flywheel)

- Failure modes observed: (a) fixing the trailing-page bug in the obvious way
  broke the opposite case (a genuinely blank last page); (b) the first crop fix
  earlier in the session traded 5 recovered provisions for 53 destroyed titles.
  Both were caught by measuring against real documents rather than reasoning.
- Graders run and results (PASS/FAIL): PASS — corpus container count 1/19 stable;
  page counts agree with `pdfinfo` on all 19; end-to-end split produced distinct
  citable labels per instrument.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after deploying, split the Compendium
  through the UI and re-run `scripts/corpus_gap_check.sql` per constituent
  instrument — gap reports become meaningful only once each law is its own source.
