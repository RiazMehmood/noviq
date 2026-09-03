//! Intent router — single chat surface. Classifies natural-language officer
//! queries into one of eight intents and dispatches to a per-intent handler
//! (FR-013–FR-020).
//!
//! Slice B MVP (this version):
//! - `classifier::classify` (rule-first) → `Intent` + confidence
//! - `slots::extract` → structured `Slots`
//! - `dispatch` prepends an intent-specific instruction to the query and
//!   delegates to the existing `query_router::route_scoped` retrieval +
//!   answer pipeline. The four US1 intents (Lookup / ListChildren /
//!   Summarize / ActionItems) share that retrieval path with different
//!   instruction prefixes. Difference / CrossCompare / Timeline / Procedure
//!   bail with a "needs deep re-index" upgrade prompt — those land in
//!   Slices C/D.

pub mod classifier;
pub mod decompose;
pub mod handlers;
pub mod slots;
pub mod types;

pub use types::{
    ChatBubble, ChatQuery, ChatResponse, Citation, FlowDiagram, FlowStep, Intent,
    IntentClassification, Scope, Slots, UpgradePrompt,
};

use crate::services::{
    embedder::EmbedderState,
    hardware_detector::HardwareTier,
    llm::ModelSlot,
    query_router::{self, Citation as QrCitation, HistoryTurn},
    reranker::RerankerSlot,
};
use anyhow::Result;
use crate::db::Db;
use std::sync::Arc;
use crate::compat::AppHandle;

