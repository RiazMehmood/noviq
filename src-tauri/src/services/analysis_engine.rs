//! Analysis engine — template runner for multi-source analysis.
//!
//! T060: 5 analysis templates, each builds a GBNF-constrained prompt,
//! routes to the best available model tier, and returns structured output.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db::Db;

use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::ModelSlot;
use crate::services::query_router::Citation;

/// All supported analysis templates.
pub const TEMPLATES: &[&str] = &[
    "compare_sources",
    "extract_action_items",
    "find_contradictions",
    "generate_timeline",
    "summarize_all",
];

/// Structured analysis result returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub template: String,
    pub title: String,
    pub output: String,
    pub citations: Vec<Citation>,
    /// True when a lower model tier was used due to hardware limits.
    pub quality_constrained: bool,
}

/// Run the requested analysis template against the notebook's indexed chunks.
///
/// Pipeline:
///   1. Pull top-50 chunks by FTS5 or full scan.
///   2. Build a template-specific system prompt + GBNF grammar.
///   3. Route to the best available model tier (honours HardwareTier).
///   4. Parse the structured JSON output.
///   5. Return AnalysisResult with citations and quality flag.
pub async fn run_template(
    pool: &Db,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    template: &str,
    app: Option<crate::compat::AppHandle>,
) -> Result<AnalysisResult> {
    // Validate template.
    if !TEMPLATES.contains(&template) {
        anyhow::bail!("unknown template: {template}");
    }

    // Retrieve top chunks for this notebook.
    let chunks = fetch_top_chunks(pool, notebook_id, 50).await?;

    if chunks.is_empty() {
        return Ok(AnalysisResult {
            template: template.to_string(),
            title: template_title(template),
            output: "No indexed content found in this notebook. Add sources and wait for indexing to complete.".to_string(),
            citations: vec![],
            quality_constrained: false,
        });
    }

    // Build context and prompt.
    let context = build_context(&chunks);
    let prompt = build_prompt(template, &context);

    // Select model based on hardware tier.
    let (model_path, quality_constrained) = select_model(tier, template);

    let output = run_inference(&app, model_slot, &model_path, &prompt, 1024, tier, &chunks, template);

    // Map chunks to citations.
    let citations = chunks
        .iter()
        .take(5)
        .map(|c| Citation {
            source_id: c.source_id.clone(),
            chunk_id: c.id.clone(),
            excerpt: c.content.chars().take(200).collect::<String>() + "…",
            page: c.page_number,
        })
        .collect();

    Ok(AnalysisResult {
        template: template.to_string(),
        title: template_title(template),
        output,
        citations,
        quality_constrained,
    })
}

pub async fn run_template_selected(
    pool: &Db,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    template: &str,
    source_ids: &[String],
    app: Option<crate::compat::AppHandle>,
) -> Result<AnalysisResult> {
    if !TEMPLATES.contains(&template) {
        anyhow::bail!("unknown template: {template}");
    }

    let chunks = fetch_top_chunks_selected(pool, notebook_id, source_ids, 50).await?;

    if chunks.is_empty() {
        return Ok(AnalysisResult {
            template: template.to_string(),
            title: template_title(template),
            output: "No indexed content found for the selected documents.".to_string(),
            citations: vec![],
            quality_constrained: false,
        });
    }

    let context = build_context(&chunks);
    let prompt = build_prompt(template, &context);
    let (model_path, quality_constrained) = select_model(tier, template);

    let output = run_inference(&app, model_slot, &model_path, &prompt, 1024, tier, &chunks, template);

    let citations = chunks
        .iter()
        .take(5)
        .map(|c| Citation {
            source_id: c.source_id.clone(),
            chunk_id: c.id.clone(),
            excerpt: c.content.chars().take(200).collect::<String>() + "…",
            page: c.page_number,
        })
        .collect();

    Ok(AnalysisResult {
        template: template.to_string(),
        title: template_title(template),
        output,
        citations,
        quality_constrained,
    })
}

