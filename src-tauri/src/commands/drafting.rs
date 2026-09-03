//! Feature 009 — Tauri commands for LGD Summary/Note drafting.
//!
//! US1 (render + export) + template CRUD (US2 persistence). Retrieval is scoped
//! to the officer-confirmed document set (the scoping mechanism, FR-016/018).
//! Cited slots resolve deterministically (ADR-0008) via the render helpers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

use serde::{Deserialize, Serialize};
use crate::db::Db;

use crate::services::drafting::chat::{self, DraftAction};
use crate::services::drafting::conversation::{self, ChatMessage};
use crate::services::drafting::docset::{self, CorpusHit};
use crate::services::drafting::gaps;
use crate::services::drafting::gold::{self, GoldExample};
use crate::services::drafting::ingest::{self, ProposedSchema};
use crate::services::drafting::session::{self, DraftState};
use crate::services::drafting::slot::SlotKind;
use crate::services::drafting::template::{TemplateHeader, TemplateSchema};
use crate::services::drafting::{export, render, template};
use crate::services::drafting::{
    Block, CorpusDocRef, DocRole, DocSetUpdate, DraftOutput, DraftSession, Gap, GapResolution,
    IntentAnswers,
};
use crate::services::embedder::EmbedderState;
use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::ModelSlot;
use crate::services::parsers::{self, FileFormat};
use crate::services::query_router::{self, Citation};

/// Managed Tauri state: a cache of live draft working states keyed by `draftId`.
/// The DB (`draft_session`, migration 022) is the durable copy — every mutation
/// is written through so an unfinished draft can be resumed after restart.
pub type DraftSessions = Mutex<HashMap<String, DraftState>>;

const POISONED: &str = "draft session store poisoned";

fn unknown_draft(draft_id: &str) -> String {
    format!("Unknown draft session: {draft_id}. Start a draft first.")
}

/// A working copy of a draft's state: from the in-memory cache, else loaded from
/// the DB (resume after restart) and re-cached. Errors if unknown.
async fn get_state(
    sessions: &DraftSessions,
    pool: &Db,
    draft_id: &str,
) -> Result<DraftState, String> {
    if let Some(s) = sessions.lock().map_err(|_| POISONED.to_string())?.get(draft_id) {
        return Ok(s.clone());
    }
    match session::load(pool, draft_id).await.map_err(|e| e.to_string())? {
        Some(s) => {
            sessions
                .lock()
                .map_err(|_| POISONED.to_string())?
                .insert(draft_id.to_string(), s.clone());
            Ok(s)
        }
        None => Err(unknown_draft(draft_id)),
    }
}

/// Write a draft's state through to BOTH the cache and the DB (FR-023).
async fn put_state(
    sessions: &DraftSessions,
    pool: &Db,
    draft_id: &str,
    state: &DraftState,
) -> Result<(), String> {
    sessions
        .lock()
        .map_err(|_| POISONED.to_string())?
        .insert(draft_id.to_string(), state.clone());
    session::save(pool, draft_id, state)
        .await
        .map_err(|e| e.to_string())
}

/// Scoped vector retrieval over the confirmed document set. Mirrors
/// `commands::search::global_search` but restricts to `source_ids` (the scope).
/// Returns `query_router::Citation` so the render helpers (ADR-0008) can select.
async fn retrieve_scoped(
    pool: &Db,
    embedder: &EmbedderState,
    query: &str,
    source_ids: &[String],
    k: i64,
) -> Result<Vec<Citation>, String> {
    if source_ids.is_empty() || query.trim().is_empty() {
        return Ok(vec![]);
    }
    let emb = embedder.embed_one(query).map_err(|e| e.to_string())?;
    let emb_json = serde_json::to_string(&emb).map_err(|e| e.to_string())?;
    let placeholders = crate::db::pg_in(source_ids.len(), 2);
    let d = crate::db::schema::embedding_dim();
    let sql = format!(
        "SELECT c.source_id, c.id, c.content, c.page_number, \
                (ce.embedding::halfvec({d}) <=> $1::vector::halfvec({d})) as distance \
         FROM chunk_embeddings ce \
         JOIN chunks c ON c.id = ce.chunk_id \
         WHERE c.source_id IN ({placeholders}) \
         ORDER BY distance ASC LIMIT {k}"
    );
    let mut q = sqlx::query_as::<_, (String, String, String, Option<i64>, f64)>(&sql).bind(&emb_json);
    for sid in source_ids {
        q = q.bind(sid);
    }
    let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(source_id, chunk_id, content, page, _distance)| Citation {
            source_id,
            chunk_id,
            excerpt: content,
            page,
        })
        .collect())
}

/// Split a confirmed document set into rules vs case source-id lists (the scope
/// each slot kind draws from: cited → rules, `fact:` → case).
fn split_by_role(confirmed: &[CorpusDocRef]) -> (Vec<String>, Vec<String>) {
    let mut rules = Vec::new();
    let mut case = Vec::new();
    for d in confirmed {
        match d.role {
            DocRole::Rules => rules.push(d.doc_id.clone()),
            DocRole::Case => case.push(d.doc_id.clone()),
        }
    }
    (rules, case)
}

/// Shown for a prose slot when no local generator is available on this hardware
/// tier (Principle VII graceful degrade). It carries no number/citation, so the
/// draft still completes and the officer can supply the narrative by hand.
const PROSE_DEGRADE: &str =
    "[Connective narrative unavailable on this device — please write this passage manually.]";

