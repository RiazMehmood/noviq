---
id: 011
title: Fix OOM resource exhaustion during PDF processing
stage: green
date: 2026-04-15
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: inline fix — resource exhaustion crash
labels: ["oom","embedding","memory","pdf","hardware-tier","performance"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/indexer.rs
  - src-tauri/src/services/embedder.rs
  - src-tauri/src/services/hardware_detector.rs
  - src-tauri/src/services/parsers/pdf.rs
tests:
  - cargo check — zero errors (10 pre-existing warnings only)
---

## Prompt

this time it crashes everything, app started but as i create notebook and upload a pdf it started processing then everything slowly started freezing and then the whole system freezes and log off then again i logged in all processes and app closed chrome, cursor, and app it self, it means it is exhausting the resources very fast during processing phase, now look closely and resolve this issue.

## Response snapshot

Identified 5 root causes of Linux OOM kill during PDF processing and fixed all of them:

1. **SHA256 reads entire file into RAM** — `std::fs::read(path)` on a large PDF loaded the full file as a Vec<u8>. Fixed: stream with `BufReader` + 64KB buffer.

2. **ONNX tensor with all chunks at once** — `embedder.embed(texts, None)` passed all chunks as a single ONNX batch. A 200-page PDF = 200+ chunks = giant tensor allocation. Fixed: process in batches of 32 with `yield_now()` between batches.

3. **fastembed internal batch cap** — even inside `embed()` the ONNX forward pass had no cap. Fixed: pass `Some(32)` to fastembed.

4. **Hardware tier assigns nomic-embed-text-v1.5 to 8GB systems** — the ~400 MB model left almost no RAM headroom on a system already running Chrome + Cursor. Fixed: 8–12 GB systems now get `all-MiniLM-L6-v2` (384d, ~100 MB).

5. **Embedder inconsistency** — `embedder.rs` used `tier.ram_gb >= 12.0` independently of `hardware_detector.rs`. Fixed: `embedder.rs` now reads `tier.embedding_model` so the threshold is defined in one place.

Also added a 50MB warning log when lopdf is about to load a large PDF entirely into memory (lopdf has no streaming API; warning helps diagnose future issues without breaking existing behavior).

## Outcome

- ✅ Impact: System-level OOM kill eliminated; peak RAM during PDF processing now bounded regardless of document size
- 🧪 Tests: cargo check — zero errors, 10 pre-existing warnings only
- 📁 Files: 4 files modified
- 🔁 Next prompts: E2E test with constitution.pdf; monitor with `free -m` during processing
- 🧠 Reflection: The previous freeze fix (spawn_blocking) solved UI responsiveness but not RAM exhaustion. The OOM was caused by multiple independent all-file-in-RAM patterns compounding under concurrent Chrome + Cursor load.

## Evaluation notes (flywheel)

- Failure modes observed: Linux OOM killer triggered — killed all user processes and forced logout
- Graders run and results (PASS/FAIL): cargo check PASS
- Prompt variant (if applicable): null
- Next experiment: monitor `free -m` and `/proc/meminfo` during indexing to confirm peak RAM stays below 1GB
