//! Track 2 (ADR-0019 §2) — build the per-provision + per-document **recall
//! summary index**.
//!
//! A summary is ONLY a recall key. Each `graph_summary` row maps back to its
//! ORIGINAL provision chunk (`chunk_id`); at query time the summary embedding is
//! matched, then the ORIGINAL chunk text is what gets retrieved, answered, and
//! cited (the verbatim contract — the answer path never reads `summary_text`).
//!
//! One-time / offline: generation runs the shared model (vLLM in prod via
//! `generate_direct`) once per primary provision (`unit_type='article'`, the L3
//! canonical unit the counting module uses) plus one document-level rollup.
//! Sequential + bounded so it competes politely with live chat. Idempotent:
//! re-running rebuilds a source's summaries from scratch.

use std::sync::Arc;

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::db::Db;
use crate::services::embedder::EmbedderState;
use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::ModelSlot;
use crate::services::query_router;

/// Generation cap per gist — a couple of sentences is plenty for a recall key.
const GEN_MAX_TOKENS: u32 = 96;
/// Generation cap per presentational whole-document summary segment. A few
/// paragraphs is plenty; a larger budget just gives a small model room to ramble
/// into leaked reasoning and repetition loops (observed at 700), which the
/// sanitizer then has to cut anyway.
const GEN_DOC_FULL_MAX_TOKENS: u32 = 400;
/// Max source characters fed into ONE whole-document reduce call. Sized for the
/// prose model's context with room for the ~400-token output; larger documents are
/// summarised window-by-window and reduced hierarchically. This governs the
/// STRUCTURE-TOLERANT `document_full` path, which reduces over content, not labels.
const DOC_FULL_INPUT_CHARS: usize = 24_000;
/// Concurrent in-flight generations. vLLM batches concurrent requests, so this
/// turns a slow sequential pass into a batched one (the http engine path does not
/// hold the ModelSlot mutex). Bounded so it stays polite to live chat.
///
/// Default lowered 16 → 6 on 2026-07-27 after every large document failed with
/// `inference engine timed out`. The timeout in `vllm.rs` guards `stream.next()`
/// — it is an INTER-TOKEN deadline, not a whole-request one. A request queued
/// behind fifteen others emits no tokens while it waits, so the queue delay
/// itself trips the deadline. Raising the timeout only masks that; fewer
/// in-flight requests on a single 8B model sharing a 12 GB slice is the actual
/// fix, and costs little throughput because one GPU was never really serving 16
/// at once. Override with `NOVIQ_SUMMARY_CONCURRENCY` — no rebuild needed.
fn gen_concurrency() -> usize {
    std::env::var("NOVIQ_SUMMARY_CONCURRENCY")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(6)
}
/// Truncate very long provisions before summarizing (keeps the prompt bounded;
/// the gist only needs the gist, and the ORIGINAL full text is what we answer with).
const MAX_PROVISION_CHARS: usize = 4000;

/// Outcome of a build for one source.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct SummaryStats {
    pub source_id: String,
    pub provisions: usize,
    pub doc_level: usize,
    /// Presentational whole-document summary segments written (migration 012).
    pub doc_full: usize,
    pub skipped: bool,
}

