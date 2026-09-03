---
id: 157
title: GPU OCR measured and rejected
stage: explainer
date: 2026-08-15
surface: agent
model: claude-opus-5
feature: none
branch: deploy-idx
user: Riaz Mehmood
command: (conversation)
labels: [ops, ocr, gpu, benchmark, indexing]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
  - src-tauri/sidecars/ingestion-sidecar/ingestion_sidecar.py (opt-in ECHO_OCR_GPU path)
  - CHANGELOG_DEV.md (2026-08-15 entry)
tests:
  - py_compile on the patched sidecar (VM, live venv): PASS
  - provider readback det/cls/rec == CUDAExecutionProvider: PASS
  - A/B benchmark CPU vs CUDA on 4 identical pages, idle box: CPU 2.9 s/page, CUDA 10.0 s/page
---

## Prompt

why tesseract we are using paddleocr

(then) and why it is running on cpu as we enabled gpu

(then) and we are using pdfplumber for other works

(then) pdftotext is only for checking document construction to identify if any
document contains multiple documents?

(then) why ocr sidecar is not on gpu i want it also on gpu

(then) what is going on explain it simply.

(then) so what is the progress? how much documents completed and how much are
remaining?

(then) 1   [= leave the run as it is]

## Response snapshot

Corrected my own error first: the run uses **PaddleOCR** via the sidecar, not
tesseract — I had quoted the stale module doc-comment at `parsers/ocr.rs:1`
instead of reading the journal, which shows `PaddleOCR extracted … chars` twelve
times. Tesseract is only the fallback.

Explained the ladder (**pdfplumber → pdftotext → lopdf → PaddleOCR → tesseract**)
and that pdftotext is not only multi-instrument detection — it is tier 1 and the
referee that refuses tier 0 when pdfplumber finds fewer provision anchors.
`detect_multi_instrument` runs only on the upload path, so it is not part of this
load at all.

OCR ran on CPU because `ECHO_EMBED_GPU=1` switches only the Rust embedder; OCR is
a separate Python process whose venv held the CPU `onnxruntime` wheel.

Built the GPU path as asked: `onnxruntime-gpu==1.19.2` in a fresh venv, det/cls/
rec all verified on `CUDAExecutionProvider`. **CUDA measured 3.4× SLOWER**
(10.0 vs 2.9 s/page, idle box, matching char counts). First benchmark had said
GPU won 1.8× — an artefact of running it while the corpus load saturated the CPU
**baseline**. Did not enable it; left it opt-in behind `ECHO_OCR_GPU=1` with
provider readback logging.

Reported progress (796/3,503, 13/39 departments, ETA 20–30 h) and offered three
options; architect chose to let it run with chat down.

## Outcome

- ✅ Impact: OCR stays on CPU on evidence; GPU path exists but is off. Corpus run
  restarted and healthy after a 30 min diagnostic pause.
- 🧪 Tests: A/B benchmark on identical pages; provider readback; py_compile.
- 📁 Files: sidecar patch (installed on the VM, backup at
  `/tmp/ingestion_sidecar.py.bak`), CHANGELOG_DEV entry.
- 🔁 Next prompts: parallel `pdftoppm` in `ocr.rs::rasterise`; `gpu-release.sh`
  when the load finishes; re-test the project upload fix.
- 🧠 Reflection: a benchmark run on a busy box measures the box. The baseline was
  the contended half, which is exactly how a 3.4× loss looked like a 1.8× win.

## Evaluation notes (flywheel)

- Failure modes observed: quoted a stale doc-comment as current behaviour;
  published a contended benchmark before re-checking it quiet.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): split one document's rasterisation
  across 4 `pdftoppm` page ranges and re-time a scan-heavy department.