/// Dispatch entry point. Caller supplies the same state bag the existing
/// `stream_query_notebook` command uses, plus the `notebook_id` +
/// `session_id` to anchor session history and audit trail.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    reranker_slot: &RerankerSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    session_id: &str,
    history: &[HistoryTurn],
    query: ChatQuery,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<AppHandle>,
) -> Result<ChatResponse> {
    let (intent, confidence) = classifier::classify(&query.text);
    let slots = slots::extract(&query.text);

    // Intents not in US1 short-circuit with an upgrade prompt; the officer
    // path is to deep-reindex one or both instruments first.
    match intent {
        Intent::Difference | Intent::CrossCompare | Intent::Timeline | Intent::Procedure => {
            return Ok(ChatResponse {
                intent,
                intent_confidence: confidence,
                bubbles: vec![ChatBubble {
                    headline: format!("This is a {:?} query.", intent),
                    body: "This intent requires deep-indexed instruments. Please run \
                           'Deep re-index with Mistral' on the relevant document(s) first."
                        .to_string(),
                    citations: vec![],
                    see_also: vec![],
                    flow: None,
                }],
                upgrade_prompt: Some(UpgradePrompt {
                    doc_ids: vec![],
                    reason: format!(
                        "Intent {:?} needs the deep-tier legal IR (amendment chain / cross-ref \
                         graph / procedure flows). Trigger 'Deep re-index with Mistral' on the \
                         document(s) and re-ask.",
                        intent
                    ),
                }),
            });
        }
        _ => {}
    }

    let scope_ids = resolve_scope_source_ids(pool, &query.scope).await;

    // Feature 008: query decomposition for capacity-limited small models.
    // Gated on generator capability (≤1.5B / low-resource, ECHO_DECOMPOSE
    // override); off for 3B+/GPU which answer multi-fact natively. When a
    // multi-fact question is detected, answer each sub-question SEQUENTIALLY
    // (Principle III — one model resident) and merge with a deterministic
    // template (no LLM merge — preserves facts/figures).
    let model_size = query_router::discover_local_gguf()
        .and_then(|(_, label)| label.parse::<f32>().ok());
    if decompose::decomposition_enabled(model_size, tier) {
        if let Some(subs) = decompose::detect_and_split(&query.text, &slots) {
            if subs.len() >= 2 {
                log::info!(
                    "Decomposition: {} sub-questions for multi-fact query",
                    subs.len()
                );
                // Head noun the officer asked in ("articles" → "Article"); Fix B
                // renders the resolved number in their terms, original corpus
                // label in brackets for traceability ("Article 31 (Section 31)").
                let requested_head = decompose::requested_head_noun(&query.text);
                let mut answers = Vec::with_capacity(subs.len());
                for (i, sq) in subs.iter().enumerate() {
                    let sq_slots = slots::extract(&sq.text);
                    let sq_prompt = build_intent_system_prompt(Intent::Lookup, &sq_slots);
                    log::info!("Decomposition sub-question {}/{}: {}", i + 1, subs.len(), sq.text);
                    let (qresp, cancelled) = query_router::route_scoped_with_override(
                        pool,
                        embedder,
                        model_slot,
                        reranker_slot,
                        tier,
                        notebook_id,
                        session_id,
                        &sq.text,
                        history,
                        scope_ids.as_deref(),
                        Some(&sq_prompt),
                        cancel.clone(),
                        app.clone(),
                        None,
                    )
                    .await?;
                    if cancelled {
                        return Ok(ChatResponse {
                            intent,
                            intent_confidence: confidence,
                            bubbles: vec![ChatBubble {
                                headline: "Cancelled".to_string(),
                                body: qresp.answer,
                                citations: vec![],
                                see_also: vec![],
                                flow: None,
                            }],
                            upgrade_prompt: None,
                        });
                    }
                    // Fix B (citation-grade contract): for "which provision deals
                    // with <topic>" sub-questions, NEVER trust the model's number.
                    // Resolve the authoritative label deterministically from the
                    // retrieved chunk whose TITLE matches the topic (not rank 1),
                    // or flag "could not verify" when nothing matches.
                    let (answer, citations) = match &sq.number_topic {
                        Some(topic) => {
                            match decompose::resolve_provision_label(topic, &qresp.citations) {
                                Some(label) => {
                                    log::info!(
                                        "Decomposition: resolved deterministic label '{}' for topic '{}'",
                                        label, topic
                                    );
                                    (
                                        decompose::authoritative_body(
                                            requested_head.as_deref(),
                                            &label,
                                            &qresp.citations,
                                        ),
                                        qresp.citations,
                                    )
                                }
                                None => {
                                    log::info!(
                                        "Decomposition: no title match for topic '{}' — flagging unverified",
                                        topic
                                    );
                                    (decompose::NUMBER_UNVERIFIED.to_string(), vec![])
                                }
                            }
                        }
                        None => (qresp.answer, qresp.citations),
                    };
                    answers.push(decompose::SubAnswer {
                        label: sq.label.clone(),
                        answer,
                        citations,
                    });
                }
                let merged = decompose::merge_subanswers(&answers);
                let citations: Vec<Citation> =
                    merged.citations.iter().map(qr_citation_to_chat).collect();
                return Ok(ChatResponse {
                    intent,
                    intent_confidence: confidence,
                    bubbles: vec![ChatBubble {
                        headline: intent_headline(intent, &slots),
                        body: merged.answer,
                        citations,
                        see_also: vec![],
                        flow: None,
                    }],
                    upgrade_prompt: None,
                });
            }
        }
    }

    // Build the per-intent system prompt. CRITICAL: this is passed to
    // route_scoped_with_override and used ONLY for LLM prompting — it never
    // touches the embedding query. The raw `query.text` is what drives
    // retrieval similarity. This separation is the load-bearing fix for the
    // 2026-06-03 hallucination bug where instruction-blob pollution of the
    // embedding caused adjacent-article retrieval.
    let system_prompt = build_intent_system_prompt(intent, &slots);

    let (qresp, cancelled) = query_router::route_scoped_with_override(
        pool,
        embedder,
        model_slot,
        reranker_slot,
        tier,
        notebook_id,
        session_id,
        &query.text,
        history,
        scope_ids.as_deref(),
        Some(&system_prompt),
        cancel,
        app,
        None,
    )
    .await?;

    if cancelled {
        return Ok(ChatResponse {
            intent,
            intent_confidence: confidence,
            bubbles: vec![ChatBubble {
                headline: "Cancelled".to_string(),
                body: qresp.answer,
                citations: vec![],
                see_also: vec![],
                flow: None,
            }],
            upgrade_prompt: None,
        });
    }

    let citations: Vec<Citation> = qresp.citations.iter().map(qr_citation_to_chat).collect();

    Ok(ChatResponse {
        intent,
        intent_confidence: confidence,
        bubbles: vec![ChatBubble {
            headline: intent_headline(intent, &slots),
            body: qresp.answer,
            citations,
            see_also: vec![],
            flow: None,
        }],
        upgrade_prompt: None,
    })
}

