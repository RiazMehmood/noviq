//! Entity extractor — two-pass NER — T069 (US4).
//!
//! Pass 1 (synchronous, during indexing): regex patterns for dates and amounts.
//! Pass 2 (async background, post-indexing): LLM prompt with structured JSON grammar
//!   for universal entity types: person, org, location, concept, term, reference, date, amount.

use anyhow::Result;
use regex_lite::Regex;
use crate::db::Db;
use std::sync::OnceLock;

use crate::db::repositories::entities as entity_repo;

// ─── Cached compiled regexes ─────────────────────────────────────────────────
// Compiled once on first use; avoids re-compilation for every chunk.

fn date_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{4}-\d{2}-\d{2})\b").unwrap())
}

fn date_re2() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2}[/-]\d{1,2}[/-]\d{2,4})\b").unwrap())
}

fn amount_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[\$£€][\d,]+(?:\.\d{1,2})?").unwrap())
}

/// Matches "Article 8", "Section 3(2)", "Clause 12A", "Schedule IV", etc.
fn article_section_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(Article|Section|Clause|Schedule|Part|Annex|Appendix|Rule|Order|Sub-section)\s+\d+[\w\(\)]*\b"
        ).unwrap()
    })
}

// ─── Pass 1: synchronous regex extraction ────────────────────────────────────

/// Extract date and amount entities from a chunk.
/// Called from a background task spawned by the indexer pipeline.
pub async fn extract_pass1(pool: &Db, chunk_id: &str, text: &str) -> Result<()> {
    // ISO dates: 2024-01-15
    for cap in date_re().captures_iter(text) {
        let value = &cap[1];
        let entity = entity_repo::upsert(pool, value, "date", Some(value)).await?;
        entity_repo::insert_mention(pool, &entity.id, chunk_id, None).await?;
        entity_repo::increment_source_count(pool, &entity.id).await?;
    }

    // dd/mm/yyyy or mm/dd/yyyy dates
    for cap in date_re2().captures_iter(text) {
        let entity = entity_repo::upsert(pool, &cap[1], "date", None).await?;
        entity_repo::insert_mention(pool, &entity.id, chunk_id, None).await?;
        entity_repo::increment_source_count(pool, &entity.id).await?;
    }

    // Monetary amounts: $1,234 or $1,234.56 or £/€ variants
    for cap in amount_re().captures_iter(text) {
        let entity = entity_repo::upsert(pool, &cap[0], "amount", None).await?;
        entity_repo::insert_mention(pool, &entity.id, chunk_id, None).await?;
        entity_repo::increment_source_count(pool, &entity.id).await?;
    }

    // Structural references: "Article 8", "Section 3(2)", "Clause 12A", etc.
    for cap in article_section_re().captures_iter(text) {
        let value = &cap[0];
        let entity = entity_repo::upsert(pool, value, "reference", Some(value)).await?;
        entity_repo::insert_mention(pool, &entity.id, chunk_id, None).await?;
        entity_repo::increment_source_count(pool, &entity.id).await?;
    }

    Ok(())
}

// ─── Pass 2: LLM-based NER ───────────────────────────────────────────────────

/// Prompt template for extracting entities AND relationships.
///
/// Uses JSONL (one JSON object per line) instead of a nested JSON object —
/// small instruction-tuned models like qwen2.5-3b frequently emit structurally
/// broken nested JSON (duplicate keys, unclosed arrays). JSONL lets us parse
/// each line independently so one glitch doesn't lose the whole extraction.
const PASS2_PROMPT: &str = r#"Extract named entities and relationships from TEXT. Output one JSON object per line. No markdown, no explanation.

Entity examples:
{"kind":"entity","type":"person","value":"Ahmed Khan"}
{"kind":"entity","type":"org","value":"Federal Board of Revenue"}
{"kind":"entity","type":"location","value":"Karachi"}
{"kind":"entity","type":"reference","value":"Article 25"}
{"kind":"entity","type":"concept","value":"fundamental rights"}
{"kind":"entity","type":"term","value":"habeas corpus"}
{"kind":"entity","type":"date","value":"1973"}
{"kind":"entity","type":"amount","value":"PKR 500,000"}

