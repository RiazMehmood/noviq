//! Ingestion pipeline: detect format → parse → chunk → embed → store.
//!
//! T028: Full indexer implementation.
//!
//! Performance design:
//!   - All sync/CPU-heavy work (file parsing, ONNX inference) runs on the
//!     blocking thread pool via `tokio::task::spawn_blocking` so that tokio
//!     worker threads (and the Tauri IPC/event loop that shares them) are
//!     never stalled.
//!   - Embedding upserts are batched into a single transaction.
//!   - Entity extraction is deferred to a background `tokio::spawn` so the
//!     "indexed" status is set as soon as chunking + embedding are done.

use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};
// F014: Emitter via the compat shim (no-op in the headless server build);
// Manager (managed-state access) is desktop-only.
use crate::compat::Emitter;
#[cfg(feature = "desktop")]
use tauri::Manager;
use crate::db::Db;
use std::io::{BufReader, Read as _};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

/// Limits concurrent file indexing (parse + embed).
///
/// Historically hard-coded to 1: adding several files at once spawned parallel
/// tokio tasks that all competed for the embedder and CPU, making every
/// individual file slower. That is still the right default for a desktop or a
/// handful of uploads, and it stays the default here.
///
/// It is wrong for a bulk corpus load. Measured on the 61-file Agriculture batch
/// (2026-08-14): 39.5 s per file, one at a time, on a 12-core VM sitting at load
/// ~0 — eleven cores idle for 40 minutes. Per-file time correlates more strongly
/// with file SIZE (r=0.86) than with chunk count (r=0.71), and the same chunk
/// count cost anywhere from 30 s to 250 s, so the dominant cost is PARSING
/// (pdftotext/OCR), not embedding. Parsing is CPU-bound and parallelises; the
/// embedder is shared and will still serialise behind its own lock.
///
/// `ECHO_INDEX_CONCURRENCY` raises it for such a load. Clamped to 1..=8: beyond
/// that the parses contend for the same cores and memory with nothing to gain,
/// and each in-flight file holds a parsed document in RAM.
static INDEXING_SEM: OnceLock<Semaphore> = OnceLock::new();

pub fn indexing_sem() -> &'static Semaphore {
    INDEXING_SEM.get_or_init(|| {
        let n = std::env::var("ECHO_INDEX_CONCURRENCY")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(1)
            .clamp(1, 8);
        if n > 1 {
            log::info!("Indexing concurrency set to {n} (ECHO_INDEX_CONCURRENCY)");
        }
        Semaphore::new(n)
    })
}

/// Limits concurrent LLM-based entity extraction (Pass 2) to 1 job at a time.
/// Without this, every indexed file spawns a background task that calls the LLM,
/// and a folder with many files piles up many simultaneous LLM inference calls,
/// exhausting CPU and RAM.
static ENTITY_EXTRACTION_SEM: OnceLock<Semaphore> = OnceLock::new();

fn entity_sem() -> &'static Semaphore {
    ENTITY_EXTRACTION_SEM.get_or_init(|| Semaphore::new(1))
}

use crate::db::repositories::{chunks as chunk_repo, sources as source_repo};
use crate::services::embedder::EmbedderState;
use crate::services::entity_extractor;
use crate::services::hardware_detector::HardwareTier;
use crate::services::parsers::{self, FileFormat, ParseError};
use crate::services::structured_importer;
use crate::services::task_budget::TaskBudget;

// Default cap for Pass-2 LLM entity extraction per source. Override via the
// `ECHO_PASS2_CHUNK_LIMIT` env var. See do_index() for the rationale (OOM on
// 222-page PDFs from running ~500 LLM inferences sequentially with the model
// resident in RAM).
const DEFAULT_PASS2_CHUNK_LIMIT: usize = 20;

// Idea 2 (2026-05-23) — incremental full-coverage Pass-2 NER.
//
// Each Pass-2 invocation processes ONE batch (ECHO_PASS2_BATCH_PCT × total,
// floored at 1) of chunks whose `ner_pass2_done = 0`, then yields. A
// per-source loop (`run_ner_loop_for_source`) repeats: process batch →
// sleep ECHO_PASS2_SLEEP_SECS → process next batch → … until every chunk
// in the source has `ner_pass2_done = 1`. The existing
// `WorkloadKind::Pass2Ner` scheduler pauses the loop when Chat takes
// foreground, so the loop only runs when the user is not chatting.
const DEFAULT_PASS2_BATCH_PCT: f32 = 10.0;
const DEFAULT_PASS2_SLEEP_SECS: u64 = 30;
const MIN_PASS2_BATCH_SIZE: usize = 1;

/// How many chunks Pass-2 covers per batch. Legacy
/// `ECHO_PASS2_CHUNK_LIMIT` (absolute count) wins if set, with a
/// deprecation warning; otherwise `ECHO_PASS2_BATCH_PCT × pending`
/// (default 10 %). Always returns at least 1.
fn pass2_batch_size(pending_total: usize) -> usize {
    if pending_total == 0 {
        return 0;
    }
    if let Ok(v) = std::env::var("ECHO_PASS2_CHUNK_LIMIT") {
        if let Ok(n) = v.parse::<usize>() {
            if n > 0 {
                log::warn!(
                    "ECHO_PASS2_CHUNK_LIMIT={n} is deprecated; use ECHO_PASS2_BATCH_PCT instead"
                );
                return n.min(pending_total).max(MIN_PASS2_BATCH_SIZE);
            }
        }
    }
    let pct = std::env::var("ECHO_PASS2_BATCH_PCT")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|&p| p > 0.0 && p <= 100.0)
        .unwrap_or(DEFAULT_PASS2_BATCH_PCT);
    let count = ((pending_total as f32 * pct / 100.0).ceil() as usize).max(MIN_PASS2_BATCH_SIZE);
    count.min(pending_total)
}

/// Seconds the iterative loop sleeps between batches. Default 30 s —
/// enough to yield clearly between LLM bursts, short enough that a
/// 10-batch run finishes in ~5 minutes of wall-clock idleness on top
/// of the per-chunk inference time. Range [1, 3600].
fn pass2_sleep_secs() -> u64 {
    std::env::var("ECHO_PASS2_SLEEP_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&s| (1..=3600).contains(&s))
        .unwrap_or(DEFAULT_PASS2_SLEEP_SECS)
}

/// Select the next batch of Pass-2 candidates from a source's pending
/// chunks. Two rules:
///  1. **Even-sampling** across pending chunks (`chunk_index`-spaced) —
///     so each pass covers a representative slice of the document.
///  2. **Salient-priority** — any pending chunk whose `unit_label` is in
///     a registered `DocProfile.salient_units` (Preamble · Abstract ·
///     Foreword · Statement of Objects · Executive Summary) is added on
///     top of the even-sample. Guarantees canonical chunks get Pass-2
///     entity extraction in the very first batch, not deferred to the
///     final pass.
///
/// Returns `(chunk_id, content)` pairs in `chunk_index` order. Empty
/// vec when no chunks remain — the iterative loop uses this as its
/// terminator.
async fn select_next_pass2_batch(
    pool: &Db,
    source_id: &str,
) -> Result<Vec<(String, String)>> {
    let pending: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, content, unit_label FROM chunks \
         WHERE source_id = $1 AND ner_pass2_done = 0 \
         ORDER BY chunk_index",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?;
    if pending.is_empty() {
        return Ok(Vec::new());
    }

    let n = pending.len();
    let batch_size = pass2_batch_size(n);

    let mut selected: std::collections::HashSet<usize> = if n <= batch_size {
        (0..n).collect()
    } else {
        (0..batch_size).map(|i| (i * n) / batch_size).collect()
    };

    // Always include pending salient chunks regardless of even-sample pattern.
    for (i, (_, _, label)) in pending.iter().enumerate() {
        if let Some(l) = label {
            if crate::services::doc_profiles::is_salient_unit(l) {
                selected.insert(i);
            }
        }
    }

    let mut indices: Vec<usize> = selected.into_iter().collect();
    indices.sort_unstable();
    Ok(indices
        .into_iter()
        .filter_map(|i| {
            pending
                .get(i)
                .map(|(id, content, _)| (id.clone(), content.clone()))
        })
        .collect())
}

/// Hierarchical chunking pipeline (Tier 3): regex Pass A -> optional LLM
/// Pass B -> tree builder -> multi-level chunker. Returns flattened
/// `ChunkSpec`s carrying `parent_index` (chunk_index of parent within this
/// document) for the indexer to back-fill into `parent_chunk_id`.
fn build_hierarchical_specs(
    text: &str,
    file_name: &str,
    classifier: Option<&dyn crate::services::heading_detector::llm_pass::Classifier>,
) -> Vec<crate::services::chunker::ChunkSpec> {
    use crate::services::heading_detector::{regex_pass, llm_pass, tree};

    let candidates = regex_pass::scan(text);
    let promoted = match classifier {
        Some(c) => {
            let paragraph_count = text.split("\n\n").count();
            let (kept, _budget) = llm_pass::disambiguate(candidates, text, paragraph_count, c);
            kept
        }
        None => candidates.into_iter().filter(|c| c.confidence >= 0.85).collect(),
    };
    let doc_tree = tree::build_tree(text, &promoted, tree::BuildOptions {
        document_title: file_name.to_string(),
    });
    crate::services::chunker::chunk_tree(text, &doc_tree)
}

#[derive(Serialize, Clone)]
pub struct IndexingProgress {
    pub source_id: String,
    pub percent: u8,
    pub stage: String,
}

/// Process-wide broadcast of indexing progress (F014/T024). The headless server
/// streams this over `GET /api/index/events` (SSE), replacing the Tauri
/// `source:indexing-progress` channel for browser clients. Bounded buffer; a
/// lagging subscriber just drops the oldest events (progress is monotonic, so a
/// missed intermediate tick is harmless). Concurrency itself stays bounded by
/// the existing `indexing_sem()` — this only adds an observation channel.
pub fn index_progress_channel() -> &'static tokio::sync::broadcast::Sender<IndexingProgress> {
    static CH: std::sync::OnceLock<tokio::sync::broadcast::Sender<IndexingProgress>> =
        std::sync::OnceLock::new();
    CH.get_or_init(|| tokio::sync::broadcast::channel(256).0)
}

/// Latest progress tick per source — the **tunnel-safe** companion to the SSE
/// broadcast above. The Cloudflare quick tunnel buffers `GET /api/index/events`
/// indefinitely, so a browser behind it never receives a tick; it polls
/// `POST /api/index_progress` instead, which reads this map. Bounded by pruning
/// completed entries (100%) on read; a stalled source expires with the process.
pub fn latest_progress() -> &'static std::sync::Mutex<std::collections::HashMap<String, (u8, String)>> {
    static MAP: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, (u8, String)>>,
    > = std::sync::OnceLock::new();
    MAP.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Read (and drop) the latest tick for each requested source. Returns only the
/// ids we know about; an absent id means "no tick yet" (the caller renders an
/// indeterminate state rather than a misleading 0%).
pub fn progress_for(source_ids: &[String]) -> Vec<IndexingProgress> {
    let Ok(mut map) = latest_progress().lock() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for id in source_ids {
        if let Some((percent, stage)) = map.get(id).cloned() {
            out.push(IndexingProgress { source_id: id.clone(), percent, stage });
            // Finished sources need not be tracked any further.
            if percent >= 100 {
                map.remove(id);
            }
        }
    }
    out
}