/// Build (or rebuild) the summary recall index for one source. Idempotent —
/// deletes the source's existing summaries first (cascade clears their
/// `summary_embeddings`). Returns `skipped=true` for structure-less sources
/// (no L3 provisions → nothing to summarize; they keep plain vec+FTS recall).
pub async fn build_summaries_for_source(
    pool: &Db,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    embedder: &Arc<EmbedderState>,
    source_id: &str,
) -> Result<SummaryStats> {
    // Idempotent re-run: clear this source's prior summaries first.
    sqlx::query("DELETE FROM graph_summary WHERE source_id = $1")
        .bind(source_id)
        .execute(pool)
        .await
        .context("clear prior summaries")?;

    // Primary-provision chunks (L3 = 'article' storage vocab — the canonical
    // provision unit; matches services::counting).
    let rows: Vec<(String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT id, unit_label, unit_path, content FROM chunks \
         WHERE source_id = $1 AND unit_type = 'article' ORDER BY chunk_index",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await
    .context("load provision chunks")?;

    let now = chrono::Utc::now().timestamp();

    // Track 2 recall index — one plain-language gist per L3 provision, embedded in
    // the SAME space as chunks so the recall vector search is comparable. This is
    // STRUCTURE-DEPENDENT by design (it keys provisions for citation recall) and is
    // skipped for structure-less sources. The whole-document presentational summary
    // below is built separately, from CONTENT, so a source with no provisions still
    // gets summarised.
    let mut gists: Vec<String> = Vec::new();
    let concurrency = gen_concurrency();
    if !rows.is_empty() {
        log::info!(
            "summary_builder: {source_id} — summarizing {} provisions (concurrency {})",
            rows.len(), concurrency
        );
        gists.reserve(rows.len());
        // Order is preserved within/across chunks, so gists stay aligned with `rows`.
        for chunk in rows.chunks(concurrency) {
            let futs = chunk
                .iter()
                .map(|(_id, _cit, _up, content)| summarize_provision(model_slot, tier, content));
            for r in futures::future::join_all(futs).await {
                gists.push(r?);
            }
        }
        // spawn_blocking — ONNX is CPU-bound.
        let embeddings = {
            let embedder_clone = Arc::clone(embedder);
            let to_embed = gists.clone();
            tokio::task::spawn_blocking(move || embedder_clone.embed(to_embed))
                .await
                .context("embed task join")?
                .context("embed provision gists")?
        };
        for (((chunk_id, citation, unit_path, _content), gist), emb) in
            rows.iter().zip(gists.iter()).zip(embeddings.iter())
        {
            insert_summary(
                pool, source_id, chunk_id, citation.as_deref(), unit_path.as_deref(),
                "provision", gist, emb, now,
            )
            .await?;
        }
    } else {
        log::info!(
            "summary_builder: source {source_id} has no L3 provisions — building content-only whole-document summary"
        );
    }

    // Anchor chunk for the document-level rows (their `chunk_id` FK target): the
    // synthetic document root when present, else the first chunk in reading order —
    // so structure-less sources (no `document` unit) still anchor their summary.
    let anchor_chunk_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM chunks WHERE source_id = $1 \
         ORDER BY (unit_type = 'document') DESC, chunk_index LIMIT 1",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .context("find anchor chunk")?;

    let mut doc_level = 0usize;
    let mut doc_full = 0usize;
    if let Some(dc) = anchor_chunk_id {
        // Document-level recall gist (Track 2) — a vector key reduced over the
        // provision gists so broad "which document discusses X" questions route
        // here. Needs provisions; skipped for structure-less sources.
        if !gists.is_empty() {
            let joined: String = gists.iter().take(200).cloned().collect::<Vec<_>>().join("\n- ");
            let doc_gist = summarize_document(model_slot, tier, &joined).await?;
            let demb = {
                let embedder_clone = Arc::clone(embedder);
                let t = doc_gist.clone();
                tokio::task::spawn_blocking(move || embedder_clone.embed_one(&t))
                    .await
                    .context("embed doc task join")?
                    .context("embed doc gist")?
            };
            insert_summary(pool, source_id, &dc, None, None, "document", &doc_gist, &demb, now).await?;
            doc_level = 1;
        }

        // Presentational WHOLE-DOCUMENT summary (migration 012) — STRUCTURE-TOLERANT:
        // reduced over the source's CONTENT (its chunk text), NOT over provision
        // labels, so a mis-parsed or structure-less document still yields a faithful
        // whole-document summary. One segment for most documents, up to 3 for very
        // long ones. NOT embedded (stays out of the recall pool) and carries no
        // citations; served when the user asks to summarize this named document.
        let segments = build_document_full(pool, model_slot, tier, source_id).await?;
        for (i, summary) in segments.iter().enumerate() {
            insert_document_full(pool, source_id, &dc, i + 1, summary, now).await?;
            doc_full += 1;
        }
    }

    log::info!(
        "summary_builder: source {source_id} → {} provision + {} document + {} whole-doc summaries",
        rows.len(), doc_level, doc_full
    );
    Ok(SummaryStats {
        source_id: source_id.to_string(),
        provisions: rows.len(),
        doc_level,
        doc_full,
        skipped: rows.is_empty() && doc_full == 0,
    })
}

/// Build summaries for EVERY indexed source. Sources with L3 provisions also get
/// the Track 2 provision recall index; every source gets a content-based
/// whole-document summary (structure-tolerant), so structure-less documents are no
/// longer skipped. Returns per-source stats.
pub async fn build_summaries_all(
    pool: &Db,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    embedder: &Arc<EmbedderState>,
) -> Result<Vec<SummaryStats>> {
    // Only sources that finished indexing. A source mid-ingest (or failed) has a
    // partial/empty chunk set, so summarising it would bake an incomplete
    // whole-document summary that a later successful re-index does not overwrite
    // cleanly. Join to `sources.indexing_status` rather than trusting chunk
    // presence alone.
    let source_ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT c.source_id FROM chunks c \
         JOIN sources s ON s.id = c.source_id \
         WHERE s.indexing_status = 'indexed'",
    )
    .fetch_all(pool)
    .await
    .context("list indexed sources")?;

    log::info!("summary_builder: building summaries for {} source(s)", source_ids.len());
    let mut out = Vec::with_capacity(source_ids.len());
    for sid in source_ids {
        match build_summaries_for_source(pool, model_slot, tier, embedder, &sid).await {
            Ok(stats) => out.push(stats),
            Err(e) => log::error!("summary_builder: source {sid} failed: {e}"),
        }
    }
    Ok(out)
}