/// ChatML prompt for connective prose. The model writes 1–3 plain sentences and
/// is explicitly barred from emitting any provision number or citation (FR-007);
/// `render::sanitize_prose` strips any that slip through as a defensive backstop.
fn build_prose_prompt(guidance: &str, context: &str) -> String {
    format!(
        "<|im_start|>system\n\
You write one short connective passage for an official government summary or note. \
Write 1 to 3 plain, formal sentences of narrative ONLY. \
CRITICAL: never mention or invent any article, section, rule, clause or regulation number, \
and never cite a source — citations are inserted separately. \
Neutral, factual tone. Output only the passage, nothing else.\
<|im_end|>\n\
<|im_start|>user\n\
Guidance: {guidance}\n\
Subject: {context}\n\
Write the passage now.\
<|im_end|>\n\
<|im_start|>assistant\n"
    )
}

/// Generate connective prose via the warm local model. Returns [`PROSE_DEGRADE`]
/// when no model is on disk or inference fails, so a prose slot never blocks a
/// draft. Always number-free at the source (the prompt) and again at the sink
/// (`render::sanitize_prose`).
async fn generate_prose(
    model_slot: &Arc<ModelSlot>,
    tier: &HardwareTier,
    guidance: &str,
    context: &str,
) -> String {
    let (model_path, _label, _notice) = query_router::select_model_for_tier(tier);
    if model_path.is_empty() || !std::path::Path::new(&model_path).exists() {
        return PROSE_DEGRADE.to_string();
    }
    let prompt = build_prose_prompt(guidance, context);
    let model_slot = Arc::clone(model_slot);
    let tier = tier.clone();
    // Inference is blocking (llama.cpp decode) — keep it off the async runtime.
    let result = tokio::task::spawn_blocking(move || {
        model_slot.infer(&model_path, &prompt, 256, &tier)
    })
    .await;
    match result {
        Ok(Ok(text)) if !text.trim().is_empty() => text,
        Ok(Ok(_)) => PROSE_DEGRADE.to_string(),
        Ok(Err(e)) => {
            log::warn!("prose generation failed: {e}");
            PROSE_DEGRADE.to_string()
        }
        Err(e) => {
            log::warn!("prose generation task panicked: {e}");
            PROSE_DEGRADE.to_string()
        }
    }
}

/// Render a SINGLE slot to its block against the confirmed scope (the one
/// contract code path — used by full render AND every targeted revision, T045).
/// Retrieval is async + scoped; the deterministic citation/fact/prose logic is
/// `render::render_slot`, fed pre-fetched candidates for the slot's role. Prose
/// is generated up front by the warm model (T017) and fed in number-free.
async fn render_one_slot(
    pool: &Db,
    embedder: &EmbedderState,
    model_slot: &Arc<ModelSlot>,
    tier: &HardwareTier,
    slot: &crate::services::drafting::slot::Slot,
    intent: &IntentAnswers,
    rules_ids: &[String],
    case_ids: &[String],
) -> Result<Block, String> {
    let (rules_cands, case_cands) = if slot.kind == SlotKind::Cited {
        let cat = slot.required_source_category.clone().unwrap_or_default();
        if let Some(fact_topic) = cat.strip_prefix("fact:") {
            let c = retrieve_scoped(pool, embedder, fact_topic.trim(), case_ids, 8).await?;
            (Vec::new(), c)
        } else {
            let c = retrieve_scoped(pool, embedder, &cat, rules_ids, 8).await?;
            (c, Vec::new())
        }
    } else {
        (Vec::new(), Vec::new())
    };
    let retrieve = |_q: &str, role: DocRole| match role {
        DocRole::Rules => rules_cands.clone(),
        DocRole::Case => case_cands.clone(),
    };
    // Generate prose once, up front (async), then feed it through the sync
    // contract path; `render::render_slot` sanitizes it so FR-007 holds.
    let prose_text = if slot.kind == SlotKind::Prose {
        let guidance = slot.prose_guidance.clone().unwrap_or_default();
        let context = format!("{} — {}", intent.purpose, intent.scheme);
        generate_prose(model_slot, tier, &guidance, &context).await
    } else {
        String::new()
    };
    let prose = |_g: &str, _c: &str| prose_text.clone();
    Ok(render::render_slot(slot, intent, &retrieve, &prose))
}

/// An officer-typed fact value recorded for this slot (FR-008a, Phase E):
/// rendered verbatim under the explicit "officer-supplied" attribution instead
/// of retrieval. Fact slots only; `None` falls through to the normal path.
fn officer_supplied_block(
    state: &DraftState,
    slot: &crate::services::drafting::slot::Slot,
) -> Option<Block> {
    if slot.kind != SlotKind::Cited
        || !slot
            .required_source_category
            .as_deref()
            .is_some_and(|c| c.starts_with("fact:"))
    {
        return None;
    }
    match state.gap_resolutions.get(&slot.ordering) {
        Some(GapResolution::OfficerSupplied { value }) => {
            Some(render::officer_supplied_fact(slot, value))
        }
        _ => None,
    }
}

/// Render the draft for a session after explicit confirmation (FR-022). Stores
/// the rendered blocks in the session so the officer can revise iteratively
/// (US5). Hand-edited prose blocks are preserved across a full re-render.
#[tauri::command]
pub async fn render_draft(
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    model_slot: State<'_, Arc<ModelSlot>>,
    tier: State<'_, HardwareTier>,
    sessions: State<'_, DraftSessions>,
    draft_id: String,
    confirmed: bool,
) -> Result<DraftOutput, String> {
    if !confirmed {
        return Err("Confirm the document set before rendering the draft.".to_string());
    }
    let mut state = get_state(sessions.inner(), pool.inner(), &draft_id).await?;
    let tpl = template::get_template(pool.inner(), state.template_id)
        .await
        .map_err(|e| e.to_string())?;
    let (rules_ids, case_ids) = split_by_role(&state.confirmed_docs);

    let mut blocks = Vec::with_capacity(tpl.slots.len());
    for slot in &tpl.slots {
        // Preserve a prior officer prose edit through a full re-render.
        if slot.kind == SlotKind::Prose {
            if let Some(edit) = state.prose_edits.get(&slot.ordering) {
                blocks.push(Block::Prose {
                    label: slot.label.clone(),
                    text: edit.clone(),
                });
                continue;
            }
        }
        // An officer-typed fact value renders verbatim, attributed (FR-008a).
        if let Some(block) = officer_supplied_block(&state, slot) {
            blocks.push(block);
            continue;
        }
        blocks.push(
            render_one_slot(
                pool.inner(),
                embedder.inner(),
                model_slot.inner(),
                tier.inner(),
                slot,
                &state.intent,
                &rules_ids,
                &case_ids,
            )
            .await?,
        );
    }
    state.blocks = blocks.clone();
    put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;
    Ok(DraftOutput { blocks })
}