Relationship examples:
{"kind":"rel","from":"Ahmed Khan","to":"Federal Board of Revenue","type":"employed_by"}
{"kind":"rel","from":"Article 25","to":"fundamental rights","type":"defines"}

Rules:
- type must be exactly one of: person, org, location, date, amount, concept, term, reference
- Emit all entity lines BEFORE any rel line that names them
- rel types (pick closest): {RELATION_TYPES}
- Extract entities from the text — do not invent entities not present
- If nothing to extract: {"kind":"none"}
- At most 10 entities and 10 rels

TEXT:
{TEXT}

OUTPUT:
"#;

/// Run LLM-based entity extraction on a chunk.
/// Submitted as TaskBudget priority 2 (below user queries, above idle).
pub async fn extract_pass2(
    pool: &Db,
    chunk_id: &str,
    text: &str,
    model_slot: &std::sync::Arc<crate::services::llm::ModelSlot>,
    model_path: &str,
    tier: &crate::services::hardware_detector::HardwareTier,
    relation_types: &[String],
    app: Option<crate::compat::AppHandle>,
) -> Result<()> {
    // Cloud routing (Groq) takes precedence — must be checked BEFORE the
    // local-model early-return, otherwise enabling Groq with no local model
    // silently skips every chunk and KG never builds. (CHANGELOG_DEV.md 2026-05-03)
    let cloud_settings = match app.as_ref() {
        Some(a) => crate::services::cloud_llm::load_settings(a),
        None => crate::services::cloud_llm::load_settings_cached(),
    };
    let cloud_active = cloud_settings.enabled && !cloud_settings.api_key.trim().is_empty();
    if model_path.is_empty() && !cloud_active {
        log::debug!("Pass 2 skipped — no LLM (local or cloud) configured (chunk {chunk_id})");
        return Ok(());
    }

    // Truncate text to ~256 tokens — 1024 chars keeps more sentence context while
    // still fitting the combined prompt well within the 2048 n_ctx budget.
    let text_trunc: String = text.chars().take(1024).collect();
    log::info!("Pass2 NER chunk={chunk_id} text_preview={:?}", text_trunc.chars().take(120).collect::<String>());

    // Cap the prompt vocabulary at 20 verbs so small instruction-tuned models
    // stay focused — longer lists noticeably hurt recall on qwen2.5-3b.
    let verbs: Vec<&str> = relation_types
        .iter()
        .take(20)
        .map(|s| s.as_str())
        .collect();
    let verbs_joined = if verbs.is_empty() {
        "works_at, part_of, mentions".to_string()
    } else {
        verbs.join(", ")
    };

    let inner = PASS2_PROMPT
        .replace("{RELATION_TYPES}", &verbs_joined)
        .replace("{TEXT}", &text_trunc);
    // Qwen2.5-Instruct requires ChatML format — without it the model echoes input.
    let prompt = format!(
        "<|im_start|>system\nYou are a named entity extractor. Follow the output format exactly.<|im_end|>\n\
         <|im_start|>user\n{inner}<|im_end|>\n\
         <|im_start|>assistant\n"
    );

    // Cloud routing (Groq) when enabled — see CHANGELOG_DEV.md.
    let cloud_text: Option<String> = if cloud_active {
        match crate::services::cloud_llm::groq_complete_blocking(
            &cloud_settings,
            "You are a named entity extractor. Follow the output format exactly. Output JSONL only.",
            &inner,
            600,
        ) {
            Ok(t) if !t.trim().is_empty() => Some(t),
            Ok(_) => { log::warn!("Groq NER empty for chunk {chunk_id}; falling back"); None }
            Err(e) => { log::warn!("Groq NER failed for chunk {chunk_id} ({e}); falling back"); None }
        }
    } else { None };
    let raw = if let Some(t) = cloud_text {
        t
    } else if !model_path.is_empty() {
        // infer() is a synchronous blocking call — run it on a blocking thread pool thread
        // so the tokio event loop stays responsive. To bound runtime on Urdu/Sindhi PDFs
        // that cause near-infinite token loops, we use an abort-flag timer rather than
        // tokio::time::timeout on the JoinHandle. Dropping a JoinHandle only abandons
        // the async future; the blocking thread keeps running and holds the ModelSlot
        // mutex, deadlocking every subsequent NER call. The abort-timer approach instead
        // calls request_abort() which sets an AtomicBool checked inside infer()'s decode
        // loop — the loop sees it on the next token (~1s), breaks, and releases the mutex
        // cleanly. See llm.rs ModelSlot::request_abort / ModelSlot::infer.
        let slot = std::sync::Arc::clone(model_slot);
        let slot_for_abort = std::sync::Arc::clone(model_slot);
        let mp = model_path.to_string();
        let pr = prompt.clone();
        let ti = tier.clone();
        let chunk_id_owned = chunk_id.to_string();

        let infer_handle = tokio::task::spawn_blocking(move || slot.infer(&mp, &pr, 150, &ti));

        // After 90s, signal the decode loop to stop — it will break and release the mutex.
        let abort_handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(90)).await;
            log::warn!("Pass 2 NER 90s abort signal sent for chunk {chunk_id_owned}");
            slot_for_abort.request_abort();
        });

        match infer_handle.await {
            Ok(Ok(r)) => {
                abort_handle.abort(); // inference completed — cancel the abort timer
                r
            }
            Ok(Err(e)) => {
                abort_handle.abort();
                log::warn!("Pass 2 LLM inference failed for chunk {chunk_id}: {e}");
                return Ok(());
            }
            Err(e) => {
                abort_handle.abort();
                log::warn!("Pass 2 spawn_blocking panicked for chunk {chunk_id}: {e}");
                return Ok(());
            }
        }
    } else {
        log::warn!("Pass 2 skipped after cloud failure — no local model fallback (chunk {chunk_id})");
        return Ok(());
    };
    log::info!("Pass2 NER chunk={chunk_id} raw_output={:?}", raw.chars().take(300).collect::<String>());

    // ── Parse JSONL output (one JSON object per line) ─────────────────────────
    // Phase 1: collect all entities into a value→id map so relationship lines
    // (which may interleave with entity lines in practice) can resolve their
    // endpoints. Phase 2: insert relationships.
    let mut value_to_id: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut entity_count = 0usize;
    let mut rel_count = 0usize;

    // Collect pending relationship tuples (from, to, type) for phase 2.
    let mut pending_rels: Vec<(String, String, String)> = Vec::new();
    let mut parse_failures = 0usize;

    for line in raw.lines() {
        let line = line.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
        if line.is_empty() { continue; }
        // Skip anything that isn't obviously a JSON object.
        if !line.starts_with('{') || !line.ends_with('}') { continue; }

        let obj: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                // Try repair fallback for bare keys / trailing commas / single quotes.
                match serde_json::from_str(&repair_json(line)) {
                    Ok(v) => v,
                    Err(_) => { parse_failures += 1; continue; }
                }
            }
        };

        let kind = obj["kind"].as_str().unwrap_or("").trim().to_lowercase();

        // Normalise entity shape — accept both the canonical form
        //   {"kind":"entity","type":"person","value":"Sarah Chen"}
        // and the common model-confused shortcut form
        //   {"kind":"person","value":"Sarah Chen"}   (kind IS the type)
        // also tolerate the field swap {"kind":"project","type":"Project Alpha","value":""}
        let maybe_entity: Option<(&str, &str)> = match kind.as_str() {
            "entity" => {
                let etype = obj["type"].as_str().unwrap_or("").trim();
                let evalue = obj["value"].as_str().unwrap_or("").trim();
                if !evalue.is_empty()
                    && ["person", "org", "location", "concept", "term", "reference", "date", "amount"].contains(&etype)
                {
                    Some((etype, evalue))
                } else { None }
            }
            "person" | "org" | "location" | "concept" | "term" | "reference" | "date" | "amount" => {
                let etype = kind.as_str();
                // Prefer "value"; if empty, fall back to "type" (some outputs
                // swap the fields).
                let v = obj["value"].as_str().unwrap_or("").trim();
                let v = if v.is_empty() {
                    obj["type"].as_str().unwrap_or("").trim()
                } else { v };
                if !v.is_empty() { Some((etype, v)) } else { None }
            }
            _ => None,
        };

        if let Some((etype, evalue)) = maybe_entity {
            if entity_count < 10 {
                let entity = entity_repo::upsert(pool, evalue, etype, None).await?;
                entity_repo::insert_mention(pool, &entity.id, chunk_id, Some(evalue)).await?;
                entity_repo::increment_source_count(pool, &entity.id).await?;
                value_to_id
                    .entry(evalue.to_lowercase())
                    .or_insert_with(|| entity.id.clone());
                entity_count += 1;
            }
            continue;
        }

        // Relationship shape.
        if kind == "rel" || kind == "relationship" {
            let (Some(from_v), Some(to_v), Some(rel_t)) = (
                obj["from"].as_str(),
                obj["to"].as_str(),
                obj["type"].as_str(),
            ) else { continue };
            let from_v = from_v.trim();
            let to_v = to_v.trim();
            let rel_t = rel_t.trim();
            // Skip the malformed "decision-shaped rel" with an empty endpoint.
            if from_v.is_empty() || to_v.is_empty() || rel_t.is_empty() { continue; }
            if pending_rels.len() >= 10 { continue; }
            pending_rels.push((
                from_v.to_string(),
                to_v.to_string(),
                rel_t.to_lowercase().replace(' ', "_"),
            ));
            continue;
        }

        // "none" sentinel and anything else — ignore.
    }

    // Phase 2 — insert relationships, resolving endpoints via the entity map.
    // If an endpoint isn't already an entity (the LLM frequently names entities
    // in rels that it forgot to emit as entity lines), auto-create it with
    // type=org as a safe default so the edge isn't lost.
    for (from_v, to_v, rel_type) in pending_rels {
        let from_key = from_v.to_lowercase();
        let to_key   = to_v.to_lowercase();
        if from_key.is_empty() || to_key.is_empty() || from_key == to_key || rel_type.is_empty() {
            continue;
        }

        let from_id = match value_to_id.get(&from_key).cloned() {
            Some(id) => id,
            None => {
                let ent = entity_repo::upsert(pool, &from_v, "org", None).await?;
                entity_repo::insert_mention(pool, &ent.id, chunk_id, Some(&from_v)).await?;
                entity_repo::increment_source_count(pool, &ent.id).await?;
                value_to_id.insert(from_key.clone(), ent.id.clone());
                entity_count += 1;
                ent.id
            }
        };
        let to_id = match value_to_id.get(&to_key).cloned() {
            Some(id) => id,
            None => {
                let ent = entity_repo::upsert(pool, &to_v, "org", None).await?;
                entity_repo::insert_mention(pool, &ent.id, chunk_id, Some(&to_v)).await?;
                entity_repo::increment_source_count(pool, &ent.id).await?;
                value_to_id.insert(to_key.clone(), ent.id.clone());
                entity_count += 1;
                ent.id
            }
        };

        match entity_repo::insert_relationship_if_new(
            pool, &from_id, &to_id, &rel_type, Some(chunk_id),
        ).await {
            Ok(true)  => rel_count += 1,
            Ok(false) => {} // duplicate — silently ignore
            Err(e)    => log::debug!(
                "relationship insert failed for chunk {chunk_id}: {e}"
            ),
        }
    }

    // Log raw output whenever relationships came back empty — most useful
    // diagnostic signal for tuning the prompt, since the LLM is already
    // producing entities but something about the rel-line format is off.
    if rel_count == 0 {
        log::warn!(
            "Pass 2 chunk {chunk_id}: {entity_count} entities, 0 relationships \
             ({parse_failures} unparseable lines). Raw LLM output follows:\n\
             ----- BEGIN RAW -----\n{raw}\n----- END RAW -----"
        );
    } else {
        log::info!(
            "Pass 2 extracted {} entities and {} relationships from chunk {chunk_id} \
             ({} unparseable lines skipped)",
            entity_count, rel_count, parse_failures
        );
    }
    Ok(())
}