fn emit_progress(app: &Option<crate::compat::AppHandle>, source_id: &str, percent: u8, stage: &str) {
    let ev = IndexingProgress {
        source_id: source_id.to_string(),
        percent,
        stage: stage.to_string(),
    };
    // Poll transport: remember the latest tick (SSE is unusable behind the tunnel).
    if let Ok(mut map) = latest_progress().lock() {
        map.insert(source_id.to_string(), (percent, stage.to_string()));
    }
    // Web transport (SSE): always publish; no subscribers = no-op.
    let _ = index_progress_channel().send(ev.clone());
    // Desktop transport (Tauri event): only when running inside the app.
    if let Some(h) = app {
        let _ = h.emit("source:indexing-progress", ev);
    }
}

/// Doc-type routing hook (feature 003, T023). If `path` is a constitution IR
/// JSON, return the parsed IR. Detection IS the deserialization — a JSON that
/// is not a constitution fails `ConstitutionIr`'s required fields. Future
/// Act/Rule profiles will be detected and routed here the same way.
fn try_load_constitution(
    path: &Path,
) -> Option<crate::services::constitution_ir::ConstitutionIr> {
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return None;
    }
    match crate::services::constitution_ir::load(path) {
        Ok(ir) if !ir.parts.is_empty() => Some(ir),
        _ => None,
    }
}

/// Constitution v1 structure-aware ingest (feature 003, T023).
///
/// Replaces flat text chunking for the constitution: build the L0–L5 tree
/// (S2), persist it into `chunks` with parent links + canonical citations (S3),
/// build the legal graph into `graph_nodes`/`graph_edges` (S4), then embed +
/// upsert (S5). `do_index` routes here when `try_load_constitution` matches.
async fn index_constitution(
    pool: &Db,
    embedder: Arc<EmbedderState>,
    source_id: &str,
    ir: crate::services::constitution_ir::ConstitutionIr,
    app: &Option<crate::compat::AppHandle>,
) -> Result<()> {
    use crate::db::repositories::graph as graph_repo;
    use crate::services::doc_profiles::constitution::CONSTITUTION;
    use crate::services::{graph_builder, structure_parser};

    emit_progress(app, source_id, 25, "chunking");

    // Re-index: drop this source's old graph rows while the chunk join is still
    // valid, then its chunks.
    graph_repo::delete_for_source(pool, source_id).await?;
    chunk_repo::delete_by_source(pool, source_id).await?;

    let tree = structure_parser::parse_tree(&ir, &CONSTITUTION);
    let inserted = structure_parser::persist_tree(pool, source_id, &tree).await?;
    if inserted.is_empty() {
        source_repo::update_error(pool, source_id, "constitution IR produced no nodes").await?;
        return Ok(());
    }

    // Legal graph overlay.
    let (nodes, edges) = graph_builder::build_graph(&tree, &inserted);
    graph_repo::insert_nodes(pool, &nodes).await?;
    graph_repo::insert_edges(pool, &edges).await?;

    let count = |t: &str| inserted.iter().filter(|c| c.unit_type == t).count();
    log::info!(
        "Indexed constitution {source_id}: {} chunks \
         (part:{} chapter:{} article:{} clause:{} subclause:{}) — \
         graph {} nodes / {} edges",
        inserted.len(),
        count("part"),
        count("chapter"),
        count("article"),
        count("clause"),
        count("subclause"),
        nodes.len(),
        edges.len(),
    );

    emit_progress(app, source_id, 35, "storing");
    if !embed_and_upsert(pool, &embedder, source_id, &inserted, app).await? {
        return Ok(()); // a failure was already recorded on the source
    }

    // Finalize — the source is searchable from here. Constitution path
    // takes an IR (canonical text); not a ParsedDocument tier — record as
    // 'native' / 1.0 since there is no extraction tier promotion to gate.
    emit_progress(app, source_id, 95, "finalizing");
    match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        source_repo::set_indexed_at(pool, source_id, "native", 1.0),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            source_repo::update_error(pool, source_id, &format!("set_indexed_at: {e}")).await?;
            return Ok(());
        }
        Err(_) => {
            source_repo::update_error(pool, source_id, "set_indexed_at timed out").await?;
            return Ok(());
        }
    }
    emit_progress(app, source_id, 100, "done");
    log::info!(
        "Indexed constitution source {source_id} — {} chunks",
        inserted.len()
    );

    // S3a (2026-05-23) — Re-enable NER for constitution chunks.
    //
    // The v1 comment here previously read "the constitution's structure
    // graph IS its knowledge graph — the generic entity-NER pass is not
    // run here." That was correct for citation-style queries (Article N
    // → fast-path), but it left definitional queries that don't name a
    // citation ("Who is the founder of Pakistan?") without entity-level
    // matching. NER on constitution chunks gives the Preamble row tags
    // like `Quaid-i-Azam`, `Mohammad Ali Jinnah`, `Founder of Pakistan`,
    // making them addressable beyond the structural fast-path.
    //
    // Spawned in the background AFTER set_indexed_at so the UI flips to
    // "indexed" immediately. Pass 1 (regex) is fast; Pass 2 (LLM) is
    // slow and capped by ECHO_PASS2_CHUNK_LIMIT.
    //
    // Only fires when AppHandle is available (Pass 2 needs the model
    // slot via Tauri State). Integration tests with `app: None` keep the
    // pre-S3a behaviour — no NER, which is fine for retrieval tests.
    //
    // S3b (queued) will wire entity matches into retrieval scoring. S3a
    // only populates the data.
    // NER Pass 2 disabled (2026-05-25) — user request to free CPU for
    // indexing. Pass 1 (rule-based) still runs in-process; only the
    // LLM-driven incremental loop is gated off. Re-enable by uncommenting
    // the spawn block below (and the sibling site in index_source).
    let _ = app; // silence unused-arg warning while pass 2 is off
    // if let Some(app_handle) = app.clone() {
    //     let pool_clone = pool.clone();
    //     let source_id_owned = source_id.to_string();
    //     let file_name = format!("constitution:{source_id}");
    //     tokio::spawn(async move {
    //         run_ner_loop_for_source(pool_clone, app_handle, source_id_owned, file_name).await;
    //     });
    // }
    Ok(())
}

/// Embed `inserted` chunks in bounded batches and upsert into `chunk_embeddings`.
/// `Ok(true)` → proceed (incl. the benign sqlite-vec-absent case); `Ok(false)`
/// → a failure was recorded on the source and the caller should stop.
/// ⚠️ `pub(crate)` so the oversize-chunk repair path (`api/maintenance.rs`) can
/// embed replacement chunks through the SAME batching, timeouts and vec0 upsert
/// the indexer uses. A second embedding path would drift from this one.
pub(crate) async fn embed_and_upsert(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    source_id: &str,
    inserted: &[chunk_repo::Chunk],
    app: &Option<crate::compat::AppHandle>,
) -> Result<bool> {
    // fastembed runs ONNX inference synchronously and is CPU-intensive — it
    // MUST go on spawn_blocking. Chunks are embedded in fixed-size batches so
    // peak memory is bounded regardless of document size (a single tensor over
    // all chunks can exhaust RAM and trip the OOM killer).
    let chunk_count = inserted.len();
    const EMBED_BATCH: usize = 32;
    log::info!("source {source_id}: embedding {chunk_count} chunks in batches of {EMBED_BATCH}");
    log::info!("METRIC embed_start source={source_id} chunks={chunk_count}");
    let t_embed = std::time::Instant::now();
    let total_batches = chunk_count.div_ceil(EMBED_BATCH).max(1);
    let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunk_count);
    for (batch_idx, batch) in inserted.chunks(EMBED_BATCH).enumerate() {
        let batch_owned: Vec<String> = batch.iter().map(|c| c.content.clone()).collect();
        let embedder_clone = Arc::clone(embedder);
        let batch_result = tokio::task::spawn_blocking(move || embedder_clone.embed(batch_owned))
            .await
            .map_err(|e| anyhow::anyhow!("embed thread panicked: {e}"))??;
        embeddings.extend(batch_result);
        let pct = 40 + ((batch_idx + 1) * 45 / total_batches) as u8;
        emit_progress(app, source_id, pct, "embedding");
        tokio::task::yield_now().await;
    }
    log::info!(
        "METRIC embed_done_ms={} source={source_id} chunks={chunk_count}",
        t_embed.elapsed().as_millis()
    );

    // ── Upsert embeddings ────────────────────────────────────────────────────
    let vec_available = match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public' AND table_name='chunk_embeddings'",
        )
        .fetch_one(pool),
    )
    .await
    {
        Ok(Ok(n)) => n > 0,
        Ok(Err(e)) => {
            log::error!("source {source_id}: vec0 probe failed: {e}");
            source_repo::update_error(pool, source_id, &format!("vec0 probe failed: {e}")).await?;
            return Ok(false);
        }
        Err(_) => {
            log::error!("source {source_id}: vec0 probe timed out after 10s");
            source_repo::update_error(pool, source_id, "vec0 probe timed out — writer pool wedged")
                .await?;
            return Ok(false);
        }
    };

    if vec_available {
        // Sub-batch the upsert so the single SQLite writer lock is held only
        // ~1 ms at a time; 30s per-batch timeout surfaces deadlocks.
        const UPSERT_BATCH: usize = 8;
        const UPSERT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
        let pairs: Vec<(&_, &_)> = inserted.iter().zip(embeddings.iter()).collect();
        // The vec0/HNSW upsert is the slowest phase for large files (minutes for
        // thousands of chunks). Report incremental progress across the 85→95 band
        // so the UI doesn't appear frozen at 85% while vectors are being indexed.
        let total_upsert_batches = pairs.len().div_ceil(UPSERT_BATCH).max(1);
        for (batch_idx, sub) in pairs.chunks(UPSERT_BATCH).enumerate() {
            let res: anyhow::Result<()> = tokio::time::timeout(UPSERT_TIMEOUT, async {
                let mut tx = pool.begin().await?;
                for &(chunk, embedding) in sub {
                    let json = serde_json::to_string(embedding)?;
                    sqlx::query(
                        "INSERT INTO chunk_embeddings (chunk_id, embedding) VALUES ($1, $2::vector) ON CONFLICT (chunk_id) DO UPDATE SET embedding = excluded.embedding",
                    )
                    .bind(&chunk.id)
                    .bind(&json)
                    .execute(&mut *tx)
                    .await?;
                }
                tx.commit().await?;
                Ok(())
            })
            .await
            .unwrap_or_else(|_| {
                Err(anyhow::anyhow!(
                    "embedding upsert batch {batch_idx} timed out after {}s",
                    UPSERT_TIMEOUT.as_secs()
                ))
            });
            if let Err(e) = res {
                log::error!("source {source_id}: {e}");
                source_repo::update_error(pool, source_id, &e.to_string()).await?;
                return Ok(false);
            }
            let pct = (85 + (batch_idx + 1) * 10 / total_upsert_batches).min(95) as u8;
            emit_progress(app, source_id, pct, "indexing vectors");
            tokio::task::yield_now().await;
        }
        log::info!("source {source_id}: upserted {} embeddings", inserted.len());
    } else {
        log::warn!("sqlite-vec not available — skipping embedding upserts for source {source_id}");
    }
    Ok(true)
}