// ── Draft session: propose & curate the document set (US3) ───────────────────

/// Retrieve over the WHOLE corpus (FR-016) and collapse to distinct documents,
/// most-relevant first. Mirrors `commands::search::global_search`'s scope (only
/// indexed sources under a watched folder) but returns per-document refs.
async fn retrieve_whole_corpus(
    pool: &Db,
    embedder: &EmbedderState,
    query: &str,
    k: i64,
) -> Result<Vec<CorpusHit>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    let emb = embedder.embed_one(query).map_err(|e| e.to_string())?;
    let emb_json = serde_json::to_string(&emb).map_err(|e| e.to_string())?;
    let d = crate::db::schema::embedding_dim();
    let sql = format!(
        "SELECT c.source_id, s.file_name, s.file_path, \
                (ce.embedding::halfvec({d}) <=> $1::vector::halfvec({d})) as distance \
         FROM chunk_embeddings ce \
         JOIN chunks c ON c.id = ce.chunk_id \
         JOIN sources s ON s.id = c.source_id \
         WHERE s.indexing_status = 'indexed' \
           AND EXISTS ( \
             SELECT 1 FROM indexed_folders f \
             WHERE s.file_path LIKE (f.folder_path || '%') \
           ) \
         ORDER BY distance ASC LIMIT {k}"
    );
    let rows = sqlx::query_as::<_, (String, String, String, f64)>(&sql)
        .bind(&emb_json)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(source_id, file_name, file_path, _distance)| CorpusHit {
            doc_id: source_id,
            title: file_name,
            source_path: file_path,
        })
        .collect())
}

/// Propose the whole-corpus document set for a draft from its intent (FR-016),
/// role-tagged (R2). Shared by `start_draft` (form path) and `send_chat_turn`
/// (conversational path) so the proposal logic exists in exactly one place.
async fn propose_for_session(
    pool: &Db,
    embedder: &EmbedderState,
    intent: &IntentAnswers,
) -> Result<Vec<CorpusDocRef>, String> {
    // A larger K than render: we want document breadth for the proposal, not the
    // single best chunk. Dedup-to-document happens in `propose_candidates`.
    let query = docset::intent_query(intent);
    let hits = retrieve_whole_corpus(pool, embedder, &query, 60).await?;
    Ok(docset::propose_candidates(&hits))
}

/// Begin a draft from a template + intent answers; propose the document set over
/// the whole corpus (FR-016), role-tagged (R2). Creates an in-memory session.
#[tauri::command]
pub async fn start_draft(
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    sessions: State<'_, DraftSessions>,
    template_id: i64,
    intent: IntentAnswers,
) -> Result<DraftSession, String> {
    let candidate_docs = propose_for_session(pool.inner(), embedder.inner(), &intent).await?;

    let draft_id = uuid::Uuid::new_v4().to_string();
    let state = DraftState::new(template_id, intent, candidate_docs.clone());
    put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;

    Ok(DraftSession {
        draft_id,
        candidate_docs,
    })
}

/// One conversational turn's outcome (Phase A). `intentComplete` flips once the
/// document set has been proposed, signalling the frontend to hand off to the
/// existing doc-set panel (parity with the old `IntentForm`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTurnResult {
    pub draft_id: String,
    pub reply: String,
    pub action: String,
    pub intent_complete: bool,
    pub candidate_docs: Vec<CorpusDocRef>,
}

