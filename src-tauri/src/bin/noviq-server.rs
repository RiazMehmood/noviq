//! Noviq headless server (F014, ADR-0017) — entrypoint (T008).
//!
//! Runs the Rust backend as a long-lived headless service (no Tauri window,
//! no X) on Linux: boots the bundled PostgreSQL + pgvector, optionally loads
//! the single LLM ModelSlot, and serves the authenticated HTTP API + the
//! static SolidJS web client on one port (FR-002/FR-003/FR-016).
//!
//! Configuration is read from the process environment (systemd `EnvironmentFile`
//! loads `.env`; see `.env.example`):
//!   NOVIQ_DATA_DIR    data dir owning the PG cluster + caches  (default ./.noviq-data)
//!   NOVIQ_PGVECTOR_DIR vendored pgvector resource tree         (default <crate>/resources/pgvector)
//!   NOVIQ_WEB_DIR     built frontend dir served as static      (default dist)
//!   NOVIQ_BIND        listen addr                              (default 127.0.0.1:8080)
//!   NOVIQ_GPU_LAYERS  llama-cpp-2 GPU offload layers           (default 0 = CPU)
//!
//! Build: `cargo build --release --features server` (add `cuda` on the GPU box).

use std::path::PathBuf;

use std::sync::Arc;

use echo_ai_lib::api::{build_router, AppState};
use echo_ai_lib::db;
use echo_ai_lib::services::embedder::EmbedderState;
use echo_ai_lib::services::hardware_detector::HardwareTier;
use echo_ai_lib::services::llm::ModelSlot;
use echo_ai_lib::services::reranker::RerankerSlot;
use echo_ai_lib::services::scheduler::Scheduler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let tier = HardwareTier::detect();
    log::info!(
        "noviq-server starting — hardware tier: max_model={}, embedding={}",
        tier.max_model_tier, tier.embedding_model
    );

    // ── Data dir + bundled PostgreSQL ──────────────────────────────────────────
    let data_dir = PathBuf::from(
        std::env::var("NOVIQ_DATA_DIR").unwrap_or_else(|_| ".noviq-data".to_string()),
    );
    std::fs::create_dir_all(&data_dir)?;
    let pg_root = data_dir.join("pg");

    let pgvector_resources = std::env::var("NOVIQ_PGVECTOR_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("pgvector")
        });

    // Engine handle is kept alive for the process lifetime (dropped on exit).
    let (_engine, pool) = db::schema::init_db(&pg_root, &pgvector_resources, &tier).await?;
    log::info!("database ready (bundled PostgreSQL at {})", pg_root.display());

    // US7 (FR-019): seed a single admin from env on first boot if none exists.
    echo_ai_lib::api::auth::seed_admin_if_configured(&pool).await;

    // Apply a persisted inference-engine override (admin dashboard toggle: local
    // GPU / hosted API + key) from db_config, if one was saved. No-op otherwise —
    // the engine stays as configured by NOVIQ_INFER_ENGINE.
    echo_ai_lib::services::inference::runtime::load_from_db(&pool).await;

    // ── Chat-pipeline singletons (same ones the desktop manages) ───────────────
    // Models load lazily on first use; constructing the slots is cheap. The
    // single ModelSlot (Principle III) serializes generation; the US4 request
    // queue (T023) bounds concurrency on top of it.
    let embedder = Arc::new(EmbedderState::new(&tier));
    let model_slot = Arc::new(ModelSlot::new().map_err(|e| anyhow::anyhow!("ModelSlot init: {e}"))?);
    let reranker_dir =
        std::env::var("ECHO_RERANKER_MODEL").unwrap_or_else(|_| "bge-reranker-base".to_string());
    // Paths relative to the working dir (/opt/noviq/echoAI). Reranker is lazy:
    // missing files degrade gracefully to RRF, so this never blocks startup.
    let reranker_slot = Arc::new(RerankerSlot::new(
        format!("models/{reranker_dir}/model.onnx"),
        format!("models/{reranker_dir}/tokenizer.json"),
    ));
    let scheduler = Scheduler::new(None); // already returns Arc<Scheduler>; headless = no AppHandle
    // Generation gate (T023): 1 permit = one LLM inference at a time.
    let gen_gate = Arc::new(tokio::sync::Semaphore::new(1));

    // Anything left mid-index by the last shutdown gets re-queued now. The
    // automatic GPU-indexing mode restarts this process on purpose (the ONNX
    // runtime switch is read at exec), so without this the very batch that
    // triggered the switch would be abandoned by it.
    {
        let n = echo_ai_lib::services::indexing::resume_interrupted_indexing(
            &pool, &embedder, &tier,
        )
        .await;
        if n > 0 {
            log::info!("resumed {n} interrupted indexing job(s) from the previous run");
        }
    }

    // Nothing may stay stuck. A file killed between being marked indexed and its
    // counter bump leaves a batch that can never reach its total — the progress
    // bar sticks, and the GPU poller reads the open flag as "work in progress"
    // and holds chat down indefinitely. That stranding happens DURING a run, so
    // checking only at startup is too late: it ran when there was nothing to fix
    // and the next restart was hours away. One cheap query a minute closes it.
    {
        let pool2 = pool.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
            tick.tick().await; // the first tick fires immediately; startup already swept
            loop {
                tick.tick().await;
                match echo_ai_lib::db::repositories::folders::close_stranded_batches(&pool2).await {
                    Ok(n) if n > 0 => log::info!("reconcile: closed {n} stranded batch(es)"),
                    Ok(_) => {}
                    Err(e) => log::warn!("reconcile: {e}"),
                }
            }
        });
    }

    // ── HTTP server: API + static web client ───────────────────────────────────
    let state = AppState { pool, tier, embedder, model_slot, reranker_slot, scheduler, gen_gate };
    let app = build_router(state);

    let bind = std::env::var("NOVIQ_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    log::info!("noviq-server listening on http://{bind}  (API at /api, web client at /)");
    axum::serve(listener, app).await?;

    Ok(())
}
