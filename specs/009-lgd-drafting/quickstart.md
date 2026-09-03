# Quickstart: LGD Summary & Note Drafting

How the feature is exercised end-to-end once implemented. Offline; uses the existing indexed corpus in `echo.db`.

## Prerequisites
- App built and running (`npm run tauri dev`).
- Corpus indexed (rules: RoB 1986, SPPRA, SLGA, Financial Powers Rules, Dev Projects Manual; plus the case documents for a scheme).
- At least one uploaded format to turn into a template.

## A. Set up a template (once)
1. Open the **Drafting** surface → **Templates** → **New from format**.
2. Choose the format file (PDF/DOCX/TXT/MD); pick instrument type (Summary or Note); mark "filled example?" if it's a real past instrument.
   - Ingestion uses the local segmentation model (Mistral-7B in `models/`); it runs once, offline, for a few minutes with progress shown. If the model is missing, ingestion refuses with an actionable error (no schema is produced).
3. The app shows a **proposed slot schema** — each region tagged boilerplate / cited / prose, each cited slot with its verbatim provision reference and a required-source category derived from it. Gate-rejected regions appear marked **low-confidence with the rejection reason** for your decision.
4. Correct anything (re-tag a region, fix a required-source category), then **Confirm & Save**.
   - A filled example is additionally stored as a gold/style reference and **never** enters retrieval.

## B. Draft a Summary/Note
1. **New draft** → select a saved template.
2. Answer the short **intent questions** (instrument, addressee/routing, purpose, scheme/subject).
3. Review the **proposed document set** (whole-corpus candidates, each tagged rules/case). **Add/remove/retag** by selection — this curation is the scope.
4. Coverage is checked against document **content** (not titles), in two tiers:
   - A category covered by an indexed document not yet in your set → a **suggestion**: "add Rules of Business 1986?" — accept in one turn.
   - A category covered by nothing in the corpus → a **gap**: "index this document in `<watched corpus folder>`" with an openable link. Drop the document there → it gets indexed → the **doc set updates** and you're notified it'll be used; **or** choose **Proceed without** (those slots will be flagged).
   - A **fact slot** with no covering case document prompts you to point to a case document **or type the value** — typed values render with an explicit "officer-supplied" attribution, never as a citation.
5. **Confirm the document set** → **Render draft**.

## C. Review the output
- Output is ordered, labelled blocks:
  - **Boilerplate** — verbatim.
  - **Cited** — verbatim quote + full hierarchical citation (from chunk metadata).
  - **Fact** — extracted value + case-document attribution.
  - **Prose** — connective narrative (contains no numbers/citations).
  - **Flagged** — "unverified — source not available" (visually distinct).
- **Export** (txt/pdf) to copy/forward.

## Acceptance smoke (maps to Success Criteria)
- [ ] SC-001: spot-check every cited block's number + quote against its source document → exact match.
- [ ] SC-002: a template whose cited slot has no available source → that slot is **flagged**, not filled.
- [ ] SC-003: a produced draft needs zero manual citation corrections before forwarding.
- [ ] SC-004: after template setup, a full draft is produced without uploading any substantive document mid-draft.
- [ ] SC-005: dropping a missing document updates the set without restarting the draft.
- [ ] SC-007: a gold example never appears as a citation/fact source.
- [ ] FR-010/010a (2026-06-10): ingesting the real gold puts the actual citations (Rule-6(c) SPSC Reg-2023, Rule-23 SCUG 2020) into cited slots verbatim; letterhead/dateline/C.C./signature are never cited slots; gate rejections show low-confidence + reason.
- [ ] FR-010b: with no segmentation model on disk, ingestion refuses with an actionable error — no schema.
- [ ] FR-019 (2026-06-10): on the 8-PDF corpus, "consultations" coverage resolves to RoB 1986 by content as a one-turn suggestion — not a missing-document gap.
- [ ] FR-008a: a typed fact value renders with "officer-supplied" attribution, visually distinct from cited blocks.

## Conversational session walkthrough (FR-027–FR-034)

The flow is one chat transcript; the draft/doc panels update live alongside it.

1. Open Drafting on an indexed corpus → the transcript greets and asks the first intent question.
2. Officer answers turn by turn (instrument, addressee, purpose, scheme). Each turn is parsed to a `DraftAction`; when intent is complete the system proposes the document set **in the conversation** and the doc panel fills.
3. Officer curates by chatting ("drop the 2019 circular", "add the PC-I") → `update_doc_set`; gaps are reported as system turns.
4. Officer resolves gaps ("treasury rules are unavailable, proceed") → flagged slots noted.
5. Officer confirms → draft renders as blocks; officer refines by chatting ("shorten paragraph 2") → targeted re-render, citations untouched.
6. Officer finalizes → locked output, exportable.
7. Close and reopen the app → `load_conversation` restores the full transcript + state; long sessions show only a summary + recent turns to the model, with every raw turn still in the DB.

**Manual checks**
- [ ] FR-031: every officer + system turn appears in `chat_message` (query by `draft_session_id`).
- [ ] FR-032: after restart, transcript + step/doc/gap/blocks state are restored.
- [ ] FR-033: with a long session, the model context = summary + recent turns (not the whole transcript); raw turns still present.
- [ ] FR-028: no chat turn ever changes a citation/number/fact; all citations resolve from chunk metadata.

## Run tests
```bash
cd "/media/riaz/New Volume/echo-AI"
cargo test --test lgd_drafting_acceptance        # feature acceptance (deterministic citation paths)
cargo test drafting                              # unit tests for services/drafting
```