/// Best-effort JSON cleanup for outputs from small instruction-tuned LLMs.
///
/// Handles three common defects we see from qwen2.5-3b:
///   1. Unquoted object keys:  `{ from: "X" }`  →  `{ "from": "X" }`
///   2. Trailing commas:        `[1,2,]` or `{"a":1,}` → `[1,2]` / `{"a":1}`
///   3. Single-quoted strings:  `{'from':'X'}`  →  `{"from":"X"}`
///
/// Intentionally conservative — only applied after a strict parse fails.
fn repair_json(s: &str) -> String {
    let mut out = s.to_string();

    // 1. Bare keys → quoted keys. Only match identifiers used as keys
    // (preceded by `{` or `,` and followed by `:`), so we don't corrupt
    // string values that happen to contain `word:`.
    if let Ok(re_keys) = regex_lite::Regex::new(
        r#"([\{,]\s*)([A-Za-z_][A-Za-z0-9_]*)(\s*:)"#,
    ) {
        out = re_keys.replace_all(&out, r#"$1"$2"$3"#).to_string();
    }

    // 2. Trailing commas before `}` or `]`.
    if let Ok(re_trail) = regex_lite::Regex::new(r#",(\s*[\}\]])"#) {
        out = re_trail.replace_all(&out, "$1").to_string();
    }

    // 3. Single-quoted → double-quoted string literals. Only convert quotes
    // paired on the same line (no escape handling needed for the shapes we
    // actually see from the model).
    if let Ok(re_sq) = regex_lite::Regex::new(r#"'([^'\n]*)'"#) {
        out = re_sq.replace_all(&out, r#""$1""#).to_string();
    }

    out
}

/// Find the first balanced JSON object `{...}` in a string.
#[allow(dead_code)]
fn extract_json(text: &str) -> &str {
    let start = match text.find('{') {
        Some(i) => i,
        None => return "{}",
    };
    let mut depth = 0i32;
    for (i, ch) in text[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[start..start + i + 1];
                }
            }
            _ => {}
        }
    }
    "{}"
}