/// Drive the drafting conversation one turn (Phase A: intent collection only).
///
/// First call: `draft_id` is `None` and `template_id` is required — a new session
/// is created (intent's `instrument_type` taken from the template) and the first
/// question is returned; any `text` is ignored on creation. Subsequent calls pass
/// `draft_id`; the trimmed `text` fills the next intent field. When all intent
/// fields are answered the document set is proposed (reusing `propose_for_session`)
/// and `intent_complete` is set. Every officer and system turn is persisted
/// (FR-031). The conversation never generates citations/numbers/facts (FR-028).
#[tauri::command]
pub async fn send_chat_turn(
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    sessions: State<'_, DraftSessions>,
    draft_id: Option<String>,
    template_id: Option<i64>,
    text: String,
) -> Result<ChatTurnResult, String> {
    // ── First turn: create the session and ask the first question ────────────
    let draft_id = match draft_id {
        None => {
            let template_id = template_id
                .ok_or_else(|| "template_id is required to start a conversation".to_string())?;
            let tpl = template::get_template(pool.inner(), template_id)
                .await
                .map_err(|e| e.to_string())?;
            let intent = IntentAnswers {
                instrument_type: tpl.instrument_type,
                addressee: String::new(),
                purpose: String::new(),
                scheme: String::new(),
            };
            let draft_id = uuid::Uuid::new_v4().to_string();
            let state = DraftState::new(template_id, intent.clone(), Vec::new());
            put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;

            let field = chat::next_field(&intent).expect("a fresh intent has questions");
            let reply = format!(
                "Let's draft your {}. {}",
                state.intent.instrument_type,
                chat::question_for(field)
            );
            conversation::append_message(pool.inner(), &draft_id, "system", &reply)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(ChatTurnResult {
                draft_id,
                reply,
                action: "ask".into(),
                intent_complete: false,
                candidate_docs: Vec::new(),
            });
        }
        Some(id) => id,
    };

    // ── Answer turn: persist it, parse, apply, then ask next or propose ──────
    let mut state = get_state(sessions.inner(), pool.inner(), &draft_id).await?;
    conversation::append_message(pool.inner(), &draft_id, "officer", &text)
        .await
        .map_err(|e| e.to_string())?;

    let (reply, action, intent_complete, candidate_docs) =
        match chat::parse_turn(&state.intent, &text) {
            DraftAction::SetIntent { field, value } => {
                chat::apply(&mut state.intent, field, value);
                if chat::intent_complete(&state.intent) {
                    let docs =
                        propose_for_session(pool.inner(), embedder.inner(), &state.intent).await?;
                    state.candidate_docs = docs.clone();
                    put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;
                    let reply = format!(
                        "Thanks — I've proposed {} document(s) for this draft. Review and confirm the set.",
                        docs.len()
                    );
                    (reply, "propose".to_string(), true, docs)
                } else {
                    put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;
                    let next = chat::next_field(&state.intent).expect("not complete");
                    (chat::question_for(next).to_string(), "ask".to_string(), false, Vec::new())
                }
            }
            DraftAction::Clarify => {
                let reply = match chat::next_field(&state.intent) {
                    Some(f) => format!("Sorry, I didn't catch that. {}", chat::question_for(f)),
                    None => "Could you rephrase that?".to_string(),
                };
                (reply, "clarify".to_string(), false, Vec::new())
            }
        };

    conversation::append_message(pool.inner(), &draft_id, "system", &reply)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ChatTurnResult {
        draft_id,
        reply,
        action,
        intent_complete,
        candidate_docs,
    })
}

/// The full conversation + current pipeline state for a draft, for resume
/// (FR-032). Phase A returns no rolling summary (that is Phase C).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationView {
    pub messages: Vec<ChatMessage>,
    pub intent: IntentAnswers,
    pub candidate_docs: Vec<CorpusDocRef>,
    pub confirmed_docs: Vec<CorpusDocRef>,
    pub finalized: bool,
}

/// Restore a drafting conversation and its current state after restart (FR-032).
#[tauri::command]
pub async fn load_conversation(
    pool: State<'_, Db>,
    sessions: State<'_, DraftSessions>,
    draft_id: String,
) -> Result<ConversationView, String> {
    let state = get_state(sessions.inner(), pool.inner(), &draft_id).await?;
    let messages = conversation::load_conversation(pool.inner(), &draft_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ConversationView {
        messages,
        intent: state.intent,
        candidate_docs: state.candidate_docs,
        confirmed_docs: state.confirmed_docs,
        finalized: state.finalized,
    })
}

/// The destination folder a missing doc should be dropped into — an existing
/// watched corpus folder (FR-019, R6). Picks the first registered folder.
async fn primary_watched_folder(pool: &Db) -> Result<String, String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT folder_path FROM indexed_folders ORDER BY id LIMIT 1")
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(row.map(|(p,)| p).unwrap_or_default())
}

/// Coverage similarity threshold (Phase E, R11): a document covers a category
/// when its best chunk's cosine SIMILARITY to the embedded category clears this.
/// Overridable via `ECHO_GAP_COVER_THRESHOLD` for calibration on a live corpus.
fn gap_cover_threshold() -> f64 {
    std::env::var("ECHO_GAP_COVER_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.45)
}

/// All indexed documents under watched folders, role-tagged — the tier-a
/// candidate pool for add-this-document suggestions (FR-019).
async fn list_corpus_docs(pool: &Db) -> Result<Vec<CorpusDocRef>, String> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT s.id, s.file_name, s.file_path FROM sources s \
         WHERE s.indexing_status = 'indexed' \
           AND EXISTS ( \
             SELECT 1 FROM indexed_folders f \
             WHERE s.file_path LIKE (f.folder_path || '%') \
           )",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(doc_id, title, source_path)| {
            let role = docset::tag_role(&title, &source_path);
            CorpusDocRef { doc_id, title, source_path, role }
        })
        .collect())
}

/// Content-semantic coverage map (Phase E, R11): for each category topic, the
/// best cosine SIMILARITY per document, computed by embedding the topic once and
/// scanning each document's indexed `chunk_embeddings` rows — never doc titles.
async fn semantic_cover_map(
    pool: &Db,
    embedder: &EmbedderState,
    topics: &[String],
) -> Result<HashMap<String, HashMap<String, f64>>, String> {
    let mut map: HashMap<String, HashMap<String, f64>> = HashMap::new();
    for topic in topics {
        if map.contains_key(topic) || topic.trim().is_empty() {
            continue;
        }
        let emb = embedder.embed_one(topic).map_err(|e| e.to_string())?;
        let emb_json = serde_json::to_string(&emb).map_err(|e| e.to_string())?;
        // MIN cosine DISTANCE per doc = the doc's best-matching chunk.
        let d = crate::db::schema::embedding_dim();
        let rows: Vec<(String, f64)> = sqlx::query_as(&format!(
            "SELECT c.source_id, MIN(ce.embedding::halfvec({d}) <=> $1::vector::halfvec({d})) as best \
             FROM chunk_embeddings ce \
             JOIN chunks c ON c.id = ce.chunk_id \
             JOIN sources s ON s.id = c.source_id \
             WHERE s.indexing_status = 'indexed' \
               AND EXISTS ( \
                 SELECT 1 FROM indexed_folders f \
                 WHERE s.file_path LIKE (f.folder_path || '%') \
               ) \
             GROUP BY c.source_id",
        ))
        .bind(&emb_json)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        let per_doc = rows
            .into_iter()
            .map(|(source_id, best_distance)| (source_id, 1.0 - best_distance))
            .collect();
        map.insert(topic.clone(), per_doc);
    }
    Ok(map)
}

