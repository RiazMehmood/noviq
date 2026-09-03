//! F014 — server-safe indexing orchestration + DTOs.
//!
//! "One implementation, two transports" (FR-001): both the desktop Tauri
//! commands (`commands/{sources,folders,entities}.rs`) and the headless HTTP
//! API (`api/documents.rs`) call these. They previously lived in `commands/`,
//! but that module is now desktop-only (`#[cfg(feature = "desktop")]`), so the
//! shared pieces moved here into the always-compiled service layer. The Tauri
//! commands re-export these names for backward compatibility; the server imports
//! them directly. `AppHandle` is `crate::compat` (a no-op stub on the server),
//! so on the headless path every caller passes `None`.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::compat::Emitter;
use crate::db::repositories::{
    entities as entity_repo, folders as folder_repo, sources as source_repo,
};
use crate::db::Db;
use crate::services::embedder::EmbedderState;
use crate::services::hardware_detector::HardwareTier;
use crate::services::indexer;
use crate::services::parsers::FileFormat;
use crate::services::task_budget::{BudgetState, TaskBudget};

// ── Sources ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SourceDto {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub format: String,
    pub file_size_bytes: i64,
    pub indexing_status: String,
    pub error_reason: Option<String>,
    pub last_modified: i64,
    pub indexed_at: Option<i64>,
    pub created_at: i64,
    pub index_tier: String,
    // F021 — per-document visibility for the share toggle (FR-012).
    #[serde(default = "default_visibility")]
    pub visibility: String,
    // Migration 021 — the corpus browser groups on `category_id` and greys out
    // rows with `disabled_at` set. Carried on the DTO because the admin document
    // list is the ONLY place either is edited, and it reads this shape.
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub disabled_at: Option<i64>,
}

impl From<source_repo::Source> for SourceDto {
    fn from(s: source_repo::Source) -> Self {
        SourceDto {
            id: s.id,
            file_path: s.file_path,
            file_name: s.file_name,
            format: s.format,
            file_size_bytes: s.file_size_bytes,
            indexing_status: s.indexing_status,
            error_reason: s.error_reason,
            last_modified: s.last_modified,
            indexed_at: s.indexed_at,
            created_at: s.created_at,
            index_tier: s.index_tier,
            visibility: s.visibility,
            category_id: s.category_id,
            disabled_at: s.disabled_at,
        }
    }
}