async fn summarize_provision(model_slot: &ModelSlot, tier: &HardwareTier, provision: &str) -> Result<String> {
    let text: String = provision.chars().take(MAX_PROVISION_CHARS).collect();
    let prompt = format!(
        "You are building a search index over legal provisions. In ONE or TWO plain-language \
         sentences, state what the following provision GOVERNS — the topics, obligations, powers, \
         rights, procedures, or subjects it covers — so a searcher can find it by meaning. Do NOT \
         quote it, do NOT restate the numbering, do NOT add any preamble. Output only the sentence(s).\
         \n\nPROVISION:\n{text}\n\nWHAT IT GOVERNS:"
    );
    let out = query_router::generate_direct(model_slot, tier, &prompt, GEN_MAX_TOKENS, None, None).await?;
    Ok(clean_gist(out))
}

async fn summarize_document(model_slot: &ModelSlot, tier: &HardwareTier, provision_gists: &str) -> Result<String> {
    let text: String = provision_gists.chars().take(MAX_PROVISION_CHARS * 3).collect();
    let prompt = format!(
        "You are building a search index. Given these per-provision topic notes for one legal \
         document, write TWO or THREE plain-language sentences describing what the DOCUMENT as a \
         whole governs and the main subjects it covers, so a searcher can find the right document \
         by meaning. Do NOT quote, do NOT list provision numbers, output only the sentences.\
         \n\nPROVISION NOTES:\n- {text}\n\nWHAT THE DOCUMENT GOVERNS:"
    );
    let out = query_router::generate_direct(model_slot, tier, &prompt, GEN_MAX_TOKENS, None, None).await?;
    Ok(clean_gist(out))
}

/// Write a presentational, user-facing summary of a whole document (or one segment
/// of a long one) from the document's own content (or from partial summaries of it).
/// Flowing prose for a government officer — NOT a search key. `seg`/`n_segments` let
/// a long document be summarised in order across a few calls; when `n_segments == 1`
/// it is a single complete summary.
async fn summarize_document_full(
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    material: &str,
    seg: usize,
    n_segments: usize,
) -> Result<String> {
    let text: String = material.chars().take(DOC_FULL_INPUT_CHARS).collect();
    let scope = if n_segments > 1 {
        format!(
            "This is part {seg} of {n_segments} of a long document. Summarise ONLY the material \
             given below; do not preface it as a part or mention the numbering. "
        )
    } else {
        String::new()
    };
    let prompt = format!(
        "You are writing a briefing summary of a document for a government officer. {scope}\
         Using the material below, write a clear, complete plain-language summary of what the \
         document covers: its purpose, the main areas it regulates, and the key procedures, \
         powers, rights, and obligations it sets out. COVER THE BREADTH of the material — \
         synthesise the DIFFERENT areas across all of it; do NOT dwell on a single topic. Begin \
         by naming what the document as a whole is and does (never open with \"This provision…\"). \
         Write flowing prose in a few short paragraphs. Do NOT quote text, do NOT cite section or \
         rule numbers, do NOT add a title, preamble, or meta commentary. Output only the summary.\
         \n\nMATERIAL:\n{text}\n\nSUMMARY:"
    );
    let out =
        query_router::generate_direct(model_slot, tier, &prompt, GEN_DOC_FULL_MAX_TOKENS, None, None)
            .await?;
    Ok(clean_gist(out))
}

/// Summarise ONE window of raw document text into a compact partial (tier 1 of the
/// hierarchical reduce). Used only when a document is too long for a single reduce
/// call; the partials are then reduced by [`summarize_document_full`].
async fn summarize_content_window(
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    window: &str,
) -> Result<String> {
    let text: String = window.chars().take(DOC_FULL_INPUT_CHARS).collect();
    let prompt = format!(
        "You are summarising ONE part of a document for a briefing. Read the text below and, in a \
         short paragraph of plain-language prose, state the topics, provisions, powers, procedures, \
         rights, and obligations it covers. Do NOT quote, do NOT cite section or rule numbers, \
         output only the paragraph.\
         \n\nTEXT:\n{text}\n\nSUMMARY OF THIS PART:"
    );
    let out =
        query_router::generate_direct(model_slot, tier, &prompt, GEN_DOC_FULL_MAX_TOKENS, None, None)
            .await?;
    Ok(clean_gist(out))
}

