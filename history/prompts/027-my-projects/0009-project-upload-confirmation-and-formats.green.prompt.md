---
id: 0009
title: Project upload confirmation and supported formats
stage: green
date: 2026-08-11
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 027-my-projects
user: Riaz Mehmood
command: (conversational)
labels: [bugfix, f027, uploads, scraping, corpus-currency]
links:
  spec: specs/027-my-projects/spec.md
  ticket: n/a
  adr: history/adr/0028-project-scoping.md
  pr: n/a
files:
  - src-tauri/src/api/uploads.rs
  - src/components/AddProjectDocuments.tsx
  - src/api/transport.ts
  - src/lib/uploadFormats.ts
  - src/components/IndexedFolders.tsx
  - src/pages/ChatPage.tsx
  - scripts/corpus_scrape/common.py
  - CHANGELOG_DEV.md
tests:
  - "npx tsc --noEmit: PASS (clean)"
  - "cargo check --no-default-features --features server (VM, detached worktree): PASS, EXIT=0, 3m10s — covers 863ade71 only"
  - "deploy verification: binary mtime 13:59 -> 16:28:25, release profile 5m55s, service restarted 16:28:26, served bundle index-DIISBodu.js matches build"
  - "cargo: NOT RUN for 7af05eff (image OCR) — uncompiled, NOT deployed"
---

## Prompt

1. "we were also working on webscraping to extract some documents this work was
   also remains in between was not complete can u recall this?"
2. "https://rtw.sindh.gov.pk/knowledge-repository, https://www.sindhlaws.gov.pk/
   these are two website from which we need to extract the documents as much as
   possible and must be good for our noviq app indexing work."
3. "while uploading files in new project at starting it always shows a error. i
   saved the error screenshot in downloads folder with name error so check that
   also"
4. "pdf, text, images, ppt, excel and powerpoint and word files only, remove
   other option like voice video etc which are not supported currently"
5. "and also user unable to upload a 20mb file crpc file in project."

## Response snapshot

**Recall.** Found the stalled work: `docs/beta-launch/corpus-currency-and-scoping-audit.md`
(2026-07-26) audited corpus currency and listed 12 documents to obtain. Ten PDFs
were fetched into `docs/beta-launch/corpus-update/` (all timestamped 2026-07-26
18:43) and the effort stopped there — untracked in git, no CHANGELOG entry, no
PHR, and **nothing was ever indexed**. Outstanding: the court judgments (PLD 2024
SC 1276, the 3 Jul 2026 promotion judgment, Zahida Parveen, SPSC/OPS) and the
RSO 2000 repeal question.

**Site reconnaissance (both mapped end-to-end).**
- `sindhlaws.gov.pk` — ASP.NET GridView. Browse axes `Alpha|Chrono|DepartmentCode`
  × `ACT|ORDINANCE|RULE`, 30 rows/page, PDFs at
  `setup/Publications_SindhCode/PUB-*.pdf`. Pagination is `__doPostBack` +
  VIEWSTATE POST — **verified working** (page 2 returned 22 PDFs); a sample PDF
  downloaded clean (373 KB, `%PDF-1.5`).
- `rtw.sindh.gov.pk` — the visible page is a decoy; real data comes from
  `GET /pagination/fetch_data?page=N&sortby=&sorttype=&query=&<filters>`
  returning tbody HTML. 750 pages ≈ 15,000 records. ⚠️ Filters are ignored on
  the page URL and the checkbox value is `provincial=Yes`, NOT
  `provincial=provincial`. Columns: record_no, title, department, source, type,
  year, link. Links point at ~40 external hosts; `type=ACT` largely mirrors
  sindhlaws, so cross-site dedup is required.

Wrote `scripts/corpus_scrape/common.py` (two-phase enumerate→download, sha256
dedup, resume via manifest, 1 s politeness delay, `as_of` captured per audit
§C.3). Site modules not yet written — work paused for the bug reports.

**Bug 1 — every project upload reported failure.** Root-caused to the
interaction between two same-day changes: the (b) fix added `AND project_id IS
NULL` to `list_sources_in_folder`, and `AddProjectDocuments.tsx:76` was using
that very listing to confirm the upload. Fixed by having the server report
`attached: Vec<String>` in `UploadOutcome`; client trusts it and drops the
lookup plus the redundant `addDocuments` call.

**Bug 2 — format list.** New `src/lib/uploadFormats.ts` as the single list,
applied to all three upload surfaces plus a matching server-side boundary
rejection. Narrowed at the boundary, not in `FileFormat`.

**Report 3 — 20 MB CrPC.** Ruled out size limits by reading the code (no
per-file cap; 20 MB is under both the 256 MB route body limit and the 500 MB
quota). Noted that upload indexes synchronously inside the request. Not
diagnosed — exact error text and server log requested rather than guessed.

## Outcome

- ✅ Impact: project uploads now confirm honestly instead of reporting failure on
  success; upload pickers advertise only formats that actually index; both
  target sites fully mapped with their non-obvious access mechanics recorded.
- 🧪 Tests: `tsc --noEmit` clean. Rust unverified — no toolchain on Windows.
- 📁 Files: 6 source files + 1 new scraper module + CHANGELOG.
- 🔁 Next prompts: compile-check + deploy `7af05eff` (image OCR); run
  `harvest.py download` (~3,886 files, est. 1.5 GB) then index; get the 20 MB
  CrPC error text.

### Session end state (appended after the deploy)

- **`863ade71` DEPLOYED and verified** — compile-checked on a side branch first
  per the architect's rule, then pushed to `release`. Verified by artefacts, not
  the commit, because `deploy.sh` fails soft.
- **Enumeration complete: 3,886 unique documents** (sindhlaws 1,789 + rtw
  2,110; only 13 cross-site duplicates — RTW's sindhlaws links are
  `Setup/Publications/`, a different corpus from `Publications_SindhCode/`).
  Nothing downloaded yet.
- **`7af05eff` committed, NOT deployed** — images never reached OCR because
  `indexer.rs` passed `None` as the vision-model path. Scanned PDFs were always
  fine on every path. ⚠️ Literal PaddleOCR SIGILLs on the deploy VM (no AVX);
  the live path is RapidOCR/PP-OCR on ONNX, which the new image path joins.
- **Open, undiagnosed:** the 20 MB CrPC upload. Needs the error text.
- 🧠 Reflection: ⭐⭐ A hardening change that HIDES rows breaks every caller
  using that listing as an existence check. The (b) fix was correct; its blast
  radius was not checked. Grep for readers of a listing before narrowing it.

## Evaluation notes (flywheel)

- Failure modes observed: assumed the RTW page URL took filters — it silently
  ignored them and returned identical results, which reads as "filter has no
  effect" rather than "wrong endpoint". Confirmed only by reading the page JS.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): enumerate-only run on both sites to
  produce real counts before committing bandwidth or a scoping decision.
