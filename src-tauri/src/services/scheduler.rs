//! Foreground-priority scheduler. See
//! docs/superpowers/specs/2026-04-24-foreground-scheduler-design.md

use dashmap::DashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::{broadcast, watch, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadKind {
    Chat,
    Batch,
    Indexing,
    Pass2Ner,
    KgExtraction,
    FileWatcher,
}

impl WorkloadKind {
    pub fn uses_llm(self) -> bool {
        matches!(self, Self::Chat | Self::Batch | Self::Pass2Ner | Self::KgExtraction)
    }

    pub fn priority(self) -> u8 {
        match self {
            Self::Chat | Self::Batch => 2,
            Self::KgExtraction | Self::Pass2Ner => 1,
            Self::Indexing | Self::FileWatcher => 0,
        }
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
    BackgroundPaused { kind: WorkloadKind },
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
    pub fn is_paused(&self) -> bool {
        self.pause.load(Ordering::Acquire)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Cooperative pause: call at natural boundaries in worker loops.
    pub async fn should_pause(&self) {
        while self.pause.load(Ordering::Acquire) && !self.cancel.is_cancelled() {
            let notified = self.notify_resume.notified();
            if !self.pause.load(Ordering::Acquire) {
                break;
            }
            notified.await;
        }
    }

    pub fn pause_job(&self) {
        self.pause.store(true, Ordering::Release);
    }

    pub fn resume_job(&self) {
        self.pause.store(false, Ordering::Release);
        self.notify_resume.notify_waiters();
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
    pub(crate) workloads: DashMap<Uuid, Arc<WorkloadHandle>>,
    /// Stop-button seam (F026): the currently-running chat turn per conversation,
    /// so an out-of-band `cancel_turn` request can find the right workload id to
    /// cancel. Populated by the chat capability around `register`/`unregister`;
    /// last-writer-wins (a new turn supersedes the previous one for the same
    /// conversation), and cleared only when the *matching* id finishes so a slow
    /// tail turn never wipes a newer turn's entry.
    pub(crate) chat_turns: DashMap<String, Uuid>,
    pub(crate) foreground: RwLock<WorkloadKind>,
    pub(crate) events: broadcast::Sender<SchedulerEvent>,
    pub(crate) app: Option<crate::compat::AppHandle>,
}

impl Scheduler {
    pub fn new(app: Option<crate::compat::AppHandle>) -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(Self {
            workloads: DashMap::new(),
            chat_turns: DashMap::new(),
            foreground: RwLock::new(WorkloadKind::Chat),
            events: tx,
            app,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SchedulerEvent> {
        self.events.subscribe()
    }

    /// Spawn a background task that broadcasts progress snapshots at ≤4 Hz.
    pub fn start_progress_pump(self: &Arc<Self>, app: crate::compat::AppHandle) {
        use crate::compat::Emitter;
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

    pub fn register(self: &Arc<Self>, kind: WorkloadKind) -> Arc<WorkloadHandle> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().timestamp_millis();
        let (tx, _rx) = watch::channel(Progress {
            started_at_ms: now,
            last_tick_at_ms: now,
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

        // Apply current foreground policy to the newly registered workload.
        let foreground = futures::executor::block_on(self.foreground.read());
        if kind != *foreground {
            self.apply_policy_to(&handle, *foreground);
        }

        self.workloads.insert(id, handle.clone());
        let _ = self.events.send(SchedulerEvent::WorkloadRegistered { id, kind });
        log::info!("METRIC workload_registered kind={kind:?} id={id}");
        handle
    }

    pub async fn set_foreground(&self, kind: WorkloadKind) {
        {
            let mut g = self.foreground.write().await;
            if *g == kind {
                return; // idempotent
            }
            *g = kind;
        }
        log::info!("METRIC foreground_set kind={kind:?}");
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
            // Unblock any thread parked in should_pause().
            h.pause.store(false, Ordering::Release);
            h.notify_resume.notify_waiters();
        }
    }

    /// Stop-button seam (F026): record which workload is serving a conversation's
    /// current chat turn. Last-writer-wins — a new turn for the same conversation
    /// supersedes the previous mapping.
    pub fn note_chat_turn(&self, conversation_id: &str, id: Uuid) {
        self.chat_turns.insert(conversation_id.to_string(), id);
    }

    /// Stop-button seam (F026): forget a conversation's mapping, but ONLY if it
    /// still points at `id`. A slow tail turn finishing after a newer turn started
    /// must not wipe the newer turn's entry (which would silently disable Stop).
    pub fn clear_chat_turn(&self, conversation_id: &str, id: Uuid) {
        self.chat_turns
            .remove_if(conversation_id, |_, current| *current == id);
    }

    /// Stop-button seam (F026): cancel whatever chat turn is currently running for
    /// this conversation. Returns `true` if a live turn was found and signalled.
    /// The turn's cooperative cancel checks (round/generation boundaries) then
    /// unwind it; nothing is lost that was not already persisted.
    pub fn cancel_conversation(&self, conversation_id: &str) -> bool {
        let Some(id) = self.chat_turns.get(conversation_id).map(|e| *e.value()) else {
            return false;
        };
        self.request_cancel(id);
        log::info!("METRIC chat_turn_cancelled conversation={conversation_id} id={id}");
        true
    }

    pub fn unregister(&self, id: Uuid) {
        if let Some((_, h)) = self.workloads.remove(&id) {
            log::info!("METRIC workload_finished kind={:?} id={id}", h.kind);
            let _ = self.events.send(SchedulerEvent::WorkloadFinished {
                id,
                kind: h.kind,
            });
        }
    }

    fn apply_policy_to(&self, h: &Arc<WorkloadHandle>, foreground: WorkloadKind) {
        if h.kind.uses_llm() && foreground.uses_llm() {
            // Only cancel a background LLM workload if the new foreground has
            // equal or higher priority. This prevents KgExtraction / Pass2Ner
            // (priority 1) from cancelling Chat / Batch (priority 2).
            if foreground.priority() > h.kind.priority() {
                h.cancel.cancel();
                h.pause.store(false, Ordering::Release);
                h.notify_resume.notify_waiters();
                if foreground == WorkloadKind::Chat {
                    use crate::compat::Emitter;
                    if let Some(a) = &self.app {
                        let _ = a.emit("chat://background-paused",
                            serde_json::json!({ "kind": format!("{:?}", h.kind) }));
                    }
                    let _ = self.events.send(SchedulerEvent::BackgroundPaused { kind: h.kind });
                }
            }
            // foreground.priority() < h.kind.priority() → background workload
            // is higher priority (e.g. Chat running, KgExtraction takes foreground)
            // → do NOT cancel it; let it finish.
        } else {
            // Non-LLM workloads (Indexing, FileWatcher): always let them run.
            // They throttle themselves via TaskBudget and INDEXING_SEM.
            // Pausing them here would permanently block them if foreground never
            // switches to Indexing (which it never does in the current UI flow).
            h.pause.store(false, Ordering::Release);
            h.notify_resume.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn set_foreground_cancels_llm_workload() {
        let sched = Scheduler::new(None);
        let h = sched.register(WorkloadKind::Pass2Ner);
        sched.set_foreground(WorkloadKind::Chat).await;
        assert!(h.is_cancelled(), "LLM-using workload must be cancelled");
    }

    #[tokio::test]
    async fn set_foreground_does_not_pause_indexing_workload() {
        // Non-LLM workloads (Indexing) must never be paused by the scheduler —
        // they use TaskBudget for self-throttling and would deadlock if paused
        // because foreground is never set to Indexing in normal UI flow.
        let sched = Scheduler::new(None);
        sched.set_foreground(WorkloadKind::Indexing).await;
        let h = sched.register(WorkloadKind::Indexing);
        sched.set_foreground(WorkloadKind::Chat).await;
        assert!(!h.is_paused(), "Indexing must not be paused when Chat is foreground");
        assert!(!h.is_cancelled());
    }

    #[tokio::test]
    async fn indexing_never_cancelled_when_chat_takes_foreground() {
        let sched = Scheduler::new(None);
        sched.set_foreground(WorkloadKind::Indexing).await;
        let h = sched.register(WorkloadKind::Indexing);
        sched.set_foreground(WorkloadKind::Chat).await;
        assert!(!h.is_cancelled(), "Indexing (non-LLM) must not be cancelled");
        assert!(!h.is_paused(), "Indexing must remain runnable");
    }

    #[tokio::test]
    async fn llm_workload_cancelled_when_different_llm_takes_foreground() {
        // Pass2Ner uses LLM; when Chat (also LLM) takes foreground, Pass2Ner is cancelled.
        let sched = Scheduler::new(None);
        let h = sched.register(WorkloadKind::Pass2Ner);
        sched.set_foreground(WorkloadKind::Chat).await;
        assert!(h.is_cancelled(), "LLM workload must be cancelled when another LLM takes foreground");
    }

    #[tokio::test]
    async fn register_foreground_kind_not_paused() {
        let sched = Scheduler::new(None);
        sched.set_foreground(WorkloadKind::Indexing).await;
        let h = sched.register(WorkloadKind::Indexing);
        assert!(!h.is_paused());
        assert!(!h.is_cancelled());
    }

    #[tokio::test]
    async fn request_cancel_trips_token() {
        let sched = Scheduler::new(None);
        let h = sched.register(WorkloadKind::Indexing);
        sched.request_cancel(h.id);
        assert!(h.is_cancelled());
    }

    #[tokio::test]
    async fn cancel_conversation_trips_the_mapped_turn() {
        // F026 stop button: note a conversation's turn, then cancel by conversation.
        let sched = Scheduler::new(None);
        let h = sched.register(WorkloadKind::Chat);
        sched.note_chat_turn("conv-1", h.id);
        assert!(sched.cancel_conversation("conv-1"), "a live turn should be found");
        assert!(h.is_cancelled(), "the mapped workload's token must trip");
        // Unknown conversation → no-op, reported as not-found.
        assert!(!sched.cancel_conversation("conv-unknown"));
    }

    #[tokio::test]
    async fn clear_chat_turn_only_removes_the_matching_id() {
        // A slow tail turn finishing must not wipe a newer turn's mapping (which
        // would silently disable Stop for the newer turn).
        let sched = Scheduler::new(None);
        let old = sched.register(WorkloadKind::Chat);
        let new = sched.register(WorkloadKind::Chat);
        sched.note_chat_turn("conv-1", old.id);
        sched.note_chat_turn("conv-1", new.id); // new turn supersedes old
        sched.clear_chat_turn("conv-1", old.id); // old finishes late — must be a no-op
        assert!(sched.cancel_conversation("conv-1"), "newer turn mapping must survive");
        assert!(new.is_cancelled(), "the newer turn is the one cancelled");
        assert!(!old.is_cancelled(), "the stale turn must be untouched");
    }

    #[tokio::test]
    async fn set_foreground_is_idempotent() {
        let sched = Scheduler::new(None);
        sched.set_foreground(WorkloadKind::Indexing).await;
        sched.set_foreground(WorkloadKind::Indexing).await; // second call — no panic
        let h = sched.register(WorkloadKind::Indexing);
        assert!(!h.is_paused());
    }

    #[tokio::test]
    async fn kg_extraction_does_not_cancel_chat() {
        // KgExtraction (priority 1) must NOT cancel Chat (priority 2).
        let sched = Scheduler::new(None);
        let h = sched.register(WorkloadKind::Chat);
        sched.set_foreground(WorkloadKind::KgExtraction).await;
        assert!(!h.is_cancelled(), "Chat must not be cancelled when KgExtraction takes foreground");
    }

    #[tokio::test]
    async fn batch_not_cancelled_by_kg_extraction() {
        // Batch (priority 2) must NOT be cancelled by KgExtraction (priority 1).
        let sched = Scheduler::new(None);
        let h = sched.register(WorkloadKind::Batch);
        sched.set_foreground(WorkloadKind::KgExtraction).await;
        assert!(!h.is_cancelled(), "Batch must not be cancelled by KgExtraction");
    }

    #[tokio::test]
    async fn chat_cancels_ner() {
        // Chat (priority 2) MUST cancel Pass2Ner (priority 1).
        let sched = Scheduler::new(None);
        let h = sched.register(WorkloadKind::Pass2Ner);
        sched.set_foreground(WorkloadKind::Chat).await;
        assert!(h.is_cancelled(), "Chat must cancel NER so it can acquire the model slot");
    }
}
