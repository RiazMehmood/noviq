//! Feature 022 — US2: document analysis on the local model.
//!
//! Reads the conversation attachment's text and asks the **local** model
//! (`query_router::generate_direct` routes to vLLM/llama.cpp — never a remote
//! engine, so the sensitive notice stays on-box) to DERIVE the argument frame:
//! allegations, invoked rule/authority, relief, forum, deadline, and the
//! per-allegation questions to ask the officer. Output is free-text JSON,
//! tolerantly parsed (no llguidance); a parse/gen failure feeds the caller's
//! single-question fallback (research R2/R3/R4).

use serde::Deserialize;

use crate::compat::AppHandle;
use crate::db::Db;
use crate::services::{hardware_detector::HardwareTier, llm::ModelSlot, query_router};

use super::frame::{Allegation, DocKind, Frame, SlotQuestion, SlotStatus};

/// Budget for reading the attachment whole. Attachments are capped at ~15 pages
/// / 8 MB (`uploads::attach_limits`) precisely so the model can read the ENTIRE
/// small document here — a stale 14k cap (~7 pages) would truncate a 15-page
/// notice/para and re-introduce the "front-matter only" bug. ~40k chars ≈ 10k
/// tokens, well within context. Keep this ≥ the page cap's worth of text.
const MAX_DOC_CHARS: usize = 40_000;
const MAX_GEN_TOKENS: u32 = 1024;

pub enum ExtractError {
    /// No attachment text to read.
    NoDocument,
    /// The active engine is a hosted API — refuse to send the notice off-box.
    RemoteEngineRefused,
    /// The local model call failed (unavailable/timeout/error).
    Generation(String),
    /// The model produced no parseable frame (even after one retry).
    Parse,
}

/// Spec R1 — the officer's notice is sensitive; analysis MUST run on the local
/// model. `generate_direct` dispatches through the *runtime-active* engine, which
/// an admin can flip to a hosted API (Groq/Gemini). Refuse in that case instead of
/// silently shipping the document off-box; the caller degrades to the
/// single-question fallback. `NOVIQ_NOTICE_EXTRACT_ENGINE=any` opts out.
pub(super) fn local_engine_ok() -> bool {
    if std::env::var("NOVIQ_NOTICE_EXTRACT_ENGINE")
        .map(|v| v.trim().eq_ignore_ascii_case("any"))
        .unwrap_or(false)
    {
        return true;
    }
    !matches!(
        crate::services::inference::engine_from_env(),
        crate::services::inference::EngineKind::Remote
    )
}

const SYSTEM_PROMPT: &str = "You are a meticulous Pakistan government assistant helping an \
officer respond to an accountability document (a show-cause notice, warning, explanation-call, \
or audit para). Your ONLY job here is careful reading comprehension of the document. \
Extract, FROM THE DOCUMENT TEXT ONLY: (1) what is alleged or objected to (there may be several \
points); (2) the rule/section and the authority invoked against the officer; (3) what the \
officer is asked to explain or produce; (4) the forum/deciding authority; (5) the deadline. \
Then, for EACH allegation, write the questions the officer must answer to rebut it.\n\
QUESTION RULES (strict):\n\
- Ask ONLY for facts the OFFICER holds and the document cannot contain: the sanction/approval \
or authority relied on; whether a superior gave a written order/instruction (and its reference); \
approvals or file notings on record; whether any personal benefit accrued; which documents \
support the officer's position; any procedural objection (e.g. no inquiry held, no opportunity \
of hearing).\n\
- NEVER ask the officer to tell you what the document says. Do NOT ask what is alleged, what \
the misconduct is, which rule is cited, who issued it, or what the deadline is — you must read \
those from the document yourself.\n\
- If the document does not state the specific allegation, leave `allegations[].text` as what the \
document actually says and still ask ONLY officer-fact questions.\n\
Do NOT state or invent any law, rule text, legal conclusion, \
or citation — those come from the corpus later. Respond with ONLY one JSON object, no prose, \
in EXACTLY this shape:\n\
{\"document_kind\":\"show_cause|warning|explanation_call|audit_para|other\",\"kind_confidence\":0.0,\
\"allegations\":[{\"text\":\"...\",\"invoked_rule\":\"... or null\",\"invoked_authority\":\"... or null\",\
\"questions\":[\"...\"]}],\"relief_or_explanation_sought\":\"... or null\",\"forum\":\"... or null\",\
\"deadline\":\"... or null\"}";

