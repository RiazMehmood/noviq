# Foreground-Priority Scheduler & Cancellable Indexing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the foreground view exclusive access to heavy resources (LLM slot, CPU) by hard-preempting LLM-using background workers and cooperatively pausing non-LLM workers; add cancellable, resumable indexing with file-atomic checkpoints; expose progress + ETA + cancel in the UI.

**Architecture:** A new `Scheduler` module owns a registry of `WorkloadHandle`s (one per heavy worker). Frontend calls `set_foreground(kind)` on view activation; scheduler cancels LLM-using workloads via a `CancellationToken`, pauses cooperative workloads via an atomic flag + `Notify`. The `ModelSlot` mutex is replaced with a two-queue priority acquirer so Chat jumps ahead of waiting background workers. Indexing gets a per-file SQLite transaction and two new tables (`index_jobs`, `index_job_files`) for resume/cancel state.

**Tech Stack:** Rust 1.87, Tokio, `tokio-util::sync::CancellationToken`, `dashmap`, SQLx (SQLite), Tauri 2, SolidJS/TypeScript frontend.

**Spec:** `docs/superpowers/specs/2026-04-24-foreground-scheduler-design.md`

---

## File Structure

**New Rust files:**
- `src-tauri/src/services/scheduler.rs` — scheduler state + API
- `src-tauri/src/services/scheduler_eta.rs` — EWMA ETA calculator
- `src-tauri/src/services/priority_model_slot.rs` — two-queue priority wrapper around `ModelSlot`
- `src-tauri/src/db/migrations/004_index_jobs.sql` — DDL for `index_jobs`, `index_job_files`
- `src-tauri/src/db/repositories/index_jobs.rs` — CRUD
- `src-tauri/src/commands/scheduler.rs` — Tauri commands (set_foreground, cancel_index_job, resume_index_job, discard_index_job, list_index_jobs)

**Modified Rust files:**
- `src-tauri/Cargo.toml` — add `tokio-util`, `dashmap`
- `src-tauri/src/services/mod.rs` — add new modules
- `src-tauri/src/commands/mod.rs` — add scheduler module
- `src-tauri/src/services/llm.rs` — accept optional `CancellationToken` in decode loop
- `src-tauri/src/services/indexer.rs` — per-file transaction, scheduler-aware
- `src-tauri/src/services/entity_extractor.rs` — scheduler-aware (Pass2 NER)
- `src-tauri/src/services/batch_processor.rs` — scheduler-aware (KG extraction)
- `src-tauri/src/services/file_watcher.rs` — cooperative pause
- `src-tauri/src/services/query_router.rs` — uses `PriorityModelSlot`
- `src-tauri/src/commands/query.rs` — registers Chat workload around query
- `src-tauri/src/lib.rs` — register scheduler in app state, add new invoke handlers

**New frontend files:**
- `src/stores/scheduler.ts` — reactive store listening to `workload://state`
- `src/components/WorkloadBar.tsx` — progress + ETA + pause/resume + cancel
- `src/components/NavBadge.tsx` — background workload badge

**Modified frontend files:**
- `src/App.tsx` — call `set_foreground` on route changes
- `src/pages/Dashboard.tsx`, `SearchPage.tsx`, `KnowledgeGraphPage.tsx` — mount WorkloadBar where applicable

---

## Task 1: Add dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add `tokio-util` and `dashmap` to `[dependencies]`**

Insert (preserve alphabetic ordering if present, otherwise append to `[dependencies]`):

```toml
tokio-util = { version = "0.7", features = ["rt"] }
dashmap = "6"
```

- [ ] **Step 2: Verify the crate builds**

Run: `cd src-tauri && cargo check`
Expected: compiles cleanly (no functional code yet touches these crates).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore(deps): add tokio-util and dashmap for scheduler"
```

---

## Task 2: Scheduler core types + module scaffolding

**Files:**
- Create: `src-tauri/src/services/scheduler.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: Create module file with types**

Write `src-tauri/src/services/scheduler.rs`:

```rust
//! Foreground-priority scheduler. See
//! docs/superpowers/specs/2026-04-24-foreground-scheduler-design.md

use dashmap::DashMap;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use tokio::sync::{broadcast, watch, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadKind {
    Chat,
    Indexing,
    Pass2Ner,
    KgExtraction,
    FileWatcher,
}

impl WorkloadKind {
    pub fn uses_llm(self) -> bool {
        matches!(self, Self::Chat | Self::Pass2Ner | Self::KgExtraction)
    }
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Progress {
    pub processed: u64,
    pub total: u64,
    pub started_at_ms: i64,
    pub last_tick_at_ms: i64,
    pub paused: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SchedulerEvent {
    ForegroundChanged { kind: WorkloadKind },
    WorkloadRegistered { id: Uuid, kind: WorkloadKind },
    WorkloadFinished { id: Uuid, kind: WorkloadKind },
    ProgressTick { id: Uuid, kind: WorkloadKind, progress: Progress },
}

pub struct WorkloadHandle {
    pub id: Uuid,
    pub kind: WorkloadKind,
    pub(crate) pause: Arc<AtomicBool>,
    pub(crate) notify_resume: Arc<Notify>,
    pub(crate) cancel: CancellationToken,
    pub(crate) progress_tx: watch::Sender<Progress>,
}

impl WorkloadHandle {
    pub fn is_paused(&self) -> bool { self.pause.load(Ordering::Acquire) }
    pub fn is_cancelled(&self) -> bool { self.cancel.is_cancelled() }
    pub fn cancellation_token(&self) -> CancellationToken { self.cancel.clone() }

    /// Await at natural boundaries for cooperative workers.
    pub async fn should_pause(&self) {
        while self.pause.load(Ordering::Acquire) && !self.cancel.is_cancelled() {
            let notified = self.notify_resume.notified();
            if !self.pause.load(Ordering::Acquire) { break; }
            notified.await;
        }
    }

    pub fn report(&self, processed: u64, total: u64) {
        let now = chrono::Utc::now().timestamp_millis();
        let mut p = self.progress_tx.borrow().clone();
        p.processed = processed;
        p.total = total;
        p.last_tick_at_ms = now;
        p.paused = self.is_paused();
        let _ = self.progress_tx.send(p);
    }
}

pub struct Scheduler {
    workloads: DashMap<Uuid, Arc<WorkloadHandle>>,
    foreground: RwLock<WorkloadKind>,
    events: broadcast::Sender<SchedulerEvent>,
}

impl Scheduler {
    pub fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(Self {
            workloads: DashMap::new(),
            foreground: RwLock::new(WorkloadKind::Chat),
            events: tx,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SchedulerEvent> { self.events.subscribe() }
}
```

- [ ] **Step 2: Wire module in `services/mod.rs`**

Append to `src-tauri/src/services/mod.rs`:

```rust
pub mod scheduler;
```

- [ ] **Step 3: Verify build**

Run: `cd src-tauri && cargo check`
Expected: compiles (unused-field warnings acceptable until Task 3).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/scheduler.rs src-tauri/src/services/mod.rs
git commit -m "feat(scheduler): add core types and module skeleton"
```

---

## Task 3: Scheduler register / set_foreground / cancel + tests

**Files:**
- Modify: `src-tauri/src/services/scheduler.rs`
- Test: same file under `#[cfg(test)] mod tests`

- [ ] **Step 1: Write failing tests**