/// Queue indexing for one or more files (shared orchestration, F014/T024).
/// `index_file`'s `TaskBudget` arg is unused, so a throwaway is fine here.
/// `app` drives Tauri progress events on desktop; headless passes `None` (SSE
/// picks up progress from the indexer broadcast).
///
/// `progress_folder_id` is the folder whose migration-026 counters this batch
/// reports into — pass `None` to index without reporting progress.
///
/// `category_id` files each document as it finishes. It is applied HERE, on the
/// id `index_file` returns, rather than by the caller matching saved paths
/// afterwards: the source rows are created inside these spawned tasks, so a
/// caller that looks them up by path immediately can miss the ones not yet
/// inserted. (That race is real and still open at `uploads.rs` for project
/// attachment — same shape, different symptom.)
pub async fn index_sources(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    tier: &HardwareTier,
    notebook_id: &str,
    file_paths: &[String],
    app: Option<crate::compat::AppHandle>,
    progress_folder_id: Option<&str>,
    category_id: Option<&str>,
) -> Result<Vec<SourceDto>, String> {
    let notebook_id = notebook_id.to_string();
    let mut results = Vec::new();

    for path in file_paths {
        // Check for dedup.
        if let Some(existing) = source_repo::get_by_path(pool, path)
            .await
            .map_err(|e| e.to_string())?
        {
            // Attach existing source to notebook.
            source_repo::attach_to_notebook(pool, &notebook_id, &existing.id)
                .await
                .map_err(|e| e.to_string())?;

            // If the source previously failed or got stuck in "processing"
            // (e.g. interrupted by a crash or the old scheduler pause bug),
            // re-trigger indexing so the user isn't stuck forever.
            let needs_reindex = matches!(
                existing.indexing_status.as_str(),
                "processing" | "failed" | "pending"
            );
            if needs_reindex {
                let pool_ref = pool.clone();
                let embedder_ref = Arc::clone(embedder);
                let tier_ref = tier.clone();
                let notebook_id_clone = notebook_id.clone();
                let source_id_clone = existing.id.clone();
                let path_clone = path.clone();
                let budget_ref = TaskBudget::new();
                let app_ref = app.clone();
                let cat_ref = category_id.map(|s| s.to_string());
                let prog_ref = progress_folder_id.map(|s| s.to_string());
                tokio::spawn(async move {
                    // Reset status so the indexer writes a fresh source row.
                    let _ = source_repo::update_status(&pool_ref, &source_id_clone, "pending").await;
                    let outcome = indexer::index_file(
                        &pool_ref,
                        Arc::clone(&embedder_ref),
                        &budget_ref,
                        &tier_ref,
                        &notebook_id_clone,
                        &path_clone,
                        app_ref,
                    )
                    .await;
                    let sid = match &outcome {
                        Ok(id) => Some(id.as_str()),
                        Err(e) => {
                            log::error!("Re-indexing failed for {path_clone}: {e}");
                            None
                        }
                    };
                    after_file_indexed(&pool_ref, sid, cat_ref.as_deref(), prog_ref.as_deref())
                        .await;
                });
                // Return a pending DTO so the UI shows progress.
                results.push(SourceDto {
                    id: existing.id,
                    file_path: existing.file_path,
                    file_name: existing.file_name,
                    format: existing.format,
                    file_size_bytes: existing.file_size_bytes,
                    indexing_status: "pending".to_string(),
                    error_reason: None,
                    last_modified: existing.last_modified,
                    indexed_at: None,
                    created_at: existing.created_at,
                    index_tier: existing.index_tier,
                    visibility: existing.visibility,
                    // Re-indexing keeps the document where the admin filed it,
                    // and keeps it withdrawn if it was withdrawn.
                    category_id: existing.category_id,
                    disabled_at: existing.disabled_at,
                });
            } else {
                // Already indexed and unchanged: no work to do, but it is still
                // one of the N files this batch was asked about, so it must tick
                // the counter or the bar stops short of its total forever.
                after_file_indexed(pool, Some(&existing.id), category_id, progress_folder_id)
                    .await;
                results.push(existing.into());
            }
            continue;
        }

        // Queue indexing (concurrency bounded by indexer::indexing_sem()).
        let pool_ref = pool.clone();
        let embedder_ref = Arc::clone(embedder);
        let tier_ref = tier.clone();
        let notebook_id_clone = notebook_id.clone();
        let path_clone = path.clone();
        let budget_ref = TaskBudget::new();
        let app_ref = app.clone();

        let cat_ref = category_id.map(|s| s.to_string());
        let prog_ref = progress_folder_id.map(|s| s.to_string());
        tokio::spawn(async move {
            let outcome = indexer::index_file(
                &pool_ref,
                Arc::clone(&embedder_ref),
                &budget_ref,
                &tier_ref,
                &notebook_id_clone,
                &path_clone,
                app_ref,
            )
            .await;
            let sid = match &outcome {
                Ok(id) => Some(id.as_str()),
                Err(e) => {
                    log::error!("Indexing failed for {path_clone}: {e}");
                    None
                }
            };
            after_file_indexed(&pool_ref, sid, cat_ref.as_deref(), prog_ref.as_deref()).await;
        });

        // Return a pending source DTO immediately.
        let file_name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string();

        results.push(SourceDto {
            id: String::new(), // will be assigned by indexer
            file_path: path.clone(),
            file_name,
            format: String::new(),
            file_size_bytes: 0,
            indexing_status: "pending".to_string(),
            error_reason: None,
            last_modified: 0,
            indexed_at: None,
            created_at: chrono::Utc::now().timestamp_millis(),
            index_tier: "fast".to_string(),
            visibility: "private".to_string(), // placeholder; real row is stamped at insert
            // A brand-new upload is Uncategorised and in service.
            category_id: None,
            disabled_at: None,
        });
    }

    Ok(results)
}