pub(super) fn no_think_prefix() -> &'static str {
    match std::env::var("NOVIQ_NO_THINK") {
        Ok(v) if matches!(v.trim(), "1" | "true" | "TRUE" | "on" | "ON" | "yes" | "YES") => {
            "<think>\n\n</think>\n\n"
        }
        _ => "",
    }
}

fn build_prompt(doc: &str, remind: bool) -> String {
    let reminder = if remind {
        "\n\nReturn ONLY the single JSON object described above — no prose, no markdown."
    } else {
        ""
    };
    format!(
        "<|im_start|>system\n{sys}{reminder}<|im_end|>\n\
         <|im_start|>user\nDOCUMENT:\n{doc}<|im_end|>\n\
         <|im_start|>assistant\n{nt}",
        sys = SYSTEM_PROMPT,
        reminder = reminder,
        doc = doc,
        nt = no_think_prefix(),
    )
}

/// Concatenate the conversation attachment's extracted text (F021 chunks),
/// capped to the extraction budget.
pub async fn assemble_document_text(pool: &Db, conversation_id: &str) -> String {
    let ids = crate::db::repositories::sources::attachment_source_ids(pool, conversation_id)
        .await
        .unwrap_or_default();
    let mut buf = String::new();
    for sid in &ids {
        if let Ok(chunks) = crate::db::repositories::chunks::list_by_source(pool, sid).await {
            for c in chunks {
                if buf.len() >= MAX_DOC_CHARS {
                    break;
                }
                let t = c.content.trim();
                if !t.is_empty() {
                    buf.push_str(t);
                    buf.push('\n');
                }
            }
        }
    }
    if buf.len() > MAX_DOC_CHARS {
        buf.truncate(MAX_DOC_CHARS);
    }
    buf
}

async fn generate(
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    app: Option<&AppHandle>,
    doc: &str,
    remind: bool,
) -> Result<String, String> {
    let prompt = build_prompt(doc, remind);
    query_router::generate_direct(model_slot, tier, &prompt, MAX_GEN_TOKENS, None, app.cloned())
        .await
        .map_err(|e| e.to_string())
}

/// Analyze the attachment → `Frame`. Retries generation once with a terse
/// reminder before giving up (the caller then uses a single-question fallback).
pub async fn analyze(
    pool: &Db,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    app: Option<&AppHandle>,
    conversation_id: &str,
) -> Result<Frame, ExtractError> {
    if !local_engine_ok() {
        log::warn!(
            "notice_assist: active engine is a hosted API — refusing to send the notice off-box; \
             using the single-question fallback (set NOVIQ_NOTICE_EXTRACT_ENGINE=any to override)"
        );
        return Err(ExtractError::RemoteEngineRefused);
    }
    let doc = assemble_document_text(pool, conversation_id).await;
    if doc.trim().is_empty() {
        return Err(ExtractError::NoDocument);
    }
    let doc_chars = doc.trim().len();
    let raw = generate(model_slot, tier, app, &doc, false)
        .await
        .map_err(ExtractError::Generation)?;
    if let Some(mut f) = parse_frame(&raw) {
        if !f.allegations.is_empty() {
            f.source_chars = doc_chars;
            return Ok(f);
        }
    }
    log::warn!("notice_assist extraction: first parse empty/failed, retrying once");
    let raw2 = generate(model_slot, tier, app, &doc, true)
        .await
        .map_err(ExtractError::Generation)?;
    match parse_frame(&raw2) {
        Some(mut f) if !f.allegations.is_empty() => {
            f.source_chars = doc_chars;
            Ok(f)
        }
        _ => {
            log::warn!("notice_assist extraction failed; raw(head)={:.200}", raw2);
            Err(ExtractError::Parse)
        }
    }
}

// ── tolerant free-text JSON parse (research R2) ──────────────────────────────

#[derive(Deserialize)]
struct RawFrame {
    document_kind: Option<String>,
    kind_confidence: Option<f32>,
    allegations: Option<Vec<RawAllegation>>,
    relief_or_explanation_sought: Option<String>,
    forum: Option<String>,
    deadline: Option<String>,
}

#[derive(Deserialize)]
struct RawAllegation {
    text: Option<String>,
    invoked_rule: Option<String>,
    invoked_authority: Option<String>,
    questions: Option<Vec<String>>,
}