/// The category TOPIC a slot is matched on (mirrors `gaps::detect_gaps`):
/// `fact:` prefix stripped, trimmed.
fn slot_topic(cat: &str) -> &str {
    cat.strip_prefix("fact:").map(str::trim).unwrap_or(cat)
}

/// Run gap detection for a draft's confirmed set against its template's cited
/// slots (FR-019, Phase E two-tier semantic coverage). Shared by
/// `update_doc_set`, `resolve_gap` and the watch re-proposal.
async fn gaps_for(
    pool: &Db,
    embedder: &EmbedderState,
    template_id: i64,
    confirmed: &[CorpusDocRef],
) -> Result<Vec<Gap>, String> {
    let tpl = template::get_template(pool, template_id)
        .await
        .map_err(|e| e.to_string())?;
    let dest = primary_watched_folder(pool).await?;
    let corpus = list_corpus_docs(pool).await?;

    let topics: Vec<String> = tpl
        .slots
        .iter()
        .filter(|s| s.kind == SlotKind::Cited)
        .filter_map(|s| s.required_source_category.as_deref())
        .map(|c| slot_topic(c).to_string())
        .collect();
    let cover_map = semantic_cover_map(pool, embedder, &topics).await?;
    let threshold = gap_cover_threshold();
    let covers = |topic: &str, doc: &CorpusDocRef| -> bool {
        cover_map
            .get(topic)
            .and_then(|per_doc| per_doc.get(&doc.doc_id))
            .map(|sim| *sim >= threshold)
            .unwrap_or(false)
    };
    Ok(gaps::detect_gaps(&tpl.slots, confirmed, &corpus, covers, &dest))
}

/// Curate the working set (add/remove/retag). Persists the officer-confirmed set
/// in the session (the scoping mechanism) and returns the gaps detected against
/// the template's cited slots (US4, FR-019).
#[tauri::command]
pub async fn update_doc_set(
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    sessions: State<'_, DraftSessions>,
    draft_id: String,
    confirmed_docs: Vec<CorpusDocRef>,
) -> Result<DocSetUpdate, String> {
    let mut state = get_state(sessions.inner(), pool.inner(), &draft_id).await?;
    state.confirmed_docs = confirmed_docs.clone();
    put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;
    let gaps = gaps_for(
        pool.inner(),
        embedder.inner(),
        state.template_id,
        &confirmed_docs,
    )
    .await?;
    Ok(DocSetUpdate { gaps })
}

/// Result of recording a per-gap decision (`resolve_gap`). For `acceptSuggestion`
/// the updated confirmed set + recomputed gaps are returned (one-turn accept).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveGapResult {
    pub accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_docs: Option<Vec<CorpusDocRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gaps: Option<Vec<Gap>>,
}

impl ResolveGapResult {
    fn accepted() -> Self {
        Self { accepted: true, confirmed_docs: None, gaps: None }
    }
}

/// Record a per-gap officer decision (FR-021 + Phase E).
/// * `proceedWithout` — slot stays flagged (refuse-or-flag); records intent.
/// * `supplied` — normally driven by the watch event (`refresh_doc_set`).
/// * `acceptSuggestion` — one-turn accept of a tier-a suggestion: the gap's
///   suggested docs join the confirmed set, gaps are recomputed (FR-019).
/// * `officerSupplied` (+ `value`) — fact slots only: the typed value is stored
///   and rendered with the explicit "officer-supplied" attribution (FR-008a).
#[tauri::command]
pub async fn resolve_gap(
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    sessions: State<'_, DraftSessions>,
    draft_id: String,
    slot_id: i64,
    decision: String,
    value: Option<String>,
) -> Result<ResolveGapResult, String> {
    let mut state = get_state(sessions.inner(), pool.inner(), &draft_id).await?;
    match decision.as_str() {
        "proceedWithout" => {
            state
                .gap_resolutions
                .insert(slot_id, GapResolution::ProceedWithout);
            put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;
            Ok(ResolveGapResult::accepted())
        }
        // `supplied` carries no doc id here; the watch re-proposal records the
        // actual document and updates the set. Accept the intent.
        "supplied" => Ok(ResolveGapResult::accepted()),
        "acceptSuggestion" => {
            // Find the current suggestion for this slot and adopt its docs.
            let gaps = gaps_for(
                pool.inner(),
                embedder.inner(),
                state.template_id,
                &state.confirmed_docs,
            )
            .await?;
            let suggested = gaps
                .iter()
                .find(|g| g.slot_ordering == slot_id)
                .and_then(|g| match &g.tier {
                    crate::services::drafting::GapTier::Suggestion { suggested_docs } => {
                        Some(suggested_docs.clone())
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    format!("No open suggestion for slot {slot_id} — re-check the gaps.")
                })?;
            let doc_ids: Vec<String> = suggested.iter().map(|d| d.doc_id.clone()).collect();
            for d in suggested {
                if !state.confirmed_docs.iter().any(|c| c.doc_id == d.doc_id) {
                    state.confirmed_docs.push(d);
                }
            }
            state
                .gap_resolutions
                .insert(slot_id, GapResolution::AcceptedSuggestion { doc_ids });
            put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;
            let gaps = gaps_for(
                pool.inner(),
                embedder.inner(),
                state.template_id,
                &state.confirmed_docs,
            )
            .await?;
            Ok(ResolveGapResult {
                accepted: true,
                confirmed_docs: Some(state.confirmed_docs.clone()),
                gaps: Some(gaps),
            })
        }
        "officerSupplied" => {
            let value = value
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| "Provide the fact value to record (officerSupplied).".to_string())?;
            // Fact slots only (FR-008a) — a typed value can never become a citation.
            let tpl = template::get_template(pool.inner(), state.template_id)
                .await
                .map_err(|e| e.to_string())?;
            let is_fact = tpl.slots.iter().any(|s| {
                s.ordering == slot_id
                    && s.kind == SlotKind::Cited
                    && s.required_source_category
                        .as_deref()
                        .is_some_and(|c| c.starts_with("fact:"))
            });
            if !is_fact {
                return Err(
                    "Officer-supplied values are allowed on fact slots only — cited rule \
                     slots always resolve from the corpus."
                        .to_string(),
                );
            }
            state
                .gap_resolutions
                .insert(slot_id, GapResolution::OfficerSupplied { value });
            put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;
            Ok(ResolveGapResult::accepted())
        }
        other => Err(format!("Unknown gap decision: {other}")),
    }
}