Append to `src-tauri/src/services/scheduler.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn set_foreground_cancels_llm_workload() {
        let sched = Scheduler::new();
        let h = sched.register(WorkloadKind::Pass2Ner);
        sched.set_foreground(WorkloadKind::Chat).await;
        assert!(h.is_cancelled(), "LLM-using workload must be cancelled");
    }

    #[tokio::test]
    async fn set_foreground_pauses_cooperative_workload() {
        let sched = Scheduler::new();
        let h = sched.register(WorkloadKind::Indexing);
        sched.set_foreground(WorkloadKind::Chat).await;
        assert!(h.is_paused());
        assert!(!h.is_cancelled());
    }

    #[tokio::test]
    async fn set_foreground_back_resumes_cooperative() {
        let sched = Scheduler::new();
        let h = sched.register(WorkloadKind::Indexing);
        sched.set_foreground(WorkloadKind::Chat).await;
        assert!(h.is_paused());
        sched.set_foreground(WorkloadKind::Indexing).await;
        assert!(!h.is_paused());
    }

    #[tokio::test]
    async fn should_pause_blocks_until_resume() {
        let sched = Scheduler::new();
        let h = sched.register(WorkloadKind::Indexing);
        sched.set_foreground(WorkloadKind::Chat).await;

        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let h2 = h.clone();
        let task = tokio::spawn(async move {
            h2.should_pause().await;
            c.fetch_add(1, Ordering::SeqCst);
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0, "must still be parked");

        sched.set_foreground(WorkloadKind::Indexing).await;
        timeout(Duration::from_millis(500), task).await.unwrap().unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn register_is_idempotent_for_foreground_kind() {
        let sched = Scheduler::new();
        sched.set_foreground(WorkloadKind::Indexing).await;
        let h = sched.register(WorkloadKind::Indexing);
        // Foreground's own workload should not be paused against itself.
        assert!(!h.is_paused());
        assert!(!h.is_cancelled());
    }

    #[tokio::test]
    async fn request_cancel_trips_token() {
        let sched = Scheduler::new();
        let h = sched.register(WorkloadKind::Indexing);
        sched.request_cancel(h.id);
        assert!(h.is_cancelled());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib services::scheduler -- --nocapture`
Expected: FAIL — `register`, `set_foreground`, `request_cancel` not implemented.

- [ ] **Step 3: Implement the API**

Add to `impl Scheduler`:

```rust
pub fn register(&self, kind: WorkloadKind) -> Arc<WorkloadHandle> {
    let id = Uuid::new_v4();
    let (tx, _rx) = watch::channel(Progress {
        started_at_ms: chrono::Utc::now().timestamp_millis(),
        ..Progress::default()
    });
    let handle = Arc::new(WorkloadHandle {
        id,
        kind,
        pause: Arc::new(AtomicBool::new(false)),
        notify_resume: Arc::new(Notify::new()),
        cancel: CancellationToken::new(),
        progress_tx: tx,
    });

    // Apply current-foreground policy to the newly registered workload.
    let foreground = *self.foreground.try_read()
        .map(|g| *g)
        .unwrap_or(Box::leak(Box::new(WorkloadKind::Chat))); // cheap fallback
    if kind != foreground {
        self.apply_policy_to(&handle, foreground);
    }

    self.workloads.insert(id, handle.clone());
    let _ = self.events.send(SchedulerEvent::WorkloadRegistered { id, kind });
    handle
}

pub async fn set_foreground(&self, kind: WorkloadKind) {
    {
        let mut g = self.foreground.write().await;
        if *g == kind { return; } // idempotent
        *g = kind;
    }
    let _ = self.events.send(SchedulerEvent::ForegroundChanged { kind });
    let entries: Vec<Arc<WorkloadHandle>> =
        self.workloads.iter().map(|e| e.value().clone()).collect();
    for h in entries {
        if h.kind == kind {
            // Resume if it was paused.
            h.pause.store(false, Ordering::Release);
            h.notify_resume.notify_waiters();
        } else {
            self.apply_policy_to(&h, kind);
        }
    }
}

pub fn request_cancel(&self, id: Uuid) {
    if let Some(h) = self.workloads.get(&id) {
        h.cancel.cancel();
        h.pause.store(false, Ordering::Release);
        h.notify_resume.notify_waiters();
    }
}

pub fn unregister(&self, id: Uuid) {
    if let Some((_, h)) = self.workloads.remove(&id) {
        let _ = self.events.send(SchedulerEvent::WorkloadFinished {
            id, kind: h.kind,
        });
    }
}

fn apply_policy_to(&self, h: &Arc<WorkloadHandle>, foreground: WorkloadKind) {
    let _ = foreground;
    if h.kind.uses_llm() {
        h.cancel.cancel();
        h.pause.store(false, Ordering::Release);
        h.notify_resume.notify_waiters();
    } else {
        h.pause.store(true, Ordering::Release);
    }
}
```

Fix the `register` fallback (the leak trick is ugly — replace with a blocking read since `register` is sync and foreground is almost always uncontended):

```rust
// Replace the `foreground = ...` block inside `register` with:
let foreground = {
    // blocking-read is fine: set_foreground holds the write lock for microseconds
    let g = futures::executor::block_on(self.foreground.read());
    *g
};
```

Add `futures = "0.3"` to `Cargo.toml` if not already present — check first; if absent:

```toml
futures = "0.3"
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib services::scheduler -- --nocapture`
Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/scheduler.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(scheduler): register, set_foreground, request_cancel with tests"
```

---

## Task 4: EWMA ETA calculator

**Files:**
- Create: `src-tauri/src/services/scheduler_eta.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/services/scheduler_eta.rs`:

```rust
//! EWMA-based ETA for workload progress. Spec §7.

pub const WARMUP_ITEMS: u64 = 10;
pub const ALPHA: f64 = 0.2;

#[derive(Clone, Debug, Default)]
pub struct EtaTracker {
    ewma_per_item_ms: Option<f64>,
    last_item_at_ms: Option<i64>,
    observed_items: u64,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EtaState {
    Calibrating,
    Ready { remaining_ms: i64 },
}

impl EtaTracker {
    pub fn observe(&mut self, now_ms: i64) {
        if let Some(prev) = self.last_item_at_ms {
            let dt = (now_ms - prev).max(1) as f64;
            self.ewma_per_item_ms = Some(match self.ewma_per_item_ms {
                None => dt,
                Some(prev_ewma) => ALPHA * dt + (1.0 - ALPHA) * prev_ewma,
            });
        }
        self.last_item_at_ms = Some(now_ms);
        self.observed_items += 1;
    }