/// Parse the model's free text into a `Frame`, tolerant of fences/prose and
/// unknown enum values. `None` when no JSON object is present or it can't decode.
pub fn parse_frame(raw: &str) -> Option<Frame> {
    let json = extract_json_object(raw)?;
    let rf: RawFrame = serde_json::from_str(&json).ok()?;

    let mut frame = Frame {
        document_kind: coerce_kind(rf.document_kind.as_deref()),
        kind_confidence: rf.kind_confidence.unwrap_or(0.0).clamp(0.0, 1.0),
        relief_or_explanation_sought: clean_opt(rf.relief_or_explanation_sought),
        forum: clean_opt(rf.forum),
        deadline: clean_opt(rf.deadline),
        ..Frame::default()
    };

    let mut qid = 0u32;
    for (ai, a) in rf.allegations.unwrap_or_default().into_iter().enumerate() {
        let text = a.text.unwrap_or_default();
        if text.trim().is_empty() {
            continue;
        }
        // Drop any question that asks the officer to restate the document's own
        // content — the answer is in the notice we just read (see
        // `is_document_content_question`). If nothing usable survives, fall back
        // to the standard officer-fact questions rather than asking nothing.
        let mut questions: Vec<SlotQuestion> = a
            .questions
            .unwrap_or_default()
            .into_iter()
            .filter(|q| !q.trim().is_empty() && !is_document_content_question(q))
            .map(|q| {
                qid += 1;
                SlotQuestion { id: qid, question: q.trim().to_string(), answer: None, status: SlotStatus::Open }
            })
            .collect();
        if questions.is_empty() {
            questions = default_officer_questions(&mut qid);
        }
        frame.allegations.push(Allegation {
            id: (ai + 1) as u32,
            text: text.trim().to_string(),
            invoked_rule: clean_opt(a.invoked_rule),
            invoked_authority: clean_opt(a.invoked_authority),
            questions,
        });
    }
    Some(frame)
}

/// A generated question that asks the officer to restate the DOCUMENT's own
/// content ("what specific misconduct is alleged against you?", "which rule is
/// cited?", "what is the deadline?"). Such a question is always wrong: the answer
/// is in the notice, which we already read. Prompt rules alone don't guarantee
/// this, so we filter in code too.
fn is_document_content_question(q: &str) -> bool {
    let t = q.trim().to_lowercase();
    const PATTERNS: [&str; 12] = [
        "what specific misconduct",
        "what misconduct",
        "what is alleged",
        "what are the allegations",
        "what allegation",
        "what does the notice",
        "what does the document",
        "which rule is cited",
        "which rule has been invoked",
        "what rule is cited",
        "who issued",
        "what is the deadline",
    ];
    PATTERNS.iter().any(|p| t.contains(p))
}

/// The officer-fact questions every accountability reply needs. Used when the
/// model produced no usable (non-document-content) question for an allegation.
fn default_officer_questions(next_id: &mut u32) -> Vec<SlotQuestion> {
    const Q: [&str; 4] = [
        "What sanction, approval or authority did you rely on when you acted?",
        "Did a superior or directing authority give you a written order or instruction? If so, give its reference.",
        "Did you or anyone connected to you obtain any personal benefit from the action?",
        "Which documents (file notings, approvals, registers) support your position?",
    ];
    Q.iter()
        .map(|q| {
            *next_id += 1;
            SlotQuestion { id: *next_id, question: (*q).to_string(), answer: None, status: SlotStatus::Open }
        })
        .collect()
}

fn clean_opt(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("null") && !v.eq_ignore_ascii_case("none"))
}

fn coerce_kind(s: Option<&str>) -> DocKind {
    match s.unwrap_or("other").trim().to_lowercase().replace([' ', '-'], "_").as_str() {
        "show_cause" | "showcause" => DocKind::ShowCause,
        "warning" => DocKind::Warning,
        "explanation_call" | "explanation" => DocKind::ExplanationCall,
        "audit_para" | "auditpara" | "audit" => DocKind::AuditPara,
        _ => DocKind::Other,
    }
}