/// Build the presentational whole-document summary segment(s) for a source,
/// STRUCTURE-TOLERANTLY — by reducing over the document's own CONTENT rather than
/// over the parser's provision labels. Content that a mis-parse mis-labelled (a
/// schedule row minted as a duplicate rule, a heading glued to the wrong provision)
/// is still summarised faithfully, and a structure-less document (no `article`
/// units) is summarised too. Small/medium documents reduce in a single call; long
/// ones are windowed, each window summarised, then the partials reduced (into 1
/// segment when they fit, else up to 3 ordered segments).
async fn build_document_full(
    pool: &Db,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    source_id: &str,
) -> Result<Vec<String>> {
    let pieces = load_content_for_summary(pool, source_id).await?;
    if pieces.is_empty() {
        return Ok(Vec::new());
    }
    // Feature 024: an extra segment enumerating the document's tabular content
    // (department schedules etc.), appended to the prose segments so the stored
    // whole-doc summary conveys table entries a flat reduce would abstract away.
    let table_seg = build_table_segment(pool, source_id).await.unwrap_or(None);
    let windows = pack_windows(&pieces, DOC_FULL_INPUT_CHARS);

    // Single window → one reduce call over the raw content.
    if windows.len() <= 1 {
        let s = summarize_document_full(model_slot, tier, &windows[0], 1, 1).await?;
        let mut segs = vec![s];
        segs.extend(table_seg);
        return Ok(finalize_segments(segs));
    }

    // Tier 1: one partial summary per window (concurrent — vLLM batches them).
    let mut partials: Vec<String> = Vec::with_capacity(windows.len());
    for batch in windows.chunks(gen_concurrency()) {
        let futs = batch.iter().map(|w| summarize_content_window(model_slot, tier, w));
        for r in futures::future::join_all(futs).await {
            partials.push(r?);
        }
    }
    partials.retain(|p| !p.trim().is_empty());
    if partials.is_empty() {
        return Ok(Vec::new());
    }

    // Tier 2: reduce the partials into the whole-document summary. When they fit one
    // window, a single final reduce → 1 segment; otherwise group into up to 3
    // ordered segments so an enormous instrument (Constitution, Compendium) keeps
    // breadth without overflowing the reduce call.
    let joined = partials.join("\n\n");
    if joined.len() <= DOC_FULL_INPUT_CHARS {
        let s = summarize_document_full(model_slot, tier, &joined, 1, 1).await?;
        let mut segs = vec![s];
        segs.extend(table_seg);
        return Ok(finalize_segments(segs));
    }
    let n_segments = 3usize;
    let per = partials.len().div_ceil(n_segments);
    let mut segments = Vec::new();
    for (i, group) in partials.chunks(per).enumerate() {
        let s = summarize_document_full(model_slot, tier, &group.join("\n\n"), i + 1, n_segments).await?;
        segments.push(s);
    }
    segments.extend(table_seg);
    Ok(finalize_segments(segments))
}

/// Feature 024 (graph-guided, table increment) — build an extra whole-document
/// summary segment that ENUMERATES the document's tabular content, so a
/// "summarize <doc>" answer conveys what a flat prose reduce abstracts away (e.g.
/// the RoB Schedule-I department list). Reads the `table_row` units captured by
/// feature 023.
///
/// The entry names are extracted DETERMINISTICALLY from the row cells (verbatim,
/// deduped) and the sentence is assembled in code — NO LLM. An earlier version
/// asked the model to "list the entries by name", which let it CONFABULATE
/// entries that were never in the table (a live summary invented "Department
/// Zoonology" from a "zooids"/"Zoological" cell and padded the list with
/// "Department Z"). Emitting cell text verbatim removes that fabrication surface
/// entirely, and also removes the chain-of-thought leak risk of the extra model
/// call. Returns `None` when the document has no nameable table rows.
async fn build_table_segment(pool: &Db, source_id: &str) -> Result<Option<String>> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT content FROM chunks \
         WHERE source_id = $1 AND unit_type = 'table_row' \
         ORDER BY chunk_index",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await
    .context("load table_row units")?;
    if rows.is_empty() {
        return Ok(None);
    }
    // Extract the name column verbatim from each row, dedup preserving order.
    let mut names: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for row in &rows {
        if let Some(name) = table_row_name(row) {
            let key = name.to_lowercase();
            if seen.insert(key) {
                names.push(name);
            }
        }
    }
    if names.is_empty() {
        return Ok(None);
    }
    let listed: String = names.join("; ").chars().take(DOC_FULL_INPUT_CHARS).collect();
    let segment = format!(
        "The document's schedules and tables list {} distinct named entries, including: {}.",
        names.len(),
        listed
    );
    Ok(Some(segment))
}