/// Payload for `drafting://docset-updated` (FR-020).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocSetUpdatedEvent {
    draft_id: String,
    added_docs: Vec<CorpusDocRef>,
    updated_gaps: Vec<Gap>,
}

/// Re-propose the document set after the watched corpus folder indexes new
/// documents during an open draft (FR-020). Any newly-indexed docs are added to
/// the confirmed set (they "will be used"), gaps are recomputed, and
/// `drafting://docset-updated` is emitted so gaps shrink without restarting
/// (SC-005). The frontend (or a future file_watcher hook) calls this on a
/// completed indexing run for the watched folder.
#[tauri::command]
pub async fn refresh_doc_set(
    app: AppHandle,
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    sessions: State<'_, DraftSessions>,
    draft_id: String,
) -> Result<DocSetUpdate, String> {
    // Snapshot what the session knows so we can diff against a fresh proposal.
    let mut state = get_state(sessions.inner(), pool.inner(), &draft_id).await?;

    let query = docset::intent_query(&state.intent);
    let hits = retrieve_whole_corpus(pool.inner(), embedder.inner(), &query, 60).await?;
    let fresh = docset::propose_candidates(&hits);
    let added_docs = docset::newly_added(&state.candidate_docs, &fresh);

    // Persist the refreshed candidates and auto-add the new docs to the confirmed
    // set so they feed the draft (FR-020).
    state.candidate_docs = fresh;
    for d in &added_docs {
        if !state.confirmed_docs.iter().any(|c| c.doc_id == d.doc_id) {
            state.confirmed_docs.push(d.clone());
        }
    }
    let confirmed = state.confirmed_docs.clone();
    let template_id = state.template_id;
    put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;

    let gaps = gaps_for(pool.inner(), embedder.inner(), template_id, &confirmed).await?;

    let _ = app.emit(
        "drafting://docset-updated",
        DocSetUpdatedEvent {
            draft_id,
            added_docs,
            updated_gaps: gaps.clone(),
        },
    );
    Ok(DocSetUpdate { gaps })
}

// ── Iterative refinement until finalize (US5) ────────────────────────────────

/// A targeted revision of a rendered draft (FR-024). Only the affected block(s)
/// re-render; the rest is preserved. The citation-grade contract holds on every
/// revision (FR-025): cited slots re-resolve deterministically (a number can
/// never be hand-edited in), prose stays number-free, free-text edits are
/// allowed on prose blocks ONLY.
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Revision {
    /// Re-resolve a cited slot, optionally from a specific newly-chosen document.
    ReresolveCited {
        slot_ordering: i64,
        new_doc_id: Option<String>,
    },
    /// Regenerate a prose slot (discards any prior hand-edit for it).
    RegenerateProse { slot_ordering: i64 },
    /// Replace a prose slot's text with an officer edit (prose only).
    EditProse { slot_ordering: i64, text: String },
    /// Change intent fields; re-render non-edited prose blocks.
    UpdateIntent { intent: IntentAnswers },
    /// Re-curate the confirmed set; re-render cited/fact blocks.
    UpdateDocSet { confirmed_docs: Vec<CorpusDocRef> },
}

/// Index of the block whose slot has the given ordering.
fn block_index(tpl: &TemplateSchema, ordering: i64) -> Result<usize, String> {
    tpl.slots
        .iter()
        .position(|s| s.ordering == ordering)
        .ok_or_else(|| format!("No slot with ordering {ordering} in this template."))
}

