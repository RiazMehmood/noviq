//! Per-query step-by-step trace logger.
//!
//! Two outputs per query (best-effort — never fails the query):
//!   1. Human-readable text file at `<app_data>/logs/queries/<ts>_<id>.log`
//!      — one file per query, easy to share or convert to PDF.
//!   2. Single-line JSON append at `<app_data>/logs/queries.jsonl`
//!      — machine-readable analytics.
//!
//! Goal: anyone (including non-technical users) can read the text file and
//! understand exactly what the app did with their question, in plain language.
//!
//! Resolution order for the log directory:
//!   1. `ECHO_LOG_DIR` env var (absolute path — explicit override).
//!   2. AppHandle's `app_data_dir()` (production path).
//!   3. `./logs` relative to current working directory (dev fallback).

use serde::Serialize;
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct StepFact {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceBlock {
    pub label: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceStep {
    pub n: u32,
    pub title: String,
    /// Milliseconds elapsed since query start when this step finished.
    pub at_ms: u128,
    pub facts: Vec<StepFact>,
    pub note: Option<String>,
    /// Multiline content blocks attached to this step (full prompt, full answer,
    /// retrieved-chunk previews). Rendered indented under the facts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<TraceBlock>,
}

/// Builder accumulating steps for a single query. Drop without `write()` =
/// trace is silently discarded (suitable for unit tests).
pub struct QueryTrace {
    id: String,
    started_at: chrono::DateTime<chrono::Utc>,
    started: Instant,
    query_text: String,
    session_id: String,
    notebook_id: String,
    steps: Vec<TraceStep>,
    total_ms: u128,
    outcome: String,
    final_answer_chars: usize,
    citations_count: usize,
    tier_used: String,
    /// The assistant capability that handled this turn (`guidance`, `count`,
    /// `draft`, …). Empty on the legacy desktop path. Surfaced in the admin
    /// query-debug list so an operator can filter by answer path.
    capability: String,
    /// Set by [`finish`]. Lets a turn-level owner (`assistant::run_turn`) detect
    /// whether a capability already recorded its precise outcome/tier, so it
    /// only stamps a generic finish when none did — see the external-trace flow.
    finished: bool,
}

impl QueryTrace {
    pub fn new(query_text: &str, session_id: &str, notebook_id: &str) -> Self {
        let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        Self {
            id,
            started_at: chrono::Utc::now(),
            started: Instant::now(),
            query_text: query_text.to_string(),
            session_id: session_id.to_string(),
            notebook_id: notebook_id.to_string(),
            steps: Vec::new(),
            total_ms: 0,
            outcome: "in_progress".to_string(),
            final_answer_chars: 0,
            citations_count: 0,
            tier_used: String::new(),
            capability: String::new(),
            finished: false,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Record the capability that handled this turn (for the debug list filter).
    pub fn set_capability(&mut self, capability: &str) {
        self.capability = capability.to_string();
    }

    /// True once [`finish`] has stamped the outcome — used by the turn-level
    /// owner to avoid clobbering a capability's precise outcome.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Append a step. Each fact is `(label, value)`.
    pub fn step(&mut self, title: &str, facts: &[(&str, String)]) {
        let n = self.steps.len() as u32 + 1;
        self.steps.push(TraceStep {
            n,
            title: title.to_string(),
            at_ms: self.started.elapsed().as_millis(),
            facts: facts
                .iter()
                .map(|(k, v)| StepFact {
                    label: (*k).to_string(),
                    value: v.clone(),
                })
                .collect(),
            note: None,
            blocks: Vec::new(),
        });
    }

    /// Attach a one-line note to the most recent step.
    pub fn note_last(&mut self, msg: &str) {
        if let Some(s) = self.steps.last_mut() {
            s.note = Some(msg.to_string());
        }
    }

    /// Attach a multiline content block (full prompt, full answer, chunk dump)
    /// to the most recent step. Content is stored verbatim; rendering indents
    /// it under the step's facts. Caller should keep blocks under ~8 KB.
    pub fn add_block(&mut self, label: &str, content: &str) {
        if let Some(s) = self.steps.last_mut() {
            s.blocks.push(TraceBlock {
                label: label.to_string(),
                content: content.to_string(),
            });
        }
    }

    pub fn finish(
        &mut self,
        outcome: &str,
        final_answer_chars: usize,
        citations: usize,
        tier_used: &str,
    ) {
        self.total_ms = self.started.elapsed().as_millis();
        self.outcome = outcome.to_string();
        self.final_answer_chars = final_answer_chars;
        self.citations_count = citations;
        self.tier_used = tier_used.to_string();
        self.finished = true;
    }

    /// Resolve the log directory using the priority described at top of file.
    pub fn resolve_log_dir(app: &Option<crate::compat::AppHandle>) -> PathBuf {
        if let Ok(p) = std::env::var("ECHO_LOG_DIR") {
            return PathBuf::from(p);
        }
        // Desktop production path: AppHandle → app_data_dir/logs. The headless
        // server build has no Tauri path resolver (compat AppHandle is a stub),
        // so it relies on ECHO_LOG_DIR above or the ./logs fallback below.
        #[cfg(feature = "desktop")]
        if let Some(a) = app {
            use tauri::Manager;
            if let Ok(d) = a.path().app_data_dir() {
                return d.join("logs");
            }
        }
        #[cfg(not(feature = "desktop"))]
        let _ = app;
        PathBuf::from("logs")
    }

    /// Write the trace to disk. Logs and swallows any IO error so the query
    /// path never fails because of trace plumbing.
    pub fn write(&self, app: &Option<crate::compat::AppHandle>) {
        let base = Self::resolve_log_dir(app);
        if let Err(e) = self.write_inner(&base) {
            log::warn!("query_trace: failed to write trace ({e})");
        } else {
            log::debug!("query_trace: wrote trace id={} to {}", self.id, base.display());
        }
    }

    fn write_inner(&self, base_dir: &std::path::Path) -> std::io::Result<()> {
        let queries_dir = base_dir.join("queries");
        create_dir_all(&queries_dir)?;
        let ts = self.started_at.format("%Y-%m-%d_%H-%M-%S");
        let txt_path = queries_dir.join(format!("{ts}_{}.log", self.id));
        std::fs::write(&txt_path, self.render_text())?;

        let jsonl_path = base_dir.join("queries.jsonl");
        let mut f = OpenOptions::new().create(true).append(true).open(&jsonl_path)?;
        let line = serde_json::to_string(&self.to_json()).unwrap_or_default();
        writeln!(f, "{line}")?;
        Ok(())
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "started_at": self.started_at.to_rfc3339(),
            "session_id": self.session_id,
            "notebook_id": self.notebook_id,
            "query": self.query_text,
            "capability": self.capability,
            "total_ms": self.total_ms,
            "outcome": self.outcome,
            "tier_used": self.tier_used,
            "answer_chars": self.final_answer_chars,
            "citations": self.citations_count,
            "steps": self.steps,
        })
    }

    fn render_text(&self) -> String {
        let mut s = String::with_capacity(2048);
        let bar = "═".repeat(80);
        let dash = "─".repeat(80);
        let preview = self.query_text.chars().take(120).collect::<String>();

        s.push_str(&bar);
        s.push_str("\n  EchoAI — Query Trace (step-by-step record)\n");
        s.push_str(&bar);
        s.push_str(&format!(
            "\n  Trace ID:        {}\n  Started (UTC):   {}\n  Notebook:        {}\n  Session:         {}\n  Capability:      {}\n  Question:        {}\n  Total time:      {} ms\n  Outcome:         {}\n  Model tier:      {}\n  Answer length:   {} chars  |  Citations: {}\n\n",
            self.id,
            self.started_at.format("%Y-%m-%d %H:%M:%S%.3f"),
            self.notebook_id,
            self.session_id,
            if self.capability.is_empty() { "—" } else { &self.capability },
            preview,
            self.total_ms,
            self.outcome,
            self.tier_used,
            self.final_answer_chars,
            self.citations_count,
        ));

        for step in &self.steps {
            s.push_str(&dash);
            s.push_str(&format!(
                "\n  STEP {} — {}    (at T+{} ms)\n",
                step.n, step.title, step.at_ms
            ));
            s.push_str(&dash);
            s.push('\n');
            for f in &step.facts {
                s.push_str(&format!("    • {:<22} {}\n", format!("{}:", f.label), f.value));
            }
            if let Some(note) = &step.note {
                s.push_str(&format!("    ⓘ {note}\n"));
            }
            for b in &step.blocks {
                s.push_str(&format!("\n    ┌── {} ──\n", b.label));
                for line in b.content.lines() {
                    s.push_str("    │ ");
                    s.push_str(line);
                    s.push('\n');
                }
                s.push_str("    └──\n");
            }
            s.push('\n');
        }
        s.push_str(&bar);
        s.push_str(&format!(
            "\n  End of trace — total {} ms ({:.2} s)\n",
            self.total_ms,
            self.total_ms as f64 / 1000.0
        ));
        s.push_str(&bar);
        s.push('\n');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn trace_writes_text_and_jsonl() {
        let tmp = TempDir::new().unwrap();
        std::env::set_var("ECHO_LOG_DIR", tmp.path());

        let mut t = QueryTrace::new("hello world", "sess-1", "nb-1");
        t.step("Receive", &[("Question", "hello world".to_string())]);
        t.step("Embed", &[("Time", "12 ms".to_string())]);
        t.finish("ok", 42, 1, "tier3");
        t.write(&None);

        let queries = tmp.path().join("queries");
        assert!(queries.exists(), "queries dir should exist");
        let entries: Vec<_> = std::fs::read_dir(&queries).unwrap().collect();
        assert_eq!(entries.len(), 1, "one .log file expected");

        let jsonl = tmp.path().join("queries.jsonl");
        assert!(jsonl.exists(), "queries.jsonl should exist");
        let body = std::fs::read_to_string(&jsonl).unwrap();
        assert!(body.contains("\"id\""));
        assert!(body.contains("\"hello world\""));

        std::env::remove_var("ECHO_LOG_DIR");
    }

    #[test]
    fn capability_is_recorded_in_json_and_text() {
        let mut t = QueryTrace::new("q", "s", "n");
        t.set_capability("count");
        t.step("Deterministic count/list (SQL)", &[("Unit", "rule".to_string())]);
        assert!(!t.is_finished());
        t.finish("count", 10, 0, "none");
        assert!(t.is_finished());
        let json = t.to_json();
        assert_eq!(json["capability"], "count");
        assert!(t.render_text().contains("Capability:"));
        assert!(t.render_text().contains("count"));
    }

    #[test]
    fn trace_render_contains_steps() {
        let mut t = QueryTrace::new("q", "s", "n");
        t.step("First", &[("k", "v".to_string())]);
        t.note_last("hello");
        t.finish("ok", 0, 0, "tier3");
        let text = t.render_text();
        assert!(text.contains("STEP 1"));
        assert!(text.contains("First"));
        assert!(text.contains("hello"));
    }

    #[test]
    fn add_block_renders_under_step() {
        let mut t = QueryTrace::new("q", "s", "n");
        t.step("LLM", &[("Tier", "tier3".to_string())]);
        t.add_block("Full prompt sent to LLM", "<|im_start|>system\nfoo\n<|im_end|>");
        t.finish("ok", 0, 0, "tier3");
        let text = t.render_text();
        assert!(text.contains("Full prompt sent to LLM"));
        assert!(text.contains("im_start"));
        // Each block line should be prefixed with the indented bar.
        assert!(text.contains("    │ <|im_start|>system"));
    }

    #[test]
    fn add_block_attaches_to_last_step_only() {
        let mut t = QueryTrace::new("q", "s", "n");
        t.step("S1", &[]);
        t.add_block("for-s1", "alpha");
        t.step("S2", &[]);
        t.add_block("for-s2", "beta");
        assert_eq!(t.steps[0].blocks.len(), 1);
        assert_eq!(t.steps[1].blocks.len(), 1);
        assert_eq!(t.steps[0].blocks[0].label, "for-s1");
        assert_eq!(t.steps[1].blocks[0].label, "for-s2");
    }
}
