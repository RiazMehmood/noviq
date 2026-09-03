//! EchoAI — Tauri application wiring.
//!
//! Registers all managed state and Tauri command handlers.

#[cfg(feature = "desktop")]
use std::sync::Arc;

// F014: the Tauri command handlers are desktop-only — gated so the headless
// `noviq-server` build links no GUI stack.
#[cfg(feature = "desktop")]
pub mod commands;
pub mod compat;
pub mod db;
pub mod services;

// F014 (ADR-0017): headless HTTP API + static web client. Compiled only with
// `--features server`; the desktop (Tauri) build never includes it.
#[cfg(feature = "server")]
pub mod api;

#[cfg(feature = "desktop")]
use tauri::Manager;
#[cfg(feature = "desktop")]
use commands::*;
#[cfg(feature = "desktop")]
use services::{
    batch_processor::BatchHandleMap,
    embedder::EmbedderState,
    file_watcher::{FileChangeKind, FileWatcherState},
    hardware_detector::HardwareTier,
    llm::ModelSlot,
    reranker::RerankerSlot,
    scheduler::{Scheduler, WorkloadKind},
    task_budget::TaskBudget,
};
#[cfg(feature = "desktop")]
use crate::services::meeting::MeetingService;
#[cfg(feature = "desktop")]
use crate::services::stt_sidecar::SttSidecar;

/// Application entry point called from `main.rs`.
#[cfg(feature = "desktop")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    // Detect hardware synchronously before the async runtime starts.
    let tier = HardwareTier::detect();
    log::info!("Hardware tier: max_model={}, embedding={}", tier.max_model_tier, tier.embedding_model);

    // Build the Tauri application.
    tauri::Builder::default()
        // ── Plugins ──────────────────────────────────────────────────────────
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        // ── Managed state ────────────────────────────────────────────────────
        .manage(tier.clone())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let tier_for_setup = tier.clone();

            // Block only on fast DB init so Db is registered before
            // the window opens (prevents the race where the frontend calls
            // list_notebooks before the pool is managed). The slow parts
            // (embedding model, LLM, file watcher) are spawned in the background
            // so they don't freeze the window while loading.
            let pool = tauri::async_runtime::block_on(init_db_only(
                &app_handle,
                &tier_for_setup,
            ));
            app_handle.manage(pool.clone());

            // Seed the cloud-settings cache so non-Tauri callers (indexer, batch)
            // see the user's persisted Groq config immediately. 2026-05-03.
            let _ = services::cloud_llm::load_settings(&app_handle);

            // Spawn the rest (slow: embedding model ~2 min, LLM, file watcher).
            tauri::async_runtime::spawn(setup_models_and_watchers(
                app_handle,
                tier_for_setup,
                pool,
            ));

            Ok(())
        })
        // ── Command handlers ─────────────────────────────────────────────────
        .invoke_handler(tauri::generate_handler![
            // Notebooks (P1)
            notebooks::create_notebook,
            notebooks::list_notebooks,
            notebooks::get_notebook,
            notebooks::rename_notebook,
            notebooks::delete_notebook,
            // Sources (P1)
            sources::add_sources,
            sources::list_notebook_sources,
            sources::remove_source,
            sources::retry_failed_source,
            sources::get_kg_status,
            sources::clean_orphan_sources,
            sources::resume_pending_ner,
            // Query (P1)
            query::create_session,
            query::chat_query_legal,
            query::cancel_chat,
            query::cancel_turn,
            query::get_session_history,
            // System (P1)
            system::get_hardware_tier,
            system::check_model,
            system::download_model,
            system::restart_app,
            // Folders (P2)
            folders::add_indexed_folder,
            folders::list_indexed_folders,
            folders::revoke_indexed_folder,
            folders::list_folder_sources,
            folders::retry_folder_source,
            folders::deep_index_folder_source,
            folders::remove_folder_source,
            // Search (P2)
            search::global_search,
            // Entities (P4)
            entities::query_entities,
            entities::get_entity_relationships,
            entities::get_entity_sources,
            entities::get_knowledge_graph,
            // Analysis (P3)
            analysis::run_analysis,
            analysis::analyze_selected,
            analysis::export_analysis,
            // Batch (P5)
            batch::create_batch_job,
            batch::get_batch_job,
            batch::get_batch_results,
            batch::export_batch_results,
            batch::pause_batch_job,
            batch::resume_batch_job,
            batch::cancel_batch_job,
            // Archive (Polish)
            archive::export_notebook,
            archive::import_notebook,
            // Audit (Polish)
            audit::get_audit_log,
            // Knowledge-graph preferences (P4)
            kg_preferences::get_kg_preferences,
            kg_preferences::set_kg_preferences,
            // Scheduler + index job management
            scheduler::set_foreground,
            scheduler::cancel_index_job,
            scheduler::discard_index_job,
            scheduler::list_active_index_jobs,
            // GPU
            system::get_gpu_settings,
            system::set_gpu_enabled,
            // Chat mode (extractive vs LLM)
            system::get_chat_extractive,
            system::set_chat_extractive,
            // Query trace logs
            system::open_query_logs_dir,
            system::read_latest_query_trace,
            system::download_query_trace_to,
            system::download_all_query_traces_to,
            system::download_recent_query_traces_to,
            sources::list_sources_in_folder,
            system::get_cloud_settings,
            system::set_cloud_settings,
            // Meeting Agent (Phase 2)
            meetings::start_meeting,
            meetings::stop_meeting,
            meetings::list_meeting_segments,
            meetings::get_meeting,
            // LLM IR importer (Feature 006) removed 2026-06 (doc_ir/Mistral retired).
            // F015 T030 — the slot/template/docset/gap/finalize drafting path
            // (Features 009/010) is RETIRED from the user-facing surface. Drafting
            // is now one capability of the unified assistant (`send_message` →
            // `draft` capability + `export_draft`). The command handlers, tables,
            // and services are left dormant per the additive policy; the citation
            // gate + `export.rs` were extracted and are reused by F015.
            // Unified bureaucrat chat assistant (Feature 015)
            assistant::send_message,
            assistant::export_draft,
            assistant::model_status,
            assistant::get_amendment_chain,
            assistant::confirm_documents,
            assistant::new_chat_from_notice,
        ])
        .build(tauri::generate_context!())
        .expect("error while building EchoAI")
        .run(|app_handle, event| {
            // F011 FR-005: clean engine shutdown on app exit.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app_handle.try_state::<PgEngineState>() {
                    if let Some(engine) = state.0.lock().ok().and_then(|mut g| g.take()) {
                        let _ = tauri::async_runtime::block_on(db::pg::engine::shutdown(engine));
                        log::info!("bundled PostgreSQL engine stopped");
                    }
                }
            }
        });
}

