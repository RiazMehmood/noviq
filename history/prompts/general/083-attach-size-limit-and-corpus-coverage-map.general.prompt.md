---
id: 083
title: Attachment size limit and audit-para corpus coverage map
stage: general
date: 2026-07-10
surface: agent
model: claude-opus-4-8
feature: 022-notice-assist
branch: release
user: Riaz Mehmood
command: (freeform)
labels: [f022, attachments, corpus, retrieval, docs]
links:
  spec: specs/022-notice-assist/
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/api/uploads.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/extract.rs
  - docs/corpus-coverage-checklist.md
  - CHANGELOG_DEV.md
tests:
  - src-tauri/src/api/uploads.rs::tests (pdf_page_count / attach_limits) — pending VM run
---

## Prompt

Across the session the architect asked, in sequence:
1. "we need to put limit for attached document, if user wants full document then ask user to index
   that documents in my document section instead of attachment, so small document allowed in
   attachment. the next issue is documents and rules stated inside audit paras how we get them?"
2. "yes only rulebooks no audit para extraction needed llm will understand this if document is small"
3. "i dont have those rulebooks."
4. "what are the possible rule books needed for our application as this is going to be used in sindh
   govt? discuss"
5. "Yes, make the corpus coverage checklist"
6. "search the remaining documents so we can add those too, then commit all work. and push too write
   necessary documents too"

## Response snapshot

**Attachment limit (code):** Added `attach_limits()` (env `NOVIQ_ATTACH_MAX_PAGES`=15,
`NOVIQ_ATTACH_MAX_MB`=8) and `pdf_page_count()` (lopdf structure-only, no OCR) to `api/uploads.rs`;
`attach_document` now rejects oversize files with 413 `attachment_too_large` pointing to My
Documents (byte cap first, then page cap for PDFs). `upload_documents` (My Documents) left unchanged.
Raised `notice_assist/extract.rs` `MAX_DOC_CHARS` 14_000 → 40_000 so a capped-small attachment is
read whole (the coupling that makes "no para picker, LLM reads it if small" actually work). 3 unit
tests added.

**Audit-para rules:** Established there is no extraction/para-picker to build — the fix is corpus
coverage. `pdftotext` over the 4 AGP gold reports showed the real citation gap is two books (GFR
Vol-I ~44, Sindh Financial Rules Vol-I ~25); SPPRA (~70) is already indexed. Referenced *evidence*
documents (vouchers/sanctions) are officer-supplied, not retrieved — already handled by CoWrite.

**Docs:** Wrote `docs/corpus-coverage-checklist.md` — tiered rulebook map for a Sindh govt officer
with per-book definition-of-done, guardrails, and (after web search) located official `.gov.pk`
source URLs S1–S10 for every missing book, flagged edition-unverified. Corrected the conduct rules to
**Sindh Civil Servants (Conduct) Rules, 2008**. Added CHANGELOG_DEV entry.

## Outcome

- ✅ Impact: attachments bounded to small docs (redirect to My Documents); audit-para path reframed as
  a corpus-coverage task with sources located; coverage map is now a tracked, tick-off-able doc.
- 🧪 Tests: 3 new unit tests (pdf_page_count, attach_limits) — **pending VM `cargo check`/`cargo test`.**
- 📁 Files: uploads.rs, extract.rs, docs/corpus-coverage-checklist.md (new), CHANGELOG_DEV.md.
- 🔁 Next prompts: VM build+test; add `provision_tokens()` `Para N` variant; edition-verify + index
  GFR Vol-I and Sindh Financial Rules Vol-I shared.
- 🧠 Reflection: capping the attachment small dissolved the para-picker requirement entirely — the
  cheaper guard removed the need for the expensive feature.

## Evaluation notes (flywheel)

- Failure modes observed: unbuilt Rust cannot be verified locally (no Windows cargo); pushing to
  `release` auto-deploys, so committed on a side branch instead.
- Graders run and results (PASS/FAIL): N/A (not built).
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): run VM `cargo test --lib uploads:: notice_assist::`,
  then index GFR Vol-I and validate detection on AP01/AP03 (which cite GFR/SFR).