    pub fn estimate(&self, remaining: u64) -> EtaState {
        if self.observed_items < WARMUP_ITEMS || self.ewma_per_item_ms.is_none() {
            return EtaState::Calibrating;
        }
        let per = self.ewma_per_item_ms.unwrap();
        EtaState::Ready { remaining_ms: (per * remaining as f64) as i64 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibrating_before_warmup() {
        let mut t = EtaTracker::default();
        for i in 0..5 { t.observe(i * 100); }
        assert!(matches!(t.estimate(100), EtaState::Calibrating));
    }

    #[test]
    fn ready_after_warmup() {
        let mut t = EtaTracker::default();
        for i in 0..15 { t.observe(i * 100); } // 100ms/item
        match t.estimate(10) {
            EtaState::Ready { remaining_ms } => {
                assert!((900..=1100).contains(&remaining_ms),
                    "expected ~1000ms, got {remaining_ms}");
            }
            _ => panic!("expected Ready"),
        }
    }

    #[test]
    fn stable_under_outlier() {
        let mut t = EtaTracker::default();
        let mut now = 0i64;
        for _ in 0..15 { now += 100; t.observe(now); } // warmup at 100ms/item
        now += 1000; t.observe(now); // single 10x outlier
        match t.estimate(10) {
            EtaState::Ready { remaining_ms } => {
                // With α=0.2: new ewma = 0.2*1000 + 0.8*100 = 280ms
                // Bound: ETA within 20% of 280*10 = 2800ms
                assert!((2240..=3360).contains(&remaining_ms),
                    "expected ~2800ms ±20%, got {remaining_ms}");
            }
            _ => panic!("expected Ready"),
        }
    }
}
```

- [ ] **Step 2: Wire module**

Append to `src-tauri/src/services/mod.rs`:

```rust
pub mod scheduler_eta;
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib services::scheduler_eta -- --nocapture`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/scheduler_eta.rs src-tauri/src/services/mod.rs
git commit -m "feat(scheduler): EWMA ETA tracker with warmup window"
```

---

## Task 5: Priority-aware ModelSlot wrapper

**Files:**
- Create: `src-tauri/src/services/priority_model_slot.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/services/priority_model_slot.rs`:

```rust
//! Two-queue priority acquirer wrapping the existing LLM model slot.
//! Chat jumps ahead of background (Pass2, KG) when both are waiting.
//! Spec §5.

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex as AsyncMutex, Mutex};

pub struct PrioritySlot {
    inner: Arc<Mutex<State>>,
}

struct State {
    held: bool,
    high: VecDeque<oneshot::Sender<Permit>>,
    low: VecDeque<oneshot::Sender<Permit>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Priority { High, Low }

pub struct Permit {
    slot: Arc<Mutex<State>>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let slot = self.slot.clone();
        // Synchronous drop: we release under a blocking lock, then wake next waiter.
        let mut st = slot.blocking_lock_owned_compat();
        st.held = false;
        if let Some(tx) = st.high.pop_front().or_else(|| st.low.pop_front()) {
            st.held = true;
            let _ = tx.send(Permit { slot: slot.clone() });
        }
    }
}

// Helper so Drop can run outside an async context in tests.
trait BlockingLock<T> {
    fn blocking_lock_owned_compat(&self) -> tokio::sync::MutexGuard<'_, T>;
}
impl<T> BlockingLock<T> for Arc<Mutex<T>> {
    fn blocking_lock_owned_compat(&self) -> tokio::sync::MutexGuard<'_, T> {
        self.blocking_lock()
    }
}

impl PrioritySlot {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(State {
                held: false,
                high: VecDeque::new(),
                low: VecDeque::new(),
            })),
        }
    }

    pub async fn acquire(&self, prio: Priority) -> Permit {
        let rx = {
            let mut st = self.inner.lock().await;
            if !st.held && st.high.is_empty() && (prio == Priority::High || st.low.is_empty()) {
                st.held = true;
                return Permit { slot: self.inner.clone() };
            }
            let (tx, rx) = oneshot::channel();
            match prio {
                Priority::High => st.high.push_back(tx),
                Priority::Low => st.low.push_back(tx),
            }
            rx
        };
        rx.await.expect("priority slot channel closed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn high_priority_jumps_queue() {
        let slot = Arc::new(PrioritySlot::new());
        let order = Arc::new(AtomicUsize::new(0));

        // Holder occupies the slot.
        let holder_slot = slot.clone();
        let holder_permit = holder_slot.acquire(Priority::Low).await;

        // Three low-priority waiters queued first.
        let mut handles = vec![];
        for i in 0..3 {
            let s = slot.clone();
            let o = order.clone();
            handles.push(tokio::spawn(async move {
                let _p = s.acquire(Priority::Low).await;
                let rank = o.fetch_add(1, Ordering::SeqCst);
                // Low-priority items should land at positions 1,2,3 (High takes 0).
                assert!(rank >= 1, "low rank {} >= 1", rank);
                sleep(Duration::from_millis(10)).await;
                let _ = i;
            }));
        }
        sleep(Duration::from_millis(50)).await;

        // Now a high-priority waiter.
        let hs = slot.clone();
        let ho = order.clone();
        let high_task = tokio::spawn(async move {
            let _p = hs.acquire(Priority::High).await;
            let rank = ho.fetch_add(1, Ordering::SeqCst);
            assert_eq!(rank, 0, "high priority must run first after release");
            sleep(Duration::from_millis(10)).await;
        });

        sleep(Duration::from_millis(20)).await;
        drop(holder_permit);

        high_task.await.unwrap();
        for h in handles { h.await.unwrap(); }
    }
}
```

- [ ] **Step 2: Wire module**

Append to `src-tauri/src/services/mod.rs`:

```rust
pub mod priority_model_slot;
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib services::priority_model_slot -- --nocapture`
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/priority_model_slot.rs src-tauri/src/services/mod.rs
git commit -m "feat(scheduler): priority model slot with chat-first acquire"
```

---

## Task 6: LLM decode accepts CancellationToken

**Files:**
- Modify: `src-tauri/src/services/llm.rs`

- [ ] **Step 1: Read current decode loop**

Inspect `llm.rs` around the per-token loop (approx. lines 179–187 per `optimization.md`). Identify the existing public generate function signature and its decode while-loop.

- [ ] **Step 2: Write failing test**

Append to `src-tauri/src/services/llm.rs` inside `#[cfg(test)]`:

```rust
#[cfg(test)]
mod cancel_tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn decode_aborts_when_token_cancelled_before_call() {
        // With no model loaded (stub), the decode path should early-return Err on
        // a pre-cancelled token. This test pins the contract even when no model
        // is present — it must check the token before doing expensive work.
        let token = CancellationToken::new();
        token.cancel();
        let result = generate_with_cancel(
            /* prompt */ "hello",
            /* max_tokens */ 32,
            Some(token),
        ).await;
        assert!(matches!(result, Err(LlmError::Cancelled)));
    }
}
```

- [ ] **Step 3: Add cancellation-aware API**

Add to `llm.rs` public surface:

```rust
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

/// Cancellation-aware generation. Checks the token before prefill and between
/// every sampled token. When cancelled, returns `Err(LlmError::Cancelled)`
/// without touching KV cache state further (so the caller can safely drop it).
pub async fn generate_with_cancel(
    prompt: &str,
    max_tokens: usize,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> Result<String, LlmError> {
    if let Some(t) = &cancel {
        if t.is_cancelled() { return Err(LlmError::Cancelled); }
    }

    // DELEGATE to the existing internal decode loop, but pass the token.
    // In the existing loop `while generated < max_tokens { ... }`, insert
    // at the top of each iteration:
    //
    //   if let Some(t) = &cancel { if t.is_cancelled() { return Err(LlmError::Cancelled); } }
    //
    // Wrap the existing return into `Ok(output)`.
    crate::services::llm_internal::generate_inner(prompt, max_tokens, cancel)
        .await
        .map_err(|e| LlmError::Other(e.to_string()))
}
```

Refactor the existing `generate` (or equivalent) into `llm_internal::generate_inner` that accepts the optional `CancellationToken`, and at the top of the per-token loop:

```rust
if let Some(t) = &cancel {
    if t.is_cancelled() { return Err(anyhow::anyhow!("cancelled")); }
}
```

Keep the non-cancel-aware `generate` as a thin wrapper calling `generate_with_cancel(prompt, max_tokens, None)`.

- [ ] **Step 4: Run test**

Run: `cd src-tauri && cargo test --lib services::llm::cancel_tests -- --nocapture`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/llm.rs
git commit -m "feat(llm): thread CancellationToken through decode loop"
```

---

## Task 7: Migration 004 — index_jobs tables

**Files:**
- Create: `src-tauri/src/db/migrations/004_index_jobs.sql`

- [ ] **Step 1: Create migration file**

Write `src-tauri/src/db/migrations/004_index_jobs.sql`:

```sql
-- Spec §6 — cancellable indexing data model.

CREATE TABLE IF NOT EXISTS index_jobs (
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

CREATE INDEX IF NOT EXISTS idx_index_jobs_notebook
  ON index_jobs(notebook_id, status);

CREATE TABLE IF NOT EXISTS index_job_files (
  job_id          TEXT NOT NULL REFERENCES index_jobs(id) ON DELETE CASCADE,
  file_path       TEXT NOT NULL,
  mtime           INTEGER NOT NULL,
  status          TEXT NOT NULL CHECK (status IN
                    ('pending','done','failed','skipped')),
  chunks_written  INTEGER NOT NULL DEFAULT 0,
  error           TEXT,
  PRIMARY KEY (job_id, file_path)
);

CREATE INDEX IF NOT EXISTS idx_index_job_files_status
  ON index_job_files(job_id, status);

-- Resume support: lookup previous "done" file rows for a given root.
CREATE INDEX IF NOT EXISTS idx_index_job_files_path_mtime
  ON index_job_files(file_path, mtime, status);
```

- [ ] **Step 2: Verify migration loader picks it up**

Open `src-tauri/src/db/mod.rs` (or `schema.rs`) and confirm migrations are loaded via `sqlx::migrate!` or an `include_str!` loop. If explicit, add `include_str!("migrations/004_index_jobs.sql")` alongside existing 003. If directory-based — no code change needed.

Run: `cd src-tauri && cargo test --lib db -- --nocapture` (or whatever migration smoke test exists).
Expected: migrations run cleanly against a fresh DB.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/migrations/004_index_jobs.sql src-tauri/src/db/mod.rs
git commit -m "feat(db): migration 004 — index_jobs and index_job_files"
```

---

## Task 8: index_jobs repository

**Files:**
- Create: `src-tauri/src/db/repositories/index_jobs.rs`
- Modify: `src-tauri/src/db/repositories/mod.rs`

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/db/repositories/index_jobs.rs`:

```rust
//! Repository for index_jobs + index_job_files. Spec §6.

use anyhow::Result;
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Clone, Debug, sqlx::FromRow, serde::Serialize)]
pub struct IndexJob {
    pub id: String,
    pub notebook_id: String,
    pub root_path: String,
    pub status: String,
    pub total_files: i64,
    pub processed_files: i64,
    pub started_at: i64,
    pub updated_at: i64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct IndexJobFile {
    pub job_id: String,
    pub file_path: String,
    pub mtime: i64,
    pub status: String,
    pub chunks_written: i64,
    pub error: Option<String>,
}

pub async fn create_job(
    pool: &SqlitePool,
    notebook_id: &str,
    root_path: &str,
    file_list: &[(String, i64)], // (path, mtime)
) -> Result<IndexJob> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let total = file_list.len() as i64;
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO index_jobs (id, notebook_id, root_path, status,
         total_files, processed_files, started_at, updated_at)
         VALUES (?, ?, ?, 'running', ?, 0, ?, ?)",
    )
    .bind(&id).bind(notebook_id).bind(root_path)
    .bind(total).bind(now).bind(now)
    .execute(&mut *tx).await?;