/// Bundled-PG engine handle kept alive for the app lifetime; taken at exit
/// for clean shutdown (F011 FR-005). Desktop-only managed state; the headless
/// server keeps its own engine handle in `noviq-server.rs`.
#[cfg(feature = "desktop")]
pub struct PgEngineState(pub std::sync::Mutex<Option<db::pg::engine::EngineHandle>>);

/// Fast DB-only init: boots the bundled PostgreSQL engine, runs the PG
/// baseline, returns the pool. Called with block_on in .setup() so the pool
/// is managed before the window opens.
#[cfg(feature = "desktop")]
async fn init_db_only(app: &tauri::AppHandle, tier: &HardwareTier) -> db::Db {
    let db_dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    std::fs::create_dir_all(&db_dir).expect("failed to create data dir");
    let pg_root = db_dir.join("pg");

    // Vendored pgvector tree: bundled resource dir in production, the repo
    // tree in dev (T022 wires the tauri.conf.json resource bundling).
    let pgvector_resources = app
        .path()
        .resource_dir()
        .ok()
        .map(|d| d.join("resources").join("pgvector"))
        .filter(|d| d.exists())
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("pgvector")
        });

    let (engine, pool) = db::init_db(&pg_root, &pgvector_resources, tier)
        .await
        .expect("failed to initialise database");
    app.manage(PgEngineState(std::sync::Mutex::new(Some(engine))));
    log::info!("Database ready (bundled PostgreSQL at {})", pg_root.display());

    // Reconcile any index jobs that were in-flight when the process last crashed.
    // SQLite WAL recovery rolls back their in-flight file transactions automatically;
    // we flip the job status from 'running' → 'paused' so the UI can offer resume.
    let reconciled = crate::db::repositories::index_jobs::reconcile_orphaned_running_jobs(&pool)
        .await
        .unwrap_or(0);
    if reconciled > 0 {
        log::warn!("reconciled {} orphaned index jobs from running → paused", reconciled);
    }

    pool
}