/// Index a single file into the database.
///
/// Pipeline: detect format → parse → chunk → embed → bulk_insert → upsert embeddings
/// CPU-heavy steps run on the blocking thread pool so the async event loop
/// stays responsive throughout.
/// File a freshly registered source into whatever category the running batch is
/// for, if any.
///
/// Best-effort and silent when there is no batch: most callers of `index_file`
/// (attachments, archive imports, single-file retries) have no category, and a
/// missing one is normal rather than an error.
async fn stamp_batch_category(pool: &Db, notebook_id: &str, source_id: &str) {
    let Some(folder_id) = notebook_id.strip_prefix("__folder__") else { return };
    let cat = crate::db::repositories::folders::batch_category(pool, folder_id)
        .await
        .ok()
        .flatten();
    let Some(cat) = cat else { return };
    if let Err(e) = crate::db::repositories::corpus_category::assign(
        pool,
        &[source_id.to_string()],
        Some(&cat),
    )
    .await
    {
        log::warn!("could not file source {source_id} into category {cat}: {e}");
    }
}

pub async fn index_file(
    pool: &Db,
    embedder: Arc<EmbedderState>,
    _task_budget: &TaskBudget,
    tier: &HardwareTier,
    notebook_id: &str,
    file_path: &str,
    app: Option<crate::compat::AppHandle>,
) -> Result<String> {
    let path = Path::new(file_path);

    // Determine format.
    let format = FileFormat::from_path(path)
        .ok_or_else(|| anyhow::anyhow!("unsupported file type: {file_path}"))?;

    // File metadata (fast sync I/O — acceptable on async thread).
    let metadata = std::fs::metadata(path)?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let file_size = metadata.len() as i64;
    let last_modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    // Compute SHA-256 hash for deduplication (reads whole file — blocking I/O).
    let path_for_hash = path.to_path_buf();
    let file_hash = tokio::task::spawn_blocking(move || compute_sha256(&path_for_hash))
        .await
        .map_err(|e| anyhow::anyhow!("hash thread panicked: {e}"))??;

    // Dedup check: if source already exists with same hash, just attach to notebook.
    if let Some(existing) = source_repo::get_by_path(pool, file_path).await? {
        if existing.file_hash == file_hash && existing.indexing_status == "indexed" {
            source_repo::attach_to_notebook(pool, notebook_id, &existing.id).await?;
            return Ok(existing.id);
        }
        // Re-index if hash changed or not yet indexed.
        // Keep status as "pending" until the semaphore is acquired so the UI
        // never shows multiple files as "Processing" simultaneously.
        source_repo::update_status(pool, &existing.id, "pending").await?;
        let _indexing_permit = indexing_sem()
            .acquire()
            .await
            .map_err(|e| anyhow::anyhow!("indexing semaphore closed: {e}"))?;
        source_repo::update_status(pool, &existing.id, "processing").await?;
        do_index(pool, embedder, tier, &existing.id, path, &format, &app).await?;
        source_repo::attach_to_notebook(pool, notebook_id, &existing.id).await?;
        return Ok(existing.id);
    }

    // Insert new source record as "pending" immediately so the UI can show it
    // and the source poller keeps running while this file waits its turn.
    // F021: ownership is stamped atomically at insert (resolved from the target
    // notebook) so a private upload is never retrievable by others, even
    // briefly. Attachment rows are pre-created by the attach endpoint and take
    // the dedup path above instead.
    let scope = source_repo::scope_for_notebook(pool, notebook_id).await;
    let source = source_repo::insert_scoped(
        pool,
        file_path,
        &file_name,
        format.as_str(),
        file_size,
        &file_hash,
        last_modified,
        &scope,
    )
    .await?;

    source_repo::attach_to_notebook(pool, notebook_id, &source.id).await?;

    // File it NOW, at registration — not when indexing completes.
    //
    // The category used to be applied after `index_file` returned. Automatic GPU
    // indexing restarts the server mid-batch by design, and any file killed
    // between "marked indexed" and that assignment kept NO category and was
    // invisible to the resume path (which only revisits pending/processing).
    // It cost 2 of 27 files on one run, and it scales with
    // ECHO_INDEX_CONCURRENCY, since that many files sit in the window at once.
    //
    // Stamping here closes it: the row carries its category from the moment it
    // exists, so nothing that happens later can lose it. Taken from the batch
    // state on the folder row (migration 027) rather than a parameter, which
    // keeps every one of `index_file`'s thirteen call sites unchanged.
    stamp_batch_category(pool, notebook_id, &source.id).await;

    // Mark NER as pending so KG tab shows progress overlay while Pass 2 runs.
    let _ = sqlx::query("UPDATE sources SET ner_status='pending', ner_progress=0 WHERE id=$1")
        .bind(&source.id)
        .execute(pool)
        .await;

    // Acquire the indexing semaphore AFTER the DB insert so only the heavy work
    // (parse + chunk + embed) is serialised, not the registration step.
    let _indexing_permit = indexing_sem()
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("indexing semaphore closed: {e}"))?;

    source_repo::update_status(pool, &source.id, "processing").await?;

    do_index(pool, embedder, tier, &source.id, path, &format, &app).await?;

    Ok(source.id)
}