// ── Folders ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct FolderDto {
    pub id: String,
    pub folder_path: String,
    pub indexing_status: String,
    pub file_count: i64,
    pub last_scanned_at: Option<i64>,
    pub created_at: i64,
    // F021 — visibility + owner surfaced to the UI (FR-012).
    #[serde(default = "default_visibility")]
    pub visibility: String,
    #[serde(default)]
    pub owner_username: Option<String>,
    // Migration 026 — the batch running right now. `indexing_total = 0` = idle.
    #[serde(default)]
    pub indexing_total: i64,
    #[serde(default)]
    pub indexing_done: i64,
}

fn default_visibility() -> String {
    "shared".to_string()
}

impl From<folder_repo::IndexedFolder> for FolderDto {
    fn from(f: folder_repo::IndexedFolder) -> Self {
        FolderDto {
            id: f.id,
            folder_path: f.folder_path,
            indexing_status: f.indexing_status,
            file_count: f.file_count,
            last_scanned_at: f.last_scanned_at,
            created_at: f.created_at,
            visibility: f.visibility,
            owner_username: f.owner_username,
            indexing_total: f.indexing_total,
            indexing_done: f.indexing_done,
        }
    }
}

/// Emitted as each file is indexed during a folder scan.
#[derive(Serialize, Clone)]
struct FolderProgress {
    folder_id: String,
    indexed: i64,
    total: i64,
    status: String,
}

/// Returns true for Office temp/lock files and hidden dot-files that should
/// never be indexed: `~$filename.docx`, `.DS_Store`, `.~lock.file.odt#`, etc.
fn is_temp_file(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.starts_with("~$") || name.starts_with('.'))
        .unwrap_or(false)
}

/// Insert a folder and kick off the recursive scan+index (shared orchestration,
/// F014/T024). `app` drives the Tauri `folder:progress` + per-file events;
/// headless passes `None` and the SSE stream carries per-file progress from the
/// indexer broadcast. Concurrency stays bounded by `indexer::indexing_sem()`.
pub async fn index_folder(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    tier: &HardwareTier,
    folder_path: &str,
    owner_id: Option<String>,
    app: Option<crate::compat::AppHandle>,
    category_id: Option<&str>,
) -> Result<FolderDto, String> {
    if !std::path::Path::new(folder_path).is_dir() {
        return Err(format!("Directory not found: {folder_path}"));
    }
    let folder = folder_repo::insert(pool, folder_path, owner_id.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    // Spawn async initial scan — walk the directory and index every supported file.
    let pool2 = pool.clone();
    let embedder2 = Arc::clone(embedder);
    let budget2 = TaskBudget::new();
    let tier2 = tier.clone();
    let folder_id = folder.id.clone();
    let root = folder_path.to_string();
    let app2 = app.clone();
    let cat2 = category_id.map(|s| s.to_string());

    tokio::spawn(async move {
        folder_repo::update_status(&pool2, &folder_id, "processing").await.ok();
        emit_folder_progress(&app2, &folder_id, 0, 0, "processing");

        // Collect all supported files recursively.
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(&root)];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if FileFormat::from_path(&p).is_some() && !is_temp_file(&p) {
                    files.push(p);
                }
            }
        }

        let total = files.len() as i64;
        log::info!("Folder scan {root}: found {total} supported files");
        folder_repo::update_file_count(&pool2, &folder_id, total).await.ok();
        // Migration 026 — publish the denominator before the first file, so the
        // UI shows "0 of N" straight away instead of an indeterminate spinner.
        folder_repo::begin_batch(&pool2, &folder_id, total, cat2.as_deref()).await.ok();
        emit_folder_progress(&app2, &folder_id, 0, total, "processing");

        let notebook_id = format!("__folder__{}", folder_id);
        let mut indexed = 0i64;
        for file in &files {
            // Adaptive back-pressure between files so the CPU scheduler breathes.
            let delay_ms: u64 = match budget2.state() {
                BudgetState::Active => 150,
                BudgetState::Throttled => 50,
                BudgetState::Idle => 0,
            };
            if delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }

            let path_str = file.to_string_lossy().to_string();
            match indexer::index_file(
                &pool2,
                Arc::clone(&embedder2),
                &budget2,
                &tier2,
                &notebook_id,
                &path_str,
                app2.clone(), // per-file progress (Tauri event + SSE broadcast)
            )
            .await
            {
                Ok(sid) => {
                    indexed += 1;
                    after_file_indexed(&pool2, Some(&sid), cat2.as_deref(), Some(&folder_id)).await;
                    emit_folder_progress(&app2, &folder_id, indexed, total, "processing");
                }
                Err(e) => {
                    log::warn!("Folder index failed for {path_str}: {e}");
                    // Still a file dealt with — see after_file_indexed.
                    after_file_indexed(&pool2, None, None, Some(&folder_id)).await;
                }
            }
        }

        folder_repo::update_file_count(&pool2, &folder_id, indexed).await.ok();
        // Clears indexing_total back to 0 (idle) and stamps last_scanned_at,
        // replacing the separate update_status + update_last_scanned pair.
        folder_repo::finish_batch(&pool2, &folder_id).await.ok();
        log::info!("Folder scan complete {root}: {indexed}/{total} files indexed");
        emit_folder_progress(&app2, &folder_id, indexed, total, "indexed");
    });

    Ok(folder.into())
}