/// Label of the header row / non-entries that must never appear as a named entry.
const TABLE_ROW_NON_ENTRIES: &[&str] = &["secretariat department", "s. no.", "s.no."];
/// Prefixes that mark an amendment/editorial-note row (not a real entry) — the row
/// is dropped when its name starts with one of these.
const NON_ENTRY_PREFIXES: &[&str] = &[
    "substituted", "corrigendum", "inserted", "deleted", "added", "notification",
    "vide notification", "amended", "omitted", "col",
];
/// Editorial-note markers that begin an amendment tail inside a name cell; the name
/// ends where one starts (case-insensitive match on the lowercased cell).
const AMENDMENT_MARKERS: &[&str] = &[
    " (substituted",
    " (inserted",
    " (added",
    " substituted vide",
    " inserted vide",
    " added vide",
    " deleted vide",
    " vide notification",
];

/// Extract an entry's NAME from a captured `table_row`'s stored content, verbatim
/// from the cell — never synthesised. Row content is the ` | `-delimited cells from
/// `render_row_with_header`; for the schedules we target (RoB Schedule-I etc.)
/// column 0 is the serial number and column 1 carries the entry name. Returns
/// `None` for the header row, amendment-only cells, and rows with no name column.
///
/// `pub(crate)` so the F025 enumeration path (`deep_search`) reuses the SAME
/// verbatim extraction — asking an LLM to "list the entries" confabulates
/// (live: "Ministry of Zoonosis Control"), this cannot.
pub(crate) fn table_row_name(content: &str) -> Option<String> {
    let cell = content.split(" | ").nth(1)?.trim();
    // Drop any header-label prefix render_row_with_header may prepend
    // ("Secretariat Department: <name>") and any cross-page cell bleed
    // ("<prev cell>.: <name>") — keep the text after the last ": ".
    let cell = cell.rsplit(": ").next().unwrap_or(cell).trim();
    // Cut an amendment/editorial tail so "<Name> (Substituted vide …)" → "<Name>".
    let lower = cell.to_lowercase();
    let end = AMENDMENT_MARKERS
        .iter()
        .filter_map(|m| lower.find(m))
        .min()
        .unwrap_or(cell.len());
    let name = cell[..end].trim().trim_end_matches(['.', ',', ';', ':']).trim();
    if name.is_empty() {
        return None;
    }
    let key = name.to_lowercase();
    if TABLE_ROW_NON_ENTRIES.iter().any(|h| key == *h) {
        return None;
    }
    // Drop amendment/editorial-note rows ("Substituted…", "Corrigendum…") and
    // synthesized column headers ("col2") — they are not real entries. `col` only
    // rejects the `col\d+` header form, not a real word starting "col".
    if NON_ENTRY_PREFIXES.iter().any(|p| {
        if *p == "col" {
            key.starts_with("col") && key[3..].chars().next().is_some_and(|c| c.is_ascii_digit())
        } else {
            key.starts_with(p)
        }
    }) {
        return None;
    }
    // Require at least TWO letters so pure serials ("7-A") and punctuation are
    // dropped (real entry names are multi-word).
    if name.chars().filter(|c| c.is_alphabetic()).count() < 2 {
        return None;
    }
    Some(name.to_string())
}

/// Clean the generated whole-document segments before storage: apply the SAME
/// near-duplicate dedup used by the query-time presentation pass
/// (`summarize_doc::dedupe_repeats`) so the STORED summary has no reworded tail
/// echoes, then drop any segment that became empty. Item-3 fix: previously the
/// stored `document_full` kept a repetition that only the serve-time pass hid.
fn finalize_segments(segments: Vec<String>) -> Vec<String> {
    use crate::services::assistant::capabilities::summarize_doc::dedupe_repeats;
    segments
        .into_iter()
        .map(|s| dedupe_repeats(&s))
        .filter(|s| !s.trim().is_empty())
        .collect()
}