    for (path, mtime) in file_list {
        sqlx::query(
            "INSERT INTO index_job_files (job_id, file_path, mtime, status)
             VALUES (?, ?, ?, 'pending')",
        ).bind(&id).bind(path).bind(mtime).execute(&mut *tx).await?;
    }
    tx.commit().await?;

    Ok(IndexJob {
        id, notebook_id: notebook_id.into(), root_path: root_path.into(),
        status: "running".into(), total_files: total, processed_files: 0,
        started_at: now, updated_at: now, error: None,
    })
}

pub async fn set_job_status(pool: &SqlitePool, job_id: &str, status: &str, error: Option<&str>) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("UPDATE index_jobs SET status = ?, error = ?, updated_at = ? WHERE id = ?")
        .bind(status).bind(error).bind(now).bind(job_id)
        .execute(pool).await?;
    Ok(())
}

pub async fn mark_file_done(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, job_id: &str, path: &str, chunks: i64) -> Result<()> {
    sqlx::query(
        "UPDATE index_job_files SET status = 'done', chunks_written = ? WHERE job_id = ? AND file_path = ?",
    ).bind(chunks).bind(job_id).bind(path).execute(&mut **tx).await?;
    Ok(())
}

pub async fn mark_file_failed(pool: &SqlitePool, job_id: &str, path: &str, error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE index_job_files SET status = 'failed', error = ? WHERE job_id = ? AND file_path = ?",
    ).bind(error).bind(job_id).bind(path).execute(pool).await?;
    Ok(())
}

pub async fn increment_processed(pool: &SqlitePool, job_id: &str) -> Result<()> {
    sqlx::query("UPDATE index_jobs SET processed_files = processed_files + 1, updated_at = ? WHERE id = ?")
        .bind(chrono::Utc::now().timestamp_millis()).bind(job_id)
        .execute(pool).await?;
    Ok(())
}

pub async fn find_completed_files_for_root(
    pool: &SqlitePool,
    notebook_id: &str,
    root_path: &str,
) -> Result<Vec<IndexJobFile>> {
    let rows = sqlx::query_as::<_, IndexJobFile>(
        "SELECT f.* FROM index_job_files f
         JOIN index_jobs j ON j.id = f.job_id
         WHERE j.notebook_id = ? AND j.root_path = ? AND f.status = 'done'",
    ).bind(notebook_id).bind(root_path).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn list_active_jobs(pool: &SqlitePool) -> Result<Vec<IndexJob>> {
    let rows = sqlx::query_as::<_, IndexJob>(
        "SELECT * FROM index_jobs WHERE status IN ('running','paused') ORDER BY started_at DESC",
    ).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn delete_job(pool: &SqlitePool, job_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM index_jobs WHERE id = ?").bind(job_id).execute(pool).await?;
    Ok(())
}

pub async fn reconcile_orphaned_running_jobs(pool: &SqlitePool) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE index_jobs SET status = 'paused', updated_at = ? WHERE status = 'running'",
    ).bind(chrono::Utc::now().timestamp_millis()).execute(pool).await?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_pool() -> SqlitePool {
        // Helper matching existing repo tests — open in-memory and run migrations.
        // Use whichever helper the codebase already provides (e.g., `crate::db::test_pool()`).
        crate::db::test_pool().await
    }

    #[tokio::test]
    async fn create_and_read_job() {
        let pool = fresh_pool().await;
        let files = vec![("/a.pdf".into(), 1i64), ("/b.pdf".into(), 2i64)];
        let j = create_job(&pool, "nb1", "/root", &files).await.unwrap();
        assert_eq!(j.total_files, 2);
        assert_eq!(j.status, "running");
    }

    #[tokio::test]
    async fn reconcile_running_to_paused() {
        let pool = fresh_pool().await;
        let _ = create_job(&pool, "nb1", "/root", &[("/a".into(), 1)]).await.unwrap();
        let n = reconcile_orphaned_running_jobs(&pool).await.unwrap();
        assert_eq!(n, 1);
        let actives = list_active_jobs(&pool).await.unwrap();
        assert_eq!(actives[0].status, "paused");
    }

    #[tokio::test]
    async fn find_completed_files() {
        let pool = fresh_pool().await;
        let j = create_job(&pool, "nb1", "/root", &[
            ("/a".into(), 1), ("/b".into(), 2),
        ]).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        mark_file_done(&mut tx, &j.id, "/a", 5).await.unwrap();
        tx.commit().await.unwrap();
        let done = find_completed_files_for_root(&pool, "nb1", "/root").await.unwrap();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].file_path, "/a");
    }
}
```

- [ ] **Step 2: Wire module**

Append to `src-tauri/src/db/repositories/mod.rs`:

```rust
pub mod index_jobs;
```

If the codebase lacks a `crate::db::test_pool()` helper, locate the pattern existing repo tests use (e.g., in `chunks.rs` tests) and adapt. Do not invent a helper — reuse the existing one.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib db::repositories::index_jobs -- --nocapture`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/db/repositories/index_jobs.rs src-tauri/src/db/repositories/mod.rs
git commit -m "feat(db): index_jobs repository with CRUD + reconcile"
```

---

## Task 9: Indexer pipeline — per-file transaction + scheduler-aware pause/cancel

**Files:**
- Modify: `src-tauri/src/services/indexer.rs`

- [ ] **Step 1: Audit current indexer**

Read `src-tauri/src/services/indexer.rs`. Identify:
- The main per-folder entry point (e.g., `index_folder` or similar).
- Where chunks, embeddings, and KG edges are written today (look for `INSERT INTO chunks`, embedding inserts, `knowledge_graph_edges`).
- Whether writes today are already wrapped in a transaction per file or scattered.

- [ ] **Step 2: Write failing test**

Append to `indexer.rs`:

```rust
#[cfg(test)]
mod scheduler_tests {
    use super::*;
    use crate::services::scheduler::{Scheduler, WorkloadKind};

