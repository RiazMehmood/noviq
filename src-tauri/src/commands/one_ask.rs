//! Feature 010 — Tauri commands for one-ask drafting (contracts/one-ask-commands.md).
//!
//! `start_one_ask` returns the draft id immediately; the pipeline runs as a
//! spawned task and streams `drafting://one-ask-progress` /
//! `drafting://one-ask-complete` events (FR-013). Cancellation uses the same
//! token-registry pattern as the F006 importer.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use serde::Serialize;
use crate::db::Db;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;

use crate::services::drafting::conversation::{self, ChatMessage};
use crate::services::drafting::one_ask::compose::SectionWriter;
use crate::services::drafting::one_ask::state::{OneAskIntent, OneAskState, Paragraph, ProvisionCard, Status};
use crate::services::drafting::one_ask::{self, intent, normalize_interrupted, state, Stage};
use crate::services::embedder::EmbedderState;
use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::ModelSlot;

// ── Cancellation registry (importer.rs pattern) ──────────────────────────────

fn registry() -> &'static RwLock<HashMap<String, CancellationToken>> {
    static REG: OnceLock<RwLock<HashMap<String, CancellationToken>>> = OnceLock::new();
    REG.get_or_init(|| RwLock::new(HashMap::new()))
}

fn register(draft_id: &str, token: CancellationToken) {
    registry().write().unwrap().insert(draft_id.to_string(), token);
}

fn unregister(draft_id: &str) {
    registry().write().unwrap().remove(draft_id);
}

// ── Writer model (Tier-4 worker, free-text — ADR-0013/0012: no llguidance) ───

const WRITER_N_CTX: u32 = 6144;
const WRITER_MAX_PROMPT: usize = 4000;
const WRITER_MAX_GEN: u32 = 700;
const WRITER_TEMP: f32 = 0.2;

fn writer_model_missing() -> String {
    "One-ask drafting requires the local writer model \
     (Mistral-7B-Instruct-v0.3 Q4_K_M). Place the .gguf in models/ir_builder/ \
     and retry."
        .to_string()
}

#[cfg(feature = "llm")]
fn discover_writer_gguf() -> Option<std::path::PathBuf> {
    crate::services::doc_ir::mistral_builder::discover_ir_builder_gguf()
}
#[cfg(not(feature = "llm"))]
fn discover_writer_gguf() -> Option<std::path::PathBuf> {
    None
}

/// Production `SectionWriter` over the single ModelSlot (constitution III).
/// `write` is called from the spawned pipeline task; `block_in_place` keeps the
/// minutes-long CPU inference off the async reactor.
struct SlotWriter {
    slot: Arc<ModelSlot>,
    tier: HardwareTier,
    model_path: String,
    cancel: CancellationToken,
    /// Passing the handle makes the slot emit `chat://token` per generated
    /// token — the frontend uses it for the live writing animation + word
    /// count during the minutes-long compose phase (FR-013 UX).
    app: AppHandle,
}

impl SectionWriter for SlotWriter {
    fn write(&self, prompt: &str) -> Result<String, String> {
        tokio::task::block_in_place(|| {
            self.slot
                .infer_with_cancel_temp(
                    &self.model_path,
                    prompt,
                    WRITER_MAX_GEN,
                    WRITER_N_CTX,
                    WRITER_MAX_PROMPT,
                    &self.tier,
                    Some(self.cancel.clone()),
                    Some(self.app.clone()),
                    Some(WRITER_TEMP),
                    None,
                )
                .map(|(text, _truncated)| text)
                .map_err(|e| format!("writer inference failed: {e}"))
        })
    }
}

// ── Event payloads + view (contract shapes) ──────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    draft_id: String,
    stage: String,
    /// Overall pipeline progress 0–100 (research 3–23, compose 25–95 split
    /// evenly across sections, verify 96, ready 100).
    percent: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    section: Option<SectionProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SectionProgress {
    index: usize,
    total: usize,
    title: String,
}

/// Full view for the frontend (contract `OneAskView`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OneAskView {
    pub draft_id: String,
    pub status: Status,
    pub intent: OneAskIntent,
    pub paragraphs: Vec<Paragraph>,
    pub cards: Vec<ProvisionCard>,
    pub finalized: bool,
    pub messages: Vec<ChatMessage>,
}

