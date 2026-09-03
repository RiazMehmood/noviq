# EchoAI E2E Validation Plan

## Objective
Validate the full app stack step-by-step: embedder → indexer → query router → LLM → response.
Each checkpoint must pass before moving to the next.

---

## Phase 1 — Environment Setup

| Step | Action | Pass Condition |
|------|--------|----------------|
| 1.1 | Run `./run-dev.sh` | Log shows "EchoAI async setup complete" |
| 1.2 | Confirm fastembed cache on NTFS | Log shows `cache: /media/riaz/New Volume/echo-AI/.fastembed_cache` |
| 1.3 | Confirm hardware tier | Log shows `tier=3, embedding=nomic-embed-text-v1.5` |
| 1.4 | Confirm DB ready | Log shows `DB ready — embedding dim: 768` |
| 1.5 | Run `./monitor-dev.sh` (Terminal 2) | Monitor loops continuously (every 4s refresh) |

**Checkpoint P0:** All 5 steps pass → proceed to Phase 2.

---

## Phase 2 — Indexing Validation

| Step | Action | Pass Condition |
|------|--------|----------------|
| 2.1 | Create new notebook in app | DB panel shows `notebooks=1` |
| 2.2 | Add a **small PDF** (<10 pages) to notebook | Log shows `METRIC embed_start` |
| 2.3 | Wait for indexing to complete | Log shows `METRIC embed_done_ms=<N>` within 2 min |
| 2.4 | Verify chunks in DB | Monitor DB panel shows `chunks > 0` |
| 2.5 | Verify source status | Monitor DB panel shows `source status: indexed=1` |

**Expected embed time:** Small PDF (<50 chunks): under 30s. Large PDF (733 chunks like constitution.pdf): under 5 min.

**Checkpoint P1:** `embed_done_ms` appears in log → indexer works. If it never appears → scheduler/embedder bug.

---

## Phase 3 — Chat Query Validation

| Step | Action | Pass Condition |
|------|--------|----------------|
| 3.1 | Navigate to notebook Chat tab | Log shows `foreground_set kind=Chat` |
| 3.2 | Ask a short question (3-5 words) | Log shows `METRIC query_start words=N` |
| 3.3 | Embedding step | Log shows `METRIC embed_query_ms=<N>` within 30s |
| 3.4 | Retrieval step | Log shows `LLM synthesis: N chunks selected` |
| 3.5 | LLM generation | Log shows `METRIC llm_start`, then `METRIC llm_done_ms=<N>` |
| 3.6 | Response in UI | App shows answer text (not loading forever) |
| 3.7 | Monitor Chat panel | Shows speed rating: FAST (<10s) / MODERATE / SLOW |

**Speed targets:**
- `embed_query_ms` ≤ 5000ms (first call includes model load)
- `llm_done_ms` ≤ 60000ms (Tier 3, 3B model on CPU)
- `query_total_ms` ≤ 90000ms

**Checkpoint P2:** Response appears in UI with answer text → full pipeline working.

---

## Phase 4 — Concurrent Operations (Scheduler)

| Step | Action | Pass Condition |
|------|--------|----------------|
| 4.1 | While query is running, check monitor Scheduler panel | Shows `foreground: Chat`, `active=1` |
| 4.2 | Add a second file while querying | Indexer starts but doesn't block query |
| 4.3 | Query completes | `workload_finished kind=Chat` in log |
| 4.4 | Indexer continues after query | Indexer `embed_done_ms` appears after query finishes |
| 4.5 | No OOM in System panel | RAM stays under 85% throughout |

**Checkpoint P3:** Both query and indexing complete without blocking each other.

---

## Phase 5 — Network Isolation Check

| Step | Action | Pass Condition |
|------|--------|----------------|
| 5.1 | Monitor Network panel during all operations | Shows "FULLY OFFLINE" |
| 5.2 | No external TCP connections from app pids | Network panel: `conn_count=0` |

**Checkpoint P4:** App is fully offline throughout.

---

## Monitoring Reference (what to watch in monitor-dev.sh)

```
Panel 3 (Database):     chunks=N, sources=N, indexed_folders=N
Panel 4 (Scheduler):    foreground=Chat, workloads active=N
Panel 5 (Chat/Query):   embed_ms, llm_ms, total_ms, FAST/MODERATE/SLOW
Panel 6 (LLM):          model loaded, last inference ms
Panel 7 (Indexer):      embed_start/embed_done, sources embedded count
Panel 8 (Network):      FULLY OFFLINE
Panel 9 (Errors):       clean — no errors
```

---

## Bug Fixes Required Before Testing

These bugs must be fixed first (identified from session 19 log analysis):

| Bug | File | Fix |
|-----|------|-----|
| `embed_one` blocks tokio thread | `query_router.rs:98` | Wrap in `spawn_blocking` |
| Scheduler pauses Indexing permanently | `scheduler.rs:apply_policy_to` | Allow non-LLM workloads to run concurrently |
| All chat queries hang on embedder mutex | Root cause of both above | Fix #1 resolves this |

---

## Session Checkpoints (save state after each phase)

- **CP-P0:** App starts clean → update memory with startup state
- **CP-P1:** Indexing verified → note embed timing and chunk count
- **CP-P2:** Query returns answer → note total_ms and speed rating
- **CP-P3:** Concurrent ops verified → note if any pauses/delays observed
- **CP-P4:** Network isolation confirmed → note offline status

Each checkpoint is saved in PHR and session memory for continuity across sessions.
