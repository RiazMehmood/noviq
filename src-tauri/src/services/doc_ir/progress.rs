//! T012 — `ProgressSink` for emitting `importer://progress` events.
//!
//! See contracts/tauri_commands.md §1.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Classifying,
    Chunking,
    Converting,
    Validating,
    Indexing,
    Repairing,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImporterProgress {
    pub source_id: String,
    pub phase: Phase,
    pub current: u32,
    pub total: u32,
    pub retries: u32,
    pub eta_seconds: Option<u32>,
    pub message: Option<String>,
}

/// Sink for progress events — `Noop` for tests; `Tauri` for production.
#[derive(Clone)]
pub enum ProgressSink {
    Noop,
    Tauri(crate::compat::AppHandle),
}

impl ProgressSink {
    pub fn emit(&self, ev: &ImporterProgress) {
        match self {
            Self::Noop => {}
            Self::Tauri(app) => {
                let _ = crate::compat::Emitter::emit(app, "importer://progress", ev);
            }
        }
    }
}