async fn do_index(
    pool: &Db,
    embedder: Arc<EmbedderState>,
    tier: &HardwareTier,
    source_id: &str,
    path: &Path,
    format: &FileFormat,
    app: &Option<crate::compat::AppHandle>,
) -> Result<()> {
    // ── Feature 007 tier check (FR-007 sticky deep) ──────────────────────────
    // Read `sources.index_tier`. Fast tier (default) runs the existing
    // 004/005 pipeline unchanged. Deep tier is officer-initiated via the
    // `deep_reindex_document` Tauri command (Slice A T053). On a re-index
    // of a deep-tier document we MUST route to the deep pipeline instead
    // of fast — silently dropping back to fast would violate FR-007.
    //
    // Slice 0 (this commit): the deep branch is dormant — log a warning
    // and fall through to fast. The dormant branch becomes live in Slice A
    // T021 when `deep_reindex::run` ships.
    let current_tier: String = sqlx::query_scalar(
        "SELECT index_tier FROM sources WHERE id = $1",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| "fast".to_string());
    if current_tier == "deep" {
        log::info!(
            "do_index: source {} is deep-tier — LLM IR (7B) path is enabled for this file",
            source_id
        );
    }

    // ── Parse document ────────────────────────────────────────────────────────
    // All parsers are synchronous (lopdf, pdftotext subprocess, docx/xlsx I/O).
    // Running them on a tokio worker thread would stall the entire async runtime.
    let path_buf = path.to_path_buf();
    let format_clone = format.clone();
    let tier_clone = tier.clone();

    // ── Storage guard (T089) ─────────────────────────────────────────────────
    // Check disk space before starting. Emit storage:low event if < 1 GB free
    // so the App.tsx banner can warn the user and halt further indexing.
    if crate::services::task_budget::TaskBudget::is_storage_low(
        &std::path::Path::new(path).to_string_lossy(),
        1.0,
    ) {
        if let Some(h) = app {
            let _ = h.emit(
                "storage:low",
                serde_json::json!({ "path": path.to_string_lossy(), "free_gb": 0 }),
            );
        }
        source_repo::update_error(
            pool,
            source_id,
            "Insufficient disk space (< 1 GB free) — free up space and retry",
        ).await?;
        return Ok(());
    }

    emit_progress(app, source_id, 5, "parsing");

    // ── Doc-type routing (feature 003, T023) ────────────────────────────────
    // A constitution_ir.json routes to the structure-aware tree + graph path;
    // ordinary documents fall through to flat hierarchical chunking. Future
    // Act/Rule profiles will be detected here the same way.
    if let Some(ir) = try_load_constitution(path) {
        return index_constitution(pool, embedder, source_id, ir, app).await;
    }

    let parse_result = tokio::task::spawn_blocking(move || -> Result<_, ParseError> {
        match &format_clone {
            FileFormat::Pdf => parsers::pdf::extract(&path_buf),
            FileFormat::Docx | FileFormat::Xlsx | FileFormat::Pptx => {
                parsers::document::extract(&path_buf)
                    .map_err(|e| ParseError::Other(format!("document parse failed: {e}")))
            }
            FileFormat::Txt | FileFormat::Markdown | FileFormat::Code => {
                parsers::text::extract(&path_buf)
                    .map_err(|e| ParseError::Other(format!("text parse failed: {e}")))
            }
            // ⚠️ OCR, not the vision model (2026-08-11). This passed `None` as the
            // vision model path, so `describe` ALWAYS returned "Vision model not
            // loaded. Please download Qwen2-VL 2B" — every image upload failed,
            // and no image ever reached OCR. Officers upload photographed and
            // scanned notices constantly; they need the TEXT, and the same
            // PP-OCR ladder scanned PDFs already use reads them fine.
            FileFormat::Image => {
                parsers::ocr::ocr_image(&path_buf, &parsers::ocr::OcrConfig::for_image())
            }
            FileFormat::Audio | FileFormat::Video => Err(ParseError::CapabilityNotAvailable {
                message: "Audio/video indexing requires Whisper model in models/".to_string(),
            }),
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("parse thread panicked: {e}"))?;

    let parsed = match parse_result {
        Ok(p) => p,
        Err(ParseError::ScannedPdf) => {
            source_repo::update_status(pool, source_id, "indexed").await?;
            log::info!(
                "PDF {source_id} is image-only; indexed with no text content (vision model not available)"
            );
            return Ok(());
        }
        Err(ParseError::CapabilityNotAvailable { message }) => {
            source_repo::update_error(pool, source_id, &message).await?;
            return Ok(());
        }
        Err(e) => {
            source_repo::update_error(pool, source_id, &e.to_string()).await?;
            return Err(e.into());
        }
    };

    // Resolve the file_name for the document title (root of the chunk tree).
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
    // Display title (2026-07-28): human-readable name derived from the file
    // name — used for citations AND title-first classification. CamelCase
    // upload names defeated `doc_type_from_title`'s tokenizer (one giant
    // token → `has("rules")` false), so the split title fixes both at once.
    let doc_title = crate::services::display_title::display_title(file_name);
    // Only when nothing has named this document already. A title set
    // deliberately — an instrument cut out of a compendium is named by the
    // operator who confirmed the split — must beat one reconstructed from a
    // filename. 2026-07-30, found end-to-end: the split recorded
    // "Sindh Local Councils (Election) Rules, 2015" and this overwrote it with
    // "Sindh Local Councils Election Rules 2015", because a filesystem-safe
    // filename cannot carry the parentheses and comma a citation needs.
    let _ = sqlx::query(
        "UPDATE sources SET display_name = $1 WHERE id = $2 AND display_name IS NULL",
    )
    .bind(&doc_title)
    .bind(source_id)
    .execute(pool)
    .await;

    // ── Routing order: DIRECT path is PRIMARY, LLM IR is SECONDARY ─────────
    // The generic structured importer (Feature 005, regex tree) runs first for
    // text-extractable documents. The Mistral 7B LLM IR builder (Feature 006)
    // is invoked ONLY as a fallback below, when the direct importer cannot
    // structure the document (e.g. scanned/unstructured PDFs). This keeps
    // text-extractable docs on the fast path and reserves the slow 7B LLM IR
    // for documents that genuinely need it (matches the standing design note:
    // LLM IR for scanned/image PDFs, not as the default).

    // ── Feature 005 — generic structured importer (PRIMARY) ────────────────
    // For supported formats: detect profile from the parsed text, build a
    // 6-level tree using regex heading patterns, and (if heading coverage is
    // sufficient) persist via the existing structure_parser + graph_builder
    // path. When it cannot structure the doc, fall back to LLM IR (secondary),
    // then to size-based chunking.
    if structured_importer::enabled()
        && matches!(format, FileFormat::Pdf | FileFormat::Docx | FileFormat::Txt | FileFormat::Markdown | FileFormat::Code)
    {
        let force = source_repo::get_doc_type_override(pool, source_id).await.ok().flatten();
        let outcome = structured_importer::import(&parsed.text, &doc_title, force.as_deref(), &parsed.page_offsets, &parsed.provision_titles);
        if outcome.structured {
            chunk_repo::delete_by_source(pool, source_id).await?;
            emit_progress(app, source_id, 25, "chunking");
            let stats = structured_importer::persist(pool, source_id, &outcome, &parsed.reconstructed_rows, parsed.reconstruction_method, &parsed.tables).await?;
            log::info!(
                "structured_importer: {} → doc_type={} chunks={} nodes={} edges={}",
                file_name, outcome.doc_type, stats.chunks, stats.graph_nodes, stats.graph_edges
            );

            // Embeddings (reuse the existing shared path).
            let inserted = chunk_repo::list_by_source(pool, source_id).await?;
            emit_progress(app, source_id, 50, "embedding");
            if !embed_and_upsert(pool, &embedder, source_id, &inserted, app).await? {
                return Ok(());
            }

            // Finalize: set_indexed_at (Feature 004 quality persistence).
            emit_progress(app, source_id, 95, "finalizing");
            source_repo::set_indexed_at(pool, source_id, parsed.extraction_method, parsed.quality_score).await?;
            emit_progress(app, source_id, 100, "done");
            log::info!("Indexed source {source_id} via structured_importer — {} chunks", inserted.len());
            return Ok(());
        }
        // Not structured — record outcome on the source row. structure_status
        // ='failed' surfaces in UI. Try LLM IR (secondary) next, then fall
        // through to size-based chunking.
        sqlx::query(
            "UPDATE sources SET doc_type = $1, doc_type_source = $2, structure_status = 'failed', structure_importer_version = $3 WHERE id = $4",
        )
        .bind(outcome.doc_type)
        .bind(outcome.doc_type_source.as_str())
        .bind(structured_importer::STRUCTURED_IMPORTER_VERSION)
        .bind(source_id)
        .execute(pool)
        .await
        .ok();

        // Feature 006 (Mistral 7B LLM-IR fallback) removed 2026-06 — ingestion
        // is now a single deterministic path. A document the structured importer
        // cannot structure falls straight through to size-based chunking below
        // (no LLM IR builder). `current_tier` no longer gates a deep LLM path.
        let _ = &current_tier;
    }

    // Delete existing chunks (re-index case).
    chunk_repo::delete_by_source(pool, source_id).await?;

    emit_progress(app, source_id, 25, "chunking");

    // Hierarchical chunking pipeline (Tier 3). do_index has access to the
    // AppHandle and therefore the ModelSlot + tier — so LLM Pass B for
    // ambiguous heading candidates is enabled here. The classifier borrow is
    // scoped to this block so it doesn't outlive any subsequent .await.
    // Desktop: AppHandle gives the ModelSlot + tier, so LLM Pass B for ambiguous
    // headings is available. Headless server build has no managed state — the
    // compat AppHandle has no `.state()`, so this whole path is desktop-gated and
    // the server always falls back to the deterministic (non-LLM) chunker.
    #[cfg(feature = "desktop")]
    let chunk_specs = if let Some(app_handle) = app.as_ref() {
        use crate::services::llm::ModelSlot;

        let model_slot = app_handle.state::<Arc<ModelSlot>>();
        let tier_state = app_handle.state::<HardwareTier>();
        // Same auto-discovery the chat uses — picks whichever GGUF is in
        // models/ instead of a hardcoded filename. See run_ner_loop_for_source.
        let (model_path, _label, _) =
            crate::services::query_router::select_model_for_tier(&tier_state);

        let classifier = crate::services::heading_detector::llm_pass::LlmClassifier {
            model_slot: &*model_slot,
            model_path: &model_path,
            tier: &*tier_state,
        };
        build_hierarchical_specs(&parsed.text, file_name, Some(&classifier))
    } else {
        build_hierarchical_specs(&parsed.text, file_name, None)
    };
    #[cfg(not(feature = "desktop"))]
    let chunk_specs = {
        let _ = app;
        build_hierarchical_specs(&parsed.text, file_name, None)
    };

    if chunk_specs.is_empty() {
        source_repo::update_error(pool, source_id, "No text content extracted").await?;
        return Ok(());
    }

    // Capture parent_index + chunk_index per spec BEFORE consuming them — we
    // need the mapping for the parent_chunk_id back-fill UPDATE pass.
    let parent_indices: Vec<Option<i64>> = chunk_specs.iter().map(|s| s.parent_index).collect();
    let chunk_indices: Vec<i64> = chunk_specs.iter().map(|s| s.chunk_index).collect();

    // First pass: insert chunks with parent_chunk_id = None; carries
    // unit_type / unit_label / unit_path through into_new_chunk_with_parent.
    let new_chunks: Vec<_> = chunk_specs
        .into_iter()
        .map(|cs| cs.into_new_chunk_with_parent(source_id, None))
        .collect();

    let chunk_count = new_chunks.len();
    log::info!("source {source_id}: {chunk_count} chunks, inserting into DB");
    emit_progress(app, source_id, 35, "storing");
    let inserted = chunk_repo::bulk_insert(pool, new_chunks).await?;

    // Second pass: back-fill parent_chunk_id by mapping chunk_index -> real UUID.
    let id_by_index: std::collections::HashMap<i64, String> = inserted.iter()
        .map(|c| (c.chunk_index, c.id.clone()))
        .collect();
    {
        let mut tx = pool.begin().await?;
        for (i, parent_idx) in parent_indices.iter().enumerate() {
            if let Some(p_idx) = parent_idx {
                if let (Some(parent_id), Some(child_id)) = (
                    id_by_index.get(p_idx),
                    id_by_index.get(&chunk_indices[i]),
                ) {
                    sqlx::query("UPDATE chunks SET parent_chunk_id = $1 WHERE id = $2")
                        .bind(parent_id)
                        .bind(child_id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
        tx.commit().await?;
    }

    log::info!(
        "Indexed {} chunks for {} (doc:{} section:{} leaf:{})",
        inserted.len(), file_name,
        inserted.iter().filter(|c| c.unit_type == "document").count(),
        inserted.iter().filter(|c| c.unit_type == "section").count(),
        inserted.iter().filter(|c| c.unit_type == "leaf").count(),
    );

    // ── Embed + upsert (shared with the constitution path, T023) ────────────
    if !embed_and_upsert(pool, &embedder, source_id, &inserted, app).await? {
        return Ok(()); // a failure was already recorded on the source
    }

    // Mark indexed now — the source is searchable from this point.
    // Timeout-wrapped for the same reason as the upsert loop above.
    // Record which extraction tier produced this source's text (feature 004).
    emit_progress(app, source_id, 95, "finalizing");
    match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        source_repo::set_indexed_at(
            pool,
            source_id,
            parsed.extraction_method,
            parsed.quality_score,
        ),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            log::error!("source {source_id}: set_indexed_at failed: {e}");
            source_repo::update_error(pool, source_id, &format!("set_indexed_at: {e}")).await?;
            return Ok(());
        }
        Err(_) => {
            log::error!("source {source_id}: set_indexed_at timed out after 30s");
            source_repo::update_error(pool, source_id, "set_indexed_at timed out").await?;
            return Ok(());
        }
    }
    emit_progress(app, source_id, 100, "done");
    log::info!("Indexed source {source_id} — {} chunks", inserted.len());

    // ── Entity extraction — deferred background task ──────────────────────────
    // Spawned AFTER set_indexed_at so the UI updates to "indexed" immediately.
    // Pass 1 (regex) is fast; Pass 2 (LLM) is slow — both run in the background
    // so the "indexed" status appears in the UI without waiting.
    //
    // Pass 2 is capped to a sampled subset of chunks to bound RAM/CPU on large
    // documents. Without this cap, a 200+ page PDF queues hundreds of LLM
    // inferences, holding the model resident in RAM long enough that
    // systemd-oomd (Linux) reaps the entire Chromium cgroup — taking the Tauri
    // dev webview (and Cursor, if running) with it.
    let pool_for_entities = pool.clone();
    let chunks_for_entities: Vec<(String, String)> = inserted
        .iter()
        .map(|c| (c.id.clone(), c.content.clone()))
        .collect();
    let pass2_limit: usize = std::env::var("ECHO_PASS2_CHUNK_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &usize| n > 0)
        .unwrap_or(DEFAULT_PASS2_CHUNK_LIMIT);
    let pass2_indices: Vec<usize> = if chunks_for_entities.len() <= pass2_limit {
        (0..chunks_for_entities.len()).collect()
    } else {
        // Even sampling across the document so coverage stays representative.
        let n = chunks_for_entities.len();
        (0..pass2_limit)
            .map(|i| (i * n) / pass2_limit)
            .collect()
    };
    let app_for_entities = app.clone();
    let source_id_for_entities = source_id.to_string();
    // Capture file_name for KG progress events (queried from DB path is overkill).
    let file_name_for_entities = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    tokio::spawn(async move {
        // ── Pass 1: regex NER (fast) ─────────────────────────────────────────
        for (chunk_id, content) in &chunks_for_entities {
            if let Err(e) =
                entity_extractor::extract_pass1(&pool_for_entities, chunk_id, content).await
            {
                log::warn!("Pass 1 entity extraction failed for chunk {chunk_id}: {e}");
            }
        }
        log::debug!(
            "Pass 1 entity extraction complete for {} chunks",
            chunks_for_entities.len()
        );

        // ── Pass 2: LLM-based NER — DISABLED (2026-05-25) ────────────────────
        // Hard-disabled per user request: Pass 2 was contending for CPU with
        // active indexing (OCR + structure parser), slowing both. Pass 1
        // regex NER above still runs. To re-enable: change `false` below
        // back to `if let Some(ref app_handle) = app_for_entities {` AND
        // uncomment the sibling spawn in the constitution path (~line 335).
        // Mark the source done so the Sources panel doesn't show NER
        // "pending" forever while Pass 2 is off.
        let _ = sqlx::query(
            "UPDATE sources SET ner_status='done', ner_progress=100 WHERE id=$1"
        )
        .bind(&source_id_for_entities)
        .execute(&pool_for_entities)
        .await;
        let _ = &file_name_for_entities; // silence unused while Pass 2 off
        let _ = &app_for_entities;       // silence unused while Pass 2 off
        let _ = pass2_indices;           // silence unused while Pass 2 off
        let _ = pass2_limit;             // silence unused while Pass 2 off
        // Desktop-only: uses `app_handle.state()` (managed ModelSlot/tier) +
        // `crate::commands::kg_preferences`, neither of which exist in the
        // headless server build. The `if false` already makes it dead code; the
        // cfg keeps the server build from even type-checking the Tauri surface.
        #[cfg(feature = "desktop")]
        if false { if let Some(ref app_handle) = app_for_entities {
            let _permit = entity_sem().acquire().await.ok(); // held until this block exits
            use std::sync::Arc;
            use crate::services::llm::ModelSlot;
            use crate::services::hardware_detector::HardwareTier;
            use tauri::Emitter;

            let model_slot = app_handle.state::<Arc<ModelSlot>>();
            let tier = app_handle.state::<HardwareTier>();
            // Auto-discover the active GGUF (same as chat) instead of a
            // hardcoded filename — see run_ner_loop_for_source for rationale.
            let (model_path, _label, _) =
                crate::services::query_router::select_model_for_tier(&tier);

            let total = pass2_indices.len();
            log::info!(
                "Pass 2 entity extraction: {} of {} chunks (cap={})",
                total,
                chunks_for_entities.len(),
                pass2_limit
            );

            // Mark source as running in DB + emit initial progress event.
            let _ = sqlx::query(
                "UPDATE sources SET ner_status='running', ner_progress=0, ner_total=$1 WHERE id=$2"
            )
            .bind(total as i64)
            .bind(&source_id_for_entities)
            .execute(&pool_for_entities)
            .await;
            let _ = app_handle.emit("kg://progress", serde_json::json!({
                "source_id": &source_id_for_entities,
                "file_name": &file_name_for_entities,
                "processed": 0,
                "total": total,
                "eta_seconds": null,
            }));

            let relation_types = crate::commands::kg_preferences::load_relation_types(
                &pool_for_entities,
            ).await;
            let t_start = std::time::Instant::now();
            for (done, &i) in pass2_indices.iter().enumerate() {
                let Some((chunk_id, content)) = chunks_for_entities.get(i) else { continue };
                if let Err(e) = entity_extractor::extract_pass2(
                    &pool_for_entities,
                    chunk_id,
                    content,
                    &*model_slot,
                    &model_path,
                    &tier,
                    &relation_types,
                    None,
                ).await {
                    log::warn!("Pass 2 entity extraction failed for chunk {chunk_id}: {e}");
                }

                let processed = done + 1;
                let progress = ((processed * 100) / total.max(1)) as i64;
                // ETA: elapsed / done * remaining
                let elapsed_secs = t_start.elapsed().as_secs_f64();
                let eta_seconds: Option<u64> = if processed > 0 {
                    let rate = elapsed_secs / processed as f64;
                    Some((rate * (total - processed) as f64).ceil() as u64)
                } else {
                    None
                };

                let _ = sqlx::query(
                    "UPDATE sources SET ner_progress=$1 WHERE id=$2"
                )
                .bind(progress)
                .bind(&source_id_for_entities)
                .execute(&pool_for_entities)
                .await;
                let _ = app_handle.emit("kg://progress", serde_json::json!({
                    "source_id": &source_id_for_entities,
                    "file_name": &file_name_for_entities,
                    "processed": processed,
                    "total": total,
                    "eta_seconds": eta_seconds,
                }));

                tokio::task::yield_now().await;
            }

            // Mark done.
            let _ = sqlx::query(
                "UPDATE sources SET ner_status='done', ner_progress=100 WHERE id=$1"
            )
            .bind(&source_id_for_entities)
            .execute(&pool_for_entities)
            .await;
            let _ = app_handle.emit("kg://progress", serde_json::json!({
                "source_id": &source_id_for_entities,
                "file_name": &file_name_for_entities,
                "processed": total,
                "total": total,
                "eta_seconds": 0,
            }));
            log::debug!(
                "Pass 2 entity extraction complete for {} sampled chunks",
                total
            );
        } } // close `if let Some(ref app_handle)` then `if false` (Pass 2 disabled)
    });

    Ok(())
}

/// Run NER for a source that is already indexed but has `ner_status='pending'`.
/// Fetches all chunks from DB, runs Pass 1 regex + Pass 2 LLM extraction.
/// Called from startup recovery and can be called from retry paths.
///
/// Desktop-only: takes an owned `tauri::AppHandle` and reads managed
/// `ModelSlot`/tier state. The headless server build never runs Pass-2 NER.
#[cfg(feature = "desktop")]
pub async fn run_ner_for_source(
    pool: &Db,
    source_id: &str,
    file_name: &str,
    app: tauri::AppHandle,
) {
    use crate::services::entity_extractor;
    use tauri::Emitter;

    // Load all chunks for this source.
    let chunks: Vec<(String, String)> = match sqlx::query_as::<_, (String, String)>(
        "SELECT c.id, c.content FROM chunks c WHERE c.source_id = $1 ORDER BY c.chunk_index",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            log::warn!("run_ner_for_source({source_id}): failed to load chunks: {e}");
            return;
        }
    };

    if chunks.is_empty() {
        log::info!("run_ner_for_source({source_id}): no chunks found, marking done");
        let _ = sqlx::query("UPDATE sources SET ner_status='done', ner_progress=100 WHERE id=$1")
            .bind(source_id)
            .execute(pool)
            .await;
        return;
    }

    // Pass 1: regex NER (fast, always runs).
    for (chunk_id, content) in &chunks {
        if let Err(e) = entity_extractor::extract_pass1(pool, chunk_id, content).await {
            log::warn!("run_ner_for_source Pass 1 failed for chunk {chunk_id}: {e}");
        }
    }

    // Pass 2 DISABLED (2026-05-25) — see indexer.rs:~335 for rationale.
    // Mark done so the Sources panel doesn't show "NER pending" forever.
    let _ = sqlx::query("UPDATE sources SET ner_status='done', ner_progress=100 WHERE id=$1")
        .bind(source_id)
        .execute(pool)
        .await;
    let _ = (file_name, &app); // silence unused
    return;
    #[allow(unreachable_code)]
    {
    // Pass 2: LLM-based NER (slow, acquires entity semaphore).
    let pass2_limit: usize = std::env::var("ECHO_PASS2_CHUNK_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &usize| n > 0)
        .unwrap_or(DEFAULT_PASS2_CHUNK_LIMIT);

    let pass2_indices: Vec<usize> = if chunks.len() <= pass2_limit {
        (0..chunks.len()).collect()
    } else {
        let n = chunks.len();
        (0..pass2_limit).map(|i| (i * n) / pass2_limit).collect()
    };

    let _permit = entity_sem().acquire().await.ok();

    use crate::services::hardware_detector::HardwareTier;
    use crate::services::llm::ModelSlot;

    let model_slot = app.state::<Arc<ModelSlot>>();
    let tier = app.state::<HardwareTier>();
    // Auto-discover the active GGUF (same path the chat uses) instead of
    // a hardcoded filename. See run_ner_loop_for_source for rationale.
    let (model_path, _label, _) =
        crate::services::query_router::select_model_for_tier(&tier);

    let total = pass2_indices.len();

    let _ = sqlx::query(
        "UPDATE sources SET ner_status='running', ner_progress=0, ner_total=$1 WHERE id=$2",
    )
    .bind(total as i64)
    .bind(source_id)
    .execute(pool)
    .await;

    let _ = app.emit("kg://progress", serde_json::json!({
        "source_id": source_id,
        "file_name": file_name,
        "processed": 0,
        "total": total,
        "eta_seconds": null,
    }));

    let relation_types = crate::commands::kg_preferences::load_relation_types(pool).await;
    let t_start = std::time::Instant::now();

    for (done, &i) in pass2_indices.iter().enumerate() {
        let Some((chunk_id, content)) = chunks.get(i) else { continue };
        // extract_pass2 handles its own 90s abort internally via ModelSlot::request_abort().
        // No outer timeout needed — the abort-flag mechanism ensures the blocking thread
        // releases the mutex cleanly (unlike dropping a JoinHandle which creates a zombie).
        match entity_extractor::extract_pass2(
            pool, chunk_id, content, &*model_slot, &model_path, &tier, &relation_types, None,
        ).await {
            Ok(()) => {}
            Err(e) => log::warn!("run_ner_for_source Pass 2 failed for chunk {chunk_id}: {e}"),
        }

        let processed = done + 1;
        let elapsed_secs = t_start.elapsed().as_secs_f64();
        let eta_seconds: Option<u64> = if processed > 0 {
            let rate = elapsed_secs / processed as f64;
            Some((rate * (total - processed) as f64).ceil() as u64)
        } else {
            None
        };

        let progress = ((processed * 100) / total.max(1)) as i64;
        let _ = sqlx::query("UPDATE sources SET ner_progress=$1 WHERE id=$2")
            .bind(progress)
            .bind(source_id)
            .execute(pool)
            .await;

        let _ = app.emit("kg://progress", serde_json::json!({
            "source_id": source_id,
            "file_name": file_name,
            "processed": processed,
            "total": total,
            "eta_seconds": eta_seconds,
        }));

        tokio::task::yield_now().await;
    }

    let _ = sqlx::query("UPDATE sources SET ner_status='done', ner_progress=100 WHERE id=$1")
        .bind(source_id)
        .execute(pool)
        .await;

    let _ = app.emit("kg://progress", serde_json::json!({
        "source_id": source_id,
        "file_name": file_name,
        "processed": total,
        "total": total,
        "eta_seconds": 0,
    }));

    log::info!("run_ner_for_source({source_id}): NER complete ({total} chunks sampled)");
    } // close `#[allow(unreachable_code)] {` opened where Pass 2 was disabled
}

/// Idea 2 (2026-05-23) — incremental full-coverage Pass-2 NER loop.
///
/// Per-source background task: runs Pass-1 (regex, all chunks) once,
/// then iteratively processes batches of pending Pass-2 chunks until
/// `chunks.ner_pass2_done = 1` for every chunk in the source. Each
/// iteration:
///   1. Selects the next batch via `select_next_pass2_batch`
///      (even-sample + always-include salient).
///   2. Runs Pass-2 LLM extraction on each chunk in the batch.
///   3. On success, sets `ner_pass2_done = 1` for that chunk (so it's
///      not re-selected next iteration).
///   4. Yields to scheduler / sleeps `ECHO_PASS2_SLEEP_SECS` before the
///      next batch (gives the user space to chat).
///
/// Chat-priority is honoured via the existing
/// `WorkloadKind::Pass2Ner` scheduler — when Chat takes foreground the
/// in-flight chunk is cancelled and the loop sees `is_cancelled`.
///
/// Exits when no pending chunks remain → marks `sources.ner_status =
/// 'done'` (final). On scheduler cancellation the function returns;
/// the loop is re-entered next time the source is touched (or on app
/// startup recovery).
///
/// Desktop-only (managed `ModelSlot`/tier state via `tauri::AppHandle`).
#[cfg(feature = "desktop")]
pub async fn run_ner_loop_for_source(
    pool: crate::db::Db,
    app: tauri::AppHandle,
    source_id: String,
    file_name: String,
) {
    // Pass 2 DISABLED (2026-05-25) — see indexer.rs:~335 for rationale.
    // Mark done so Sources panel doesn't show "NER pending" forever.
    let _ = sqlx::query("UPDATE sources SET ner_status='done', ner_progress=100 WHERE id=$1")
        .bind(&source_id)
        .execute(&pool)
        .await;
    let _ = (&app, &file_name); // silence unused
    log::info!("run_ner_loop_for_source({source_id}): SKIPPED — Pass 2 hardcoded off");
    return;
    #[allow(unreachable_code)]
    {
    use crate::services::entity_extractor;
    use tauri::Emitter;

    log::info!("run_ner_loop_for_source({source_id}): starting incremental Pass-2 NER");

    // Pass 1 (regex) over ALL chunks first — fast, idempotent, populates
    // reference-type entities the salience boost can use even before any
    // Pass-2 batch completes.
    let all_chunks: Vec<(String, String)> = match sqlx::query_as(
        "SELECT id, content FROM chunks WHERE source_id = $1 ORDER BY chunk_index",
    )
    .bind(&source_id)
    .fetch_all(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("run_ner_loop_for_source({source_id}): load chunks failed: {e}");
            return;
        }
    };
    if all_chunks.is_empty() {
        let _ = sqlx::query("UPDATE sources SET ner_status='done', ner_progress=100 WHERE id=$1")
            .bind(&source_id)
            .execute(&pool)
            .await;
        return;
    }
    for (chunk_id, content) in &all_chunks {
        if let Err(e) = entity_extractor::extract_pass1(&pool, chunk_id, content).await {
            log::warn!("Pass 1 failed for chunk {chunk_id}: {e}");
        }
    }
    let total_chunks = all_chunks.len();

    let _ = sqlx::query("UPDATE sources SET ner_status='running', ner_total=$1 WHERE id=$2")
        .bind(total_chunks as i64)
        .bind(&source_id)
        .execute(&pool)
        .await;

    // Serialisation matches the existing inline Pass-2 path (do_index +
    // legacy run_ner_for_source): a single-permit `entity_sem()` ensures
    // only one Pass-2 LLM call runs at a time. We deliberately do NOT
    // register with the foreground scheduler — `WorkloadKind::Pass2Ner`
    // would be cancelled by the default-foreground `WorkloadKind::Chat`,
    // and no production code currently calls `set_foreground(Chat)` to
    // re-claim it. Wiring full scheduler coordination for Pass-2 is a
    // separate piece of work (queued follow-up). For now, Chat queries
    // and Pass-2 chunks compete for the LLM via `ModelSlot`'s mutex —
    // worst case the user waits one Pass-2 chunk (≈5–10 s) before chat
    // starts; subsequent chunks are gated by `entity_sem`.

    let sleep_dur = std::time::Duration::from_secs(pass2_sleep_secs());
    let mut pass_num = 0_u32;

    loop {
        let batch = match select_next_pass2_batch(&pool, &source_id).await {
            Ok(b) => b,
            Err(e) => {
                log::warn!("run_ner_loop_for_source({source_id}): batch selection failed: {e}");
                return;
            }
        };
        if batch.is_empty() {
            // All chunks processed.
            break;
        }
        pass_num += 1;
        let batch_n = batch.len();

        // Process one batch.
        let _permit = entity_sem().acquire().await.ok();
        let model_slot = app.state::<std::sync::Arc<crate::services::llm::ModelSlot>>();
        let tier = app.state::<crate::services::hardware_detector::HardwareTier>();
        // Use the same auto-discovery the chat path uses — whichever GGUF
        // is present in models/ (largest by parsed param size, fallback
        // file size). Replaces the legacy hardcoded "1.5B" path that
        // silently failed every Pass-2 call when the user only had the
        // 0.5B fine-tune downloaded.
        let (model_path, _label, _) =
            crate::services::query_router::select_model_for_tier(&tier);
        let relation_types =
            crate::commands::kg_preferences::load_relation_types(&pool).await;

        let t_batch = std::time::Instant::now();
        for (chunk_id, content) in &batch {
            match entity_extractor::extract_pass2(
                &pool,
                chunk_id,
                content,
                &*model_slot,
                &model_path,
                &tier,
                &relation_types,
                None,
            )
            .await
            {
                Ok(()) => {
                    let _ = sqlx::query("UPDATE chunks SET ner_pass2_done = 1 WHERE id = $1")
                        .bind(chunk_id)
                        .execute(&pool)
                        .await;
                }
                Err(e) => log::warn!("Pass 2 failed for chunk {chunk_id}: {e}"),
            }
            tokio::task::yield_now().await;
        }
        drop(_permit);

        // Update progress + emit event after each batch.
        let done_total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chunks WHERE source_id = $1 AND ner_pass2_done = 1",
        )
        .bind(&source_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
        let progress = (done_total * 100 / total_chunks as i64) as i32;
        let _ = sqlx::query("UPDATE sources SET ner_progress=$1 WHERE id=$2")
            .bind(progress as i64)
            .bind(&source_id)
            .execute(&pool)
            .await;
        let _ = app.emit(
            "kg://progress",
            serde_json::json!({
                "source_id": source_id,
                "file_name": file_name,
                "processed": done_total,
                "total": total_chunks,
                "pass": pass_num,
                "batch_size": batch_n,
                "batch_ms": t_batch.elapsed().as_millis(),
            }),
        );

        log::info!(
            "source {source_id}: Pass-2 batch {pass_num} processed {batch_n} chunks \
             ({done_total}/{total_chunks} = {progress}%) in {} ms",
            t_batch.elapsed().as_millis()
        );

        // Yield to chat / let scheduler park us if needed.
        tokio::time::sleep(sleep_dur).await;
    }

    let _ = sqlx::query("UPDATE sources SET ner_status='done', ner_progress=100 WHERE id=$1")
        .bind(&source_id)
        .execute(&pool)
        .await;
    log::info!(
        "run_ner_loop_for_source({source_id}): 100% complete after {pass_num} batches"
    );
    let _ = app.emit(
        "kg://progress",
        serde_json::json!({
            "source_id": source_id,
            "file_name": file_name,
            "processed": total_chunks,
            "total": total_chunks,
            "pass": pass_num,
            "complete": true,
        }),
    );
    } // close `#[allow(unreachable_code)] {` opened where Pass 2 was disabled
}

/// Startup recovery: scan sources with any pending Pass-2 chunks and
/// spawn `run_ner_loop_for_source` for each. Idempotent — called once
/// at app startup; a source already fully done is skipped.
///
/// Desktop-only (spawns `run_ner_loop_for_source` with a `tauri::AppHandle`).
#[cfg(feature = "desktop")]
pub fn resume_pending_pass2_ner(pool: crate::db::Db, app: tauri::AppHandle) {
    tokio::spawn(async move {
        let pending: Vec<(String, String)> = match sqlx::query_as(
            "SELECT DISTINCT s.id, COALESCE(s.file_name, s.file_path) \
             FROM sources s JOIN chunks c ON c.source_id = s.id \
             WHERE c.ner_pass2_done = 0",
        )
        .fetch_all(&pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                log::warn!("resume_pending_pass2_ner: scan failed: {e}");
                return;
            }
        };
        log::info!(
            "resume_pending_pass2_ner: found {} sources with pending Pass-2 chunks",
            pending.len()
        );
        for (source_id, file_name) in pending {
            let p = pool.clone();
            let a = app.clone();
            tokio::spawn(async move {
                run_ner_loop_for_source(p, a, source_id, file_name).await;
            });
        }
    });
}

// ─── T9/T10: Scheduler-aware batch indexing ──────────────────────────────────

/// Scheduler-aware batch indexing entry point. Processes each file in its own
/// SQLite transaction, checking for pause/cancel at every file boundary.
pub async fn run_index_job(
    pool: &Db,
    handle: &Arc<crate::services::scheduler::WorkloadHandle>,
    job_id: &str,
    notebook_id: &str,
    files: &[(String, i64)], // (path, mtime)
    embedder: Option<Arc<EmbedderState>>,
) -> anyhow::Result<()> {
    use crate::db::repositories::index_jobs as repo;

    for (i, (path, _mtime)) in files.iter().enumerate() {
        // Cooperative pause: parks here until foreground flips back.
        handle.should_pause().await;
        if handle.is_cancelled() {
            repo::set_job_status(pool, job_id, "cancelled", None).await?;
            return Ok(());
        }

        let mut tx = pool.begin().await?;
        match ingest_single_file_tx(&mut tx, notebook_id, path, embedder.clone()).await {
            Ok(chunks_written) => {
                repo::mark_file_done(&mut tx, job_id, path, chunks_written as i64).await?;
                tx.commit().await?;
                repo::increment_processed(pool, job_id).await?;
            }
            Err(e) => {
                let _ = tx.rollback().await;
                repo::mark_file_failed(pool, job_id, path, &e.to_string()).await?;
            }
        }
        handle.report((i as u64) + 1, files.len() as u64);
    }

    repo::set_job_status(pool, job_id, "completed", None).await?;
    Ok(())
}

/// Ingest one file into the provided transaction. Returns number of chunks written.
/// All writes (source record, chunks) are inside the transaction — caller commits or rolls back.
/// Embeddings are upserted separately after commit (they use a virtual table that
/// doesn't play well with all transaction semantics and are not critical for atomicity).
async fn ingest_single_file_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    notebook_id: &str,
    file_path: &str,
    embedder: Option<Arc<EmbedderState>>,
) -> anyhow::Result<usize> {
    use crate::services::parsers::FileFormat;
    use sha2::{Digest, Sha256};
    use std::io::{BufReader, Read as _};

    let path = std::path::Path::new(file_path);
    let format = FileFormat::from_path(path)
        .ok_or_else(|| anyhow::anyhow!("unsupported file type: {file_path}"))?;

    let metadata = std::fs::metadata(path)?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
    let file_size_bytes = metadata.len() as i64;
    let last_modified = metadata.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64).unwrap_or(0);

    // Compute hash
    let path_buf = path.to_path_buf();
    let file_hash = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let file = std::fs::File::open(&path_buf)?;
        let mut reader = BufReader::with_capacity(65_536, file);
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 65_536];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 { break; }
            hasher.update(&buf[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }).await.map_err(|e| anyhow::anyhow!("hash thread: {e}"))??;

    // Create or get source record. Use the transaction connection for the SELECT
    // (not the pool) to avoid holding tx + acquiring a second pool connection,
    // which deadlocks on a single-connection in-memory SQLite pool.
    let source_id = {
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM sources WHERE file_path = $1",
        )
        .bind(file_path)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some((existing_id,)) = existing {
            // Upsert source inside tx
            sqlx::query("UPDATE sources SET file_hash = $1, last_modified = $2, indexing_status = 'processing' WHERE id = $3")
                .bind(&file_hash).bind(last_modified).bind(&existing_id)
                .execute(&mut **tx).await?;
            existing_id
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().timestamp_millis();
            sqlx::query(
                "INSERT INTO sources (id, file_path, file_name, format, file_size_bytes, file_hash, last_modified, indexing_status, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 'processing', $8)"
            )
            .bind(&id).bind(file_path).bind(&file_name).bind(format.as_str())
            .bind(file_size_bytes).bind(&file_hash).bind(last_modified).bind(now)
            .execute(&mut **tx).await?;

            // Attach to notebook
            sqlx::query("INSERT INTO notebook_sources (notebook_id, source_id, added_at) VALUES ($1, $2, $3) ON CONFLICT (notebook_id, source_id) DO NOTHING")
                .bind(notebook_id).bind(&id).bind(now)
                .execute(&mut **tx).await?;

            id
        }
    };

    // Parse
    let path_buf2 = path.to_path_buf();
    let format_clone = format.clone();
    let parsed = tokio::task::spawn_blocking(move || -> anyhow::Result<crate::services::parsers::ParsedDocument> {
        match &format_clone {
            FileFormat::Pdf => crate::services::parsers::pdf::extract(&path_buf2).map_err(Into::into),
            FileFormat::Txt | FileFormat::Markdown | FileFormat::Code =>
                crate::services::parsers::text::extract(&path_buf2).map_err(|e| anyhow::anyhow!("{e}")),
            FileFormat::Docx | FileFormat::Xlsx | FileFormat::Pptx =>
                crate::services::parsers::document::extract(&path_buf2).map_err(|e| anyhow::anyhow!("{e}")),
            _ => Err(anyhow::anyhow!("format not supported for batch indexing")),
        }
    }).await.map_err(|e| anyhow::anyhow!("parse thread: {e}"))??;

    // Delete existing chunks for this source (re-index case)
    sqlx::query("DELETE FROM chunks WHERE source_id = $1")
        .bind(&source_id).execute(&mut **tx).await?;

    // Hierarchical chunking (regex-only — batch path has no AppHandle / LLM access).
    let chunk_specs = build_hierarchical_specs(&parsed.text, &file_name, None);
    if chunk_specs.is_empty() {
        return Ok(0);
    }

    let chunk_count = chunk_specs.len();

    // Pre-allocate UUIDs so we can resolve parent_chunk_id within the same
    // transaction (single-pass insert; no follow-up UPDATE).
    let pre_ids: Vec<String> = (0..chunk_count).map(|_| uuid::Uuid::new_v4().to_string()).collect();
    let id_by_index: std::collections::HashMap<i64, String> = chunk_specs.iter().zip(&pre_ids)
        .map(|(s, id)| (s.chunk_index, id.clone())).collect();

    let now = chrono::Utc::now().timestamp_millis();
    for (i, spec) in chunk_specs.iter().enumerate() {
        let chunk_id = &pre_ids[i];
        let parent_chunk_id = spec.parent_index.and_then(|pi| id_by_index.get(&pi).cloned());
        sqlx::query(
            "INSERT INTO chunks (id, source_id, chunk_index, content, char_start, char_end, page_number, token_count, created_at, \
             parent_chunk_id, unit_type, unit_label, unit_path, domain_metadata) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"
        )
        .bind(chunk_id).bind(&source_id).bind(spec.chunk_index)
        .bind(&spec.content).bind(spec.char_start).bind(spec.char_end)
        .bind(spec.page_number).bind(spec.token_count).bind(now)
        .bind(parent_chunk_id)
        .bind(&spec.unit_type)
        .bind(&spec.unit_label)
        .bind(&spec.unit_path)
        .bind::<Option<String>>(None) // domain_metadata: reserved for future
        .execute(&mut **tx).await?;
    }

    // Mark indexed + record extraction provenance (feature 004).
    sqlx::query("UPDATE sources SET indexing_status = 'indexed', extraction_method = $1, quality_score = $2 WHERE id = $3")
        .bind(parsed.extraction_method)
        .bind(parsed.quality_score)
        .bind(&source_id)
        .execute(&mut **tx).await?;

    // Embeddings happen after commit (not blocking atomicity) — handled by caller if embedder provided
    drop(embedder); // not used in tx; post-commit embedding is deferred

    Ok(chunk_count)
}