/// Load a source's substantive text in reading order for the content-based
/// whole-document summary: every non-boilerplate chunk EXCEPT page chrome and
/// amendment-footnote lines, each with its repeated unit-label prefix stripped.
/// STRUCTURE-TOLERANT — it depends on the chunk TEXT, not on the parser having
/// labelled provisions correctly.
async fn load_content_for_summary(pool: &Db, source_id: &str) -> Result<Vec<String>> {
    // Feature 023: include the whole-table chunk (`unit_type='table'`, compact
    // markdown of the entire table — so the summary reflects table content) but
    // EXCLUDE the per-row `unit_type='table_row'` chunks, which would duplicate the
    // same rows the whole-table chunk already carries. Per-row chunks exist only
    // for targeted retrieval.
    let rows: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT unit_label, content FROM chunks \
         WHERE source_id = $1 AND is_boilerplate = false \
           AND unit_type != 'table_row' \
           AND (block_class IS NULL OR block_class NOT IN ('chrome', 'footnote')) \
         ORDER BY chunk_index",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await
    .context("load content chunks")?;
    Ok(rows
        .into_iter()
        .map(|(label, content)| strip_label_prefix(&content, label.as_deref()))
        .filter(|s| !s.trim().is_empty())
        .collect())
}

/// Drop a chunk's stored label-prefix line (`"Rule 11\n…"`) so the repeated
/// citation labels don't bias the summariser toward numbering. Returns the content
/// unchanged when it does not start with the label.
fn strip_label_prefix(content: &str, label: Option<&str>) -> String {
    if let Some(lbl) = label {
        if !lbl.is_empty() {
            if let Some(rest) = content.strip_prefix(lbl) {
                return rest.trim_start_matches(['\n', '\r', ' ']).to_string();
            }
        }
    }
    content.to_string()
}

/// Greedily pack ordered text pieces into windows of at most `max_chars`. A single
/// oversized piece is truncated into its own window (keeps every reduce call
/// bounded); a piece never straddles two windows.
fn pack_windows(pieces: &[String], max_chars: usize) -> Vec<String> {
    let mut windows: Vec<String> = Vec::new();
    let mut cur = String::new();
    for p in pieces {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if p.len() > max_chars {
            if !cur.is_empty() {
                windows.push(std::mem::take(&mut cur));
            }
            windows.push(p.chars().take(max_chars).collect());
            continue;
        }
        if !cur.is_empty() && cur.len() + 1 + p.len() > max_chars {
            windows.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(p);
    }
    if !cur.is_empty() {
        windows.push(cur);
    }
    windows
}

/// Insert one presentational whole-document summary segment (`level='document_full'`).
/// Unlike [`insert_summary`], it writes NO `summary_embeddings` row — these rows are
/// served by named-document match, never by vector recall. `seg` is the 1-based
/// ordering key, stored in `unit_path` so segments concatenate in reading order.
async fn insert_document_full(
    pool: &Db,
    source_id: &str,
    chunk_id: &str,
    seg: usize,
    summary_text: &str,
    now: i64,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO graph_summary \
         (id, source_id, chunk_id, citation, unit_path, level, summary_text, created_at) \
         VALUES ($1,$2,$3,NULL,$4,'document_full',$5,$6)",
    )
    .bind(&id)
    .bind(source_id)
    .bind(chunk_id)
    .bind(format!("{seg:04}"))
    .bind(summary_text)
    .bind(now)
    .execute(pool)
    .await
    .context("insert document_full summary")?;
    Ok(())
}

/// Line/sentence-initial markers that signal the model has stopped answering and
/// started leaking its own chain-of-thought or echoing the prompt scaffold.
/// Everything from the first such marker onward is discarded. Lowercased; matched
/// only at a line/sentence boundary so a legitimate mid-sentence word is never
/// clipped. (Root cause: the vLLM model emits reasoning as PLAIN TEXT — not
/// `<think>` tags — despite NOVIQ_NO_THINK, so tag-stripping alone missed it and
/// the leaked reasoning was polluting every summary. Proven in the 2026-07-11
/// summarize eval.)
const LEAK_MARKERS: &[&str] = &[
    "okay, let me", "okay, let's", "okay, so", "okay, i ", "okay, the ", "okay,let", "okay, first",
    "let's tackle", "let's start", "let's see", "let me try to figure", "let me start",
    "let me check", "let me think", "let me re",
    "the user wants", "the user provided", "the user is asking", "the user has given",
    "the user gave", "the user's",
    "looking at the", "looking back",
    "first, i need", "first, let me",
    "so, the task", "so, to create", "so, i need", "so, applying", "so, for the",
    "i need to make sure", "i should ", "i'll ", "i will now", "my task",
    "wait, ", "hmm,",
    "provision:", "what it governs:", "what the document governs:",
    "the sample summary", "the example response", "the previous summary",
    "the provided list", "the given topic notes",
];

