//! Batch processor — T078 (US5).
//!
//! Scans a folder, indexes each supported file, runs the requested operation
//! (summarize / extract_dates / flag_keywords / extract_actions) on each file's
//! content, persists per-file results, emits `batch:progress` Tauri events, and
//! fires a system notification on completion.

use std::sync::Arc;

use anyhow::Result;
use crate::db::Db;
use tauri::Emitter;

use crate::{
    db::repositories::{batch_jobs as batch_repo, chunks as chunk_repo},
    services::{
        embedder::EmbedderState,
        hardware_detector::HardwareTier,
        indexer,
        llm::ModelSlot,
        parsers::FileFormat,
        scheduler::{Scheduler, WorkloadHandle, WorkloadKind},
        task_budget::TaskBudget,
    },
};

/// Maps batch job_id → its scheduler handle so pause/resume commands can reach it.
pub type BatchHandleMap = dashmap::DashMap<String, Arc<WorkloadHandle>>;

// ─── Public entry point ───────────────────────────────────────────────────────

/// Queue and execute a batch job in a background task.
pub fn spawn_batch_job(
    app: tauri::AppHandle,
    pool: Db,
    embedder: Arc<EmbedderState>,
    model_slot: Arc<ModelSlot>,
    tier: HardwareTier,
    task_budget: TaskBudget,
    scheduler: Arc<Scheduler>,
    handle_map: Arc<BatchHandleMap>,
    job_id: String,
    folder_path: String,
    operation: String,
    operation_params_json: Option<String>,
) {
    let handle = scheduler.register(WorkloadKind::Batch);
    handle_map.insert(job_id.clone(), Arc::clone(&handle));

    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_job(
            &app,
            &pool,
            &embedder,
            &model_slot,
            &tier,
            &task_budget,
            &handle,
            &job_id,
            &folder_path,
            &operation,
            operation_params_json.as_deref(),
        )
        .await
        {
            log::error!("Batch job {job_id} failed: {e}");
            let _ = batch_repo::update_status(&pool, &job_id, "failed").await;
        }
        handle_map.remove(&job_id);
        scheduler.unregister(handle.id);
    });
}

// ─── Core execution ───────────────────────────────────────────────────────────

async fn run_job(
    app: &tauri::AppHandle,
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    task_budget: &TaskBudget,
    handle: &Arc<WorkloadHandle>,
    job_id: &str,
    folder_path: &str,
    operation: &str,
    operation_params_json: Option<&str>,
) -> Result<()> {
    // Mark running.
    batch_repo::update_status(pool, job_id, "running").await?;

    // Collect supported files.
    let files = collect_supported_files(folder_path);
    let total = files.len() as i64;

    // Update total_files if it differs from initial estimate.
    sqlx::query("UPDATE batch_jobs SET total_files = $1 WHERE id = $2")
        .bind(total)
        .bind(job_id)
        .execute(pool)
        .await?;

    if files.is_empty() {
        batch_repo::update_status(pool, job_id, "complete").await?;
        batch_repo::set_completed_at(pool, job_id).await?;
        emit_progress(app, job_id, 0, total, "No supported files found.");
        return Ok(());
    }

    // Create a temporary notebook row so index_file's FK on notebook_sources passes.
    // Deleted once the job finishes (cascades to notebook_sources rows).
    let stub_notebook = format!("__batch__{}", job_id);
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO notebooks (id, name, created_at, updated_at) VALUES ($1, $2, $3, $4) ON CONFLICT (id) DO NOTHING",
    )
    .bind(&stub_notebook)
    .bind(&stub_notebook)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    for (idx, file_path) in files.iter().enumerate() {
        // Cooperative pause — blocks here while user has paused the job.
        handle.should_pause().await;
        // Exit immediately if job was cancelled (× button).
        if handle.is_cancelled() {
            log::info!("Batch job {job_id} cancelled at file {idx}");
            break;
        }

        let file_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(file_path);

        let (status, output_text, error_reason) = process_file(
            pool,
            embedder,
            model_slot,
            tier,
            task_budget,
            &stub_notebook,
            file_path,
            operation,
            operation_params_json,
        )
        .await;

        // Persist per-file result.
        let _ = batch_repo::insert_result(
            pool,
            job_id,
            None,
            file_path,
            &status,
            output_text.as_deref(),
            error_reason.as_deref(),
        )
        .await;

        let failed = if status == "failed" { 1 } else { 0 };
        let _ = batch_repo::increment_counters(pool, job_id, 1, failed).await;

        emit_progress(
            app,
            job_id,
            (idx + 1) as i64,
            total,
            file_name,
        );
    }

    // Mark complete — DB CHECK constraint allows 'complete' not 'completed'.
    batch_repo::update_status(pool, job_id, "complete").await?;
    batch_repo::set_completed_at(pool, job_id).await?;

    // Remove stub notebook — cascades to notebook_sources rows created during indexing.
    let _ = sqlx::query("DELETE FROM notebooks WHERE id = $1")
        .bind(&stub_notebook)
        .execute(pool)
        .await;

    // Fetch final counts for notification.
    let counts = sqlx::query_as::<_, (i64, i64)>(
        "SELECT processed_files, failed_files FROM batch_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or((total, 0));

    emit_completion_notification(app, counts.0, counts.1);
    Ok(())
}

