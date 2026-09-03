# Foreground-Priority Scheduler & Cancellable Indexing — Design

**Date:** 2026-04-24
**Branch:** 002-agent-task-overlay
**Status:** Spec (pre-plan)
**Sequencing:** Cycle 1 of 2. Cycle 2 (chat internals — Fix D token streaming, Fix E retrieval-only fast path, 1.5B fast tier) is out of scope for this spec and will be brainstormed separately after this ships and is measured.

---

## 1. Problem

Today the app has two observed performance problems that compound:

1. **Baseline chat latency is still high** even on an idle machine (Fix A+B+C from `optimization.md` shipped 2026-04-20 but was never runtime-verified).
2. **Chat becomes catastrophically slow when background work is running** — indexing, Pass-2 NER, and KG extraction all contend for CPU, RAM, and (critically) the single LLM model slot.

The user-facing consequence is that chatting while indexing is effectively impossible, and there is no cancel button on indexing — a batch job cannot be interrupted once started.

This spec addresses problem (2) and the indexing-UX gap. Problem (1) is deferred to cycle 2 on purpose: we want clean, un-contended measurements of idle chat latency before deciding what further chat-internals work is worthwhile.

## 2. Goals

- When the user is on the **Chat** view, chat gets exclusive access to heavy resources (LLM slot, CPU budget). LLM-using background workloads must release within ≤1 token (~800 ms on CPU); non-LLM cooperative workloads must park at the next file/event boundary (target ≤2 s, bounded by the §9 stuck-worker watchdog at 10 s).
- When the user is on the **Indexing** view (or any other workload-owning view), that workload gets full resources; chat-adjacent background tasks pause.
- Every long-running workload surfaces **progress**, **ETA**, and a **cancel** button.
- Cancelling indexing mid-batch leaves the corpus in a consistent state. Resuming skips already-ingested files.

## 3. Non-Goals

- No changes to chat prompt shape, model selection, retrieval tiers, token streaming, or fast-path heuristics. (Cycle 2.)
- No OS-level thread priority (`nice(1)`) — we stay inside the process with cooperative + cancellation-token primitives.
- No multi-chat concurrency. At most one chat query runs at a time.
- No preemption of an **in-flight chat response** when the user navigates away — the user asked a question and should get the answer.

## 4. Workload Taxonomy

Five workload kinds are registered with the scheduler:

| Kind | Uses LLM slot? | Pause model |
|---|---|---|
| `Chat` | Yes | N/A — registered on query start, unregistered on response completion; in-flight chat is never paused or preempted, even on nav-away |
| `Indexing` | No | Cooperative (file boundary) |
| `Pass2Ner` | Yes | Hard preempt (cancellation token + in-decode check) |
| `KgExtraction` | Yes | Hard preempt |
| `FileWatcher` | No | Cooperative (event boundary) |

Views that don't launch heavy work (settings, search results page, KG graph view) are not governed by the scheduler.

## 5. Architecture

One new module, `src-tauri/src/services/scheduler.rs`, owns scheduling state. It exposes:

```rust
pub enum WorkloadKind { Chat, Indexing, Pass2Ner, KgExtraction, FileWatcher }

pub struct WorkloadHandle {
    pub id: Uuid,
    pub kind: WorkloadKind,
    pause: Arc<AtomicBool>,
    cancel: CancellationToken,   // tokio_util::sync
    progress: watch::Sender<Progress>,
}

pub struct Scheduler { /* … */ }

impl Scheduler {
    pub fn register(&self, kind: WorkloadKind) -> Arc<WorkloadHandle>;
    pub fn set_foreground(&self, kind: WorkloadKind);
    pub fn request_cancel(&self, id: Uuid);
    pub fn report_progress(&self, id: Uuid, processed: u64, total: u64);
    pub fn subscribe_events(&self) -> broadcast::Receiver<SchedulerEvent>;
}
```

Frontend calls `set_foreground(kind)` via a Tauri command on view activation. On foreground change, for every registered workload whose kind ≠ new foreground:

- **LLM-using** (`Pass2Ner`, `KgExtraction`): `cancel.cancel()`. Worker catches it, persists a paused checkpoint, drops the model slot, and exits its current task. Its owning subsystem re-registers a fresh handle (new id) the next time foreground is not `Chat` and there is queued work — in practice, triggered by the same `SchedulerEvent::ForegroundChanged` broadcast that owners already subscribe to.
- **Non-LLM** (`Indexing`, `FileWatcher`): `pause.store(true)`. Worker finishes the **current file's transaction** (commit or rollback — whichever state it is in), then calls `handle.should_pause().await` at the next file boundary and parks on a `Notify` until foreground flips back. The atomicity invariant in §6 guarantees no half-ingested files even across pauses.

The `ModelSlot` is re-implemented from a plain `tokio::sync::Mutex` to a custom two-queue acquirer: a high-priority `VecDeque<oneshot::Sender<Permit>>` for `Chat` and a low-priority one for everything else. On release, the high queue drains before the low queue. In practice, by the time `Chat` tries to acquire, `set_foreground(Chat)` has already cancelled any LLM-using holder, so the slot is usually idle — the priority queue is the safety net that covers the race window between cancel-issued and cancel-observed.

LLM decode gets preemption: `llm.rs` accepts an optional `CancellationToken`; between sampled tokens it checks `is_cancelled()` and returns `Err(Aborted)`. Preempt latency ≤ one token (~200–800 ms on CPU).

Progress is published over a Tauri event `workload://state`, debounced to 4 Hz.

## 6. Cancellable Indexing — Data Model

Additive SQLite migration:

```sql
CREATE TABLE index_jobs (
  id              TEXT PRIMARY KEY,
  notebook_id     TEXT NOT NULL,
  root_path       TEXT NOT NULL,
  status          TEXT NOT NULL CHECK (status IN
                    ('running','paused','cancelled','completed','failed')),
  total_files     INTEGER NOT NULL,
  processed_files INTEGER NOT NULL DEFAULT 0,
  started_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  error           TEXT
);

CREATE TABLE index_job_files (
  job_id          TEXT NOT NULL REFERENCES index_jobs(id) ON DELETE CASCADE,
  file_path       TEXT NOT NULL,
  mtime           INTEGER NOT NULL,
  status          TEXT NOT NULL CHECK (status IN
                    ('pending','done','failed','skipped')),
  chunks_written  INTEGER NOT NULL DEFAULT 0,
  error           TEXT,
  PRIMARY KEY (job_id, file_path)
);
CREATE INDEX idx_job_files_status ON index_job_files(job_id, status);
```

**Atomicity invariant:** each file is ingested inside **one SQLite transaction** that writes chunks, embeddings, and KG edges, and flips `index_job_files.status = 'done'` in the same commit. Cancellation is only checked at **file boundaries** (before `BEGIN` of the next file). The in-flight file is always allowed to finish — commit on success, rollback on error. No half-ingested files exist because SQLite transactions are atomic, not because the worker voluntarily aborts mid-file.

**Cancel flow:**
1. Frontend: `invoke("cancel_index_job", { job_id })` → `scheduler.request_cancel(handle.id)`.
2. Cancellation token flips. The current file finishes its transaction (commit or error-rollback). The worker then checks the token at the file boundary, sets `index_jobs.status = 'cancelled'`, and exits.
3. Per-file rows keep their `done` / `failed` / `pending` state. Files that were never started remain `pending`.

**Resume flow:** new `index_jobs` row for the same `root_path` + `notebook_id`. During pre-scan, files whose `(file_path, mtime)` match a `done` row in a prior job for the same root are written as `skipped`. Everything else is `pending`.

**Restart-from-scratch flow:** explicit UI button deletes prior job rows and re-runs from zero.

**Crash recovery:** on startup, any `index_jobs.status = 'running'` is reconciled to `'paused'`. Any in-flight file transaction was rolled back by SQLite WAL recovery, so state is consistent. User is prompted to resume.

## 7. Progress & ETA

Each workload reports `{processed, total, started_at, last_tick_at}` via its `watch::Sender<Progress>`. Scheduler aggregates and emits `workload://state` at ≤4 Hz.

ETA:

- **Warm-up (first 10 items):** indeterminate spinner + "Calibrating…".
- **After 10 items:** EWMA of per-item duration with α=0.2. `eta_seconds = (total - processed) * ewma_per_item`. Displayed as `~Nm Ss remaining`.
- While paused: ETA frozen, label changes to `Paused — resumes when you return to {view}` where `{view}` is the owning view name (e.g. `Indexing`). The label is parameterized on the workload kind, not hardcoded to Chat.