// ─── T10: Resume flow ────────────────────────────────────────────────────────

pub struct ResumePlan {
    pub skipped: Vec<String>,
    pub to_process: Vec<String>,
}

pub async fn plan_resume(
    pool: &Db,
    notebook_id: &str,
    root_path: &str,
    scanned: &[(String, i64)],
) -> anyhow::Result<ResumePlan> {
    use crate::db::repositories::index_jobs as repo;
    let done = repo::find_completed_files_for_root(pool, notebook_id, root_path).await?;
    let done_map: std::collections::HashMap<&str, i64> =
        done.iter().map(|f| (f.file_path.as_str(), f.mtime)).collect();

    let mut skipped = Vec::new();
    let mut to_process = Vec::new();
    for (path, mtime) in scanned {
        match done_map.get(path.as_str()) {
            Some(prev_mtime) if *prev_mtime == *mtime => skipped.push(path.clone()),
            _ => to_process.push(path.clone()),
        }
    }
    Ok(ResumePlan { skipped, to_process })
}

// ─── Feature 004 — startup re-extract hook ───────────────────────────────────

/// Quality threshold below which an indexed source is re-queued for extraction
/// on next boot. Picked to align with `text_quality::score`'s tier outputs
/// (anything `Suspect` scores ≤ 0.4).
const QUALITY_REEXTRACT_THRESHOLD: f32 = 0.6;

