---
id: 082
title: Support-while-writing, grounding precision, slash commands
stage: green
date: 2026-07-10
surface: agent
model: claude-opus-4-8
feature: 022-notice-assist
branch: release
user: Riaz Mehmood
command: implement
labels: [notice-assist, grounding, ocr, progress-ring, slash-commands, ui]
links:
  spec: specs/022-notice-assist/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/assistant/capabilities/notice_assist/compose.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/mod.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/extract.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/frame.rs
  - src-tauri/src/services/assistant/intent.rs
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/services/indexer.rs
  - src-tauri/src/api/documents.rs
  - src-tauri/src/api/router.rs
  - src-tauri/sidecars/ingestion-sidecar/ingestion_sidecar.py
  - src/components/ProgressRing.tsx
  - src/components/ConfidentialComposer.tsx
  - src/pages/ChatPage.tsx
  - src/pages/SettingsPage.tsx
  - src/App.tsx
  - src/api/transport.ts
  - src/api/index.ts
tests:
  - "VM cargo check --release --features llm,server,cuda = EXIT 0"
  - "VM cargo test --lib -- assistant::intent assistant::capabilities::notice_assist = 38 passed / 0 failed"
  - "frontend npm run build (tsc + vite) = EXIT 0"
  - "prod sidecar OCR on SC02 page: backend=rapidocr, 1703 chars, mean_conf 0.943"
  - "POST /api/index_progress -> 200 [] (idle source); 401 without token"
---

## Prompt

(session, in order) verify the pdfplumber/lopdf/PaddleOCR/tesseract pipeline · index the rules ·
push the @-mention and `/` features to the browser so I can eval on the real corpus · admin
"My documents" and folder panes both show every document · attach SC02: ring works, but it still
can't extract the rule, copies the first para every turn, asks too many questions, and just writes
the same things — I don't want a ready-made reply, I want support while I write, and the rules that
protect me · the latest query cited rules that aren't in the attached document · remove the "Noviq"
wordmark from the top bar (logo only) and drop the duplicate logo in chat · after `/cancel` it ran
LLM inference and returned an answer — from where?

## Response snapshot

**Pipeline verified (two corrections).** Real ladder is pdfplumber → pdftotext(+`-bbox` reflow) →
lopdf → OCR; the bbox reflow is on the *pdftotext* tier, not lopdf. **PaddleOCR was dead**: the VM
vCPU is masked to Core2 (`sse4_2`, no AVX) so `import paddle` raises SIGILL and **kills the process**
(uncatchable) — even the official `noavx` 2.4.2 wheel SIGILLs on the first matmul. Everything had
been silently falling back to **tesseract**, which destroyed the allegation block in real scans. Fixed
by running the **same PP-OCR models on ONNX Runtime** (`rapidocr-onnxruntime`) — SC02 now yields 1703
chars @ 0.943 conf with the full allegation.

**Grounding precision (two real bugs).** `ground_one` matched the whole reference sentence verbatim →
every rule `found=false`; now `provision_tokens()` splits into `Rule 4` / `Rule 4A(1)` / `Rule 5(3)`.
And `Rule 4` exists in many statutes → wrong-Act citations; now `instrument_scope()` pins retrieval to
the statute the notice names (an unresolvable statute is a MISS). Tests caught two more parse bugs
(`Rules-1973` → "Rule 1973"; `sub-rule (3) of rule 5` → "Rule 3").

**Product reshaped** (architect chose *no questions* + *interactive co-writing*): removed `ask.rs`
(questionnaire) and `draft()` (template letter). Now `ground_support()` retrieves the provisions that
**protect** the officer from the same statute; `support_brief()` shows charge + invoked rule + those
provisions, all verbatim/cited; `Phase::CoWrite` critiques each paragraph he writes
(Support / Needs evidence / Risk / Sharper wording) and declines to write the reply.

**Attach UX:** new `ProgressRing` (determinate/indeterminate); real upload % via `XMLHttpRequest`
(`fetch` can't); real index % via a new **tunnel-safe** `POST /api/index_progress` backed by
`indexer::latest_progress()` (the quick tunnel buffers SSE to nothing). A source with no tick is
absent from the response → indeterminate ring, never a fake 0%.

**Slash commands:** `/cancel` outside assist mode ran a 38.8 s RAG turn and answered from SPPRA about
"cancellation of a bidding process" (proven in `query_log`). `intent::is_slash_command()` +
`is_assist_exit_command()`; `run_turn` routes every slash command away from retrieval; unknown
commands answer instantly with zero inference.

**Admin settings:** admin uploads are global (`owner_id NULL`) so `myFolders()` (owner-name match)
dropped them — indexing had succeeded silently. First fix matched by path, which **duplicated the
whole corpus** into "My Documents"; reverted to owner-scope + an admin explainer; usage bar hidden for
admins (always 0). Folder input was permanently `webkitdirectory`, so "index a file" was impossible →
split into "Add files" / "Add folder".

**Eval golds** downloaded to `Desktop/notices/` (17 PDFs, Sindh + AGP only): 11 show-cause notices
(10 are scans), 4 AGP audit reports, and the two rulebooks — **RSO 2000** (SC12) and **E&D Rules
1973** (SC01–SC10), both since indexed shared (172 + 80 chunks).

## Outcome

- ✅ Impact: `/help` now grounds the right statute, arms the officer with protective provisions, and
  co-writes instead of templating; OCR actually works on real Sindh scans; slash commands are inert.
- 🧪 Tests: 38 passed / 0 failed (notice_assist + intent); cargo check EXIT 0; npm build EXIT 0.
- 📁 Files: 10 Rust/py, 7 TS.
- 🔁 Next prompts: audit-para flow (para picker + audit rulebooks) — design discussion open;
  tune `ground_support` topic queries; T002/T027 move prompt+templates to TOML.
- 🧠 Reflection: three of the four "model is bad" complaints were **data-path bugs** (tesseract
  destroying the text, whole-sentence rule matching, corpus-wide retrieval). Verify the pipeline
  before blaming the model.

## Evaluation notes (flywheel)

- Failure modes observed: silent OCR fallback masking a fatal SIGILL; verbatim matching at the wrong
  granularity; retrieval unpinned from the cited instrument; unknown slash commands treated as queries.
- Graders run and results (PASS/FAIL): VM cargo check PASS; targeted tests PASS (38/0); npm build PASS;
  live OCR PASS (0.943 conf); `index_progress` PASS.
- Prompt variant (if applicable): extraction prompt gained strict QUESTION RULES (later superseded —
  questions removed entirely).
- Next experiment (smallest change to try): attach SC02 fresh, confirm `Rule 4 — Penalties` is quoted
  from **E&D 1973** (not SPPRA) and that "Provisions in your favour" is non-empty.
