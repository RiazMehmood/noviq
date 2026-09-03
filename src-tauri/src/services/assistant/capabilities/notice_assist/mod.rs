//! Feature 022 — Notice Assist (`/help`) capability.
//!
//! Intercepted in `run_turn` (like transform/additive): a `/help|/reply|/audit`
//! command, or an already-open assist session, routes the turn here.
//!
//! **Noviq does not write the officer's reply.** It arms him and then checks him:
//!   1. **Analyze** (`extract`) — the local model reads the attached notice and
//!      derives the charge, the statute invoked, the forum and the deadline.
//!   2. **Ground** (`compose`) — the invoked provision *and the provisions that
//!      protect him* (inquiry, hearing, penalty scale, appeal) are retrieved
//!      verbatim from the SAME statute and cited.
//!   3. **CoWrite** — every later turn is a paragraph of HIS reply. We say which
//!      `[n]` supports each point, what still needs evidence, and what weakens him.
//!
//! Integrity: no fact the officer did not give, no rule the corpus did not supply.
//! The citation gate strips any provision the retrieved context cannot support, so
//! an invented rule cannot ship. Earlier builds interrogated the officer with a
//! questionnaire and produced a ready-made letter; both were removed on request —
//! the officer wants support while HE writes, not a template.

pub mod frame;
pub mod session;

// `compose` is public to the crate so the main retrieval path can reuse its
// instrument-disambiguation primitives (`instrument_score`) rather than growing
// a second, divergent copy — the two-places-measuring-one-thing trap.
pub(crate) mod compose;
mod extract;

use std::sync::Arc;

use crate::compat::AppHandle;
use crate::db::Db;
use crate::services::assistant::{
    self, context_budget::EstimateCounter, intent::Capability, memory::MemoryContext,
    persona::PersonaConfig, AssembleInput, TurnResponse,
};
use crate::services::query_trace::QueryTrace;
use crate::services::{
    embedder::EmbedderState, hardware_detector::HardwareTier, llm::ModelSlot, scheduler::Scheduler,
};

use frame::{Allegation, Frame, NoticeAssistState, Phase};

/// True when the officer asked to leave assist mode (`/cancel`, or plain words).
fn is_exit(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    crate::services::assistant::intent::is_assist_exit_command(text)
        || t == "cancel assist"
        || t == "stop assist"
        || t == "exit assist"
}

/// The commands this capability owns. Anything else beginning with `/` is a
/// typo — answered instantly, never sent to retrieval or the model.
const KNOWN_COMMANDS: &str =
    "`/help` — respond to a notice · `/reply` — show-cause · `/audit` — audit para · `/cancel` — leave assist mode";