/// Find indexed PDF sources whose stored text is suspect — either never scored
/// (NULL: legacy rows pre-feature-004) or scored below threshold and not
/// already produced by OCR (no point re-OCRing a bad OCR result). Flip them to
/// `indexing_status='pending'` so the next indexer pass re-extracts.
///
/// Returns the number of rows enqueued. Idempotent: subsequent calls after
/// re-extraction restored quality are no-ops.
pub async fn enqueue_quality_reextract(pool: &crate::db::Db) -> sqlx::Result<usize> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM sources \
         WHERE indexing_status = 'indexed' \
           AND format = 'pdf' \
           AND (quality_score IS NULL \
                OR (quality_score < $1 AND COALESCE(extraction_method, '') != 'ocr'))",
    )
    .bind(QUALITY_REEXTRACT_THRESHOLD)
    .fetch_all(pool)
    .await?;

    let n = ids.len();
    for id in &ids {
        sqlx::query("UPDATE sources SET indexing_status = 'pending' WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(n)
}

/// Feature 005 — find indexed sources whose `structure_importer_version` is
/// older than the current binary's, and flip them to `'pending'` so the
/// indexer re-processes them through the structured importer.
pub async fn enqueue_structure_reextract(pool: &crate::db::Db) -> sqlx::Result<usize> {
    // Restrict to formats the Feature-005 generic importer can actually re-process.
    // constitution_ir.json (format='code') was indexed via the legacy
    // constitution-specific path (Feature 003) and carries version=0 forever — it
    // is a derived JSON IR, not a re-parseable source. Without this filter every
    // restart would flip it pending → failed via reset_stuck_pending.
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM sources \
         WHERE indexing_status = 'indexed' \
           AND structure_importer_version < $1 \
           AND format = 'pdf'",
    )
    .bind(structured_importer::STRUCTURED_IMPORTER_VERSION)
    .fetch_all(pool)
    .await?;

    let n = ids.len();
    for id in &ids {
        sqlx::query("UPDATE sources SET indexing_status = 'pending' WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(n)
}

/// Server-side structure re-extraction (2026-07-12). The desktop app enqueues
/// stale sources at startup (`enqueue_structure_reextract` inside the desktop-only
/// `setup_models_and_watchers`), but the headless `noviq-server` runs no such
/// startup hook and no pending-source worker — so a bumped
/// `STRUCTURED_IMPORTER_VERSION` never re-processes anything there. This drives
/// the re-extraction DIRECTLY: for every indexed PDF still below the current
/// importer version, re-run `do_index` (re-parses the file with the current
/// parser, re-chunks, re-embeds; idempotent — it clears the source's old chunks
/// and graph first). Serialised on `indexing_sem`. Returns
/// `(attempted, succeeded, failed)`.
pub async fn reindex_structure_all(
    pool: &Db,
    embedder: Arc<EmbedderState>,
    tier: &HardwareTier,
) -> Result<(usize, usize, usize)> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM sources \
         WHERE indexing_status = 'indexed' \
           AND structure_importer_version < $1 \
           AND format = 'pdf'",
    )
    .bind(structured_importer::STRUCTURED_IMPORTER_VERSION)
    .fetch_all(pool)
    .await?;

    let attempted = ids.len();
    let (mut ok, mut failed) = (0usize, 0usize);
    for id in ids {
        let src = match source_repo::get_by_id(pool, &id).await {
            Ok(Some(s)) => s,
            _ => { failed += 1; continue; }
        };
        let path = std::path::PathBuf::from(&src.file_path);
        let Some(format) = FileFormat::from_path(&path) else { failed += 1; continue; };
        // Serialise: one indexing job at a time (same gate the upload path uses).
        let _permit = match indexing_sem().acquire().await {
            Ok(p) => p,
            Err(e) => { log::error!("reindex: semaphore closed: {e}"); failed += 1; continue; }
        };
        source_repo::update_status(pool, &id, "processing").await.ok();
        match do_index(pool, Arc::clone(&embedder), tier, &id, &path, &format, &None).await {
            Ok(()) => { ok += 1; }
            Err(e) => {
                log::error!("reindex: source {id} ({}) failed: {e}", src.file_path);
                source_repo::update_status(pool, &id, "indexed").await.ok(); // restore
                failed += 1;
            }
        }
    }
    log::info!("reindex_structure_all: {attempted} attempted, {ok} ok, {failed} failed");
    Ok((attempted, ok, failed))
}

