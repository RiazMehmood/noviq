//! Two-queue priority acquirer wrapping the existing LLM model slot.
//! Chat jumps ahead of background (Pass2, KG) when both are waiting.
//! Spec §5.
//!
//! Uses std::sync::Mutex internally so Drop can release safely from async
//! context without calling tokio blocking_lock (which panics in async).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Priority { High, Low }

struct State {
    held: bool,
    high: VecDeque<oneshot::Sender<Permit>>,
    low: VecDeque<oneshot::Sender<Permit>>,
}

pub struct PrioritySlot {
    inner: Arc<Mutex<State>>,
}

pub struct Permit {
    slot: Arc<Mutex<State>>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut st = self.slot.lock().unwrap();
        st.held = false;
        if let Some(tx) = st.high.pop_front().or_else(|| st.low.pop_front()) {
            st.held = true;
            let slot = self.slot.clone();
            let _ = tx.send(Permit { slot });
        }
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
            let mut st = self.inner.lock().unwrap();
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
        let holder_permit = slot.acquire(Priority::Low).await;

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