/// Re-queue every file that was mid-index when the process last stopped.
///
/// Called once at server startup. Two reasons it has to exist:
///
///  1. **Automatic GPU indexing restarts the server on purpose.** Switching the
///     ONNX runtime between the CPU and CUDA builds is an `ORT_DYLIB_PATH` /
///     `LD_LIBRARY_PATH` change, both read at process start, so the switch costs
///     a restart — which kills whatever was indexing. Without this, entering GPU
///     mode would silently abandon the batch it was entered FOR.
///  2. A crash or a deploy mid-batch left rows stuck in `pending`/`processing`
///     forever, with the folder's progress bar frozen below its total. That was
///     already true before GPU mode; nothing recovered them.
///
/// Progress accounting stays correct across the restart: `bump_indexed` runs
/// only AFTER `index_file` returns, so a file killed mid-index never counted
/// itself, and counting it once now is exactly right.
///
/// Files whose source row survives but whose bytes are gone are marked failed
/// rather than retried forever.
pub async fn resume_interrupted_indexing(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    tier: &HardwareTier,
) -> usize {
    // ── First, close any STRANDED batch ──────────────────────────────────────
    // A file killed between "marked indexed" and `after_file_indexed` never
    // bumped the counter, so its batch can never reach its total: the progress
    // bar sits at 6/7 forever and the UI polls forever. Seen live 2026-08-14 —
    // the same one-file race that cost that file its category.
    //
    // This runs BEFORE the re-queue below, deliberately: at this point the rows
    // still carry their pre-shutdown statuses, so "no pending work" is a true
    // reading. After the re-queue it would race the spawned tasks, which set
    // 'pending' asynchronously, and could close a batch that is about to run.
    let _ = folder_repo::close_stranded_batches(pool).await;

    let rows: Vec<(String, String)> = match sqlx::query_as(
        "SELECT s.id, s.file_path FROM sources s \
         WHERE s.indexing_status IN ('pending','processing')",
    )
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("resume: could not list interrupted sources: {e}");
            return 0;
        }
    };
    if rows.is_empty() {
        return 0;
    }

    let mut queued = 0usize;
    for (source_id, file_path) in rows {
        if !std::path::Path::new(&file_path).exists() {
            log::warn!("resume: {file_path} is gone; marking failed rather than retrying");
            let _ = source_repo::update_error(pool, &source_id, "file missing at resume").await;
            continue;
        }
        // The stub notebook the file already belongs to, so the resumed index
        // re-attaches it where it was rather than orphaning it.
        let notebook: Option<String> = sqlx::query_scalar(
            "SELECT notebook_id FROM notebook_sources WHERE source_id = $1 LIMIT 1",
        )
        .bind(&source_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        let Some(notebook_id) = notebook else {
            log::warn!("resume: {file_path} has no notebook link; skipping");
            continue;
        };

        // Which folder's counter this file reports into — resolved the same way
        // the batch was set up, so the bar continues instead of restarting.
        let folder_id: Option<String> = notebook_id.strip_prefix("__folder__").map(str::to_string);

        // Migration 027 — and the reason it exists. The category is applied per
        // file AS IT FINISHES, so a file killed mid-index never received one.
        // This function used to pass None here, which meant the automatic GPU
        // switch (a restart, early in every batch, by design) filed an entire
        // 157-document upload as Uncategorised. The batch's category is now on
        // the folder row, so the resumed files land where the admin asked.
        let cat: Option<String> = match folder_id.as_deref() {
            Some(fid) => folder_repo::batch_category(pool, fid).await.ok().flatten(),
            None => None,
        };

        let pool_ref = pool.clone();
        let embedder_ref = Arc::clone(embedder);
        let tier_ref = tier.clone();
        let budget_ref = TaskBudget::new();
        tokio::spawn(async move {
            let _ = source_repo::update_status(&pool_ref, &source_id, "pending").await;
            let outcome = indexer::index_file(
                &pool_ref,
                Arc::clone(&embedder_ref),
                &budget_ref,
                &tier_ref,
                &notebook_id,
                &file_path,
                None,
            )
            .await;
            let sid = match &outcome {
                Ok(id) => Some(id.as_str()),
                Err(e) => {
                    log::error!("resume: indexing failed for {file_path}: {e}");
                    None
                }
            };
            after_file_indexed(&pool_ref, sid, cat.as_deref(), folder_id.as_deref()).await;
        });
        queued += 1;
    }
    log::info!("resume: re-queued {queued} interrupted file(s)");
    queued
}