// ─── Scheduler-aware Pass2 NER entry point ────────────────────────────────────

/// A chunk reference for batch pass2 processing.
pub struct ChunkRef {
    pub id: String,
    pub content: String,
}

/// Scheduler-aware Pass2 NER. Checks pause/cancel at each chunk boundary.
/// Returns immediately if the handle is pre-cancelled — no DB writes.
pub async fn run_pass2_ner(
    _pool: &Db,
    chunks: &[ChunkRef],
    handle: &std::sync::Arc<crate::services::scheduler::WorkloadHandle>,
) -> Result<()> {
    for (i, chunk) in chunks.iter().enumerate() {
        handle.should_pause().await;
        if handle.is_cancelled() {
            return Ok(());
        }

        let token = handle.cancellation_token();
        let prompt = format!("Extract named entities from: {}", &chunk.content);
        match crate::services::llm::generate_with_cancel(&prompt, 256, Some(token)).await {
            Ok(_output) => {
                // In production: parse output and call persist_entities
                // For now the cancel-boundary contract is what matters.
            }
            Err(crate::services::llm::LlmError::Cancelled) => return Ok(()),
            Err(e) => log::warn!("pass2 ner failed on chunk {}: {e}", chunk.id),
        }
        handle.report((i as u64) + 1, chunks.len() as u64);
    }
    Ok(())
}

#[cfg(test)]
mod scheduler_tests {
    use super::*;
    use crate::services::scheduler::{Scheduler, WorkloadKind};

    fn make_synthetic_chunks(n: usize) -> Vec<ChunkRef> {
        (0..n).map(|i| ChunkRef {
            id: format!("chunk-{i}"),
            content: format!("Synthetic content {i}"),
        }).collect()
    }

    #[tokio::test]
    async fn pass2_exits_on_precancelled_handle() {
        let pool = crate::db::test_pool().await;
        let sched = Scheduler::new(None);
        sched.set_foreground(WorkloadKind::Pass2Ner).await;
        let h = sched.register(WorkloadKind::Pass2Ner);
        h.cancel.cancel();
        let res = run_pass2_ner(&pool, &make_synthetic_chunks(5), &h).await;
        assert!(res.is_ok());
        let entities: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entities")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(entities, 0);
    }
}