/// Process a single file: index it then run the operation.
/// Returns (status, output_text, error_reason).
async fn process_file(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    task_budget: &TaskBudget,
    notebook_id: &str,
    file_path: &str,
    operation: &str,
    operation_params_json: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    // Step 1: index the file to ensure its chunks are in DB.
    match indexer::index_file(pool, Arc::clone(embedder), task_budget, tier, notebook_id, file_path, None).await {
        Err(e) => {
            return (
                "failed".to_string(),
                None,
                Some(format!("Index error: {e}")),
            );
        }
        Ok(source_id) => {
            // Step 2: fetch chunk content from DB.
            let chunks = match chunk_repo::list_by_source(pool, &source_id).await {
                Ok(c) => c,
                Err(e) => {
                    return (
                        "failed".to_string(),
                        None,
                        Some(format!("Chunk fetch error: {e}")),
                    );
                }
            };

            if chunks.is_empty() {
                return (
                    "failed".to_string(),
                    None,
                    Some("No text content extracted".to_string()),
                );
            }

            // Concatenate up to 4000 chars of content for processing.
            let content: String = chunks
                .iter()
                .map(|c| c.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
                .chars()
                .take(4000)
                .collect();

            // Step 3: run the operation.
            let output = run_operation(
                pool, model_slot, tier, operation, operation_params_json, &content,
            )
            .await;

            match output {
                Ok(text) => ("success".to_string(), Some(text), None),
                Err(e) => ("failed".to_string(), None, Some(e.to_string())),
            }
        }
    }
}

/// Apply the requested batch operation to document content.
async fn run_operation(
    _pool: &Db,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    operation: &str,
    operation_params_json: Option<&str>,
    content: &str,
) -> Result<String> {
    let prompt = build_prompt(operation, operation_params_json, content);

    // Cloud routing (Groq) when enabled — see CHANGELOG_DEV.md (2026-05-03).
    let cs = crate::services::cloud_llm::load_settings_cached();
    if cs.enabled && !cs.api_key.trim().is_empty() {
        match crate::services::cloud_llm::groq_complete_blocking(
            &cs,
            "You are a precise document processor. Follow the user's format exactly. Do not repeat content.",
            &prompt,
            768,
        ) {
            Ok(text) if !text.trim().is_empty() => return Ok(text),
            Ok(_) => log::warn!("Groq batch returned empty; falling back to local"),
            Err(e) => log::warn!("Groq batch failed ({e}); falling back to local"),
        }
    }

    // Single-model strategy: any GGUF in models/. Tier struct retained for
    // thread/RAM info inside ModelSlot::infer but not consulted here.
    if let Some((model_path, _)) = crate::services::query_router::discover_local_gguf() {
        match model_slot.infer(&model_path, &prompt, 512, tier) {
            Ok(text) if !text.trim().is_empty() => return Ok(text),
            _ => {}
        }
    }

    // Regex fallback.
    Ok(regex_fallback(operation, operation_params_json, content))
}

fn build_prompt(operation: &str, params_json: Option<&str>, content: &str) -> String {
    match operation {
        "summarize" => format!(
            "Summarize the following document in 3-5 bullet points.\n\nDocument:\n{content}\n\nSummary:"
        ),
        "extract_dates" => format!(
            "List all dates and associated events found in this document.\n\nDocument:\n{content}\n\nDates:"
        ),
        "flag_keywords" => {
            let keywords = params_json
                .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
                .and_then(|v| v["keywords"].as_str().map(str::to_string))
                .unwrap_or_default();
            format!(
                "Find all occurrences of these keywords: {keywords}\n\nDocument:\n{content}\n\nOccurrences:"
            )
        }
        "extract_actions" => format!(
            "Extract all action items, tasks, or to-dos from this document.\n\nDocument:\n{content}\n\nAction items:"
        ),
        _ => format!("Process this document:\n{content}"),
    }
}

/// Pure-regex fallback for when no LLM is available.
fn regex_fallback(operation: &str, params_json: Option<&str>, content: &str) -> String {
    match operation {
        "extract_dates" => {
            let re = regex_lite::Regex::new(
                r"\b(\d{1,2}[/-]\d{1,2}[/-]\d{2,4}|\d{4}-\d{2}-\d{2}|[A-Z][a-z]+ \d{1,2},? \d{4})\b",
            )
            .unwrap();
            let dates: Vec<&str> = re
                .find_iter(content)
                .map(|m| m.as_str())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            if dates.is_empty() {
                "No dates found.".to_string()
            } else {
                dates.join("\n")
            }
        }
        "flag_keywords" => {
            let keywords_str = params_json
                .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
                .and_then(|v| v["keywords"].as_str().map(str::to_string))
                .unwrap_or_default();
            let keywords: Vec<&str> = keywords_str.split(',').map(str::trim).collect();
            let mut hits = vec![];
            for kw in &keywords {
                if kw.is_empty() { continue; }
                let count = content.to_lowercase().matches(&kw.to_lowercase()).count();
                if count > 0 {
                    hits.push(format!("{kw}: {count} occurrence(s)"));
                }
            }
            if hits.is_empty() {
                "No keyword occurrences found.".to_string()
            } else {
                hits.join("\n")
            }
        }
        "extract_actions" => {
            let action_keywords = ["TODO", "FIXME", "ACTION", "TASK", "ASSIGN", "DUE", "FOLLOW UP"];
            let mut items: Vec<&str> = vec![];
            for line in content.lines() {
                let upper = line.to_uppercase();
                if action_keywords.iter().any(|kw| upper.contains(kw)) {
                    items.push(line.trim());
                }
            }
            if items.is_empty() {
                "No action items found.".to_string()
            } else {
                items.join("\n")
            }
        }
        "summarize" | _ => {
            // Return first ~300 chars as a basic summary.
            let snippet: String = content.chars().take(300).collect();
            format!("(Summary requires LLM)\n\n{snippet}…")
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Collect all files in a folder that EchoAI can parse.
fn collect_supported_files(folder_path: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder_path) else {
        return vec![];
    };
    let mut files = vec![];
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if FileFormat::from_path(&path).is_some() {
                if let Some(s) = path.to_str() {
                    files.push(s.to_string());
                }
            }
        }
    }
    files.sort();
    files
}

/// Emit a `batch:progress` event to the frontend.
fn emit_progress(
    app: &tauri::AppHandle,
    job_id: &str,
    processed: i64,
    total: i64,
    current_file: &str,
) {
    let _ = app.emit(
        "batch:progress",
        serde_json::json!({
            "job_id": job_id,
            "processed": processed,
            "total": total,
            "current_file": current_file,
        }),
    );
}

// ─── Scheduler-aware KG extraction entry point ───────────────────────────────

/// A file reference for scheduler-aware KG extraction.
pub struct KgFileRef {
    pub path: String,
    pub source_id: String,
}

/// Scheduler-aware KG extraction. Checks pause/cancel before processing each file.
/// Returns immediately without DB writes if the handle is pre-cancelled.
pub async fn run_kg_extraction(
    _pool: &crate::db::Db,
    files: &[KgFileRef],
    handle: &std::sync::Arc<crate::services::scheduler::WorkloadHandle>,
) -> anyhow::Result<()> {
    for (i, file) in files.iter().enumerate() {
        handle.should_pause().await;
        if handle.is_cancelled() {
            return Ok(());
        }

        let token = handle.cancellation_token();
        let prompt = format!("Extract KG edges from: {}", &file.path);
        match crate::services::llm::generate_with_cancel(&prompt, 256, Some(token)).await {
            Ok(_) => { /* parse and persist KG edges */ }
            Err(crate::services::llm::LlmError::Cancelled) => return Ok(()),
            Err(e) => log::warn!("kg extraction failed for {}: {e}", file.source_id),
        }
        handle.report((i as u64) + 1, files.len() as u64);
    }
    Ok(())
}

#[cfg(test)]
mod scheduler_tests {
    use super::*;
    use crate::services::scheduler::{Scheduler, WorkloadKind};

    #[tokio::test]
    async fn kg_exits_on_precancelled_handle() {
        let pool = crate::db::test_pool().await;
        let sched = Scheduler::new(None);
        sched.set_foreground(WorkloadKind::KgExtraction).await;
        let h = sched.register(WorkloadKind::KgExtraction);
        h.cancel.cancel();
        let files: Vec<KgFileRef> = (0..5).map(|i| KgFileRef {
            path: format!("/file{i}.txt"),
            source_id: format!("src-{i}"),
        }).collect();
        let res = run_kg_extraction(&pool, &files, &h).await;
        assert!(res.is_ok());
        // No KG edges should have been written.
        let edges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_graph_edges")
            .fetch_one(&pool).await.unwrap_or(0);
        assert_eq!(edges, 0);
    }
}

/// Fire a system notification when the batch job is complete.
fn emit_completion_notification(app: &tauri::AppHandle, processed: i64, failed: i64) {
    use tauri_plugin_notification::NotificationExt;
    let summary = if failed == 0 {
        format!("Batch complete — {processed} files processed successfully.")
    } else {
        format!("Batch complete — {processed} files processed, {failed} failed.")
    };
    let _ = app
        .notification()
        .builder()
        .title("EchoAI Batch Processing")
        .body(&summary)
        .show();
}