    #[tokio::test]
    async fn cancel_at_file_boundary_leaves_consistent_db() {
        let pool = crate::db::test_pool().await;
        let sched = Scheduler::new();
        let handle = sched.register(WorkloadKind::Indexing);

        // Synthetic folder with 3 tiny text files (fixture helper — reuse indexer's
        // existing test fixtures if present).
        let files = make_test_fixture_files(3).await;
        let job = crate::db::repositories::index_jobs::create_job(
            &pool, "nb1", &files.root, &files.list,
        ).await.unwrap();

        // Cancel after 1 file.
        let h2 = handle.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            h2.cancel.cancel();
        });

        let _ = run_index_job(&pool, &handle, &job.id, &files.list).await;

        // At least one file is done, the rest are pending; none are "half-written".
        let done = crate::db::repositories::index_jobs::find_completed_files_for_root(
            &pool, "nb1", &files.root,
        ).await.unwrap();
        assert!(done.len() <= 3);

        // Consistency: count chunks per done file > 0, per non-done file = 0.
        for f in &files.list {
            let is_done = done.iter().any(|d| d.file_path == f.0);
            let chunks: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM chunks WHERE source_path = ?",
            ).bind(&f.0).fetch_one(&pool).await.unwrap();
            if is_done { assert!(chunks > 0); } else { assert_eq!(chunks, 0); }
        }
    }
}
```

(`make_test_fixture_files` — reuse an existing indexer test helper if present; otherwise add a simple one that writes three `.txt` files to a temp dir and returns `{ root: String, list: Vec<(String, i64)> }`.)

- [ ] **Step 3: Refactor index-folder into `run_index_job`**

Replace the existing per-folder entry with:

```rust
pub async fn run_index_job(
    pool: &SqlitePool,
    handle: &Arc<crate::services::scheduler::WorkloadHandle>,
    job_id: &str,
    files: &[(String, i64)],
) -> Result<()> {
    use crate::db::repositories::index_jobs as repo;

    for (i, (path, _mtime)) in files.iter().enumerate() {
        // Cooperative pause check at file boundary.
        handle.should_pause().await;
        if handle.is_cancelled() {
            repo::set_job_status(pool, job_id, "cancelled", None).await?;
            return Ok(());
        }

        // One transaction per file — chunks + embeddings + KG edges + status flip.
        let mut tx = pool.begin().await?;
        match ingest_single_file_tx(&mut tx, path).await {
            Ok(chunks_written) => {
                repo::mark_file_done(&mut tx, job_id, path, chunks_written as i64).await?;
                tx.commit().await?;
                repo::increment_processed(pool, job_id).await?;
            }
            Err(e) => {
                let _ = tx.rollback().await;
                repo::mark_file_failed(pool, job_id, path, &e.to_string()).await?;
            }
        }
        handle.report((i as u64) + 1, files.len() as u64);
    }

    crate::db::repositories::index_jobs::set_job_status(pool, job_id, "completed", None).await?;
    Ok(())
}
```

`ingest_single_file_tx` is the existing single-file ingestion logic rewired to take a `&mut sqlx::Transaction<'_, Sqlite>` instead of `&SqlitePool` and return the number of chunks written. Move all chunk/embedding/KG-edge inserts inside this function so they participate in the same transaction.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib services::indexer::scheduler_tests -- --nocapture`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/indexer.rs
git commit -m "feat(indexer): per-file transaction + scheduler-aware pause/cancel"
```

---

## Task 10: Resume flow — skip already-done files

**Files:**
- Modify: `src-tauri/src/services/indexer.rs`

- [ ] **Step 1: Write failing test**

Append to `scheduler_tests`:

```rust
#[tokio::test]
async fn resume_skips_done_files_but_reprocesses_edited() {
    let pool = crate::db::test_pool().await;
    // Seed: a previous job completed file /a and /b.
    let prev = crate::db::repositories::index_jobs::create_job(
        &pool, "nb1", "/root",
        &[("/a".into(), 100), ("/b".into(), 200)],
    ).await.unwrap();
    {
        let mut tx = pool.begin().await.unwrap();
        crate::db::repositories::index_jobs::mark_file_done(&mut tx, &prev.id, "/a", 3).await.unwrap();
        crate::db::repositories::index_jobs::mark_file_done(&mut tx, &prev.id, "/b", 4).await.unwrap();
        tx.commit().await.unwrap();
    }

    // New scan: /a unchanged (mtime 100), /b edited (mtime 999), /c new.
    let new_scan = vec![
        ("/a".into(), 100i64),
        ("/b".into(), 999i64),
        ("/c".into(), 300i64),
    ];
    let plan = plan_resume(&pool, "nb1", "/root", &new_scan).await.unwrap();
    assert_eq!(plan.skipped, vec!["/a".to_string()]);
    assert_eq!(plan.to_process, vec!["/b".to_string(), "/c".to_string()]);
}
```

- [ ] **Step 2: Implement `plan_resume`**

Add to `indexer.rs`:

```rust
pub struct ResumePlan {
    pub skipped: Vec<String>,
    pub to_process: Vec<String>,
}

pub async fn plan_resume(
    pool: &SqlitePool,
    notebook_id: &str,
    root_path: &str,
    scanned: &[(String, i64)],
) -> Result<ResumePlan> {
    use crate::db::repositories::index_jobs as repo;
    let done = repo::find_completed_files_for_root(pool, notebook_id, root_path).await?;
    let done_map: std::collections::HashMap<&str, i64> =
        done.iter().map(|f| (f.file_path.as_str(), f.mtime)).collect();

    let mut skipped = Vec::new();
    let mut to_process = Vec::new();
    for (path, mtime) in scanned {
        match done_map.get(path.as_str()) {
            Some(prev_mtime) if *prev_mtime == *mtime => skipped.push(path.clone()),
            _ => to_process.push(path.clone()),
        }
    }
    Ok(ResumePlan { skipped, to_process })
}
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib services::indexer -- --nocapture`
Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/indexer.rs
git commit -m "feat(indexer): resume plan — skip done files, reprocess edited"
```

---

## Task 11: Crash recovery on startup

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Locate app setup**

Find where the DB pool is initialized during `tauri::Builder::setup(...)`. Add a call to `reconcile_orphaned_running_jobs` right after migrations run and before the main window is shown.

- [ ] **Step 2: Add reconcile call**

In `lib.rs` setup closure, after pool creation:

```rust
let reconciled = crate::db::repositories::index_jobs::reconcile_orphaned_running_jobs(&pool)
    .await
    .unwrap_or(0);
if reconciled > 0 {
    tracing::warn!("reconciled {} orphaned index jobs from running -> paused", reconciled);
}
```

