---
id: 0013
title: Phase 9b PaddleOCR scan sidecar
stage: green
date: 2026-06-21
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: Riaz Mehmood
command: "go for 9b"
labels: [ingestion, ocr, sidecar, paddleocr, scans, deterministic]
links:
  spec: specs/013-ingestion-hardening/phase9-extraction-sidecars.plan.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/sidecars/ingestion-sidecar/ingestion_sidecar.py
  - src-tauri/sidecars/ingestion-sidecar/setup-ocr-venv.sh
  - src-tauri/src/services/parsers/ocr_sidecar.rs
  - src-tauri/src/services/parsers/ocr.rs
  - src-tauri/src/services/parsers/pdf.rs
  - src-tauri/src/services/parsers/mod.rs
tests:
  - "cargo test --lib: 533 passed / 0 failed / 2 ignored"
  - "smoke_ocr_images (#[ignore], real venv): reads '2013', conf 0.98-0.99"
  - "sidecar ocr op end-to-end on scan pages 3 & 7 (correct numbers, no oneDNN crash)"
---

## Prompt

go for 9b

(Clarified: runtime = Python venv, NOT frozen — Paddle is heavy/finicky to freeze and the IBA GPU
server runs Python directly. Plan: `phase9-extraction-sidecars.plan.md`.)

## Response snapshot

Built Phase 9b end-to-end:
1. **`ocr` op** in the ingestion sidecar — PaddleOCR 3.7 over page images → per-page text +
   mean_conf. Key fix: `enable_mkldnn=False` **constructor kwarg** dodges the PaddlePaddle 3.x
   oneDNN/PIR crash (the env var `FLAGS_use_mkldnn=0` is ignored by the PIR executor).
2. **`setup-ocr-venv.sh`** — reproducible PaddleOCR venv.
3. **`ocr_sidecar.rs`** — Rust client (discover venv python + script, spawn, JSON, timeout, fall
   back to tesseract on any failure).
4. **`ocr.rs`** — `paddle_ocr_enabled()` (`ECHO_PADDLE_OCR=1`, default off) + `ocr_pdf_paddle`
   (reuses `rasterise`, `extraction_method="paddleocr"`).
5. **`pdf.rs`** — Tier-3 prefers Paddle when enabled+available, else tesseract.

Verified on the one real scan (`04_Sindh_Local_Government_Act_2013.pdf`): tesseract's invented
`2613`/`7013` read correctly as `2013`; conf 0.98–0.99; ~1.9 min/page CPU.

## Outcome

- ✅ Impact: scanned docs can be OCR'd without tesseract's number fabrication (the "Section 20413"
  false-citation root cause), opt-in and with safe tesseract fallback.
- 🧪 Tests: `cargo test --lib` 533/0/2; sidecar op + Rust client verified live on the scan.
- 📁 Files: sidecar `ocr` op + venv script, Rust client, ocr/pdf wiring.
- 🔁 Next prompts: full app re-index of doc 04 with `ECHO_PADDLE_OCR=1`; GPU paddle build on IBA
  server; low-confidence → `label_status='unresolved'` flag.
- 🧠 Reflection: the venv-vs-freeze call was right — freezing Paddle would've been a multi-hour
  yak-shave for zero benefit on a Python-capable server.

## Evaluation notes (flywheel)

- Failure modes observed: oneDNN crash (`ConvertPirAttribute2RuntimeAttribute`) — fixed by
  `enable_mkldnn=False` kwarg, NOT the documented env var (3.x PIR executor ignores it).
- Graders run and results (PASS/FAIL): scan-page numeric check PASS (Nov 02 2013 / 02, 2013 correct;
  no 2613/7013); Rust client smoke PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): GPU paddlepaddle build on the IBA server → measure
  per-page latency vs the ~1.9 min CPU baseline.
