//! T029b — single-flight import queue (FR-024).
//!
//! One LLM conversion runs at a time. Pending imports queue up and broadcast
//! their position via `importer://queued` so the UI can render "Queued — N
//! ahead" without polling.

use std::collections::VecDeque;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct ImporterQueued {
    pub source_id: String,
    pub position: u32,        // 0 = currently running; 1+ = waiting
    pub queue_length: u32,
}

#[derive(Default)]
pub struct ImportQueue {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Index 0 is the actively running entry (if any). Subsequent entries are
    /// waiting in FIFO order.
    entries: VecDeque<String>,
}

impl ImportQueue {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Enqueue `source_id` and return its position (0 if no one's running).
    /// Caller should broadcast `importer://queued` for itself + for each
    /// entry whose position changed.
    pub async fn enqueue(&self, source_id: String) -> EnqueueResult {
        let mut inner = self.inner.lock().await;
        inner.entries.push_back(source_id.clone());
        let position = (inner.entries.len() - 1) as u32;
        let queue_length = inner.entries.len() as u32;
        let all_positions: Vec<(String, u32, u32)> = inner
            .entries
            .iter()
            .enumerate()
            .map(|(i, sid)| (sid.clone(), i as u32, queue_length))
            .collect();
        EnqueueResult {
            position,
            queue_length,
            broadcast: all_positions,
        }
    }

    /// Pop the head when the active import finishes/cancels. Returns the new
    /// head (the next entry to start) plus the updated positions for everyone
    /// still waiting.
    pub async fn pop_head(&self, expect_source_id: &str) -> PopResult {
        let mut inner = self.inner.lock().await;
        if let Some(head) = inner.entries.front() {
            if head != expect_source_id {
                // Mismatch — someone else's cancel raced ahead. No-op.
                return PopResult::default();
            }
        }
        inner.entries.pop_front();
        let queue_length = inner.entries.len() as u32;
        let next_head = inner.entries.front().cloned();
        let broadcast: Vec<(String, u32, u32)> = inner
            .entries
            .iter()
            .enumerate()
            .map(|(i, sid)| (sid.clone(), i as u32, queue_length))
            .collect();
        PopResult {
            next_head,
            broadcast,
        }
    }

    /// Returns the position of `source_id` in the queue (0-based; 0 = head).
    pub async fn position_of(&self, source_id: &str) -> Option<u32> {
        let inner = self.inner.lock().await;
        inner
            .entries
            .iter()
            .position(|s| s == source_id)
            .map(|p| p as u32)
    }

    pub async fn length(&self) -> u32 {
        self.inner.lock().await.entries.len() as u32
    }
}

#[derive(Debug)]
pub struct EnqueueResult {
    pub position: u32,
    pub queue_length: u32,
    /// (source_id, position, queue_length) — caller emits `importer://queued`
    /// for each entry.
    pub broadcast: Vec<(String, u32, u32)>,
}

#[derive(Debug, Default)]
pub struct PopResult {
    pub next_head: Option<String>,
    pub broadcast: Vec<(String, u32, u32)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enqueue_returns_position_zero_when_empty() {
        let q = ImportQueue::new();
        let r = q.enqueue("a".into()).await;
        assert_eq!(r.position, 0);
        assert_eq!(r.queue_length, 1);
    }

    #[tokio::test]
    async fn second_enqueue_gets_position_one() {
        let q = ImportQueue::new();
        q.enqueue("a".into()).await;
        let r = q.enqueue("b".into()).await;
        assert_eq!(r.position, 1);
        assert_eq!(r.queue_length, 2);
    }

    #[tokio::test]
    async fn pop_head_advances_others() {
        let q = ImportQueue::new();
        q.enqueue("a".into()).await;
        q.enqueue("b".into()).await;
        q.enqueue("c".into()).await;
        let p = q.pop_head("a").await;
        assert_eq!(p.next_head.as_deref(), Some("b"));
        // After pop: b is at 0, c is at 1, length is 2.
        assert_eq!(p.broadcast.len(), 2);
        assert_eq!(p.broadcast[0], ("b".to_string(), 0, 2));
        assert_eq!(p.broadcast[1], ("c".to_string(), 1, 2));
    }

    #[tokio::test]
    async fn pop_with_mismatched_head_is_noop() {
        let q = ImportQueue::new();
        q.enqueue("a".into()).await;
        let p = q.pop_head("b").await; // wrong id
        assert!(p.next_head.is_none());
        assert_eq!(q.length().await, 1);
    }
}