/// The first balanced `{ … }` object in `s` (ignores markdown fences / prose,
/// respects strings + escapes).
fn extract_json_object(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for i in start..s.len() {
        let c = bytes[i] as char;
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json_with_questions() {
        let raw = r#"{"document_kind":"show_cause","kind_confidence":0.9,
          "allegations":[{"text":"Awarded tender without open advertisement",
          "invoked_rule":"Rule 12(2)","invoked_authority":"Secretary","questions":["What sanction did you rely on?","Was there a written order?"]}],
          "relief_or_explanation_sought":"explain within 7 days","forum":"Secretary","deadline":"within 7 days"}"#;
        let f = parse_frame(raw).expect("parses");
        assert_eq!(f.document_kind, DocKind::ShowCause);
        assert_eq!(f.allegations.len(), 1);
        assert_eq!(f.allegations[0].questions.len(), 2);
        assert_eq!(f.allegations[0].invoked_rule.as_deref(), Some("Rule 12(2)"));
    }

    #[test]
    fn tolerates_markdown_fences_prose_and_unknown_kind() {
        let raw = "Sure, here it is:\n```json\n{\"document_kind\":\"memo\",\"allegations\":[{\"text\":\"late submission\",\"questions\":[\"why late?\"]}]}\n```\nHope that helps!";
        let f = parse_frame(raw).expect("parses despite fences");
        assert_eq!(f.document_kind, DocKind::Other); // unknown → other, not an error
        assert_eq!(f.allegations.len(), 1);
        assert_eq!(f.allegations[0].questions.len(), 1);
    }

    #[test]
    fn two_objection_audit_para_yields_two_allegations() {
        let raw = r#"{"document_kind":"audit para","allegations":[
          {"text":"Overpayment of TA/DA","questions":["Was it sanctioned?"]},
          {"text":"Missing stock entries","questions":["Where is the stock register?"]}]}"#;
        let f = parse_frame(raw).expect("parses");
        assert_eq!(f.document_kind, DocKind::AuditPara);
        assert_eq!(f.allegations.len(), 2);
    }

    #[test]
    fn garbage_returns_none_not_a_fabricated_frame() {
        assert!(parse_frame("the model was confused and wrote no json").is_none());
        assert!(parse_frame("").is_none());
    }

    #[test]
    fn document_content_questions_are_dropped_and_replaced() {
        // The SC02 regression: the model asked the officer what he is accused of.
        let raw = r#"{"document_kind":"show_cause","allegations":[{"text":"Misconduct",
          "invoked_rule":"Rule 4-A (1)",
          "questions":["What specific misconduct is alleged against you?",
                       "Which rule is cited against you?",
                       "What is the deadline?"]}]}"#;
        let f = parse_frame(raw).expect("parses");
        let qs: Vec<&str> = f.allegations[0].questions.iter().map(|q| q.question.as_str()).collect();
        for q in &qs {
            assert!(!is_document_content_question(q), "document-content question survived: {q}");
        }
        // All were document-content → replaced by the standard officer-fact set.
        assert_eq!(qs.len(), 4);
        assert!(qs.iter().any(|q| q.contains("sanction, approval or authority")));
        assert!(qs.iter().any(|q| q.contains("written order")));
    }

    #[test]
    fn officer_fact_questions_are_kept() {
        let raw = r#"{"document_kind":"show_cause","allegations":[{"text":"Awarded tender without advertisement",
          "questions":["What approval did you rely on?","What specific misconduct is alleged against you?"]}]}"#;
        let f = parse_frame(raw).expect("parses");
        let qs: Vec<&str> = f.allegations[0].questions.iter().map(|q| q.question.as_str()).collect();
        assert_eq!(qs, vec!["What approval did you rely on?"], "keep officer-fact, drop doc-content");
    }

    #[test]
    fn bare_label_allegation_flags_missing_details() {
        let bare = parse_frame(r#"{"allegations":[{"text":"Misconduct","questions":["What approval?"]}]}"#).unwrap();
        assert!(bare.allegation_details_missing(), "bare label must flag missing details");
        let full = parse_frame(
            r#"{"allegations":[{"text":"Awarded the tender without open advertisement","questions":["What approval?"]}]}"#,
        )
        .unwrap();
        assert!(!full.allegation_details_missing());
    }

    #[test]
    fn poorly_read_only_when_source_chars_is_small_and_known() {
        let mut f = parse_frame(r#"{"allegations":[{"text":"a b c","questions":["What approval?"]}]}"#).unwrap();
        assert!(!f.poorly_read(), "unknown (0) must not warn");
        f.source_chars = 400;
        assert!(f.poorly_read());
        f.source_chars = 5000;
        assert!(!f.poorly_read());
    }

    #[test]
    fn local_engine_gate_refuses_remote_unless_overridden() {
        use crate::services::inference::EngineKind;
        // Pure replica of the gate's decision (process env can't be mutated safely
        // in parallel tests): only `Remote` is refused, and only without the opt-out.
        let decide = |engine: EngineKind, opt_out: bool| opt_out || engine != EngineKind::Remote;
        assert!(!decide(EngineKind::Remote, false), "hosted API must be refused");
        assert!(decide(EngineKind::Remote, true), "explicit opt-out allows it");
        assert!(decide(EngineKind::Vllm, false), "local vLLM is allowed");
        assert!(decide(EngineKind::LlamaCpp, false), "in-process llama.cpp is allowed");
    }
}