// ─── Private helpers ──────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ChunkRow {
    id: String,
    source_id: String,
    content: String,
    page_number: Option<i64>,
}

async fn fetch_top_chunks(
    pool: &Db,
    notebook_id: &str,
    limit: i64,
) -> Result<Vec<ChunkRow>> {
    // Pull chunks from sources attached to the notebook.
    let rows: Vec<ChunkRow> = sqlx::query_as(
        r#"
        SELECT c.id, c.source_id, c.content, c.page_number
        FROM chunks c
        JOIN notebook_sources ns ON ns.source_id = c.source_id
        WHERE ns.notebook_id = $1
        ORDER BY c.created_at ASC
        LIMIT $2
        "#,
    )
    .bind(notebook_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

async fn fetch_top_chunks_selected(
    pool: &Db,
    notebook_id: &str,
    source_ids: &[String],
    limit: i64,
) -> Result<Vec<ChunkRow>> {
    if source_ids.is_empty() {
        return fetch_top_chunks(pool, notebook_id, limit).await;
    }
    let placeholders = crate::db::pg_in(source_ids.len(), 2);
    let p_limit = source_ids.len() + 2;
    let sql = format!(
        "SELECT c.id, c.source_id, c.content, c.page_number \
         FROM chunks c \
         JOIN notebook_sources ns ON ns.source_id = c.source_id \
         WHERE ns.notebook_id = $1 \
           AND c.source_id IN ({placeholders}) \
         ORDER BY RANDOM() \
         LIMIT ${p_limit}"
    );
    let mut q = sqlx::query_as::<_, ChunkRow>(&sql).bind(notebook_id);
    for id in source_ids {
        q = q.bind(id);
    }
    let rows = q.bind(limit).fetch_all(pool).await?;
    Ok(rows)
}

fn build_context(chunks: &[ChunkRow]) -> String {
    // Truncate each chunk to 400 chars to stay within the model's context window.
    // 50 chunks × 400 chars = 20k chars is still large; MAX_PROMPT_TOKENS in infer
    // will trim the final prompt, but truncating here keeps representative content.
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let excerpt: String = c.content.chars().take(400).collect();
            format!("[{}] {}", i + 1, excerpt)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn build_prompt(template: &str, context: &str) -> String {
    let instruction = match template {
        "compare_sources" => {
            "Compare the provided sources and identify key similarities, differences, \
             and any conflicting information. Structure your response as:\n\
             SIMILARITIES:\n- ...\nDIFFERENCES:\n- ...\nCONFLICTS:\n- ..."
        }
        "extract_action_items" => {
            "Extract all action items, tasks, and commitments from the provided sources. \
             Format each as: [OWNER/Team] Action description (Due: date if available)"
        }
        "find_contradictions" => {
            "Identify any contradictions, inconsistencies, or conflicting statements \
             across the provided sources. For each contradiction, cite the relevant passages."
        }
        "generate_timeline" => {
            "Extract all dates, events, and milestones mentioned in the provided sources \
             and arrange them in chronological order. Format as: YYYY-MM-DD | Event description"
        }
        "summarize_all" => {
            "Provide a comprehensive summary of all the provided sources. \
             Include the main themes, key findings, and important details."
        }
        _ => "Analyze the following content and provide a structured response:",
    };

    // Fix (2026-05-03): explicit anti-repetition + cite-once rules to prevent
    // small models from copy-pasting the same sentence across the answer.
    format!(
        "You are an expert analyst. {instruction}\n\n\
         Critical rules:\n\
         - Use ONLY information from the SOURCES below.\n\
         - Each point must appear ONCE — never repeat the same sentence or paragraph.\n\
         - Cite supporting passages by their bracketed number, e.g. [3].\n\
         - If the sources do not contain enough information for a section, write \"Not found in sources.\"\n\
         - Stop when you have covered every distinct point — do not pad.\n\n\
         SOURCES:\n{context}\n\n\
         ANALYSIS:"
    )
}

/// Run inference for analysis: cloud (Groq) when enabled, else local llama.cpp,
/// else regex fallback. Added 2026-05-03 — see CHANGELOG_DEV.md.
fn run_inference(
    app: &Option<crate::compat::AppHandle>,
    model_slot: &ModelSlot,
    model_path: &str,
    prompt: &str,
    max_tokens: u32,
    tier: &HardwareTier,
    chunks: &[ChunkRow],
    template: &str,
) -> String {
    if let Some(a) = app {
        let s = crate::services::cloud_llm::load_settings(a);
        if s.enabled && !s.api_key.trim().is_empty() {
            match crate::services::cloud_llm::groq_complete_blocking(
                &s,
                "You are an expert document analyst. Follow the user's structure exactly and never repeat sentences.",
                prompt,
                max_tokens,
            ) {
                Ok(text) if !text.trim().is_empty() => return text,
                Ok(_) => log::warn!("Groq returned empty for analysis; falling back to local"),
                Err(e) => log::warn!("Groq analysis failed ({e}); falling back to local"),
            }
        }
    }
    if !model_path.is_empty() {
        model_slot
            .infer(model_path, prompt, max_tokens, tier)
            .unwrap_or_else(|e| format!("LLM inference failed: {e}"))
    } else {
        extract_without_llm(template, chunks)
    }
}

fn select_model(tier: &HardwareTier, _template: &str) -> (String, bool) {
    // Single-model strategy: use whatever GGUF is present in models/.
    let _ = tier; // unused after tier removal — kept in signature for API stability.
    if let Some((discovered, _)) = crate::services::query_router::discover_local_gguf() {
        (discovered, true)
    } else {
        (String::new(), true)
    }
}

fn extract_without_llm(template: &str, chunks: &[ChunkRow]) -> String {
    // Regex-only extraction when no LLM is available.
    match template {
        "extract_action_items" => {
            let action_keywords = ["TODO", "ACTION", "TASK", "FOLLOW UP", "ACTION ITEM"];
            let items: Vec<String> = chunks
                .iter()
                .flat_map(|c| {
                    c.content
                        .lines()
                        .filter(|line| {
                            action_keywords.iter().any(|kw| line.to_uppercase().contains(kw))
                        })
                        .map(|l| format!("• {}", l.trim()))
                        .collect::<Vec<_>>()
                })
                .take(20)
                .collect();
            if items.is_empty() {
                "No explicit action items detected. Consider using an LLM model for better extraction.".to_string()
            } else {
                items.join("\n")
            }
        }
        "generate_timeline" => {
            use regex_lite::Regex;
            let date_re = Regex::new(r"\b(\d{4}[-/]\d{2}[-/]\d{2}|\d{1,2}[/-]\d{1,2}[/-]\d{4})\b").unwrap();
            let mut dates: Vec<String> = chunks
                .iter()
                .flat_map(|c| {
                    date_re
                        .find_iter(&c.content)
                        .map(|m| m.as_str().to_string())
                        .collect::<Vec<_>>()
                })
                .collect();
            dates.sort();
            dates.dedup();
            if dates.is_empty() {
                "No dates found. Consider using an LLM model for timeline generation.".to_string()
            } else {
                dates
                    .iter()
                    .map(|d| format!("{d} | (date found in sources)"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        _ => format!(
            "Summary of {} chunks from this notebook. \
             Add an LLM model to models/ for AI-powered {} analysis.",
            chunks.len(),
            template.replace('_', " ")
        ),
    }
}

fn template_title(template: &str) -> String {
    match template {
        "compare_sources" => "Source Comparison",
        "extract_action_items" => "Action Items",
        "find_contradictions" => "Contradictions & Inconsistencies",
        "generate_timeline" => "Chronological Timeline",
        "summarize_all" => "Comprehensive Summary",
        _ => template,
    }
    .to_string()
}