#[allow(clippy::too_many_arguments)]
pub async fn run(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    scheduler: &Arc<Scheduler>,
    app: Option<&AppHandle>,
    cfg: &PersonaConfig,
    mem: &MemoryContext,
    conversation_id: &str,
    text: &str,
    scope_ids: &[String],
    trace: &mut QueryTrace,
) -> Result<TurnResponse, String> {
    let _ = scheduler; // grounding retrieval is not scheduler-gated (no generation)

    use crate::services::assistant::intent;

    trace.step("Notice Assist (/help)", &[
        ("Command", intent::is_notice_assist_command(text).to_string()),
        ("Has open session",
            session::load(pool, conversation_id).await.ok().flatten().is_some().to_string()),
        ("Sources in scope", scope_ids.len().to_string()),
    ]);

    // Exit → clear the session so ordinary chat resumes. Works whether or not a
    // session is open, so `/cancel` is always a cheap, local answer.
    if is_exit(text) {
        let had_session = session::load(pool, conversation_id).await.ok().flatten().is_some();
        session::delete(pool, conversation_id).await?;
        let msg = if had_session {
            "Left assist mode. Ask me anything, or type `/help` to respond to another notice."
        } else {
            "You're not in assist mode. Ask me anything, or type `/help` to respond to a notice."
        };
        return Ok(reply(cfg, mem, model_slot, tier, conversation_id, text, msg.into()));
    }

    // An unrecognised slash command is a typo, not a question. Answer it here —
    // it must never reach retrieval or the model.
    if intent::is_slash_command(text) && !intent::is_notice_assist_command(text) {
        let cmd = text.trim().split_whitespace().next().unwrap_or("/").to_string();
        return Ok(reply(
            cfg, mem, model_slot, tier, conversation_id, text,
            format!("Unknown command `{cmd}`.\n\nAvailable: {KNOWN_COMMANDS}"),
        ));
    }

    // A fresh `/help|/reply|/audit` command (re)starts the session; otherwise we
    // continue the open session with the officer's latest turn.
    let command = intent::is_notice_assist_command(text);
    let mut state = if command {
        NoticeAssistState::new()
    } else {
        session::load(pool, conversation_id).await?.unwrap_or_else(NoticeAssistState::new)
    };

    // Does THIS conversation have a live attachment to read?
    let has_attachment = !crate::db::repositories::sources::attachment_source_ids(pool, conversation_id)
        .await
        .unwrap_or_default()
        .is_empty();

    // Sessions persisted by the questionnaire build land in a retired phase; drive
    // them straight into co-writing (their frame is still valid).
    if matches!(state.phase, Phase::Gather | Phase::Ground | Phase::Draft | Phase::Done) {
        state.phase = Phase::CoWrite;
    }

    // Phase machine — no questionnaire. Analyze reads the notice, grounds the
    // invoked rule + the officer's safeguards, and hands him a cited support
    // brief. Every later turn is a paragraph of HIS reply, which we critique.
    let answer = match state.phase {
        Phase::Analyze => {
            if !has_attachment {
                "**Assist mode.** Attach the notice, warning, or audit para (paperclip, or type \
                 `@` and pick the file). I'll read it, show you the rule invoked against you and \
                 the provisions that protect you — then you write the reply and I'll check it."
                    .to_string()
            } else {
                emit_phase(app, "notice_analyze");
                let mut prefix = String::new();
                let frame = match extract::analyze(pool, model_slot, tier, app, conversation_id).await {
                    Ok(f) => f,
                    Err(extract::ExtractError::RemoteEngineRefused) => {
                        prefix.push_str(
                            "_⚠ Automatic reading is disabled because chat is currently served by a \
                             hosted API. Your notice was **not** sent off this machine. Switch the \
                             inference engine to the local GPU to enable it._\n\n",
                        );
                        fallback_frame()
                    }
                    // No text (bad scan) / generation / parse failure → never a
                    // fabricated frame; the brief says the copy was unreadable.
                    Err(_) => fallback_frame(),
                };
                state.frame = frame;

                emit_phase(app, "notice_ground");
                compose::ground(pool, embedder, &mut state.frame, scope_ids).await;
                // Record WHY each provision grounded the way it did. Trace 020ac9ac
                // reported "not found in the indexed corpus" and the trace held none
                // of the inputs that produced it — the whole assist path emitted one
                // step, so the failure could not be diagnosed from the log at all.
                trace.step("Notice Assist — grounding outcome", &[
                    ("Provisions invoked", state.frame.cited_rule_text.len().to_string()),
                    ("Per provision", if state.frame.cited_rule_text.is_empty() {
                        "(none identified in the notice)".to_string()
                    } else {
                        state.frame.cited_rule_text.iter()
                            .map(|c| format!(
                                "{}={:?}{}",
                                c.invoked_ref,
                                c.outcome,
                                c.citation.as_ref()
                                    .map(|x| format!(" ← {}", x.instrument))
                                    .unwrap_or_default(),
                            ))
                            .collect::<Vec<_>>().join(", ")
                    }),
                    ("Support provisions", state.frame.support_rules.len().to_string()),
                    ("Sources in scope", scope_ids.len().to_string()),
                ]);
                state.phase = Phase::CoWrite;
                let brief = format!("{prefix}{}", compose::support_brief(&state));
                state.drafted_reply = Some(brief.clone());
                session::save(pool, conversation_id, &state).await?;
                return Ok(grounded_turn(
                    cfg, mem, model_slot, tier, conversation_id, text, &state, brief,
                ));
            }
        }
        // The officer is writing. Each turn is a paragraph of HIS reply; we check
        // it against the grounded provisions. We never write it for him.
        Phase::CoWrite => {
            if state.frame.allegations.is_empty() {
                "Attach the notice first (paperclip or `@`), then start writing your reply here."
                    .to_string()
            } else if wants_us_to_write(text) {
                "I won't write the reply for you — the words must be yours, and an authority reads \
                 a template instantly. Write a paragraph (even roughly) and I'll check it against \
                 the provisions above, tell you which `[n]` supports each point, and flag anything \
                 you haven't evidenced."
                    .to_string()
            } else if text.trim().split_whitespace().count() < 4 {
                "Write a sentence or two of your reply and I'll check it. For example: what \
                 authority you acted under, or why the delay occurred."
                    .to_string()
            } else {
                emit_phase(app, "notice_review");
                let review = critique(model_slot, tier, app, &state, text).await;
                session::save(pool, conversation_id, &state).await?;
                return Ok(grounded_turn(
                    cfg, mem, model_slot, tier, conversation_id, text, &state, review,
                ));
            }
        }
        // Unreachable: normalised above.
        Phase::Gather | Phase::Ground | Phase::Draft | Phase::Done => {
            "Write your reply here and I'll check it, or `/cancel` to leave assist mode.".to_string()
        }
    };

    session::save(pool, conversation_id, &state).await?;
    Ok(reply(cfg, mem, model_slot, tier, conversation_id, text, answer))
}

/// The officer asked Noviq to write the reply. We deliberately decline.
fn wants_us_to_write(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    t == "draft it"
        || t == "draft"
        || t.contains("write it for me")
        || t.contains("write the reply")
        || t.contains("draft the reply")
        || t.contains("prepare the reply")
}