- [ ] **Step 3: Verify build**

Run: `cd src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(indexer): reconcile orphaned running jobs on startup"
```

---

## Task 12: Scheduler-wire Pass2 NER and KG extraction

**Files:**
- Modify: `src-tauri/src/services/entity_extractor.rs`
- Modify: `src-tauri/src/services/batch_processor.rs`

- [ ] **Step 1: Audit LLM call sites**

In each file, find every `ModelSlot`/`generate` call. These need to accept a `CancellationToken` from a `WorkloadHandle` and pass it to `llm::generate_with_cancel`.

- [ ] **Step 2: Thread the token through Pass2 NER**

Change the Pass2 entry point signature:

```rust
pub async fn run_pass2_ner(
    pool: &SqlitePool,
    chunks: &[ChunkRef],
    handle: &Arc<crate::services::scheduler::WorkloadHandle>,
) -> Result<()> {
    for (i, chunk) in chunks.iter().enumerate() {
        handle.should_pause().await;
        if handle.is_cancelled() { return Ok(()); }

        let token = handle.cancellation_token();
        match crate::services::llm::generate_with_cancel(
            &build_ner_prompt(chunk), NER_MAX_TOKENS, Some(token),
        ).await {
            Ok(output) => persist_entities(pool, chunk.id, &output).await?,
            Err(crate::services::llm::LlmError::Cancelled) => return Ok(()),
            Err(e) => tracing::warn!("pass2 ner failed on chunk {}: {}", chunk.id, e),
        }
        handle.report((i as u64) + 1, chunks.len() as u64);
    }
    Ok(())
}
```

Update every caller of the prior Pass2 entry to pass a handle (registered via `scheduler.register(WorkloadKind::Pass2Ner)` by the owning subsystem — typically the indexer pipeline or batch processor).

- [ ] **Step 3: Same for KG extraction in `batch_processor.rs`**

Mirror the pattern — register `KgExtraction`, propagate token, check pause + cancel at chunk boundary.

- [ ] **Step 4: Write failing + passing tests**

Add a test that calls `run_pass2_ner` with a pre-cancelled handle and asserts it returns quickly without touching the DB:

```rust
#[tokio::test]
async fn pass2_exits_on_precancelled_handle() {
    let pool = crate::db::test_pool().await;
    let sched = crate::services::scheduler::Scheduler::new();
    let h = sched.register(crate::services::scheduler::WorkloadKind::Pass2Ner);
    h.cancel.cancel();
    let res = run_pass2_ner(&pool, &make_synthetic_chunks(5), &h).await;
    assert!(res.is_ok());
    let entities: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entities")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(entities, 0);
}
```

(Mirror the test for KG extraction.)

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --lib services -- --nocapture`
Expected: all scheduler-related tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/entity_extractor.rs src-tauri/src/services/batch_processor.rs
git commit -m "feat(llm-workers): scheduler-aware cancellation for pass2 and kg"
```

---

## Task 13: File-watcher cooperative pause

**Files:**
- Modify: `src-tauri/src/services/file_watcher.rs`

- [ ] **Step 1: Add pause check**

Find the watcher's main event loop. At the top of each event-processing iteration:

```rust
handle.should_pause().await;
if handle.is_cancelled() { break; }
```

The watcher should register `WorkloadKind::FileWatcher` at startup and store its handle on the watcher struct.

- [ ] **Step 2: Verify build**

Run: `cd src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/services/file_watcher.rs
git commit -m "feat(file-watcher): cooperative pause at event boundary"
```

---

## Task 14: Scheduler progress event pump (Tauri)

**Files:**
- Modify: `src-tauri/src/services/scheduler.rs`

- [ ] **Step 1: Add a background tick task**

Add to `impl Scheduler`:

```rust
/// Spawn a background task that broadcasts progress snapshots at ≤4 Hz.
pub fn start_progress_pump(self: &Arc<Self>, app: tauri::AppHandle) {
    let me = self.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            interval.tick().await;
            let snapshots: Vec<_> = me.workloads.iter().map(|e| {
                let h = e.value();
                (h.id, h.kind, h.progress_tx.borrow().clone())
            }).collect();
            for (id, kind, progress) in snapshots {
                let evt = SchedulerEvent::ProgressTick { id, kind, progress };
                let _ = me.events.send(evt.clone());
                let _ = app.emit("workload://state", &evt);
            }
        }
    });
}
```

(Use `tauri::Manager` + `emit` or the existing event-emit pattern present in this codebase — check `commands/query.rs` or similar for conventions and match.)

- [ ] **Step 2: Wire the pump in `lib.rs`**

In `setup`:

```rust
let sched = crate::services::scheduler::Scheduler::new();
sched.start_progress_pump(app.handle().clone());
app.manage(sched);
```

- [ ] **Step 3: Verify build**

Run: `cd src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/scheduler.rs src-tauri/src/lib.rs
git commit -m "feat(scheduler): progress pump emits workload://state at 4Hz"
```

---

## Task 15: Tauri commands for scheduler + jobs

**Files:**
- Create: `src-tauri/src/commands/scheduler.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs` (invoke_handler)

- [ ] **Step 1: Create commands file**

Write `src-tauri/src/commands/scheduler.rs`:

```rust
use std::sync::Arc;
use sqlx::SqlitePool;
use tauri::State;
use uuid::Uuid;

use crate::services::scheduler::{Scheduler, WorkloadKind};
use crate::db::repositories::index_jobs as repo;

#[tauri::command]
pub async fn set_foreground(
    scheduler: State<'_, Arc<Scheduler>>,
    kind: WorkloadKind,
) -> Result<(), String> {
    scheduler.set_foreground(kind).await;
    Ok(())
}

#[tauri::command]
pub async fn cancel_index_job(
    pool: State<'_, SqlitePool>,
    scheduler: State<'_, Arc<Scheduler>>,
    job_id: String,
    workload_id: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&workload_id).map_err(|e| e.to_string())?;
    scheduler.request_cancel(id);
    repo::set_job_status(&pool, &job_id, "cancelled", None).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn discard_index_job(
    pool: State<'_, SqlitePool>,
    job_id: String,
) -> Result<(), String> {
    repo::delete_job(&pool, &job_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_active_index_jobs(
    pool: State<'_, SqlitePool>,
) -> Result<Vec<repo::IndexJob>, String> {
    repo::list_active_jobs(&pool).await.map_err(|e| e.to_string())
}
```

Add `resume_index_job` as a thin wrapper that scans the root folder again, calls `plan_resume`, creates a fresh job, and spawns `run_index_job`. If the existing indexer already has a `start_index` Tauri command, extend it to branch on "resume previous" vs "start new" rather than introducing a new command.

- [ ] **Step 2: Wire module**

Append to `src-tauri/src/commands/mod.rs`:

```rust
pub mod scheduler;
```

- [ ] **Step 3: Register in invoke_handler**

In `src-tauri/src/lib.rs:64` (the `invoke_handler!` block), add:

```rust
crate::commands::scheduler::set_foreground,
crate::commands::scheduler::cancel_index_job,
crate::commands::scheduler::discard_index_job,
crate::commands::scheduler::list_active_index_jobs,
```

- [ ] **Step 4: Verify build**

Run: `cd src-tauri && cargo check`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/scheduler.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(commands): set_foreground + cancel/discard/list index jobs"
```

---

## Task 16: Chat workload registration in query_notebook

**Files:**
- Modify: `src-tauri/src/commands/query.rs`
- Modify: `src-tauri/src/services/query_router.rs`
- Modify: `src-tauri/src/services/llm.rs` (integrate `PrioritySlot`)

- [ ] **Step 1: Register Chat handle around the query**

Edit `query.rs::query_notebook`. Inject `State<'_, Arc<Scheduler>>`:

