---
id: 0012
title: Phase 9a pdfplumber table sidecar
stage: green
date: 2026-06-20
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: Riaz Mehmood
command: "now start 9a and 9b"
labels: [ingestion, tables, sidecar, pdfplumber, deterministic]
links:
  spec: specs/013-ingestion-hardening/phase9-extraction-sidecars.plan.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/sidecars/ingestion-sidecar/ingestion_sidecar.py
  - src-tauri/sidecars/ingestion-sidecar/build.sh
  - src-tauri/sidecars/ingestion-sidecar/BUNDLED_LICENSES.md
  - src-tauri/src/services/parsers/table_sidecar.rs
  - src-tauri/src/services/parsers/table.rs
  - src-tauri/src/services/parsers/pdf.rs
  - src-tauri/src/services/parsers/mod.rs
  - src-tauri/tauri.conf.json
  - src-tauri/tests/fixtures/table_sidecar/delegation_p8.json
  - .gitignore
tests:
  - "cargo test --lib: 529 passed / 0 failed / 1 ignored"
  - "golden_delegation_p8_reflow (fixture → parse_response → reflow_with_sidecar)"
  - "sidecar_reflow_* (3 unit tests in table.rs)"
  - "spike_extract_and_reflow (#[ignore] real-binary harness)"
---

## Prompt

now start 9a and 9b

(Clarified to scope **9a only**, shipped as a **PyInstaller frozen binary** — 9b
PaddleOCR deferred. Plan: `phase9-extraction-sidecars.plan.md`.)

## Response snapshot

Built the Phase 9a pdfplumber table sidecar end-to-end:

1. **Python sidecar** (`ingestion_sidecar.py`) — `extract_tables` op via pdfplumber
   (MIT). Per page: verbatim cell rows + header labels + table bbox + prose
   above/below the table band. One-shot CLI (JSON stdin → JSON stdout); any error
   → `{ok:false}` so Rust falls back. Tested directly on all 4 spike PDFs.
2. **Frozen binary** (`build.sh`, PyInstaller) → `binaries/ingestion-sidecar-<triple>`
   (29 MB), registered in `tauri.conf.json` externalBin, gitignored (rebuildable).
   Built on native fs to dodge ntfs-3g spurious ENOSPC.
3. **Rust client** (`table_sidecar.rs`) — discover (env / next-to-exe / dev
   `binaries/`) + spawn (`std::process::Command`, 300 s watchdog) + `parse_response`;
   every failure path returns `None` → projection-engine fallback.
4. **Reflow** (`table.rs::reflow_with_sidecar`) — pure; drops `(1)(2)(3)(4)`
   enumerator, collapses single-cell `Note:`/continuation rows, strips pdfplumber's
   serial dot, reuses the existing `linearize` so the golden row format is identical.
5. **Seam** (`pdf.rs::maybe_reflow_tables`) — gated `ECHO_TABLE_SIDECAR=1`; output
   type unchanged so `tag_table_row_chunks` is untouched.

## Outcome

- ✅ Impact: digital ruled tables now extract correct cells (verbatim, schema-anchored)
  instead of column-interleaved `-layout` guesses; opt-in, zero behaviour change when off
  or when the binary is absent.
- 🧪 Tests: `cargo test --lib` 529/0/1; new golden + 3 unit tests + ignored real-binary harness.
- 📁 Files: 1 Python sidecar + build script, 1 Rust client, table/pdf/mod wiring, tauri.conf, fixture, gitignore.
- 🔁 Next prompts: live re-index verification → default-on for digital; then Phase 9b (PaddleOCR scans).
- 🧠 Reflection: reusing `linearize` (not a new delimiter) kept the golden format stable across both extractors — the key to a drop-in seam.

## Evaluation notes (flywheel)

- Failure modes observed: pdfplumber serials carry a trailing dot (`"1."`) → `"1.."` after linearize (fixed by `normalize_serial`); empty-layout reflow skips all pages as out-of-range (harness bug, fixed).
- Graders run and results (PASS/FAIL): spike 4-doc verification PASS (Delegation 75t/796r; SPPRA none→fallback; Compendium 29t/82r; PC-I 63t/373r); golden PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): default-on `ECHO_TABLE_SIDECAR` for digital PDFs after a full live re-index + retrieval spot-check.
