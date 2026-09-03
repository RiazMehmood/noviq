# Plan: Adaptive Resource Management & Smart Background Processing

**Status**: DRAFT — for discussion before implementation  
**Author**: Riaz Mehmood  
**Date**: 2026-04-19

---

## Problem Statement

Currently:
- Hardware tier is detected **once at startup** and never re-evaluated
- `ECHO_FORCE_TIER=3` is a dev-only test flag with no production equivalent
- Heavy work (indexing, analysis, batch) competes with the user's other apps
- No runtime response to memory pressure — app can be killed by systemd-oomd
- User has no feedback when the system is under load

The goal is: **the app self-regulates based on real-time hardware conditions, stays alive under pressure, and never forces the user to close other apps.**

---

## What Already Exists (Do Not Re-Build)

| Component | What it does |
|-----------|-------------|
| `hardware_detector.rs` | Detects RAM at startup → selects tier 1–5 |
| `task_budget.rs` | Pauses indexing on battery or high CPU |
| `QualityNoticeBanner` | Fires when tier 3 is used instead of tier 4 |
| `SourceStatusBadge` | Per-file indexing progress (pending/processing/indexed/failed) |
| `BatchResultTable` | Per-file batch progress with expand/collapse |
| `systemd-run` scope | Prevents oomd from killing the terminal (run-dev.sh) |
| `ECHO_PASS2_CHUNK_LIMIT` | Caps Pass-2 NER to avoid OOM on large PDFs |

---

## Proposed Changes (5 Areas)

### Area 1 — Remove `ECHO_FORCE_TIER` from testing; make P3 self-testing

**Problem**: `ECHO_FORCE_TIER=3` is set in `run-dev.sh` but is **not read anywhere in the Rust backend**. The quality notice in P3 currently fires because the 7B model file is absent, not because of the env var. This is accidental and fragile.

**Fix**:
- Read `ECHO_FORCE_TIER` in `hardware_detector.rs` and cap `max_model_tier` to the forced value
- Document clearly that this is test-only (already done in DEMO_GUIDE.md)
- **No UX change** — this is a correctness fix only

**Effort**: Small (1 file, 5 lines)

---

### Area 2 — Dynamic tier re-evaluation before heavy operations

**Problem**: Tier is assessed once. If the user opens 10 browser tabs after startup, available RAM drops and the LLM load that was safe at launch may now cause memory pressure.

**Fix**:
- Before each LLM load (query, analysis, batch, Pass-2 NER), sample `sys.available_memory()` via sysinfo
- If available RAM < 2 GB: downgrade to the next lower tier for that operation only
- Emit a Tauri event `hardware:tier-downgraded` → frontend shows a one-time dismissible toast

```
Available RAM < 2 GB detected before LLM load
→ Using Tier 3 model for this operation
→ Toast: "Running on lighter model — system RAM is low"
```

**Effort**: Medium (hardware_detector + query_router + analysis_engine + frontend toast)

---

### Area 3 — Proactive LLM unloading when idle

**Problem**: Once the LLM is loaded into RAM (~1.9 GB for 3B model), it stays resident until the app closes. This holds memory that other apps could use.

**Fix**:
- `ModelSlot` already has a mutex-gated slot
- Add an idle timer: if no LLM inference for 3 minutes, unload the model and release RAM
- Reload on next request (adds ~2–5s latency for the first query after idle)
- Show a subtle "Loading model…" indicator on the chat input when reload is in progress

**Effort**: Medium (llm.rs + frontend loading state)

---

### Area 4 — CPU-aware throttling for background tasks

**Problem**: During indexing of a large PDF, CPU hits 250%+ which makes the machine sluggish. `task_budget.rs` has a battery check but not a CPU load check.

**Fix**:
- `task_budget.rs`: before submitting each chunk for embedding, read system load average
- If 1-minute load > (cpu_cores × 0.8): insert a short yield (100ms) before the next chunk
- This keeps CPU headroom for the user's other work without pausing indexing entirely
- Expose as a toggle in Settings: **"Efficient mode"** (default on) vs **"Fast mode"**

**Effort**: Small–Medium (task_budget.rs + settings UI toggle)

---

### Area 5 — Richer progress for analysis and batch

**Problem**: When "Compare Sources" runs on 3 large documents, the UI shows a spinner but no progress. For batch processing of 20 files, there is per-file status but no overall ETA.

**Fix**:
- Analysis: emit `analysis:progress { step: "retrieving chunks" | "building prompt" | "generating" | "parsing", percent: u8 }` Tauri event → show step label + progress bar in AnalysisResultView
- Batch: add estimated time remaining based on average time-per-file from completed items
- Both: persist to DB so if the user navigates away and back, progress is visible

**Effort**: Medium (analysis_engine + batch_processor + frontend components)

---

## Implementation Order (suggested)

| # | Area | Why first |
|---|------|-----------|
| 1 | Fix ECHO_FORCE_TIER wiring | Correctness; unblocks reliable P3 testing |
| 2 | CPU-aware throttling | Immediate UX improvement; small change |
| 3 | LLM idle unloading | Fixes lingering RAM usage after queries |
| 4 | Dynamic tier re-evaluation | Prevents mid-session OOM on busy machines |
| 5 | Analysis/batch progress | UX polish; can ship independently |

---

## What This Does NOT Change

- Tier detection logic (RAM thresholds stay the same)
- The 5-tier routing pipeline
- Any existing API contracts (Tauri commands, DB schema)
- The quality notice banner (already works correctly)

---

## Questions for Discussion

1. **Area 3 (LLM unloading)**: 3-minute idle timeout — is that too aggressive? Users who pause to read results may find the next query slower.
2. **Area 4 (throttling)**: Should "Efficient mode" be on by default, or opt-in? Default-on is friendlier to shared machines; opt-out may surprise power users.
3. **Area 5 (analysis progress)**: Should the progress bar be inline in the result area, or a toast/sidebar indicator?

Reply with your decisions on these three questions and I will start implementation.