/// Critique one paragraph of the officer's draft against the grounded provisions.
///
/// The model may ONLY reason over the provisions supplied verbatim; the citation
/// gate then strips any provision reference the context cannot support, so an
/// invented rule cannot survive. Never rewrites the whole reply.
async fn critique(
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    app: Option<&AppHandle>,
    state: &NoticeAssistState,
    draft: &str,
) -> String {
    let provisions = compose::numbered_provisions(&state.frame);
    if provisions.trim().is_empty() {
        return "I have no grounded provisions for this notice (the governing Rules are not in the \
                indexed corpus), so I cannot check your paragraph against the law. Index the \
                statute the notice cites, then paste your paragraph again."
            .to_string();
    }
    if !extract::local_engine_ok() {
        return "_⚠ Review is disabled because chat is currently served by a hosted API — your \
                draft was not sent off this machine. Switch to the local GPU engine._"
            .to_string();
    }

    let sys = "You are a senior Pakistan government advisor helping an officer write HIS OWN reply \
to a show-cause notice or audit para. You will be given the governing PROVISIONS (verbatim, each \
tagged [n]) and one PARAGRAPH of the officer's draft.\n\
Respond with short bullets under exactly these headings:\n\
**Support** — for each point he makes that a provision backs, name the point and cite [n].\n\
**Needs evidence** — any statement asserting a fact with no document behind it; say which document \
he must attach.\n\
**Risk** — anything that weakens him (an admission, an unsupported legal claim, a rule cited that \
is NOT among the provisions).\n\
**Sharper wording** — ONE improved sentence he could use, in his own voice.\n\
RULES: Use ONLY the provisions given. Never quote or cite a rule that is not among them. Never \
invent facts. Do not rewrite the whole paragraph. Be brief and concrete.";

    let prompt = format!(
        "<|im_start|>system\n{sys}<|im_end|>\n\
         <|im_start|>user\nPROVISIONS:\n{provisions}\n\nOFFICER'S PARAGRAPH:\n{draft}<|im_end|>\n\
         <|im_start|>assistant\n{nt}",
        sys = sys,
        provisions = provisions,
        draft = draft.trim(),
        nt = extract::no_think_prefix(),
    );

    match crate::services::query_router::generate_direct(
        model_slot, tier, &prompt, 640, None, app.cloned(),
    )
    .await
    {
        Ok(out) if !out.trim().is_empty() => out,
        _ => "I couldn't review that paragraph just now. Try again, or rephrase it.".to_string(),
    }
}

fn emit_phase(app: Option<&AppHandle>, phase: &str) {
    if let Some(a) = app {
        use crate::compat::Emitter;
        let _ = a.emit("assistant://progress", serde_json::json!({ "phase": phase }));
    }
}

/// The drafted reply turn — carries the grounded rules as real citations, gated
/// A grounded assist turn — carries the retrieved provisions as real citations,
/// gated against the verbatim corpus text they came from (FR-008, Principle XI).
/// Any provision the answer names but the context cannot support is flagged, so an
/// invented rule can never ship.
#[allow(clippy::too_many_arguments)]
fn grounded_turn(
    cfg: &PersonaConfig,
    mem: &MemoryContext,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    conversation_id: &str,
    text: &str,
    state: &NoticeAssistState,
    answer: String,
) -> TurnResponse {
    let context = compose::gate_context(&state.frame);
    let citations = compose::citations(&state.frame);
    let window = assistant::resolve_active_window(model_slot, tier);
    let plan = assistant::plan_budget(
        &EstimateCounter, window, Capability::Draft, cfg, false, mem, text, &[context.clone()],
    );
    assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: Capability::Draft,
        answer,
        context: &context,
        citations,
        plan: &plan,
        fill_threshold: cfg.settings.context_fill_threshold,
        chat_fill: None,
        offer_tailor: false,
    })
}

/// Fallback frame (research R4) when the attached copy cannot be read. It states
/// the fact; it never fabricates an allegation.
fn fallback_frame() -> Frame {
    Frame {
        allegations: vec![Allegation {
            id: 1,
            text: "Not legible".into(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// redacting those references — these turns are questions, not legal claims.
fn reply(
    cfg: &PersonaConfig,
    mem: &MemoryContext,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    conversation_id: &str,
    text: &str,
    answer: String,
) -> TurnResponse {
    let window = assistant::resolve_active_window(model_slot, tier);
    let plan = assistant::plan_budget(
        &EstimateCounter, window, Capability::Guidance, cfg, false, mem, text, &[],
    );
    let context = answer.clone();
    assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: Capability::Guidance,
        answer,
        context: &context,
        citations: Vec::new(),
        plan: &plan,
        fill_threshold: cfg.settings.context_fill_threshold,
        chat_fill: None,
        offer_tailor: false,
    })
}