/// Apply a targeted revision and re-render only the affected block(s), preserving
/// the rest (FR-024/025). Returns the full current draft.
#[tauri::command]
pub async fn revise_draft(
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    model_slot: State<'_, Arc<ModelSlot>>,
    tier: State<'_, HardwareTier>,
    sessions: State<'_, DraftSessions>,
    draft_id: String,
    revision: Revision,
) -> Result<DraftOutput, String> {
    let mut state = get_state(sessions.inner(), pool.inner(), &draft_id).await?;
    if state.finalized {
        return Err("This draft is finalized and locked; revision is no longer allowed.".to_string());
    }
    let tpl = template::get_template(pool.inner(), state.template_id)
        .await
        .map_err(|e| e.to_string())?;
    if state.blocks.len() != tpl.slots.len() {
        return Err("Render the draft before revising it.".to_string());
    }
    let (rules_ids, case_ids) = split_by_role(&state.confirmed_docs);

    match revision {
        Revision::ReresolveCited { slot_ordering, new_doc_id } => {
            let idx = block_index(&tpl, slot_ordering)?;
            let slot = &tpl.slots[idx];
            if slot.kind != SlotKind::Cited {
                return Err("Only cited slots can be re-resolved.".to_string());
            }
            // A specific chosen document narrows the scope for the slot's role.
            let cat = slot.required_source_category.clone().unwrap_or_default();
            let is_fact = cat.starts_with("fact:");
            let (r_ids, c_ids) = match (&new_doc_id, is_fact) {
                (Some(id), true) => (rules_ids.clone(), vec![id.clone()]),
                (Some(id), false) => (vec![id.clone()], case_ids.clone()),
                (None, _) => (rules_ids.clone(), case_ids.clone()),
            };
            state.blocks[idx] = render_one_slot(
                pool.inner(),
                embedder.inner(),
                model_slot.inner(),
                tier.inner(),
                slot,
                &state.intent,
                &r_ids,
                &c_ids,
            )
            .await?;
        }
        Revision::RegenerateProse { slot_ordering } => {
            let idx = block_index(&tpl, slot_ordering)?;
            let slot = &tpl.slots[idx];
            if slot.kind != SlotKind::Prose {
                return Err("Only prose slots can be regenerated.".to_string());
            }
            state.prose_edits.remove(&slot_ordering); // regenerate overrides any edit
            state.blocks[idx] = render_one_slot(
                pool.inner(),
                embedder.inner(),
                model_slot.inner(),
                tier.inner(),
                slot,
                &state.intent,
                &rules_ids,
                &case_ids,
            )
            .await?;
        }
        Revision::EditProse { slot_ordering, text } => {
            let idx = block_index(&tpl, slot_ordering)?;
            let slot = &tpl.slots[idx];
            if slot.kind != SlotKind::Prose {
                return Err("Free-text editing is allowed on prose blocks only.".to_string());
            }
            // Sanitize so an edit can never introduce a number/citation (FR-007/025).
            let clean = render::sanitize_prose(&text);
            state.prose_edits.insert(slot_ordering, clean.clone());
            state.blocks[idx] = Block::Prose {
                label: slot.label.clone(),
                text: clean,
            };
        }
        Revision::UpdateIntent { intent } => {
            state.intent = intent;
            // Prose depends on intent; cited/fact depend on category. Re-render
            // prose blocks that the officer has NOT hand-edited (preserve edits).
            for (idx, slot) in tpl.slots.iter().enumerate() {
                if slot.kind == SlotKind::Prose && !state.prose_edits.contains_key(&slot.ordering) {
                    state.blocks[idx] = render_one_slot(
                        pool.inner(),
                        embedder.inner(),
                        model_slot.inner(),
                        tier.inner(),
                        slot,
                        &state.intent,
                        &rules_ids,
                        &case_ids,
                    )
                    .await?;
                }
            }
        }
        Revision::UpdateDocSet { confirmed_docs } => {
            state.confirmed_docs = confirmed_docs;
            let (r_ids, c_ids) = split_by_role(&state.confirmed_docs);
            // Cited/fact blocks draw from the set; re-render them, preserve the rest.
            for (idx, slot) in tpl.slots.iter().enumerate() {
                if slot.kind == SlotKind::Cited {
                    // Officer-typed fact values persist across set changes (FR-008a).
                    if let Some(block) = officer_supplied_block(&state, slot) {
                        state.blocks[idx] = block;
                        continue;
                    }
                    state.blocks[idx] = render_one_slot(
                        pool.inner(),
                        embedder.inner(),
                        model_slot.inner(),
                        tier.inner(),
                        slot,
                        &state.intent,
                        &r_ids,
                        &c_ids,
                    )
                    .await?;
                }
            }
        }
    }

    let blocks = state.blocks.clone();
    put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;
    Ok(DraftOutput { blocks })
}

/// Lock a draft as final (FR-026). After finalize the session is no longer
/// revisable; the locked blocks are returned for export.
#[tauri::command]
pub async fn finalize_draft(
    pool: State<'_, Db>,
    sessions: State<'_, DraftSessions>,
    draft_id: String,
) -> Result<DraftOutput, String> {
    let mut state = get_state(sessions.inner(), pool.inner(), &draft_id).await?;
    if state.blocks.is_empty() {
        return Err("Nothing to finalize — render the draft first.".to_string());
    }
    state.finalized = true;
    let blocks = state.blocks.clone();
    put_state(sessions.inner(), pool.inner(), &draft_id, &state).await?;
    Ok(DraftOutput { blocks })
}

/// Export a rendered draft to disk (US1). `format` = "txt" | "md".
#[tauri::command]
pub async fn export_draft(
    draft: DraftOutput,
    dest_path: String,
    format: String,
) -> Result<String, String> {
    export::write_draft(&draft, &dest_path, &format).map_err(|e| e.to_string())?;
    Ok(dest_path)
}

// ── Template setup (US2, Phase D rewrite — FR-010/010a/010b, ADR-0012) ───────

/// Payload for `drafting://progress` during template ingestion (one-time,
/// minutes-long offline operation — the officer sees what stage it is in).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestProgressEvent {
    file: String,
    stage: String,
    attempt: u32,
}

/// Generation knobs proven in the 2026-06-10 Option-C spike (free-text JSON on
/// Mistral-7B; llguidance grammar hard-aborts and is NOT used — ADR-0012).
const INGEST_N_CTX: u32 = 6144;
const INGEST_MAX_PROMPT: usize = 4000;
const INGEST_MAX_GEN: u32 = 1536;
/// Bounded retries with temperature escalation (F006 pattern): a parse failure
/// re-asks slightly warmer; exhaustion REFUSES (FR-010b) — never a partial schema.
const INGEST_TEMPS: [f32; 3] = [0.0, 0.4, 0.7];

/// Actionable refusal when the segmentation model is missing (FR-010b). There is
/// deliberately NO heuristic fallback — a degraded schema would silently poison
/// every future draft built from the template (ADR-0012).
fn segmentation_model_missing() -> String {
    "Template ingestion requires the local segmentation model \
     (Mistral-7B-Instruct-v0.3 Q4_K_M). Place the .gguf in models/ir_builder/ \
     and retry."
        .to_string()
}