async fn view_of(
    pool: &Db,
    draft_id: &str,
    st: OneAskState,
) -> Result<OneAskView, String> {
    let messages = conversation::load_conversation(pool, draft_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(OneAskView {
        draft_id: draft_id.to_string(),
        status: st.status,
        intent: st.intent,
        paragraphs: st.paragraphs,
        cards: st.cards,
        finalized: st.finalized,
        messages,
    })
}

fn emit_progress(app: &AppHandle, ev: ProgressEvent) {
    let _ = app.emit("drafting://one-ask-progress", ev);
}

/// Compose occupies 25–95% of the bar, split evenly across sections.
const COMPOSE_BASE_PCT: usize = 25;
const COMPOSE_SPAN_PCT: usize = 70;

fn stage_event(draft_id: &str, stage: &Stage) -> ProgressEvent {
    let (name, percent, section, detail, error) = match stage {
        Stage::Researching => ("researching", 2u8, None, None, None),
        Stage::Note { percent, detail } => ("note", *percent, None, Some(detail.clone()), None),
        Stage::Composing { index, total, title } => (
            "composing",
            (COMPOSE_BASE_PCT + (index - 1) * COMPOSE_SPAN_PCT / (*total).max(1)) as u8,
            Some(SectionProgress {
                index: *index,
                total: *total,
                title: title.to_string(),
            }),
            None,
            None,
        ),
        Stage::Verifying => ("verifying", 96, None, None, None),
        Stage::Ready => ("ready", 100, None, None, None),
        Stage::Failed(e) => ("failed", 0, None, None, Some(e.clone())),
        Stage::Cancelled => ("cancelled", 0, None, None, None),
    };
    ProgressEvent {
        draft_id: draft_id.to_string(),
        stage: name.to_string(),
        percent,
        section,
        detail,
        error,
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Officer submits the one-sentence ask (FR-001). Returns immediately; the
/// pipeline streams progress and finishes with `drafting://one-ask-complete`.
#[tauri::command]
pub async fn start_one_ask(
    app: AppHandle,
    pool: State<'_, Db>,
    text: String,
) -> Result<String, String> {
    // Validations with human-readable remediation (constitution constraints).
    let parsed = intent::parse_intent(&text)?;

    let (indexed,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sources s WHERE s.indexing_status='indexed' \
         AND EXISTS (SELECT 1 FROM indexed_folders f \
                     WHERE s.file_path LIKE (f.folder_path || '%'))",
    )
    .fetch_one(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    if indexed == 0 {
        return Err(
            "No indexed documents found. Index the folder containing the \
             governing documents (rules, manuals, delegations) first, then retry."
                .to_string(),
        );
    }

    let model_path = discover_writer_gguf().ok_or_else(writer_model_missing)?;
    let model_path = model_path.to_string_lossy().to_string();

    let draft_id = uuid::Uuid::new_v4().to_string();
    let st = OneAskState::new(parsed);
    state::save(pool.inner(), &draft_id, &st).await?;
    conversation::append_message(pool.inner(), &draft_id, "officer", text.trim())
        .await
        .map_err(|e| e.to_string())?;
    conversation::append_message(
        pool.inner(),
        &draft_id,
        "system",
        "Drafting started — researching the indexed corpus.",
    )
    .await
    .map_err(|e| e.to_string())?;

    let token = CancellationToken::new();
    register(&draft_id, token.clone());

    let app2 = app.clone();
    let did = draft_id.clone();
    tauri::async_runtime::spawn(async move {
        let pool: State<Db> = app2.state();
        let embedder: State<Arc<EmbedderState>> = app2.state();
        let slot: State<Arc<ModelSlot>> = app2.state();
        let tier: State<HardwareTier> = app2.state();

        let writer = SlotWriter {
            slot: Arc::clone(slot.inner()),
            tier: tier.inner().clone(),
            model_path,
            cancel: token.clone(),
            app: app2.clone(),
        };
        let emb = Arc::clone(embedder.inner());
        let embed = move |q: &str| emb.embed_one(q).map_err(|e| e.to_string());

        let app3 = app2.clone();
        let did2 = did.clone();
        let on_stage = move |s: Stage| emit_progress(&app3, stage_event(&did2, &s));

        let result = one_ask::run_pipeline(
            pool.inner(),
            &embed,
            &writer,
            &did,
            true, // US4/D7: production drafts deterministically (no LLM-authored prose)
            &token,
            &on_stage,
        )
        .await;
        unregister(&did);

        match result {
            Ok(()) => {
                if let Ok(Some(st)) = state::load(pool.inner(), &did).await {
                    let note = match st.status {
                        Status::Ready => "Draft is ready for review.",
                        Status::Cancelled => "Drafting was cancelled.",
                        _ => "Drafting stopped.",
                    };
                    let _ =
                        conversation::append_message(pool.inner(), &did, "system", note).await;
                    if let Ok(view) = view_of(pool.inner(), &did, st).await {
                        let _ = app2.emit("drafting://one-ask-complete", view);
                    }
                }
            }
            Err(e) => {
                let _ = conversation::append_message(
                    pool.inner(),
                    &did,
                    "system",
                    &format!("Drafting failed: {e}"),
                )
                .await;
                emit_progress(
                    &app2,
                    ProgressEvent {
                        draft_id: did.clone(),
                        stage: "failed".into(),
                        percent: 0,
                        section: None,
                        detail: None,
                        error: Some(e),
                    },
                );
            }
        }
    });

    Ok(draft_id)
}

/// Cancel a running generation (idempotent).
#[tauri::command]
pub async fn cancel_one_ask(draft_id: String) -> Result<(), String> {
    if let Some(token) = registry().read().unwrap().get(&draft_id) {
        token.cancel();
    }
    Ok(())
}

/// Load a draft for display/resume (FR-010). Interrupted sessions surface as
/// re-runnable `failed` — never a half-draft presented as finished.
#[tauri::command]
pub async fn get_one_ask(
    pool: State<'_, Db>,
    draft_id: String,
) -> Result<OneAskView, String> {
    let mut st = state::load(pool.inner(), &draft_id)
        .await?
        .ok_or_else(|| format!("Unknown draft {draft_id}. Start a new ask."))?;
    let running = registry().read().unwrap().contains_key(&draft_id);
    if !running {
        st.status = normalize_interrupted(&st.status);
    }
    view_of(pool.inner(), &draft_id, st).await
}