#[cfg(test)]
mod reextract_tests {
    use super::*;

    async fn fresh_pool() -> crate::db::Db {
        let pool = crate::db::test_pool().await;
        pool
    }

    async fn insert_source(
        pool: &crate::db::Db,
        id: &str,
        method: Option<&str>,
        quality: Option<f32>,
    ) {
        sqlx::query(
            "INSERT INTO sources (id, file_path, file_name, format, file_size_bytes, \
             file_hash, last_modified, indexing_status, created_at, extraction_method, \
             quality_score) VALUES ($1, $2, $3, 'pdf', 1, 'h', 0, 'indexed', 0, $4, $5)",
        )
        .bind(id)
        .bind(format!("/{id}.pdf"))
        .bind(format!("{id}.pdf"))
        .bind(method)
        .bind(quality)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn enqueues_only_low_quality_non_ocr_sources() {
        let pool = fresh_pool().await;
        insert_source(&pool, "a", Some("lopdf"), Some(0.95)).await;       // good — skip
        insert_source(&pool, "b", Some("pdftotext"), Some(0.30)).await;   // bad — enqueue
        insert_source(&pool, "c", Some("ocr"), Some(0.25)).await;         // already OCR — skip
        insert_source(&pool, "d", None, None).await;                       // NULL — enqueue

        let n = enqueue_quality_reextract(&pool).await.unwrap();
        assert_eq!(n, 2);

        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sources WHERE indexing_status = 'pending'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending, 2);

        // Second call is a no-op (the two pending rows no longer match
        // `indexing_status = 'indexed'`).
        let n2 = enqueue_quality_reextract(&pool).await.unwrap();
        assert_eq!(n2, 0);
    }

    // ── Feature 005 — structure-version re-extract ─────────────────────────

