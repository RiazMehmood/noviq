//! TaskBudget — three-state priority scheduler for background tasks.
//!
//! States: Active → Throttled → Idle
//! Background tasks are submitted with a priority (0=interactive, 1=normal, 2=low).
//! Only tasks at or below the currently-permitted priority level are dispatched.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::System;
use tokio::sync::mpsc::{self, Receiver, Sender};

/// The current scheduling state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetState {
    /// User is actively interacting — only priority 0 tasks run.
    Active,
    /// Moderate load — priority 0 and 1 tasks run.
    Throttled,
    /// System is idle — all tasks run.
    Idle,
}

/// A queued background task.
pub struct BackgroundTask {
    pub priority: u8,
    pub description: String,
    /// The async closure to execute. Boxed for type erasure.
    pub work: Box<dyn FnOnce() + Send + 'static>,
}

/// Shared state for the budget scheduler.
struct BudgetInner {
    state: BudgetState,
    last_user_activity: Instant,
}

/// The public handle to the TaskBudget.
#[derive(Clone)]
pub struct TaskBudget {
    inner: Arc<Mutex<BudgetInner>>,
    sender: Sender<BackgroundTask>,
}

impl TaskBudget {
    /// Create a new TaskBudget and start the background worker.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<BackgroundTask>(1024);
        let inner = Arc::new(Mutex::new(BudgetInner {
            state: BudgetState::Active,
            last_user_activity: Instant::now(),
        }));

        let inner_clone = inner.clone();
        tokio::spawn(budget_monitor(inner_clone));

        let inner_worker = inner.clone();
        tokio::spawn(task_worker(rx, inner_worker));

        TaskBudget { inner, sender: tx }
    }

    /// Submit a background task. If the channel is full the task is dropped and a warning logged.
    pub fn submit(&self, priority: u8, description: impl Into<String>, work: impl FnOnce() + Send + 'static) {
        let task = BackgroundTask {
            priority,
            description: description.into(),
            work: Box::new(work),
        };
        if self.sender.try_send(task).is_err() {
            log::warn!("TaskBudget channel full — task dropped");
        }
    }

    /// Check if the disk partition containing `path` has less than `min_free_gb` free.
    /// Returns `true` (low storage) or `false` (OK).
    pub fn is_storage_low(path: &str, min_free_gb: f64) -> bool {
        use sysinfo::Disks;
        let disks = Disks::new_with_refreshed_list();
        for disk in disks.list() {
            let mount = disk.mount_point().to_str().unwrap_or("");
            if path.starts_with(mount) {
                let free_gb = disk.available_space() as f64 / 1_073_741_824.0;
                return free_gb < min_free_gb;
            }
        }
        false // unknown → don't block
    }

    /// Signal that the user interacted with the app (reset Active timer).
    pub fn signal_user_activity(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_user_activity = Instant::now();
            inner.state = BudgetState::Active;
        }
    }

    /// Get the current budget state.
    pub fn state(&self) -> BudgetState {
        self.inner.lock().map(|g| g.state).unwrap_or(BudgetState::Throttled)
    }
}

/// Poll system metrics every 5 seconds and transition budget state.
async fn budget_monitor(inner: Arc<Mutex<BudgetInner>>) {
    let mut sys = System::new_all();
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        sys.refresh_cpu();

        let avg_cpu = sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>()
            / sys.cpus().len() as f32;

        if let Ok(mut g) = inner.lock() {
            let since_active = g.last_user_activity.elapsed();

            g.state = if since_active < Duration::from_secs(30) {
                BudgetState::Active
            } else if avg_cpu > 70.0 || since_active < Duration::from_secs(120) {
                BudgetState::Throttled
            } else {
                BudgetState::Idle
            };

            log::debug!("TaskBudget state: {:?} (cpu={:.0}%)", g.state, avg_cpu);
        }
    }
}

/// Drain the task queue respecting priority gating.
async fn task_worker(mut rx: Receiver<BackgroundTask>, inner: Arc<Mutex<BudgetInner>>) {
    while let Some(task) = rx.recv().await {
        let state = inner.lock().map(|g| g.state).unwrap_or(BudgetState::Throttled);

        let permitted = match state {
            BudgetState::Active    => task.priority == 0,
            BudgetState::Throttled => task.priority <= 1,
            BudgetState::Idle      => true,
        };

        if permitted {
            log::debug!("TaskBudget: executing '{}' (priority {})", task.description, task.priority);
            (task.work)();
        } else {
            log::debug!("TaskBudget: deferring '{}' (state too busy)", task.description);
            // Re-queue with a short delay.
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }
}