/// The segmentation GGUF (reuses the F006 IR-builder discovery). Without the
/// `llm` feature no model can run, so ingestion refuses (FR-010b).
#[cfg(feature = "llm")]
fn discover_segmentation_gguf() -> Option<std::path::PathBuf> {
    crate::services::doc_ir::mistral_builder::discover_ir_builder_gguf()
}
#[cfg(not(feature = "llm"))]
fn discover_segmentation_gguf() -> Option<std::path::PathBuf> {
    None
}

/// Parse an uploaded format into a *proposed* slot schema (Phase D, Option C).
/// The Tier-4 worker proposes the segmentation (free-text JSON); the
/// deterministic verbatim-verify gate in `drafting::ingest` is the sole content
/// authority (FR-010a). One-time offline operation — never on the per-draft path.
#[tauri::command]
pub async fn ingest_template(
    app: AppHandle,
    model_slot: State<'_, Arc<ModelSlot>>,
    tier: State<'_, HardwareTier>,
    file_path: String,
    instrument_type: String,
    is_filled_example: bool,
) -> Result<ProposedSchema, String> {
    let path = std::path::Path::new(&file_path);
    let file_label = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&file_path)
        .to_string();
    let emit_stage = |stage: &str, attempt: u32| {
        let _ = app.emit(
            "drafting://progress",
            IngestProgressEvent {
                file: file_label.clone(),
                stage: stage.to_string(),
                attempt,
            },
        );
    };

    // FR-010b: refuse up front when the segmentation model is absent.
    let model_path = discover_segmentation_gguf().ok_or_else(segmentation_model_missing)?;
    let model_path = model_path.to_string_lossy().to_string();

    emit_stage("extract", 0);
    let unsupported = || "Unsupported file type; use PDF, DOCX, TXT, or Markdown.".to_string();
    let fmt = FileFormat::from_path(path).ok_or_else(unsupported)?;
    let parsed = match fmt {
        FileFormat::Pdf => parsers::pdf::extract(path),
        FileFormat::Docx => parsers::document::extract(path),
        FileFormat::Txt | FileFormat::Markdown => parsers::text::extract(path),
        _ => return Err(unsupported()),
    }
    .map_err(|e| format!("Could not read the template ({e}). Use a text-based PDF, DOCX, TXT, or Markdown."))?;

    // Model proposes (bounded retries, temp escalation); the gate verifies.
    let prompt = ingest::build_segmentation_prompt(&parsed.text);
    let mut last_err = String::new();
    for (attempt, temp) in INGEST_TEMPS.iter().enumerate() {
        emit_stage("segment", attempt as u32);
        let slot = Arc::clone(model_slot.inner());
        let t = tier.inner().clone();
        let mp = model_path.clone();
        let pr = prompt.clone();
        let temp = *temp;
        let raw = tokio::task::spawn_blocking(move || {
            slot.infer_with_cancel_temp(
                &mp,
                &pr,
                INGEST_MAX_GEN,
                INGEST_N_CTX,
                INGEST_MAX_PROMPT,
                &t,
                None,
                None,
                Some(temp),
                None,
            )
        })
        .await
        .map_err(|e| format!("segmentation task failed: {e}"))?;

        let raw = match raw {
            Ok((text, _truncated)) => text,
            Err(e) => {
                last_err = format!("segmentation inference failed: {e}");
                continue;
            }
        };

        emit_stage("verify", attempt as u32);
        match ingest::propose_schema_from_model(&parsed.text, &raw, &instrument_type) {
            Ok(mut proposed) => {
                proposed.name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_string);
                if is_filled_example {
                    proposed.gold_example_ref = Some(file_path);
                }
                emit_stage("done", attempt as u32);
                return Ok(proposed);
            }
            Err(e) => {
                log::warn!("ingest parse attempt {attempt} failed: {e}");
                // Raw output is otherwise lost — preserve it for diagnosis
                // (F006 failure-dump pattern; a few KB max).
                let dump = std::path::Path::new("/tmp/echo-ai-debug");
                if std::fs::create_dir_all(dump).is_ok() {
                    let _ =
                        std::fs::write(dump.join(format!("ingest_attempt{attempt}.txt")), &raw);
                }
                last_err = e;
            }
        }
    }
    // FR-010b: exhausted retries → refuse with an actionable error, no schema.
    Err(format!(
        "Template ingestion could not produce a usable segmentation after \
         {} attempts ({last_err}). Retry, or re-export the document as text-based \
         PDF/TXT and try again.",
        INGEST_TEMPS.len()
    ))
}

/// Persist a corrected schema (create or overwrite). If a gold example was staged
/// (filled-example upload), commit its METADATA only — content never enters the
/// retrieval corpus (SC-007, enforced by `gold::record_gold_example`).
#[tauri::command]
pub async fn confirm_template(
    pool: State<'_, Db>,
    tpl: TemplateSchema,
    gold_example_ref: Option<String>,
) -> Result<i64, String> {
    let id = template::save_template(pool.inner(), &tpl)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(content_ref) = gold_example_ref {
        gold::record_gold_example(
            pool.inner(),
            &GoldExample {
                id: None,
                template_id: Some(id),
                instrument_type: tpl.instrument_type.clone(),
                content_ref,
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(id)
}

#[tauri::command]
pub async fn list_templates(pool: State<'_, Db>) -> Result<Vec<TemplateHeader>, String> {
    template::list_templates(pool.inner())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_template(
    pool: State<'_, Db>,
    template_id: i64,
) -> Result<TemplateSchema, String> {
    template::get_template(pool.inner(), template_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_template(pool: State<'_, Db>, template_id: i64) -> Result<(), String> {
    template::delete_template(pool.inner(), template_id)
        .await
        .map_err(|e| e.to_string())
}