/// Runs once per file the moment it leaves the indexer, whatever the outcome.
///
/// Two jobs, deliberately together so neither can be forgotten at a call site:
///
/// 1. **File it.** The category is applied to the id `index_file` returned, so
///    there is no window in which the row does not exist yet. A batch uploaded
///    into a category re-files a document that was already in the corpus under a
///    different one — the admin picking a category for this batch is an explicit
///    statement about these files, and silently ignoring it would be worse than
///    honouring it.
/// 2. **Tick the counter**, migration 026. This happens even when indexing
///    FAILED: the bar counts files *dealt with*, not files that succeeded. If it
///    only counted successes, one unparseable PDF would strand the bar below its
///    total forever and look like a hang — which is exactly the symptom this
///    whole change exists to remove. Failures are visible per-document in the
///    list, which is the right place for them.
///
/// Both are best-effort: a failure here must not fail the file that just
/// indexed successfully, so each is logged and swallowed.
async fn after_file_indexed(
    pool: &Db,
    source_id: Option<&str>,
    category_id: Option<&str>,
    progress_folder_id: Option<&str>,
) {
    if let (Some(sid), Some(cat)) = (source_id, category_id) {
        if let Err(e) =
            crate::db::repositories::corpus_category::assign(pool, &[sid.to_string()], Some(cat))
                .await
        {
            log::warn!("category assign failed for source {sid}: {e}");
        }
    }
    if let Some(fid) = progress_folder_id {
        if let Err(e) = folder_repo::bump_indexed(pool, fid).await {
            log::warn!("indexing progress bump failed for folder {fid}: {e}");
        }
    }
}

/// Folder-level progress (desktop Tauri event only; web gets per-file progress
/// over the SSE stream). No-op when `app` is `None` (headless).
fn emit_folder_progress(
    app: &Option<crate::compat::AppHandle>,
    folder_id: &str,
    indexed: i64,
    total: i64,
    status: &str,
) {
    if let Some(a) = app {
        let _ = a.emit(
            "folder:progress",
            FolderProgress {
                folder_id: folder_id.to_string(),
                indexed,
                total,
                status: status.to_string(),
            },
        );
    }
}

// ── Entities ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct EntityDto {
    pub id: String,
    pub canonical_value: String,
    pub entity_type: String,
    pub normalized_value: Option<String>,
    pub source_count: i64,
    pub created_at: i64,
}

impl From<entity_repo::Entity> for EntityDto {
    fn from(e: entity_repo::Entity) -> Self {
        EntityDto {
            id: e.id,
            canonical_value: e.canonical_value,
            entity_type: e.entity_type,
            normalized_value: e.normalized_value,
            source_count: e.source_count,
            created_at: e.created_at,
        }
    }
}