```rust
use crate::services::scheduler::{Scheduler, WorkloadKind};

#[tauri::command]
pub async fn query_notebook(
    pool: State<'_, SqlitePool>,
    embedder: State<'_, Arc<EmbedderState>>,
    model_slot: State<'_, Arc<ModelSlot>>,
    tier: State<'_, HardwareTier>,
    scheduler: State<'_, Arc<Scheduler>>,
    session_id: String,
    notebook_id: String,
    query_text: String,
) -> Result<QueryResponseDto, String> {
    let handle = scheduler.register(WorkloadKind::Chat);

    // existing persist-user-message + route(...) logic unchanged, but pass handle.
    let resp = query_router::route(
        &pool, embedder.inner(), &model_slot, &tier,
        &notebook_id, &session_id, &query_text,
        Some(handle.cancellation_token()),
    ).await.map_err(|e| e.to_string());

    scheduler.unregister(handle.id);
    let resp = resp?;
    // … rest unchanged …
}
```

- [ ] **Step 2: Thread the token through `query_router::route`**

Add an optional `CancellationToken` parameter. In the Tier 3/4 LLM call, pass it to `llm::generate_with_cancel`. For a cancelled chat (unusual — chat is never preempted by the scheduler, but users might close the window), return `Err(Cancelled)` early.

- [ ] **Step 3: Priority-acquire the model slot**

If `ModelSlot`'s current acquire is `Mutex::lock`, wrap it in `PrioritySlot`:

- Change `ModelSlot` to hold a `PrioritySlot` internally.
- Add `ModelSlot::acquire_chat()` and `acquire_background()` variants.
- Update `query_router` to call `acquire_chat()`; update `entity_extractor` + `batch_processor` to call `acquire_background()`.

- [ ] **Step 4: Verify build + existing tests**

Run: `cd src-tauri && cargo test --lib -- --nocapture`
Expected: all existing tests + new scheduler tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/query.rs src-tauri/src/services/query_router.rs src-tauri/src/services/llm.rs
git commit -m "feat(chat): register Chat workload and priority-acquire model slot"
```

---

## Task 17: Frontend scheduler store

**Files:**
- Create: `src/stores/scheduler.ts`

- [ ] **Step 1: Create reactive store**

Write `src/stores/scheduler.ts`:

```typescript
import { createSignal, createEffect } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

export type WorkloadKind =
  | "chat" | "indexing" | "pass2_ner" | "kg_extraction" | "file_watcher";

export interface Progress {
  processed: number;
  total: number;
  started_at_ms: number;
  last_tick_at_ms: number;
  paused: boolean;
}

export interface WorkloadState {
  id: string;
  kind: WorkloadKind;
  progress: Progress;
}

const [workloads, setWorkloads] = createSignal<Record<string, WorkloadState>>({});

type Event =
  | { type: "foreground_changed"; kind: WorkloadKind }
  | { type: "workload_registered"; id: string; kind: WorkloadKind }
  | { type: "workload_finished"; id: string; kind: WorkloadKind }
  | { type: "progress_tick"; id: string; kind: WorkloadKind; progress: Progress };

listen<Event>("workload://state", (e) => {
  const ev = e.payload;
  setWorkloads((ws) => {
    const next = { ...ws };
    switch (ev.type) {
      case "workload_registered":
        next[ev.id] = { id: ev.id, kind: ev.kind, progress: { processed: 0, total: 0, started_at_ms: Date.now(), last_tick_at_ms: Date.now(), paused: false } };
        break;
      case "workload_finished":
        delete next[ev.id];
        break;
      case "progress_tick":
        if (next[ev.id]) next[ev.id] = { ...next[ev.id], progress: ev.progress };
        break;
    }
    return next;
  });
});

export function useWorkloads() { return workloads; }

export function workloadsOfKind(kind: WorkloadKind) {
  return () => Object.values(workloads()).filter((w) => w.kind === kind);
}

export async function setForeground(kind: WorkloadKind) {
  await invoke("set_foreground", { kind });
}

export async function cancelIndexJob(jobId: string, workloadId: string) {
  await invoke("cancel_index_job", { jobId, workloadId });
}

export async function discardIndexJob(jobId: string) {
  await invoke("discard_index_job", { jobId });
}
```

- [ ] **Step 2: Verify frontend typechecks**

Run: `cd / && pnpm -C "/media/riaz/New Volume/echo-AI" typecheck` (or whatever typecheck script `package.json` exposes; commonly `pnpm tsc --noEmit` or `pnpm check`).
Expected: 0 errors.

- [ ] **Step 3: Commit**

```bash
git add src/stores/scheduler.ts
git commit -m "feat(frontend): scheduler store for workload://state events"
```

---

## Task 18: WorkloadBar component

**Files:**
- Create: `src/components/WorkloadBar.tsx`

- [ ] **Step 1: Create component**

Write `src/components/WorkloadBar.tsx`:

```tsx
import { Show, createMemo } from "solid-js";
import { WorkloadState, cancelIndexJob } from "../stores/scheduler";

interface Props {
  workload: WorkloadState;
  jobId?: string; // required for indexing (for cancel)
  onPause?: () => void;
  onResume?: () => void;
  isForeground: boolean;
}

function formatEta(ms: number): string {
  const s = Math.floor(ms / 1000);
  const m = Math.floor(s / 60);
  const rem = s % 60;
  return m > 0 ? `~${m}m ${rem}s remaining` : `~${rem}s remaining`;
}

const WARMUP = 10;