## 8. UI (SolidJS frontend)

`<WorkloadBar>` component pinned to the bottom of any page that owns a long-running workload:

- Determinate progress bar: `processed / total`.
- Text: `42 / 318 files (13%)`.
- ETA text: `~4m 20s remaining` / `Calibrating…` / `Paused`.
- **Pause / Resume** button — only visible for the current foreground workload.
- **Cancel** button → confirm dialog.

Nav-rail badge shows any *background* workload state (e.g. `Indexing paused • 42/318`) so the user knows parked work exists.

Cancel confirm dialog has two follow-up actions: **[Resume later]** (keeps checkpoint rows) and **[Discard progress]** (deletes job rows).

## 9. Error Handling & Invariants

**Worker-side:**
- **Single file fails:** transaction rolls back, row marked `failed` with error, job keeps running. UI shows failed count separately.
- **Systemic failure:** `index_jobs.status = 'failed'`, surfaced in WorkloadBar with **[Retry]** / **[View log]**.
- **Process crash:** startup reconcile — `running` → `paused`; user prompted to resume.

**Preemption:**
- **LLM worker doesn't honor cancel within 2 s:** scheduler logs a warning; does not forcibly kill. Worst case: one extra token of wait (~800 ms). Forcibly aborting llama.cpp can corrupt KV cache.
- **Cooperative worker stuck past boundary >10 s:** diagnostic event; UI banner "Indexing is taking longer than expected to pause." No forced kill.

**Scheduler invariants:**
- `set_foreground(kind)` is idempotent.
- `register` after `request_cancel` for the same id is rejected.
- At most one `Chat` workload in flight (second concurrent request → `Error::AlreadyInFlight`, UI should already disable the send button).

**Corpus invariant:** after any cancel / crash / preempt, the retrievable corpus is a subset of files whose ingestion transaction committed. No half-chunked files. No dangling embeddings.

## 10. Testing

**Unit tests (Rust, `#[tokio::test]`):**
- `scheduler::set_foreground_preempts_llm_workload`
- `scheduler::set_foreground_pauses_cooperative_worker`
- `scheduler::in_flight_chat_not_cancelled_on_nav_away`
- `scheduler::second_chat_rejected`
- `indexing::file_atomicity_on_cancel`
- `indexing::resume_skips_done_files` (including mtime-changed files re-ingesting)
- `indexing::crash_recovery_marks_running_as_paused`
- `eta::ewma_stable_after_outlier`

**Integration tests (Tauri harness):**
- `query_notebook` with a seeded indexing job → indexing pauses, chat completes, indexing resumes on nav-away.
- `cancel_index_job` → status `cancelled`, file table unchanged, resume produces consistent corpus.

**Manual smoke (DEMO_GUIDE addendum):**
- Index a 100-file folder → switch to Chat → ask a question → verify WorkloadBar shows "Paused" and chat responds. Navigate back → verify resume from checkpoint.
- Cancel mid-batch → **Discard progress** → verify job rows gone, corpus has only completed files.

## 11. Out of Scope (Cycle 2)

- Fix D token streaming to the UI.
- Fix E retrieval-only fast path for high-confidence factual lookups.
- Qwen2.5-1.5B fast tier.
- Speculative decoding, prompt-caching (KV prefix reuse), n_threads tuning.
- OS-level thread priority or `nice(1)`.

These are intentionally deferred until after this cycle ships and we have clean idle-case chat latency numbers to justify further work.

## 12. Open Risks

1. **Cancellation plumbing touches `llm.rs` and every LLM-using pipeline.** Risk of incomplete coverage — any code path that calls `llm.rs` without threading a cancellation token will not be preemptible. Mitigation: make the token a required parameter on the public LLM API; audit all callers.
2. **Per-file transactions may be larger than SQLite's comfortable write size** for very big PDFs (thousands of chunks). If commit latency becomes the new bottleneck, revisit with per-file-grouped but intra-file-incremental commit strategy in cycle 2.
3. **Frontend must reliably call `set_foreground` on view transitions.** A bug that forgets to flip foreground back to `Chat` on nav will leave indexing paused silently. Mitigation: centralize in the router, cover with a Playwright/E2E test in cycle 2 when UI tests are added.