/// Slow background init: embedding model, LLM slot, task budget, file watcher.
/// Spawned after the window opens so the UI isn't blocked.
#[cfg(feature = "desktop")]
async fn setup_models_and_watchers(app: tauri::AppHandle, tier: HardwareTier, pool: crate::db::Db) {
    // ── Embedding model ───────────────────────────────────────────────────────
    let embedder = Arc::new(EmbedderState::new(&tier));

    // ── LLM model slot ────────────────────────────────────────────────────────
    let model_slot = ModelSlot::new().expect("failed to init ModelSlot");
    let model_slot = Arc::new(model_slot);

    // Apply persisted GPU preference before warm-up so first load uses correct setting.
    {
        let gpu_val: Option<String> = sqlx::query_scalar(
            "SELECT value FROM db_config WHERE key = 'gpu_enabled'"
        )
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);
        let gpu_on = gpu_val.map(|v| v != "0").unwrap_or(true);
        model_slot.set_gpu_enabled(gpu_on);
        log::info!("GPU acceleration at startup: {gpu_on}");
    }

    // Feature 004 — re-extract any PDF whose stored text is suspect (legacy
    // rows with NULL quality, or sub-threshold non-OCR rows). The file watcher
    // and resume flow pick them up from 'pending' on their next pass.
    match services::indexer::enqueue_quality_reextract(&pool).await {
        Ok(n) if n > 0 => log::info!("startup re-extract: {n} sources enqueued"),
        Ok(_) => log::debug!("startup re-extract: no sources needed re-extraction"),
        Err(e) => log::warn!("startup re-extract failed: {e}"),
    }

    // Feature 005 — re-process anything still at an older structured-importer
    // version (e.g. pre-Feature-005 rows with structure_importer_version=0).
    match services::indexer::enqueue_structure_reextract(&pool).await {
        Ok(n) if n > 0 => log::info!("startup structure re-index: {n} sources enqueued"),
        Ok(_) => log::debug!("startup structure re-index: no sources needed re-indexing"),
        Err(e) => log::warn!("startup structure re-index failed: {e}"),
    }

    // ── Warm up embedder + LLM in background so first query is fast ──────────
    {
        use crate::services::query_router::resolve_model_path;
        let embedder_w = Arc::clone(&embedder);
        let model_slot_w = Arc::clone(&model_slot);
        let tier_w = tier.clone();
        let pool_w = pool.clone();
        tokio::task::spawn_blocking(move || {
            log::info!("Startup warm-up: loading embedding model…");
            if let Err(e) = embedder_w.embed_one("warmup") {
                log::warn!("Embedding warm-up failed: {e}");
            } else {
                log::info!("Startup warm-up: embedding model ready");
            }
            // sqlite-vec virtual-table cold-init costs ~40 s on the first
            // vec_distance_cosine call (P1 observed 39.5 s). Issue a dummy
            // query against chunk_embeddings to pay that cost up-front so the
            // user's first real query doesn't eat it.
            // Use a real embedding (not zero vector) so the ANN lookup
            // touches the same shard/page topology a real user query will hit.
            // Zero vector goes through the index but doesn't pre-warm the page
            // cache for relevant clusters (P3 still observed 56 s cold).
            let warmup_vec: Vec<f32> = embedder_w
                .embed_one("civil servant rules procedure")
                .unwrap_or_else(|_| vec![0.0_f32; 768]);
            let pool_w_inner = pool_w.clone();
            let rt = tokio::runtime::Handle::try_current();
            if let Ok(handle) = rt {
                handle.spawn(async move {
                    let emb_json = match serde_json::to_string(&warmup_vec) {
                        Ok(b) => b,
                        Err(_) => return,
                    };
                    let t = std::time::Instant::now();
                    let d = crate::db::schema::embedding_dim();
                    let _ = sqlx::query(&format!(
                        "SELECT chunk_id FROM chunk_embeddings \
                         ORDER BY embedding::halfvec({d}) <=> $1::vector::halfvec({d}) ASC LIMIT 1"
                    ))
                    .bind(&emb_json)
                    .fetch_optional(&pool_w_inner)
                    .await;
                    log::info!("Startup warm-up: vector index ready ({} ms)", t.elapsed().as_millis());
                });
            }
            // Warm up LLM only if the model file exists.
            let (rel_path, _, _) = crate::services::query_router::select_model_for_tier(&tier_w);
            let model_path = resolve_model_path(&rel_path);
            if std::path::Path::new(&model_path).exists() {
                log::info!("Startup warm-up: loading LLM {model_path}…");
                if let Err(e) = model_slot_w.load(&model_path, &tier_w) {
                    log::warn!("LLM warm-up failed: {e}");
                } else {
                    log::info!("Startup warm-up: LLM ready");
                }
            } else {
                log::info!("Startup warm-up: no LLM model file found, skipping");
            }
        });
    }

    // ── Task budget scheduler ────────────────────────────────────────────────
    let task_budget = TaskBudget::new();

    // ── Foreground priority scheduler ─────────────────────────────────────────
    let scheduler = Scheduler::new(Some(app.clone()));

    // ── File watcher ──────────────────────────────────────────────────────────
    let file_watcher = FileWatcherState::new().expect("failed to init FileWatcher");

    // On startup: reset any 'pending' sources to 'failed' so they can be retried.
    // 'pending' at startup means the app was killed mid-indexing — those sources
    // will never self-recover without a retry.
    {
        use crate::db::repositories::sources as source_repo;
        if let Err(e) = source_repo::reset_stuck_pending(&pool).await {
            log::warn!("Failed to reset stuck pending sources: {e}");
        }
    }

    // On startup: reset sources stuck in 'processing' to 'failed' so the user
    // can retry. Auto-re-trigger is unsafe — notebook links may be orphaned
    // after a crash-interrupted delete-and-re-add sequence.
    {
        use crate::db::repositories::sources as source_repo;
        if let Err(e) = source_repo::reset_stuck_processing(&pool).await {
            log::warn!("Failed to reset stuck-processing sources: {e}");
        }
    }

    // On startup: resume NER for any sources that were indexed but have
    // ner_status='pending' (app restarted mid-NER).
    {
        let pending: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
            "SELECT id, file_name FROM sources WHERE indexing_status = 'indexed' AND ner_status = 'pending'"
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        if !pending.is_empty() {
            log::info!("Startup NER recovery: resuming {} pending source(s)", pending.len());
            let pool2 = pool.clone();
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                for (source_id, file_name) in pending {
                    crate::services::indexer::run_ner_for_source(
                        &pool2, &source_id, &file_name, app2.clone(),
                    ).await;
                }
            });
        }
    }

    // Idea 2 (2026-05-23) — Incremental Pass-2 NER resume.
    // Scans for sources with ANY chunks where `ner_pass2_done = 0` and
    // re-enters the iterative loop for each. Covers both: sources that
    // never finished (pending) and sources marked 'done' by the legacy
    // single-shot path but missing per-chunk coverage. Idempotent —
    // spawns at most one loop per source.
    crate::services::indexer::resume_pending_pass2_ner(pool.clone(), app.clone());

    // One-time backfill: ensure every indexed_folder has its matching
    // `__folder__<id>` stub notebook. Prior versions created the folder row
    // without the stub, which broke notebook_sources FK on every index.
    {
        use crate::db::repositories::folders as folder_repo;
        match folder_repo::backfill_stub_notebooks(&pool).await {
            Ok(n) if n > 0 => log::info!("Backfilled {n} folder stub notebook(s)"),
            Ok(_) => {}
            Err(e) => log::warn!("Folder stub backfill failed: {e}"),
        }
    }

    // T052: Restore watches for all previously registered folders.
    {
        use crate::db::repositories::folders as folder_repo;
        let fw = file_watcher;
        if let Ok(folders) = folder_repo::list(&pool).await {
            for f in &folders {
                if let Err(e) = fw.watch(&f.folder_path) {
                    log::warn!("Could not watch {}: {e}", f.folder_path);
                }
            }
        }

        // Register the watcher in managed state.
        app.manage(fw);
    }

    // Startup folder reconciliation — diff on-disk files against the `sources`
    // table for every registered folder. The live PollWatcher only fires events
    // for changes that happen *while the app is running*; files added or removed
    // while the app was closed become part of the watcher's baseline snapshot and
    // produce no event. Without this scan, "drop a PDF into the folder while the
    // app is off → relaunch" never indexes the file (and a file deleted while off
    // never leaves the DB). We reconcile add+remove only (no content-change
    // re-index): files on disk but missing from `sources` get indexed; `sources`
    // rows whose file no longer exists on disk are cleaned (chunks + embeddings +
    // row), mirroring the watcher's Delete path so search/graph stay consistent.
    {
        let pool2 = pool.clone();
        let embedder2 = Arc::clone(&embedder);
        let task_budget2 = task_budget.clone();
        let tier2 = tier.clone();
        let app2 = app.clone();

        tauri::async_runtime::spawn(async move {
            use crate::db::repositories::folders as folder_repo;
            use crate::services::indexer;
            use crate::services::parsers::FileFormat;
            use crate::services::task_budget::BudgetState;
            use std::collections::HashSet;
            use std::path::PathBuf;
            use tauri::Emitter;

            let folders = match folder_repo::list(&pool2).await {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("Startup reconcile: folder list failed: {e}");
                    return;
                }
            };

            for f in &folders {
                // 1. Walk the folder for currently-supported files on disk.
                let mut on_disk: HashSet<String> = HashSet::new();
                let mut stack = vec![PathBuf::from(&f.folder_path)];
                while let Some(dir) = stack.pop() {
                    let Ok(rd) = std::fs::read_dir(&dir) else { continue };
                    for entry in rd.flatten() {
                        let p = entry.path();
                        if p.is_dir() {
                            stack.push(p);
                            continue;
                        }
                        // Skip Office temp files (~$...) and hidden dot-files,
                        // matching the live watcher's filter.
                        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                            if name.starts_with("~$") || name.starts_with('.') {
                                continue;
                            }
                        }
                        if FileFormat::from_path(&p).is_some() {
                            on_disk.insert(p.to_string_lossy().to_string());
                        }
                    }
                }

                // 2. What the DB currently thinks lives under this folder.
                let prefix = if f.folder_path.ends_with('/') {
                    f.folder_path.clone()
                } else {
                    format!("{}/", f.folder_path)
                };
                let in_db: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
                    "SELECT id, file_path FROM sources WHERE file_path LIKE $1",
                )
                .bind(format!("{prefix}%"))
                .fetch_all(&pool2)
                .await
                .unwrap_or_default();

                // 3. Removals: DB rows whose file no longer exists on disk.
                for (source_id, file_path) in &in_db {
                    if on_disk.contains(file_path) {
                        continue;
                    }
                    // Double-check the file is genuinely gone before deleting.
                    if std::path::Path::new(file_path).exists() {
                        continue;
                    }
                    log::info!("Startup reconcile: cleaning removed file {file_path}");
                    let mut tx = match pool2.begin().await {
                        Ok(t) => t,
                        Err(e) => {
                            log::warn!("Startup reconcile: begin tx failed: {e}");
                            continue;
                        }
                    };
                    let _ = sqlx::query(
                        "DELETE FROM chunk_embeddings WHERE chunk_id IN \
                         (SELECT id FROM chunks WHERE source_id = $1)",
                    )
                    .bind(source_id)
                    .execute(&mut *tx)
                    .await;
                    let _ = sqlx::query("DELETE FROM chunks WHERE source_id = $1")
                        .bind(source_id)
                        .execute(&mut *tx)
                        .await;
                    let _ = sqlx::query("DELETE FROM sources WHERE id = $1")
                        .bind(source_id)
                        .execute(&mut *tx)
                        .await;
                    if let Err(e) = tx.commit().await {
                        log::warn!("Startup reconcile: delete commit failed: {e}");
                    }
                }

                // 4. Additions: on-disk files the DB has never indexed.
                let known: HashSet<&str> = in_db.iter().map(|(_, p)| p.as_str()).collect();
                let to_index: Vec<&String> =
                    on_disk.iter().filter(|p| !known.contains(p.as_str())).collect();
                if to_index.is_empty() {
                    // Still refresh count in case removals changed it.
                    if !in_db.is_empty() {
                        let count: i64 = sqlx::query_scalar(
                            "SELECT COUNT(*) FROM sources WHERE file_path LIKE $1 \
                             AND indexing_status = 'indexed'",
                        )
                        .bind(format!("{prefix}%"))
                        .fetch_one(&pool2)
                        .await
                        .unwrap_or(0);
                        folder_repo::update_file_count(&pool2, &f.id, count).await.ok();
                        let _ = app2.emit(
                            "folder:progress",
                            serde_json::json!({
                                "folder_id": f.id, "indexed": count,
                                "total": count, "status": "indexed",
                            }),
                        );
                    }
                    continue;
                }

                log::info!(
                    "Startup reconcile: {} new file(s) under {}",
                    to_index.len(),
                    f.folder_path
                );
                let notebook_id = format!("__folder__{}", f.id);
                for path_str in to_index {
                    // Light back-pressure between files (matches add-folder path).
                    let delay_ms: u64 = match task_budget2.state() {
                        BudgetState::Active => 150,
                        BudgetState::Throttled => 50,
                        BudgetState::Idle => 0,
                    };
                    if delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    match indexer::index_file(
                        &pool2,
                        Arc::clone(&embedder2),
                        &task_budget2,
                        &tier2,
                        &notebook_id,
                        path_str,
                        Some(app2.clone()),
                    )
                    .await
                    {
                        Ok(_) => {
                            let count: i64 = sqlx::query_scalar(
                                "SELECT COUNT(*) FROM sources WHERE file_path LIKE $1 \
                                 AND indexing_status = 'indexed'",
                            )
                            .bind(format!("{prefix}%"))
                            .fetch_one(&pool2)
                            .await
                            .unwrap_or(0);
                            folder_repo::update_file_count(&pool2, &f.id, count).await.ok();
                            let _ = app2.emit(
                                "folder:progress",
                                serde_json::json!({
                                    "folder_id": f.id, "indexed": count,
                                    "total": count, "status": "indexed",
                                }),
                            );
                        }
                        Err(e) => log::warn!("Startup reconcile: index failed for {path_str}: {e}"),
                    }
                }
                folder_repo::update_last_scanned(&pool2, &f.id).await.ok();
            }
        });
    }

    // T052: Spawn file-watcher event loop — auto-index on Create/Modify,
    // auto-clean on Delete. The previous implementation poll-locked a shared
    // receiver and used non-blocking `try_recv`, which made it miss rapid
    // inotify bursts (Create → Modify → Close-Write for a single new file) and
    // also tried to index 0-byte files during in-progress copies. This version:
    //   * owns the receiver in a dedicated task (no Mutex on the hot path),
    //   * debounces per path for 800 ms so we only index the final state of a
    //     write, not intermediate zero-byte snapshots,
    //   * skips files whose current size is 0, and
    //   * cleans chunks + source rows on Delete so search stays consistent.
    {
        let pool2 = pool.clone();
        let embedder2 = Arc::clone(&embedder);
        let task_budget2 = task_budget.clone();
        let tier2 = tier.clone();
        let app2 = app.clone();
        let fw_handle = scheduler.register(WorkloadKind::FileWatcher);

        // Pull the receiver out of managed state once; keep it in this task.
        let mut rx = app
            .state::<FileWatcherState>()
            .take_receiver()
            .expect("file watcher receiver already taken");

        tauri::async_runtime::spawn(async move {
            use crate::services::indexer;
            use crate::services::parsers::FileFormat;
            use crate::db::repositories::folders as folder_repo;
            use std::collections::HashMap;
            use std::path::PathBuf;
            use std::time::{Duration, Instant};
            use tauri::Emitter;

            // Emit the same `folder:progress` shape the add-folder flow uses so
            // the Settings page refreshes file_count + source list live.
            async fn emit_folder_refresh(
                app: &tauri::AppHandle,
                pool: &crate::db::Db,
                folder_id: &str,
            ) {
                let count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM sources s \
                     JOIN indexed_folders f ON s.file_path LIKE (f.folder_path || '%') \
                     WHERE f.id = $1 AND s.indexing_status = 'indexed'",
                )
                .bind(folder_id)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
                let _ = app.emit(
                    "folder:progress",
                    serde_json::json!({
                        "folder_id": folder_id,
                        "indexed": count,
                        "total": count,
                        "status": "indexed",
                    }),
                );
            }

            const DEBOUNCE: Duration = Duration::from_millis(800);

            // path → (last event instant, was_deleted?)
            let mut pending: HashMap<PathBuf, (Instant, bool)> = HashMap::new();
            let mut ticker = tokio::time::interval(Duration::from_millis(300));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                // Cooperative pause: parks until foreground flips back to non-Chat.
                fw_handle.should_pause().await;
                if fw_handle.is_cancelled() {
                    break;
                }
                tokio::select! {
                    maybe_ev = rx.recv() => {
                        let Some(ev) = maybe_ev else {
                            log::warn!("File watcher channel closed — stopping event loop");
                            return;
                        };
                        // Skip Office temp files (~$...) and hidden dot-files early.
                        if let Some(name) = ev.path.file_name().and_then(|n| n.to_str()) {
                            if name.starts_with("~$") || name.starts_with('.') {
                                continue;
                            }
                        }
                        // Only care about supported formats for create/modify.
                        let is_delete = matches!(ev.kind, FileChangeKind::Deleted);
                        if !is_delete && FileFormat::from_path(&ev.path).is_none() {
                            continue;
                        }
                        // Record/refresh the debounce entry. A later Delete
                        // supersedes any prior Create/Modify for the same path.
                        log::info!("File watcher debounce: {:?} on {}", ev.kind, ev.path.display());
                        let entry = pending
                            .entry(ev.path.clone())
                            .or_insert((Instant::now(), is_delete));
                        entry.0 = Instant::now();
                        if is_delete {
                            entry.1 = true;
                        }
                    }
                    _ = ticker.tick() => {
                        // Flush any paths whose last event is older than DEBOUNCE.
                        let now = Instant::now();
                        let ready: Vec<(PathBuf, bool)> = pending
                            .iter()
                            .filter(|(_, (t, _))| now.duration_since(*t) >= DEBOUNCE)
                            .map(|(p, (_, d))| (p.clone(), *d))
                            .collect();
                        for (path, was_deleted) in ready {
                            pending.remove(&path);
                            let path_str = path.to_string_lossy().to_string();

                            // Find the indexed folder that owns this path — if
                            // none, the event is in a path we don't manage.
                            let folders = match folder_repo::list(&pool2).await {
                                Ok(v) => v,
                                Err(e) => {
                                    log::warn!("folder_repo::list failed in watcher: {e}");
                                    continue;
                                }
                            };
                            log::info!("File watcher flush: path={path_str}, {} indexed folders", folders.len());
                            // Use `<folder>/` prefix so `/foo/bar` doesn't match `/foo/barbaz/f`.
                            let Some(owner) = folders.iter().find(|f| {
                                let prefix = if f.folder_path.ends_with('/') {
                                    f.folder_path.clone()
                                } else {
                                    format!("{}/", f.folder_path)
                                };
                                path_str.starts_with(&prefix)
                            }) else {
                                log::info!("File watcher: no indexed folder owns {path_str}");
                                continue;
                            };

                            if was_deleted {
                                // Auto-clean: drop the source and its chunks so
                                // search + graph no longer reference the file.
                                match sqlx::query_scalar::<_, String>(
                                    "SELECT id FROM sources WHERE file_path = $1",
                                )
                                .bind(&path_str)
                                .fetch_optional(&pool2)
                                .await
                                {
                                    Ok(Some(source_id)) => {
                                        log::info!("Auto-cleaning deleted file: {path_str}");
                                        let mut tx = match pool2.begin().await {
                                            Ok(t) => t,
                                            Err(e) => {
                                                log::warn!("begin tx failed: {e}");
                                                continue;
                                            }
                                        };
                                        let _ = sqlx::query(
                                            "DELETE FROM chunk_embeddings WHERE chunk_id IN \
                                             (SELECT id FROM chunks WHERE source_id = $1)",
                                        )
                                        .bind(&source_id)
                                        .execute(&mut *tx)
                                        .await;
                                        let _ = sqlx::query("DELETE FROM chunks WHERE source_id = $1")
                                            .bind(&source_id)
                                            .execute(&mut *tx)
                                            .await;
                                        let _ = sqlx::query("DELETE FROM sources WHERE id = $1")
                                            .bind(&source_id)
                                            .execute(&mut *tx)
                                            .await;
                                        if let Err(e) = tx.commit().await {
                                            log::warn!("tx commit failed on delete cleanup: {e}");
                                        }
                                        emit_folder_refresh(&app2, &pool2, &owner.id).await;
                                    }
                                    Ok(None) => { /* unknown path — ignore */ }
                                    Err(e) => log::warn!("source lookup on delete failed: {e}"),
                                }
                                continue;
                            }

                            // Create/Modify: only index if the file is present
                            // and non-empty. Editors often fire Create with a
                            // zero-byte file before the first write completes.
                            match tokio::fs::metadata(&path).await {
                                Ok(md) if md.is_file() && md.len() > 0 => {}
                                _ => continue,
                            }

                            let folder_id = owner.id.clone();
                            let notebook_id = format!("__folder__{}", folder_id);
                            log::info!("Auto-indexing changed file: {path_str}");
                            match indexer::index_file(
                                &pool2,
                                Arc::clone(&embedder2),
                                &task_budget2,
                                &tier2,
                                &notebook_id,
                                &path_str,
                                Some(app2.clone()), // needed for Pass 2 NER + ner_status='done'
                            )
                            .await
                            {
                                Ok(_) => emit_folder_refresh(&app2, &pool2, &owner.id).await,
                                Err(e) => log::warn!("Auto-index failed for {path_str}: {e}"),
                            }
                        }
                    }
                }
            }
        });
    }

    // Start the scheduler progress pump (emits workload://state at ≤4 Hz).
    scheduler.start_progress_pump(app.clone());

    // Tier 3 Task 11: lazy cross-encoder reranker.
    // Default: bge-reranker-base (280M params). Override with
    // `ECHO_RERANKER_MODEL=bge-reranker-large` to use the 560M variant for
    // better domain accuracy (≈2× slower). Files must live at
    // `models/<name>/model.onnx` and `models/<name>/tokenizer.json`.
    // Missing files → score_pairs errors and query_router falls back to RRF.
    let reranker_dir = std::env::var("ECHO_RERANKER_MODEL")
        .unwrap_or_else(|_| "bge-reranker-base".to_string());
    log::info!("reranker model: {reranker_dir}");
    let reranker_slot = Arc::new(RerankerSlot::new(
        crate::services::query_router::resolve_model_path(
            &format!("models/{reranker_dir}/model.onnx")),
        crate::services::query_router::resolve_model_path(
            &format!("models/{reranker_dir}/tokenizer.json")),
    ));

    // Register all remaining state on the app handle.
    app.manage(pool);
    app.manage(embedder);
    app.manage(model_slot);
    app.manage(reranker_slot);
    app.manage(task_budget);
    app.manage(scheduler);
    app.manage(Arc::new(BatchHandleMap::new()));

    // ── Meeting Agent (Phase 2) ──────────────────────────────────────────────
    // Spawn the Whisper sidecar in the background; on success, register the
    // MeetingService so Tauri commands can drive Start/Stop. If the sidecar
    // binary isn't bundled (Phase 1 build step skipped) we just log a warning
    // — the rest of the app continues to work.
    {
        let app_for_meeting = app.clone();
        let pool_for_meeting = app.state::<crate::db::Db>().inner().clone();
        tauri::async_runtime::spawn(async move {
            match SttSidecar::spawn(&app_for_meeting).await {
                Ok(sidecar) => {
                    let svc = Arc::new(MeetingService::new(pool_for_meeting, Arc::new(sidecar)));
                    app_for_meeting.manage(svc);
                    log::info!("Meeting Agent: sidecar spawned, MeetingService registered");
                }
                Err(e) => {
                    log::warn!("Meeting Agent disabled — sidecar spawn failed: {e}");
                }
            }
        });
    }

    log::info!("EchoAI async setup complete");
}