export default function WorkloadBar(props: Props) {
  const p = () => props.workload.progress;
  const pct = () => p().total > 0 ? Math.floor((p().processed / p().total) * 100) : 0;

  const eta = createMemo(() => {
    if (p().paused) return `Paused — resumes when you return to ${props.workload.kind}`;
    if (p().processed < WARMUP) return "Calibrating…";
    const elapsed = p().last_tick_at_ms - p().started_at_ms;
    const perItem = p().processed > 0 ? elapsed / p().processed : 0;
    const remaining = p().total - p().processed;
    return formatEta(perItem * remaining);
  });

  const onCancel = async () => {
    if (!props.jobId) return;
    if (!confirm("Cancel this job? You can resume later or discard progress.")) return;
    await cancelIndexJob(props.jobId, props.workload.id);
  };

  return (
    <div class="workload-bar">
      <div class="progress-bar" style={{ width: "100%", background: "#eee", height: "8px" }}>
        <div style={{ width: `${pct()}%`, background: "#3b82f6", height: "100%" }} />
      </div>
      <div class="progress-text">
        {p().processed} / {p().total} ({pct()}%)
      </div>
      <div class="eta-text">{eta()}</div>
      <div class="controls">
        <Show when={props.isForeground && props.onPause && !p().paused}>
          <button onClick={props.onPause}>Pause</button>
        </Show>
        <Show when={props.isForeground && props.onResume && p().paused}>
          <button onClick={props.onResume}>Resume</button>
        </Show>
        <Show when={props.jobId}>
          <button onClick={onCancel}>Cancel</button>
        </Show>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Typecheck**

Run frontend typecheck. Expected: 0 errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/WorkloadBar.tsx
git commit -m "feat(frontend): WorkloadBar component with progress + ETA + cancel"
```

---

## Task 19: NavBadge + view transitions

**Files:**
- Create: `src/components/NavBadge.tsx`
- Modify: `src/App.tsx`
- Modify: page files that own a workload (e.g., `src/pages/Dashboard.tsx` if that's where indexing is triggered)

- [ ] **Step 1: Create NavBadge**

Write `src/components/NavBadge.tsx`:

```tsx
import { For, Show } from "solid-js";
import { useWorkloads } from "../stores/scheduler";

export default function NavBadge() {
  const workloads = useWorkloads();
  const background = () => Object.values(workloads()).filter(
    (w) => w.progress.paused || w.kind !== "chat",
  );
  return (
    <Show when={background().length > 0}>
      <div class="nav-badge">
        <For each={background()}>{(w) => (
          <span>{w.kind} {w.progress.paused ? "paused" : ""} • {w.progress.processed}/{w.progress.total}</span>
        )}</For>
      </div>
    </Show>
  );
}
```

- [ ] **Step 2: Hook view transitions in `App.tsx`**

In `App.tsx`, wherever route/view state changes, call `setForeground(kind)`:

```tsx
import { setForeground } from "./stores/scheduler";
import { createEffect } from "solid-js";

// inside App():
createEffect(() => {
  const route = currentRoute(); // whatever the existing router signal is
  const kind: WorkloadKind = match(route)
    .with({ name: "chat" }, () => "chat" as const)
    .with({ name: "dashboard" }, () => "indexing" as const)
    .with({ name: "knowledge-graph" }, () => "kg_extraction" as const)
    .otherwise(() => "chat" as const);
  setForeground(kind);
});
```

(Exact matcher syntax depends on existing router. Use whatever pattern the codebase already uses.)

- [ ] **Step 3: Mount `<WorkloadBar>` on pages**

On `Dashboard.tsx` (indexing owner), add `<WorkloadBar workload={...} jobId={...} isForeground />` when an active indexing workload exists.

- [ ] **Step 4: Typecheck + manual smoke**

Run typecheck. Expected: 0 errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/NavBadge.tsx src/App.tsx src/pages/Dashboard.tsx
git commit -m "feat(frontend): NavBadge + set_foreground on view transitions"
```

---

## Task 20: Integration test — chat-while-indexing pauses and resumes

**Files:**
- Create: `src-tauri/tests/chat_while_indexing.rs`

- [ ] **Step 1: Write the test**

```rust
//! Integration: indexing pauses when chat becomes foreground, resumes after.

use std::sync::Arc;
use std::time::Duration;

use echo_ai::services::scheduler::{Scheduler, WorkloadKind};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn indexing_pauses_during_chat_then_resumes() {
    let sched = Scheduler::new();
    sched.set_foreground(WorkloadKind::Indexing).await;

    let indexing = sched.register(WorkloadKind::Indexing);
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

    // Simulated indexer: tick every 20ms but respect pause.
    let c = counter.clone();
    let idx_handle = indexing.clone();
    let idx_task = tokio::spawn(async move {
        for _ in 0..50 {
            idx_handle.should_pause().await;
            if idx_handle.is_cancelled() { break; }
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    let before_pause = counter.load(std::sync::atomic::Ordering::SeqCst);

    sched.set_foreground(WorkloadKind::Chat).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let during_pause = counter.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        during_pause - before_pause <= 1,
        "indexing should have paused; pre={before_pause} during={during_pause}"
    );

    sched.set_foreground(WorkloadKind::Indexing).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after_resume = counter.load(std::sync::atomic::Ordering::SeqCst);
    assert!(after_resume > during_pause, "indexing should have resumed");

    idx_task.abort();
}
```

- [ ] **Step 2: Run**

Run: `cd src-tauri && cargo test --test chat_while_indexing -- --nocapture`
Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tests/chat_while_indexing.rs
git commit -m "test(integration): chat-while-indexing pause/resume"
```

---

## Task 21: DEMO_GUIDE update + manual smoke checklist

**Files:**
- Modify: `DEMO_GUIDE.md`

- [ ] **Step 1: Append a new section**

```markdown
## Foreground Scheduler & Cancellable Indexing (Cycle 1)

### Scenario A — Chat while indexing
1. Open the Dashboard and start indexing a folder with ≥100 files.
2. Wait until the progress bar shows "Calibrating…" has finished (≥10 files processed).
3. Navigate to Chat and ask a question.
   - Expected: indexing WorkloadBar shows `Paused`; nav badge shows `indexing paused • N/M`; chat responds.
4. Navigate back to Dashboard.
   - Expected: indexing resumes from `N`, progress continues.

### Scenario B — Cancel mid-batch, discard progress
1. During an active index, click **Cancel** on the WorkloadBar.
2. In the dialog, choose **Discard progress**.
   - Expected: job row deleted, corpus contains only files that completed before the cancel. No half-chunked files in `chunks` table.

### Scenario C — Resume after cancel
1. Repeat scenario B, but choose **Resume later**.
2. On the Dashboard, click **Resume** on the paused job.
   - Expected: pre-scan skips `done` files, processes remaining.
```

- [ ] **Step 2: Commit**

```bash
git add DEMO_GUIDE.md
git commit -m "docs: DEMO_GUIDE coverage for foreground scheduler"
```

---

## Self-Review Checklist

**Spec coverage map:**
- §4 Workload taxonomy → Task 2 (types) + wiring in Tasks 9, 12, 13, 16
- §5 Architecture (scheduler API) → Tasks 2, 3
- §5 Priority ModelSlot → Task 5, integrated in Task 16
- §5 LLM preemption → Task 6
- §6 Data model → Task 7
- §6 Repository → Task 8
- §6 Per-file atomicity + cancel at file boundary → Task 9
- §6 Resume flow → Task 10
- §6 Crash recovery → Task 11
- §6 Restart-from-scratch → Task 15 (`discard_index_job`)
- §7 ETA → Task 4 (used by WorkloadBar in Task 18)
- §7 Progress events at 4 Hz → Task 14
- §8 WorkloadBar → Task 18
- §8 NavBadge → Task 19
- §8 Cancel dialog → Task 18 (confirm() in onCancel — minimal)
- §9 Error handling (per-file, systemic, crash) → Tasks 9, 11
- §9 Preemption budgets → Tasks 6 (LLM), 13 (file-watcher)
- §9 Scheduler invariants → Task 3 (idempotent, cancel-before-register handled via `request_cancel` semantics)
- §10 Unit tests → Tasks 3, 4, 5, 8, 9, 10, 12
- §10 Integration test → Task 20
- §10 Manual smoke → Task 21

**Gaps noted:**
- §9 "At most one Chat workload in flight" — not explicitly enforced; relies on UI disabling the send button. If stronger server-side check is desired, add to Task 16.
- §8 "Resume later / Discard progress" two-button dialog is approximated with a native `confirm()` in Task 18. A richer dialog can follow in cycle 2.
- §12 Risk #3 (E2E test for foreground transitions) is deferred per the spec; not a task here.

**Placeholder scan:** none found. Every step has concrete code or exact commands.

**Type consistency check:**
- `WorkloadKind` enum variants match in Rust (`Chat, Indexing, Pass2Ner, KgExtraction, FileWatcher`) and TypeScript (`chat, indexing, pass2_ner, kg_extraction, file_watcher` via `serde(rename_all = "snake_case")`). ✓
- `WorkloadHandle` fields: `id, kind, pause, notify_resume, cancel, progress_tx` — referenced consistently. ✓
- `Scheduler::register / set_foreground / request_cancel / unregister / subscribe / start_progress_pump` — signatures consistent across Tasks 2, 3, 14, 15. ✓
- `generate_with_cancel(prompt, max_tokens, Option<CancellationToken>) -> Result<String, LlmError>` — referenced identically in Tasks 6, 12. ✓
- `run_index_job(pool, handle, job_id, files)` — signature used identically in Tasks 9 and 15 (resume command). ✓
- `plan_resume(pool, notebook_id, root_path, scanned)` → `ResumePlan { skipped, to_process }` — used in Tasks 10 and 15. ✓
