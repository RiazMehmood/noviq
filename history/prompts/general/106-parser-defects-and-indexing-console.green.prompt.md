---
id: 106
title: Parser extraction defects and document indexing console
stage: green
date: 2026-07-27
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (conversation)
labels: [parser, pdfplumber, ingestion, citations, admin-ui]
links:
  spec: none
  ticket: trace a72f1539
  adr: none
  pr: none
files:
  - src-tauri/sidecars/ingestion-sidecar/ingestion_sidecar.py
  - src-tauri/src/services/parsers/front_matter.rs
  - src-tauri/src/services/parsers/table_sidecar.rs
  - src-tauri/src/services/parsers/pdf.rs
  - src-tauri/src/services/parsers/mod.rs
  - src-tauri/src/services/doc_profiles/{manual,act,rules}.rs
  - src-tauri/src/services/structured_importer.rs
  - src-tauri/src/api/{admin,documents,router}.rs
  - src-tauri/src/services/assistant/{mod.rs,capabilities/*}
  - src/components/{ChunkViewer,AnswerBody}.tsx
  - src/pages/Admin.tsx
tests:
  - cargo test --lib --no-default-features --features server (714 pass / 69 pre-existing fail)
  - baseline in same worktree (696 pass / 69 fail) — failure set identical
  - npx tsc --noEmit — clean
  - corpus sweep over all 18 PDFs — zero anchor regressions
---

## Prompt

"i want a full solution, first plan correctly, search the net if required, but a
single fix for all defects and thats it, and i also want a proper document
indexing tab where we can easily work around a document we can download
individual document chunks also view them, and properly select multiple document
for deletion also, so we need to work on both sides perfectly."

Follow-ups: "search the net how notebooklm correct these type of things? and
citation or only availble during hover but as we click it it does not show
anything but notebooklm shows its content also"; "dont only check that on
constitute only, on all corpus where defects exist."

## Response snapshot

Established ground truth in code before planning, having twice planned on an
unverified assumption earlier in the thread. Confirmed pdfplumber (not pdftotext)
is the production extractor, then measured Articles 50–80 anchoring in the real
Constitution body: 19/31.

Found **four** defects, not one, and said so rather than claiming a single fix:
D1 superscript fusion, D2 marginal-catchline column flattening (the dominant
one, 10 of 31), D3 dot-leader-less TOC minting fake Articles, D4 amendment
brackets after the dot. Cumulative: 19 → 21 → 28 → **31/31**.

The architect's instruction to check the whole corpus changed the design. The
first D2 implementation regressed two documents (60→33, 37→30) that use a hanging
indent — geometrically identical to a catchline margin. Replaced the geometric
proxy with `_crop_preserves_anchors`, which measures the crop's own effect and
refuses to apply if the provision count drops.

D3 similarly failed twice: it broke the existing
`mid_body_dotted_line_is_not_front_matter` test, and length cannot separate TOC
titles (median 32, max 56) from body lines (median 54). Replaced shape/density
with a semantic test — a contents entry points at a provision recurring below it
(94–100% real vs 0% body).

Then built the console: paginated chunk browse, bulk delete with a server-computed
preview, and one `ChunkViewer` drawer serving both the admin console and citation
click-through. Discovered `browse_chunks` is admin-gated while officers click
citations, so added an officer-scoped `POST /api/source_chunks` behind
`visible_source_ids`.

## Outcome

- ✅ Impact: Articles 50–80 anchoring 19/31 → 31/31 through the production
  extractor; zero regressions across all 18 corpus documents; 313 provision titles
  now populate the long-dormant `DocNode::title`; citations open their source
  passage on click.
- 🧪 Tests: 714 pass / 69 fail vs a 696 / 69 baseline measured in the same
  worktree; failure set identical (pre-existing DB tests, `initdb` needs
  `libssl.so.3`). tsc clean.
- 📁 Files: 54 committed as `ef776876`; sidecar binary rebuilt and installed by
  hand (gitignored, so the deploy cannot carry it).
- 🔁 Next prompts: re-index the corpus from the Admin UI and verify the label-run
  and unresolved-count queries; investigate the two zero-character scanned PDFs.
- 🧠 Reflection: every wrong turn this session came from validating a fix on the
  case that motivated it. The corpus sweep caught two regressions the Constitution
  alone would have hidden.

## Evaluation notes (flywheel)

- Failure modes observed: (1) planning on an unverified premise — retracted two
  diagnoses; (2) a bulk-patch script matching `pub struct X {` and `-> X {` as
  struct literals; (3) reading an exit code from the wrong end of a pipeline
  (`PIPESTATUS` on a subshell) — the same adjacent-signal error as a prior session.
- Graders run and results: corpus anchor sweep PASS (zero regressions);
  baseline-vs-change test diff PASS (identical failure set).
- Prompt variant: n/a
- Next experiment: add a Constitution fixture to
  `tests/fixtures/structured_indexing/` so these defects are caught by CI rather
  than by a production trace.