/// Build the per-intent system prompt. Always returns a self-contained
/// `<role + task + anti-hallucination + format>` block. The named target
/// (section path / instrument) is woven in so the LLM is bound to the
/// specific provision and cannot drift to adjacent retrieved chunks —
/// the 2026-06-03 Article 19 → 15/17 hallucination root cause.
fn build_intent_system_prompt(intent: Intent, slots: &Slots) -> String {
    let multi_target = slots.section_paths.len() >= 2;
    let target_clause = match (slots.section_paths.as_slice(), &slots.instrument) {
        ([], Some(i))  => format!("The named target instrument is **{i}**."),
        ([], None)     => "No specific provision was named.".to_string(),
        ([only], Some(i)) => format!("The named target is **{only}** of **{i}**."),
        ([only], None)    => format!("The named target is **{only}**."),
        (many, Some(i)) => format!(
            "The named targets are **{}** — all of **{}**.",
            many.join(", "), i
        ),
        (many, None) => format!("The named targets are **{}**.", many.join(", ")),
    };

    let body = match intent {
        Intent::Lookup if multi_target => format!(
            "You are a Pakistan government legal assistant. \
             {target_clause}\n\
             Task: Provide the verbatim relevant text for EACH named provision in turn, then \
             synthesise the combined rights/duties they create together. RULES:\n\
             1. Organise the answer one paragraph (or bullet) per named provision, in the order given.\n\
             2. For each provision: quote or closely paraphrase the relevant clauses, end with its citation [n].\n\
             3. After the per-provision section, add a short 'Taken together' paragraph that combines them.\n\
             4. Do NOT invent provisions. Do NOT pull content from articles that were not named.\n\
             5. If a named provision is absent from the retrieved context, say so explicitly for that provision \
                rather than skipping it silently."
        ),
        Intent::Lookup => format!(
            "You are a Pakistan government legal assistant. \
             {target_clause}\n\
             Task: Answer using ONLY the verbatim text of the named provision found in the context. \
             RULES:\n\
             1. If the named provision appears in the context, reply with its exact text. Do not paraphrase.\n\
             2. Do NOT pull content from adjacent or related provisions, even if retrieval returned them.\n\
             3. If the named provision is NOT in the context, reply exactly: \
                'The named provision was not found in the retrieved passages.'\n\
             4. End every claim with its citation in the form [n] matching a context passage."
        ),
        Intent::ListChildren => format!(
            "You are a Pakistan government legal assistant. \
             {target_clause}\n\
             Task: List the immediate children of the named container, copying their labels and \
             titles directly from the retrieved context. RULES:\n\
             1. Output ONLY a bulleted list. No tables. No prose preamble. No tail commentary.\n\
             2. Each bullet: '- <exact child label as written> — <one-line title from context> [n]'.\n\
             3. CONFABULATION GUARD: every bullet MUST appear verbatim in a retrieved passage. \
                If the only retrieved passage about the container is a meta-description \
                (e.g. 'these shall be known as the X' or 'the provisions of this Chapter…') \
                WITHOUT the enumeration itself, you MUST reply exactly: \
                'No enumerable children found in the retrieved passages.'\n\
             4. Do NOT invent items, values, principles, or rights based on general knowledge.\n\
             5. Do NOT include children of children — only the immediate level."
        ),
        Intent::Summarize => format!(
            "You are a Pakistan government legal assistant. \
             {target_clause}\n\
             Task: Summarise ONLY the named provision. RULES:\n\
             1. Use 3–5 short sentences OR up to 6 bullets. Officer-grade, no rhetoric.\n\
             2. Do NOT bring in content from adjacent provisions even if retrieved.\n\
             3. If the named provision is not in the retrieved context, reply exactly: \
                'The named provision was not found in the retrieved passages.'\n\
             4. Cite every claim inline as [n]."
        ),
        Intent::ActionItems => format!(
            "You are a Pakistan government legal assistant. \
             {target_clause}\n\
             Task: Extract enforceable obligations ('shall', 'must', duties, prohibitions, timelines) \
             from the named provision. RULES:\n\
             1. Output ONLY a bulleted list. No prose preamble.\n\
             2. Each bullet: '- <actor>: <action> — <deadline or condition if stated> [n]'.\n\
             3. Do NOT invent obligations. Do NOT include permissive language ('may') unless paired \
                with a binding condition.\n\
             4. If no obligations are stated in the context, reply exactly: \
                'No actionable obligations found in the retrieved passages.'"
        ),
        Intent::Fallback => format!(
            "You are a Pakistan government legal assistant. {target_clause}\n\
             Answer the officer's question using ONLY the retrieved context. Cite every claim as [n]. \
             If the answer is not in the context, say so explicitly."
        ),
        _ => String::new(),
    };
    body
}

fn intent_headline(intent: Intent, slots: &Slots) -> String {
    let target = if slots.section_paths.len() >= 2 {
        slots.section_paths.join(", ")
    } else if let Some(p) = slots.section_paths.first() {
        p.clone()
    } else {
        slots
            .instrument
            .clone()
            .or_else(|| slots.topic.clone())
            .unwrap_or_else(|| "your query".to_string())
    };
    match intent {
        Intent::Lookup => format!("{}", target),
        Intent::ListChildren => format!("Children of {}", target),
        Intent::Summarize => format!("Summary of {}", target),
        Intent::ActionItems => format!("Action items in {}", target),
        Intent::Fallback => "Answer".to_string(),
        _ => format!("{:?}: {}", intent, target),
    }
}