    #[tokio::test]
    async fn enqueue_structure_reextract_picks_up_old_version_sources() {
        let pool = fresh_pool().await;
        // 'current' tracks the live constant so a version bump doesn't break this
        // test — only genuinely-behind sources should be enqueued.
        sqlx::query(
            "INSERT INTO sources \
                 (id, file_path, file_name, format, file_size_bytes, file_hash, \
                  last_modified, indexing_status, created_at, structure_importer_version) \
             VALUES \
                 ('old',     '/a', 'a.pdf', 'pdf', 1, 'h', 0, 'indexed', 0, 0), \
                 ('current', '/b', 'b.pdf', 'pdf', 1, 'h', 0, 'indexed', 0, $1)",
        )
        .bind(structured_importer::STRUCTURED_IMPORTER_VERSION)
        .execute(&pool)
        .await
        .unwrap();

        let n = enqueue_structure_reextract(&pool).await.unwrap();
        assert_eq!(n, 1);

        let st: String = sqlx::query_scalar("SELECT indexing_status FROM sources WHERE id='old'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(st, "pending");

        let st2: String = sqlx::query_scalar("SELECT indexing_status FROM sources WHERE id='current'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(st2, "indexed");
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod scheduler_tests {
    use super::*;
    use crate::services::scheduler::{Scheduler, WorkloadKind};

    struct TestFixture {
        root: String,
        list: Vec<(String, i64)>,
        _tmpdir: tempfile::TempDir,
    }

    async fn make_test_fixture_files(n: usize) -> TestFixture {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().to_string_lossy().to_string();
        let mut list = Vec::new();
        for i in 0..n {
            let path = dir.path().join(format!("file{i}.txt"));
            std::fs::write(&path, format!("Hello from file {i}. This is test content for chunk {i}.")).unwrap();
            let mtime = std::fs::metadata(&path).unwrap()
                .modified().unwrap()
                .duration_since(std::time::UNIX_EPOCH).unwrap()
                .as_millis() as i64;
            list.push((path.to_string_lossy().to_string(), mtime));
        }
        TestFixture { root, list, _tmpdir: dir }
    }

    #[tokio::test]
    async fn cancel_at_file_boundary_leaves_consistent_db() {
        let pool = crate::db::test_pool().await;
        // Start with Indexing as foreground so the workload is NOT paused on registration.
        let sched = Scheduler::new(None);
        sched.set_foreground(WorkloadKind::Indexing).await;
        let handle = sched.register(WorkloadKind::Indexing);
        let fixture = make_test_fixture_files(3).await;

        // Insert a dummy notebook so the FK on notebook_sources is satisfied
        sqlx::query("INSERT INTO notebooks (id, name, created_at, updated_at) VALUES ('nb1', 'Test', 0, 0)")
            .execute(&pool).await.unwrap();

        let job = crate::db::repositories::index_jobs::create_job(
            &pool, "nb1", &fixture.root, &fixture.list,
        ).await.unwrap();

        // Cancel after ~50ms via the scheduler so notify_resume is also woken,
        // allowing should_pause() to unblock and see the cancellation.
        let sched2 = sched.clone();
        let handle_id = handle.id;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            sched2.request_cancel(handle_id);
        });

        let _ = run_index_job(&pool, &handle, &job.id, "nb1", &fixture.list, None).await;

        let done = crate::db::repositories::index_jobs::find_completed_files_for_root(
            &pool, "nb1", &fixture.root,
        ).await.unwrap();

        // Files that are done must have chunks; files not done must have 0 chunks.
        for (path, _mtime) in &fixture.list {
            let is_done = done.iter().any(|d| d.file_path == *path);
            let source_id: Option<String> = sqlx::query_scalar(
                "SELECT id FROM sources WHERE file_path = $1"
            ).bind(path).fetch_optional(&pool).await.unwrap();
            let chunk_count: i64 = if let Some(sid) = source_id {
                sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE source_id = $1")
                    .bind(sid).fetch_one(&pool).await.unwrap()
            } else {
                0
            };
            if is_done {
                assert!(chunk_count > 0, "done file {path} should have chunks");
            } else {
                assert_eq!(chunk_count, 0, "non-done file {path} should have 0 chunks");
            }
        }
    }

    #[tokio::test]
    async fn resume_skips_done_files_but_reprocesses_edited() {
        let pool = crate::db::test_pool().await;
        let prev = crate::db::repositories::index_jobs::create_job(
            &pool, "nb1", "/root",
            &[("/a".into(), 100), ("/b".into(), 200)],
        ).await.unwrap();
        {
            let mut tx = pool.begin().await.unwrap();
            crate::db::repositories::index_jobs::mark_file_done(&mut tx, &prev.id, "/a", 3).await.unwrap();
            crate::db::repositories::index_jobs::mark_file_done(&mut tx, &prev.id, "/b", 4).await.unwrap();
            tx.commit().await.unwrap();
        }

        let new_scan = vec![
            ("/a".into(), 100i64),   // unchanged
            ("/b".into(), 999i64),   // edited
            ("/c".into(), 300i64),   // new
        ];
        let plan = plan_resume(&pool, "nb1", "/root", &new_scan).await.unwrap();
        assert_eq!(plan.skipped, vec!["/a".to_string()]);
        assert_eq!(plan.to_process, vec!["/b".to_string(), "/c".to_string()]);
    }
}

/// Stream-hash the file in 64 KB chunks — avoids reading the entire file into RAM.
fn compute_sha256(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(65_536, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65_536];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

// ─── Idea 2 — incremental Pass-2 NER tests ───────────────────────────────────

#[cfg(test)]
mod pass2_coverage_tests {
    use super::*;

    /// `pass2_batch_size` honours percentage on a typical corpus.
    #[test]
    fn batch_size_pct_default_is_ten_percent() {
        // Defaults: 10% of 100 = 10
        std::env::remove_var("ECHO_PASS2_CHUNK_LIMIT");
        std::env::remove_var("ECHO_PASS2_BATCH_PCT");
        assert_eq!(pass2_batch_size(100), 10);
        // 10% of 33 ⇒ ceil(3.3) = 4
        assert_eq!(pass2_batch_size(33), 4);
    }

    /// Tiny corpora still return at least 1 chunk per batch.
    #[test]
    fn batch_size_floors_at_one() {
        std::env::remove_var("ECHO_PASS2_CHUNK_LIMIT");
        std::env::remove_var("ECHO_PASS2_BATCH_PCT");
        assert_eq!(pass2_batch_size(0), 0);
        assert_eq!(pass2_batch_size(1), 1);
        assert_eq!(pass2_batch_size(5), 1); // ceil(0.5) = 1
    }

    /// Legacy `ECHO_PASS2_CHUNK_LIMIT` still works (deprecated but honoured).
    #[test]
    fn legacy_absolute_count_overrides_pct() {
        std::env::set_var("ECHO_PASS2_CHUNK_LIMIT", "5");
        std::env::remove_var("ECHO_PASS2_BATCH_PCT");
        assert_eq!(pass2_batch_size(100), 5);
        assert_eq!(pass2_batch_size(3), 3, "cap clipped to pending size");
        std::env::remove_var("ECHO_PASS2_CHUNK_LIMIT");
    }

    /// Sleep knob honours range, falls back to default outside.
    #[test]
    fn sleep_secs_respects_range() {
        std::env::remove_var("ECHO_PASS2_SLEEP_SECS");
        assert_eq!(pass2_sleep_secs(), DEFAULT_PASS2_SLEEP_SECS);
        std::env::set_var("ECHO_PASS2_SLEEP_SECS", "5");
        assert_eq!(pass2_sleep_secs(), 5);
        std::env::set_var("ECHO_PASS2_SLEEP_SECS", "0"); // out of range
        assert_eq!(pass2_sleep_secs(), DEFAULT_PASS2_SLEEP_SECS);
        std::env::set_var("ECHO_PASS2_SLEEP_SECS", "99999"); // out of range
        assert_eq!(pass2_sleep_secs(), DEFAULT_PASS2_SLEEP_SECS);
        std::env::remove_var("ECHO_PASS2_SLEEP_SECS");
    }

    async fn pool_with_source() -> crate::db::Db {
        let pool = crate::db::test_pool().await;
        sqlx::query(
            "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
             VALUES('s1','/c','c','pdf',1,'h',0,0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    /// Insert N chunks; one of them gets `unit_label='Preamble'` so we can
    /// test the salient-priority rule. Default all rows to `ner_pass2_done=0`.
    async fn seed_chunks(pool: &crate::db::Db, n: usize, preamble_at: Option<usize>) {
        for i in 0..n {
            let id = format!("c{i:04}");
            let label = if Some(i) == preamble_at {
                Some("Preamble".to_string())
            } else {
                Some(format!("Article {i}"))
            };
            sqlx::query(
                "INSERT INTO chunks(id, source_id, chunk_index, char_start, char_end, \
                                    page_number, token_count, parent_chunk_id, \
                                    unit_type, unit_label, unit_path, domain_metadata, \
                                    content, created_at, ner_pass2_done) \
                 VALUES ($1, 's1', $2, 0, 10, NULL, 5, NULL, 'article', $3, $4, NULL, $5, 0, 0)",
            )
            .bind(&id)
            .bind(i as i64)
            .bind(&label)
            .bind(label.as_deref().unwrap_or(""))
            .bind(format!("content {i}"))
            .execute(pool)
            .await
            .unwrap();
        }
    }

    /// First batch on a 100-chunk source picks ~10 chunks AND always
    /// includes the salient Preamble chunk even when it falls outside
    /// the even-sample positions.
    #[tokio::test]
    async fn first_batch_includes_salient_even_when_offset() {
        std::env::remove_var("ECHO_PASS2_CHUNK_LIMIT");
        std::env::remove_var("ECHO_PASS2_BATCH_PCT");
        let pool = pool_with_source().await;
        // Preamble at index 1 — between even-sample steps (0, 10, 20, …).
        seed_chunks(&pool, 100, Some(1)).await;

        let batch = select_next_pass2_batch(&pool, "s1").await.unwrap();
        // Even-sample picks 10 indices: 0, 10, 20, 30, 40, 50, 60, 70, 80, 90
        // Salient priority adds index 1 → 11 distinct chunks.
        assert_eq!(batch.len(), 11, "got {} chunks", batch.len());
        // The Preamble chunk (id c0001) must be in the batch.
        let ids: Vec<&str> = batch.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"c0001"), "Preamble must be included: {ids:?}");
    }

    /// After marking the first batch done, the next batch must exclude
    /// those chunks and select fresh ones — iterative coverage.
    #[tokio::test]
    async fn second_batch_excludes_first_batch_chunks() {
        std::env::remove_var("ECHO_PASS2_CHUNK_LIMIT");
        std::env::remove_var("ECHO_PASS2_BATCH_PCT");
        let pool = pool_with_source().await;
        seed_chunks(&pool, 100, None).await;

        let first = select_next_pass2_batch(&pool, "s1").await.unwrap();
        assert_eq!(first.len(), 10, "first batch is 10% of 100");

        // Mark first batch done.
        for (id, _) in &first {
            sqlx::query("UPDATE chunks SET ner_pass2_done = 1 WHERE id = $1")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }

        let second = select_next_pass2_batch(&pool, "s1").await.unwrap();
        let first_ids: std::collections::HashSet<&str> =
            first.iter().map(|(id, _)| id.as_str()).collect();
        let second_ids: std::collections::HashSet<&str> =
            second.iter().map(|(id, _)| id.as_str()).collect();
        assert!(
            first_ids.is_disjoint(&second_ids),
            "second batch overlaps first: first={first_ids:?} second={second_ids:?}"
        );
        // Second batch is 10% of remaining 90 = ceil(9) = 9.
        assert_eq!(second.len(), 9);
    }

    /// When every chunk is done, the batch is empty — loop's terminator.
    #[tokio::test]
    async fn empty_batch_when_all_done() {
        let pool = pool_with_source().await;
        seed_chunks(&pool, 10, None).await;
        sqlx::query("UPDATE chunks SET ner_pass2_done = 1 WHERE source_id = 's1'")
            .execute(&pool)
            .await
            .unwrap();
        let batch = select_next_pass2_batch(&pool, "s1").await.unwrap();
        assert!(batch.is_empty());
    }
}