/// Sanitise a raw generation into a clean summary: strip any `<think>` block, cut
/// at the first leaked-reasoning / prompt-echo marker, and drop repeated sentences
/// (the small model degenerates into loops at higher token counts). Returns "" if
/// nothing usable survives.
fn clean_gist(s: String) -> String {
    // 1) strip a leading <think>…</think> reasoning block; an UNCLOSED leading
    //    <think> means the whole output is reasoning → nothing usable.
    let body: &str = if let Some(end) = s.find("</think>") {
        &s[end + "</think>".len()..]
    } else if s.trim_start().starts_with("<think>") {
        return String::new();
    } else {
        &s[..]
    };

    // 2) cut at the earliest leaked-reasoning / echo marker that sits at a
    //    line/sentence boundary (keeps the real answer that precedes it).
    let lower = body.to_ascii_lowercase();
    let mut cut = body.len();
    for m in LEAK_MARKERS {
        let mut from = 0usize;
        while let Some(rel) = lower[from..].find(m) {
            let idx = from + rel;
            let pre = &lower[..idx];
            let boundary = pre.trim_end().is_empty()
                || pre.ends_with('\n')
                || pre.ends_with(". ")
                || pre.ends_with(".\n")
                || pre.ends_with("? ")
                || pre.ends_with("! ");
            if boundary {
                cut = cut.min(idx);
                break;
            }
            from = idx + m.len();
        }
    }
    let body = &body[..cut];

    // 3) drop sentences that repeat one seen in the last few (loop guard).
    dedupe_sentences(body).trim().to_string()
}