/// Map the existing `query_router::Citation` shape to the new wire-format
/// `intent_router::Citation`. Fast-tier docs do not yet carry a
/// `LegalCitation`, so the officer/short forms degrade to a short excerpt
/// label until the doc is deep-re-indexed. Deep-tier docs will route
/// through `Citation::from_legal` directly from the per-intent handler.
fn qr_citation_to_chat(c: &QrCitation) -> Citation {
    let excerpt_short = c
        .excerpt
        .chars()
        .take(80)
        .collect::<String>()
        .trim()
        .to_string();
    let label = if excerpt_short.is_empty() {
        format!("source {}", &c.source_id[..c.source_id.len().min(8)])
    } else {
        excerpt_short
    };
    Citation {
        officer_format: label.clone(),
        short_format: label,
        doc_id: c.source_id.clone(),
        node_id: Some(c.chunk_id.clone()),
        page: c.page.map(|p| p as u32),
    }
}

/// Resolve a `Scope` into a flat list of `source_id`s for the retrieval
/// SQL filter. Restored from the old `commands::query::resolve_scope_ids`
/// path (2026-06-03) — that function was deleted in the streaming-cleanup
/// commit `1226ad9a` but turned out to be load-bearing: the vector + FTS5
/// SQL in `query_router::get_context_chunks` falls back to a notebook-only
/// filter when given `None`, and most user notebooks (including the
/// default "test" notebook) contain ZERO sources — the constitution and
/// other ingested PDFs live under `__folder__…` notebooks created by
/// `add_indexed_folder`. Without expansion, `Scope::Both` produces zero
/// chunks for every standard retrieval.
///
/// Returns:
///   - `Some(vec)` for any scope that names specific sources / folders /
///     notebooks. Vec may be empty if the scope refers to an empty
///     container; the caller short-circuits to "no context" in that case.
///   - `None` for `Scope::Both` ONLY when nothing is indexed yet, so the
///     caller still falls through to the notebook-only legacy path.
async fn resolve_scope_source_ids(pool: &crate::db::Db, scope: &Scope) -> Option<Vec<String>> {
    match scope {
        Scope::FolderDoc { doc_id } | Scope::NotebookDoc { doc_id } => {
            Some(vec![doc_id.clone()])
        }
        Scope::Both => {
            // Union of (a) all sources in any user notebook, EXCLUDING
            // synthetic `__batch__*` / `__folder__*` shells, and
            // (b) all sources whose file path falls under any indexed
            // folder root. This mirrors the deleted resolve_scope_ids
            // logic; the SQL is intentionally permissive — if a source
            // is reachable from any scope the user has set up, it's in.
            let from_notebooks: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT source_id FROM notebook_sources \
                 WHERE notebook_id NOT LIKE '\\_\\_batch\\_\\_%' ESCAPE '\\' \
                   AND notebook_id NOT LIKE '\\_\\_folder\\_\\_%' ESCAPE '\\'",
            )
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            let folder_paths: Vec<String> =
                sqlx::query_scalar("SELECT folder_path FROM indexed_folders")
                    .fetch_all(pool)
                    .await
                    .unwrap_or_default();

            let mut acc: Vec<String> = Vec::new();
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            for id in from_notebooks {
                if seen.insert(id.clone()) { acc.push(id); }
            }
            for fp in folder_paths {
                let prefix = format!("{fp}%");
                let rows: Vec<String> = sqlx::query_scalar(
                    "SELECT id FROM sources WHERE file_path LIKE $1 AND indexing_status = 'indexed'",
                )
                .bind(prefix)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
                for id in rows {
                    if seen.insert(id.clone()) { acc.push(id); }
                }
            }
            if acc.is_empty() { None } else { Some(acc) }
        }
        Scope::Notebook => None, // notebook-only — let the existing SQL filter handle it
        Scope::Folder => {
            // All folder-indexed sources, no specific notebook membership required.
            let folder_paths: Vec<String> =
                sqlx::query_scalar("SELECT folder_path FROM indexed_folders")
                    .fetch_all(pool)
                    .await
                    .unwrap_or_default();
            let mut acc: Vec<String> = Vec::new();
            for fp in folder_paths {
                let prefix = format!("{fp}%");
                let rows: Vec<String> = sqlx::query_scalar(
                    "SELECT id FROM sources WHERE file_path LIKE $1 AND indexing_status = 'indexed'",
                )
                .bind(prefix)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
                acc.extend(rows);
            }
            if acc.is_empty() { None } else { Some(acc) }
        }
    }
}