/// Remove near-duplicate sentences within a short sliding window — kills the
/// "The Act also outlines … The Act also establishes …" repetition loops a small
/// model falls into. Comparison is on an alphanumeric-normalised form so
/// punctuation/spacing differences don't defeat it.
fn dedupe_sentences(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut recent: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    for part in s.split_inclusive(['.', '\n']) {
        let norm: String = part
            .to_ascii_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();
        if norm.len() > 15 && recent.contains(&norm) {
            continue; // skip a repeat of a recent sentence
        }
        out.push_str(part);
        if norm.len() > 15 {
            recent.push_back(norm);
            if recent.len() > 6 {
                recent.pop_front();
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
async fn insert_summary(
    pool: &Db,
    source_id: &str,
    chunk_id: &str,
    citation: Option<&str>,
    unit_path: Option<&str>,
    level: &str,
    summary_text: &str,
    embedding: &[f32],
    now: i64,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO graph_summary \
         (id, source_id, chunk_id, citation, unit_path, level, summary_text, created_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(&id)
    .bind(source_id)
    .bind(chunk_id)
    .bind(citation)
    .bind(unit_path)
    .bind(level)
    .bind(summary_text)
    .bind(now)
    .execute(pool)
    .await
    .context("insert graph_summary")?;

    let json = serde_json::to_string(embedding).context("serialize summary embedding")?;
    sqlx::query("INSERT INTO summary_embeddings (summary_id, embedding) VALUES ($1, $2::vector)")
        .bind(&id)
        .bind(&json)
        .execute(pool)
        .await
        .context("insert summary embedding")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_gist_strips_think_block_and_trims() {
        assert_eq!(clean_gist("<think>\nreasoning\n</think>\n\nGoverns appeals.".into()), "Governs appeals.");
        assert_eq!(clean_gist("  Plain gist.  ".into()), "Plain gist.");
        assert_eq!(clean_gist("No think here.".into()), "No think here.");
    }

    #[test]
    fn clean_gist_cuts_leaked_reasoning_keeps_prefix() {
        // The real answer, then the model rambling — keep only the answer.
        let raw = "The Ordinance governs removal from service and the right of appeal. \
                   Okay, let me try to figure out what the user is asking for.".to_string();
        assert_eq!(
            clean_gist(raw),
            "The Ordinance governs removal from service and the right of appeal."
        );
        // Prompt-scaffold echo on its own line is cut.
        let raw2 = "Governs penalties.\n\nPROVISION:\nRule 8\nWHAT IT GOVERNS: appeals.".to_string();
        assert_eq!(clean_gist(raw2), "Governs penalties.");
        // A marker word mid-sentence must NOT be clipped (no boundary before it).
        let raw3 = "The board may check compliance and impose penalties.".to_string();
        assert_eq!(clean_gist(raw3), "The board may check compliance and impose penalties.");
        // Feature 024: the "Okay, let's tackle this query." leak seen on the RoB
        // table segment must be stripped (was missing from LEAK_MARKERS).
        let raw4 = "The schedules list Finance Department and Home Department. \
                    Okay, let's tackle this query.".to_string();
        assert_eq!(
            clean_gist(raw4),
            "The schedules list Finance Department and Home Department."
        );
    }

    #[test]
    fn table_row_name_extracts_department_verbatim() {
        // Real RoB Schedule-I rows (` | `-delimited cells; col1 = the department).
        assert_eq!(
            table_row_name(
                "SCHEDULE-I (See Rule – 3 (i): 1 | Agriculture, Supply & Prices Department | (a) Agriculture Engineering"
            ),
            Some("Agriculture, Supply & Prices Department".to_string())
        );
        // Header-label prefix on the name cell is dropped.
        assert_eq!(
            table_row_name("S. NO.: 6 | Secretariat Department: Cooperative Department | heads"),
            Some("Cooperative Department".to_string())
        );
        // Cross-page cell bleed: keep the text after the last ": ".
        assert_eq!(
            table_row_name(
                "7.: 8 | Culture, Tourism , Antiquities & Archives Department (Substituted vide … 02.01.2020).: Energy Department | (a) …"
            ),
            Some("Energy Department".to_string())
        );
        // Amendment tail is cut off the name.
        assert_eq!(
            table_row_name(
                "S. NO.: 5 | College Education Department Inserted vide Notification No. SORI dated 06.10.2016 | i. Directorate"
            ),
            Some("College Education Department".to_string())
        );
    }

    #[test]
    fn table_row_name_rejects_header_and_non_entries() {
        // The header row itself must not become an "entry".
        assert_eq!(
            table_row_name(
                "SCHEDULE-I (See Rule – 3 (i): S. NO. | Secretariat Department | Attached Department | Head"
            ),
            None
        );
        // A serial-only / punctuation-only name column yields nothing.
        assert_eq!(table_row_name("row | 7-A | | "), None);
        // A single-cell row has no name column.
        assert_eq!(table_row_name("Agriculture Department"), None);
        // Regression: the source "zooids"/"Zoological" cell text is a FUNCTION line,
        // never a department — verbatim extraction can only ever surface real cell
        // text, so the invented "Department Zoonology"/"Department Z" cannot recur.
        assert_eq!(
            table_row_name(
                "S. NO.: 12 | Livestock & Fisheries Department | (a) All matters relating to animal husbandry, zooids, cruelty to animals"
            ),
            Some("Livestock & Fisheries Department".to_string())
        );
        // F025 enumeration cleanup: synthesized column headers and amendment/
        // editorial-note rows must not be counted as entries.
        assert_eq!(table_row_name("row | col2 | header"), None);
        assert_eq!(table_row_name("row | Substituted | note"), None);
        assert_eq!(table_row_name("row | Corrigendum No. SORI(S&GAD)2-4/2001 | note"), None);
        // But a real name that merely CONTAINS such a word is kept.
        assert_eq!(
            table_row_name("S. NO.: 5 | Colleges & Higher Education Department | heads"),
            Some("Colleges & Higher Education Department".to_string())
        );
    }

    #[test]
    fn strip_label_prefix_drops_leading_citation() {
        assert_eq!(strip_label_prefix("Rule 11\n11. Repeal.- text", Some("Rule 11")), "11. Repeal.- text");
        // no prefix match → unchanged
        assert_eq!(strip_label_prefix("Some content", Some("Rule 9")), "Some content");
        // no label → unchanged
        assert_eq!(strip_label_prefix("Plain content", None), "Plain content");
        // empty label → unchanged
        assert_eq!(strip_label_prefix("Plain content", Some("")), "Plain content");
    }

    #[test]
    fn pack_windows_bounds_and_never_straddles() {
        let pieces = vec!["aaaa".to_string(), "bbbb".to_string(), "cccc".to_string()];
        // window of 9 fits "aaaa\nbbbb" (9), then "cccc".
        let w = pack_windows(&pieces, 9);
        assert_eq!(w, vec!["aaaa\nbbbb".to_string(), "cccc".to_string()]);
        // every window is within the cap
        assert!(w.iter().all(|s| s.len() <= 9));
    }

    #[test]
    fn pack_windows_truncates_oversized_piece() {
        let pieces = vec!["x".repeat(30)];
        let w = pack_windows(&pieces, 10);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].len(), 10);
    }

    #[test]
    fn pack_windows_single_when_all_fit() {
        let pieces = vec!["one".to_string(), "two".to_string()];
        assert_eq!(pack_windows(&pieces, 100), vec!["one\ntwo".to_string()]);
    }

    #[test]
    fn dedupe_sentences_kills_repetition_loop() {
        let looped = "The Act sets up the authority. The Act sets up the authority. \
                      The Act sets up the authority.";
        let out = dedupe_sentences(looped);
        assert_eq!(out.matches("The Act sets up the authority").count(), 1, "{out}");
    }
}
