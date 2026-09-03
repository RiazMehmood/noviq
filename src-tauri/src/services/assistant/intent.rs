//! T010 — deterministic **capability classification** (FR-005b, research R3).
//!
//! Reuses the proven weighted-cue classifier (`intent_router::classifier`) and
//! maps its `Intent` onto the seven F015 capabilities, adding high-precision
//! cues for the two the legacy router lacks (draft, conflict). Default:
//! `Guidance`. No LLM call (Constitution II — cheapest tier first); a misroute
//! still cannot fabricate (the citation gate covers it).

use crate::services::counting;
use crate::services::intent_router::classifier;
use crate::services::intent_router::types::Intent;
use regex_lite::Regex;
use std::sync::OnceLock;

/// The capability a turn is dispatched to on the single conversational surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Guidance,
    Draft,
    SummarizeDoc,
    Compare,
    Conflict,
    AmendmentChain,
    ActionItems,
    /// ADR-0019 — deterministic count/list, answered by SQL (never the LLM).
    Count,
    /// Arithmetic over an attached spreadsheet, computed in Rust (never the
    /// LLM, which scored 19/53 on exactly these questions). The model only
    /// translates the question into a plan.
    Tabular,
}

impl Capability {
    /// True for capabilities that operate on user-named documents and therefore
    /// may need scope confirmation (FR-005e).
    pub fn is_document_scoped(self) -> bool {
        matches!(
            self,
            Capability::SummarizeDoc
                | Capability::Compare
                | Capability::Conflict
                | Capability::ActionItems
        )
    }

    /// Stable wire name (matches the `capability` field in the command contract).
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Guidance => "guidance",
            Capability::Draft => "draft",
            Capability::SummarizeDoc => "summarize_doc",
            Capability::Compare => "compare",
            Capability::Conflict => "conflict",
            Capability::AmendmentChain => "amendment_chain",
            Capability::ActionItems => "action_items",
            Capability::Count => "count",
            Capability::Tabular => "tabular",
        }
    }

    /// True for whole-/cross-document capabilities that run via map-reduce
    /// (FR-005d) rather than a single pass.
    pub fn is_map_reduce(self) -> bool {
        matches!(
            self,
            Capability::SummarizeDoc | Capability::Compare | Capability::Conflict | Capability::ActionItems
        )
    }
}

/// "draft/write/prepare a summary/note/…" — an explicit drafting request.
fn draft_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?i)\b(draft|prepare|compose|write up)\b.{0,30}\b(summary|note|noting|letter|memo|minute|memorandum)\b|\b(draft|write|prepare)\s+(me\s+)?a\b",
        )
        .expect("draft cue regex compiles")
    })
}

/// Conflict/contradiction language — distinct from compare.
fn conflict_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?i)\b(conflicts?|contradict(s|ion|ory)?|inconsisten(t|cy|cies)|clash(es)?|at odds|repugnan(t|cy)|incompatib)\b",
        )
        .expect("conflict cue regex compiles")
    })
}

/// True when the turn is a summarise request (any form of "summar…"). Used to
/// trigger the pre-baked whole-document summary intercept INDEPENDENTLY of the
/// capability classifier: `parse_document_scope` only fires when the title
/// carries a doc-type keyword (act/rules/ordinance…), so a document named without
/// one — e.g. "Sindh Delegation of Financial Powers 2019" — classified as Guidance
/// and never reached the pre-baked path. `try_prebaked` still requires a real
/// named-document match, so a topical "summarise the penalties" (no named doc)
/// harmlessly falls through.
pub fn is_summarize_request(query: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)\bsummar(y|ies|ise|ize|ised|ized|isation|ization)\b")
            .expect("summarize-request regex compiles")
    });
    re.is_match(query)
}

/// True when a summary request is explicitly broad / whole-corpus (so the
/// expensive map-reduce over every provision is what the user actually wants),
/// rather than a topical question that merely begins with "summarize".
fn is_broad_overview(query: &str) -> bool {
    let q = query.to_lowercase();
    [
        "all documents", "all the documents", "all indexed", "entire corpus",
        "whole corpus", "across all", "everything", "all chapters",
        "table of contents", "all the acts", "all acts", "all rules",
    ]
    .iter()
    .any(|m| q.contains(m))
}

/// True when the turn asks to REWRITE the previous answer — reformat / restyle /
/// translate / shorten / expand — rather than ask a new question. Deterministic;
/// routes to the no-retrieval transform path (`capabilities::transform`). Only
/// acted on when a prior assistant answer exists (guarded in `run_turn`), so a
/// misdetection at worst rewrites the last answer instead of answering anew.
pub fn is_transform_request(query: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(make (it|this)|rewrite|re-write|rephrase|reword|reformat|redo (it|this)|shorten|shorter|make it (short|brief|long)|longer|lengthen|(expand|elaborate)( on)? (it|this)|condense|simplif(y|ies)|polish|more (formal|professional|concise|detailed|brief|elaborate|polished)|less (formal|detailed|wordy)|in (urdu|sindhi|english)|translate|as a (formal )?(note|letter|memo|draft|email|summary)|bullet(s| points?)?|change the tone|tone (it )?down)\b",
        )
        .expect("transform-request regex compiles")
    });
    re.is_match(query)
}

/// True when the turn asks to EXTEND the previous answer with a new aspect —
/// "also cover…", "add a section on…", "expand on the penalties" — content the
/// prior answer lacks, so it needs fresh retrieval (unlike a transform, which
/// only restyles). Checked AFTER `is_transform_request` in `run_turn`, so
/// anaphoric rewrites ("expand on this") stay on the no-retrieval path. Only
/// acted on when a prior assistant answer exists.
pub fn is_additive_request(query: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?i)(^(please |kindly |now |then )?(add|include)\b|\b(can|could|will|would) you (add|include)\b|\balso (cover|include|add|mention|tell|explain|discuss|describe|list|give|state|talk about)\b|\bin addition\b|\badditionally\b|\bas well as\b|\b(expand|elaborate) on\b|\bwhat else\b|\bdon'?t forget\b)",
        )
        .expect("additive-request regex compiles")
    });
    re.is_match(query)
}

/// Classify a user turn into a capability (deterministic).
pub fn classify_capability(query: &str) -> Capability {
    // High-precision overrides the legacy router doesn't cover.
    if draft_re().is_match(query) {
        return Capability::Draft;
    }
    if conflict_re().is_match(query) {
        return Capability::Conflict;
    }
    // ADR-0019 — a count of a recognised structural/corpus unit ("how many
    // rules / sections / acts …") is a deterministic SQL capability. Gated on
    // `is_structural_count_query` (NOT bare "how many"), so overloaded factual
    // questions ("how many members must be present", "how many days to appeal")
    // still route to normal retrieval.
    if counting::is_structural_count_query(query) {
        return Capability::Count;
    }
    // Map the proven intent classifier onto a capability.
    let (intent, _conf) = classifier::classify(query);
    match intent {
        // Whole-corpus map-reduce (SummarizeDoc) runs ONE LLM pass per
        // window-sized batch over EVERY in-scope provision ("full coverage by
        // construction" — capabilities/mod.rs). On a large corpus + a weak GPU
        // that is minutes per turn (measured 2026-06-27: 7.1 min for a single
        // "summarize …" turn). A *topical* summary that names no document is
        // really a targeted question ("summarize the approval limits of the
        // DDWP and CDWP") — route it to Guidance (fast hybrid RAG, ~6 s).
        // Only take the expensive map-reduce path when the user scopes to a
        // named instrument OR explicitly asks for a broad/whole-corpus overview.
        // See CHANGELOG_DEV 2026-06-27.
        Intent::Summarize => {
            if !parse_document_scope(query).is_empty() || is_broad_overview(query) {
                Capability::SummarizeDoc
            } else {
                Capability::Guidance
            }
        }
        Intent::CrossCompare => Capability::Compare,
        Intent::Timeline | Intent::Difference => Capability::AmendmentChain,
        Intent::ActionItems => Capability::ActionItems,
        // Lookup / Procedure / ListChildren / Fallback → general guidance.
        _ => Capability::Guidance,
    }
}

/// Document-scope parse (FR-005e): candidate named-instrument mentions in the
/// query (e.g. "Sindh Local Government Act 2013", "Police Order 2002"). Returns
/// the matched spans; full source-id resolution stays with the existing scope
/// machinery (`intent_router::resolve_scope_source_ids`) which confirms before use.
pub fn parse_document_scope(query: &str) -> Vec<String> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        // up to ~6 leading words, then a document-type keyword, then optional year.
        Regex::new(
            r"(?i)([A-Za-z][\w.&'\-]*(?:\s+[\w.&'\-]+){0,6}\s+(?:act|ordinance|rules?|regulations?|policy|order|constitution|code))(?:\s+(?:19|20)\d{2})?",
        )
        .expect("doc-scope regex compiles")
    });
    re.find_iter(query).map(|m| m.as_str().trim().to_string()).collect()
}

/// True for ANY leading slash command (`/help`, `/cancel`, `/whatever`).
///
/// A slash command is an instruction to the app, never a question for the corpus.
/// Before this existed, an unrecognised command fell through to ordinary chat: a
/// `/cancel` sent outside assist mode ran a full RAG turn on the word "cancel"
/// and answered about the *cancellation of a bidding process* from the SPPRA
/// rules — 38 s of inference and a nonsense answer. `run_turn` now routes every
/// slash command away from retrieval.
pub fn is_slash_command(query: &str) -> bool {
    let t = query.trim_start();
    t.starts_with('/') && t[1..].chars().next().is_some_and(|c| c.is_ascii_alphabetic())
}

/// True when the turn is a greeting or an "introduce yourself" / capability
/// question — not a corpus question. These are answered with a short intro to
/// Noviq + suggested actions and carry NO citations (nothing is retrieved).
/// Conservative: matches only a SHORT message (so "tell me about the powers of
/// the Chief Minister" is NOT caught) that is a bare greeting or an about/help
/// phrase.
/// The vocabularies live in `bureaucrat.toml` `[guardrails]` (Constitution VI), so
/// the lists can be tuned from real `query_log` traffic without a rebuild.
pub fn is_greeting_or_intro(query: &str) -> bool {
    let g = &crate::services::assistant::persona::PersonaConfig::get().guardrails;
    is_greeting_or_intro_with(query, &g.greetings, &g.about)
}

/// Pure form of [`is_greeting_or_intro`] — takes its vocabularies, so the rule is
/// unit-tested without loading the process-wide persona config.
pub fn is_greeting_or_intro_with(query: &str, greetings: &[String], about: &[String]) -> bool {
    let t = query.trim().trim_end_matches(['.', '!', '?']).to_lowercase();
    if t.is_empty() {
        return false;
    }
    // Bare greetings (exact, so "hi" ≠ "highway").
    if greetings.iter().any(|g| g.as_str() == t) {
        return true;
    }
    // About / capability phrases — only on a short message (≤ 10 words) so we don't
    // swallow a real legal question that happens to contain one of these words.
    let words = t.split_whitespace().count();
    if words > 10 {
        return false;
    }
    about.iter().any(|p| t.contains(p.as_str()))
}

/// True when the turn carries no answerable question — a bare probe typed to
/// test the box: a single stray character, pure digits, punctuation only, or a
/// known filler token ("a", "test", "123", "asdf", "...", "qwerty"). These must
/// NOT reach retrieval + a ~30 s inference that would answer junk with junk
/// citations; they get a short nudge instead (see `assistant::run_turn`).
///
/// Deliberately conservative: a single MEANINGFUL word (e.g. "penalties",
/// "misconduct", "SPPRA") is a lazy but real query and is NOT caught — only
/// obviously content-free input is. Bare greetings are handled separately by
/// `is_greeting_or_intro`.
/// The probe list lives in `bureaucrat.toml` `[guardrails]` (Constitution VI).
pub fn is_low_signal(query: &str) -> bool {
    is_low_signal_with(query, &crate::services::assistant::persona::PersonaConfig::get().guardrails.probes)
}

/// Pure form of [`is_low_signal`] — takes its probe list, so the rule is
/// unit-tested without loading the process-wide persona config.
pub fn is_low_signal_with(query: &str, probes: &[String]) -> bool {
    let t = query.trim().to_lowercase();
    if t.is_empty() {
        return true;
    }
    // Alphanumeric skeleton, punctuation/whitespace removed.
    let skeleton: String = t.chars().filter(|c| c.is_alphanumeric()).collect();
    // Pure punctuation / symbols ("...", "??", "!!").
    if skeleton.is_empty() {
        return true;
    }
    // One or two alphanumeric characters ("a", "ab", "1", "42").
    if skeleton.chars().count() <= 2 {
        return true;
    }
    // Pure digits ("123", "007").
    if skeleton.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // Known probe / filler messages — exact match on the whole trimmed message
    // (with trailing sentence punctuation removed), so "test the SPPRA rules" or
    // "sample selection under PPRA" are NOT caught.
    let normalized = t.trim_end_matches(['.', '!', '?', ' ']);
    probes.iter().any(|p| p.as_str() == normalized)
}

/// True when the turn opens the Notice Assist (`/help`) flow — a leading slash
/// command `/help`, `/reply`, or `/audit` (case-insensitive, tolerant of trailing
/// text). Feature 022: routed in `run_turn` before capability dispatch.
pub fn is_notice_assist_command(query: &str) -> bool {
    starts_with_command(query, &["/help", "/reply", "/audit"])
}

/// True when the officer asks to leave assist mode. Recognised even when no
/// session is open, so `/cancel` never reaches the corpus.
pub fn is_assist_exit_command(query: &str) -> bool {
    starts_with_command(query, &["/cancel", "/exit", "/stop", "/done"])
}

fn starts_with_command(query: &str, cmds: &[&str]) -> bool {
    let t = query.trim_start().to_lowercase();
    cmds.iter().any(|cmd| {
        t == *cmd || t.starts_with(&format!("{cmd} ")) || t.starts_with(&format!("{cmd}\n"))
    })
}

/// True when the turn refers to the conversation's *attachment* rather than the
/// wider corpus — "this document", "the attached file", "from this", "the doc I
/// uploaded", etc. When it fires AND the conversation has a live attachment, the
/// turn is scoped strictly to that attachment (see `assistant::run_turn`).
///
/// Deliberately conservative: it keys on a demonstrative/attach verb paired with
/// a document noun (or the bare phrase "from this"), so an ordinary question that
/// merely contains the word "document" does NOT hijack the scope.
pub fn references_attachment(query: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:(?:this|that|the|my|attached|uploaded|above)\s+(?:attached\s+|uploaded\s+)?(?:document|doc|file|pdf|attachment|upload)|(?:from|in|within|of|per)\s+this\b|attached\s+(?:here|herewith)|i\s+(?:just\s+)?(?:attached|uploaded)|(?:you|it)\s+(?:the\s+)?attached)",
        )
        .expect("attachment-reference regex compiles")
    });
    re.is_match(query)
}

/// True when the turn is asking for external legal authority — "which laws /
/// rules / sections support this", "what provision governs…", "is there a legal
/// basis…". Such a question needs the CORPUS even when it references an
/// attachment (the attached show-cause / audit para is only the premise; the
/// governing law lives in the corpus), so it must NOT collapse to attachment-only
/// scope. See `assistant::run_turn`.
pub fn seeks_legal_authority(query: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:laws?|rules?|sections?|provisions?|acts?|ordinances?|regulations?|articles?|clauses?|schedules?|statute|legal\s+basis|authorit(?:y|ies)|precedent|case\s+law|jurisdiction|grounds?)\b",
        )
        .expect("legal-authority regex compiles")
    });
    re.is_match(query)
}

/// True when the turn should be answered STRICTLY from the conversation's
/// attachment (extract/summarize the document's own content) rather than the
/// corpus: it references the attachment AND is not seeking external legal
/// authority.
///
/// NOTE (scope-routing 2026-07): no longer the scope gate — the wording-based
/// gate leaked legal questions about an attachment to the whole corpus, so
/// `run_turn` now scopes to a live attachment by DEFAULT (narrow-first) and
/// offers a widen button. Retained for reference/tests and possible reuse.
#[allow(dead_code)]
pub fn wants_attachment_only(query: &str) -> bool {
    references_attachment(query) && !seeks_legal_authority(query)
}

/// Extract the first provision reference from a turn (e.g. "Section 18",
/// "Article 25A", "Rule-6(c)") — the lookup key for the amendment-chain
/// capability (US8). Returns `None` when the turn names no provision.
pub fn extract_provision_ref(query: &str) -> Option<String> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        Regex::new(
            r"(?i)\b(article|rule|section|regulation|clause|schedule|chapter)[-\s]+\d+[0-9A-Za-z()\-]*",
        )
        .expect("provision-ref regex compiles")
    });
    re.find(query).map(|m| m.as_str().trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_intro_matches_greetings_and_about_not_real_questions() {
        // Greetings + about/capability phrases → true.
        for q in [
            "hi", "Hello", "Assalam o Alaikum", "who are you?", "tell me about yourself",
            "what can you do?", "How can you help me", "what is Noviq",
        ] {
            assert!(is_greeting_or_intro(q), "should greet: {q:?}");
        }
        // Real legal questions → false (must not be swallowed).
        for q in [
            "what are the penalties under the E&D Rules 1973",
            "tell me about the powers of the Chief Minister under the Rules of Business",
            "who is the appointing authority for BS-18 posts",
            "summarize the SPPRA Rules 2010",
            "highway construction rules",
        ] {
            assert!(!is_greeting_or_intro(q), "should NOT greet: {q:?}");
        }
    }

    #[test]
    fn low_signal_matches_probes_not_real_questions() {
        // Content-free probes → true.
        for q in [
            "a", "ab", "  ", "...", "??", "!!", "1", "42", "123", "007",
            "test", "Testing", "test test", "asdf", "QWERTY", "abc", "xyz",
            "blah blah", "foo", "dummy", "sample", "ping", "n/a", "lorem ipsum",
        ] {
            assert!(is_low_signal(q), "should be low-signal: {q:?}");
        }
        // Lazy-but-real single/short words + real questions → false.
        for q in [
            "penalties", "misconduct", "SPPRA", "constitution", "appeal",
            "test the SPPRA rules", "sample selection under PPRA",
            "what are the penalties under the E&D Rules 1973",
            "who is the appointing authority",
        ] {
            assert!(!is_low_signal(q), "should NOT be low-signal: {q:?}");
        }
    }

    #[test]
    fn extracts_provision_reference_for_amendment_lookups() {
        assert_eq!(
            extract_provision_ref("how has section 18 been amended over time").as_deref(),
            Some("section 18"),
        );
        assert_eq!(extract_provision_ref("show the amendment history of Article 25A").as_deref(), Some("Article 25A"));
        assert!(extract_provision_ref("what changed recently").is_none());
    }

    #[test]
    fn detects_attachment_references() {
        for q in [
            "summarize this document",
            "brief me on the attached document",
            "what does the attached file say about leave",
            "from this, what is the approval limit",
            "give me the action items from this document",
            "I just attached the policy — summarize it",
            "explain the uploaded pdf",
        ] {
            assert!(references_attachment(q), "should detect attachment reference: {q}");
        }
        // Must NOT fire on ordinary questions that merely mention documents.
        for q in [
            "what documents are required for a leave application",
            "which act governs procurement",
            "list the departments in the Sindh Rules of Business",
            "how do I draft a summary note",
        ] {
            assert!(!references_attachment(q), "should NOT hijack scope: {q}");
        }
    }

    #[test]
    fn detects_notice_assist_commands() {
        for q in ["/help", "/HELP", "  /reply", "/audit please", "/help summarize this notice"] {
            assert!(is_notice_assist_command(q), "should be a command: {q}");
        }
        for q in ["help me", "what does /help do", "reply to this", "/helpful hints", "summarize"] {
            assert!(!is_notice_assist_command(q), "should NOT be a command: {q}");
        }
    }

    #[test]
    fn every_slash_command_is_kept_away_from_retrieval() {
        // The regression: "/cancel" outside assist mode ran a 38 s RAG turn and
        // answered from the SPPRA rules about cancelling a bidding process.
        for q in ["/cancel", "/CANCEL", "  /exit", "/done", "/stop", "/help", "/audit", "/wat"] {
            assert!(is_slash_command(q), "must be treated as a command: {q}");
        }
        // Not commands: ordinary questions, paths, and bare slashes.
        for q in ["cancel the tender", "what is /help", "/ 5 marks", "/", "a/b testing"] {
            assert!(!is_slash_command(q), "must NOT be a command: {q}");
        }
        assert!(is_assist_exit_command("/cancel"));
        assert!(is_assist_exit_command("/EXIT"));
        assert!(!is_assist_exit_command("/help"));
        assert!(!is_assist_exit_command("cancel"));
        // Exit commands are not assist-entry commands, and vice versa.
        assert!(!is_notice_assist_command("/cancel"));
        assert!(is_notice_assist_command("/reply"));
    }

    #[test]
    fn attachment_only_excludes_law_seeking_questions() {
        // Wants the document's OWN content → attachment-only.
        for q in [
            "summarize this document",
            "what are the action items in this document",
            "brief me on the attached file",
        ] {
            assert!(wants_attachment_only(q), "should be attachment-only: {q}");
        }
        // References the attachment but asks for external legal authority → the
        // corpus must stay in scope, so NOT attachment-only.
        for q in [
            "which laws support me to answer this document",
            "what rules apply to the attached show-cause",
            "is there any legal basis in this document's favour",
            "which sections of law govern the attached audit para",
        ] {
            assert!(seeks_legal_authority(q), "law-seeking cue expected: {q}");
            assert!(!wants_attachment_only(q), "must keep corpus in scope: {q}");
        }
    }

    #[test]
    fn classifies_each_capability() {
        assert_eq!(
            classify_capability("summarize the Sindh Local Government Act 2013"),
            Capability::SummarizeDoc
        );
        assert_eq!(classify_capability("draft a summary for the chief secretary"), Capability::Draft);
        assert_eq!(classify_capability("please draft a note on this matter"), Capability::Draft);
        assert_eq!(
            classify_capability("compare the Sindh Local Government Act with the Police Order"),
            Capability::Compare
        );
        assert_eq!(
            classify_capability("is there any conflict between the Act and the Rules"),
            Capability::Conflict
        );
        assert_eq!(classify_capability("how has section 18 been amended over time"), Capability::AmendmentChain);
        assert_eq!(classify_capability("what are the action items in this document"), Capability::ActionItems);
    }

    #[test]
    fn detects_transform_requests() {
        for q in [
            "make it more formal",
            "rewrite this as a formal letter",
            "can you shorten it",
            "make it in urdu",
            "rephrase in simpler words",
            "expand on this",
            "condense the answer into bullet points",
            "translate to sindhi",
            "as a memo please",
        ] {
            assert!(is_transform_request(q), "should detect transform: {q}");
        }
    }

    #[test]
    fn plain_questions_are_not_transform_requests() {
        for q in [
            "what is the procedure for long leave",
            "who approves procurement above 100 million",
            "summarize the Sindh Local Government Act 2013",
            "what does section 18 say",
            "is there any conflict between the Act and the Rules",
        ] {
            assert!(!is_transform_request(q), "should NOT detect transform: {q}");
        }
    }

    #[test]
    fn detects_additive_requests() {
        for q in [
            "also cover the penalties for non-compliance",
            "add a section on leave rules",
            "please include the appeal procedure",
            "can you add the approval limits",
            "expand on the delegation of powers",
            "what else does the act require",
            "in addition, mention the timelines",
            "don't forget the notice period",
        ] {
            assert!(is_additive_request(q), "should detect additive: {q}");
        }
    }

    #[test]
    fn plain_and_transform_queries_are_not_additive() {
        for q in [
            "what is the procedure for long leave",
            "who approves procurement above 100 million",
            "make it more formal",
            "shorten it",
            "does the act include penalties for late payment", // question, not an ask to extend
        ] {
            assert!(!is_additive_request(q), "should NOT detect additive: {q}");
        }
    }

    #[test]
    fn anaphoric_expand_stays_transform_topical_expand_does_not() {
        // "expand on this" = restyle the previous answer (transform, checked first)
        assert!(is_transform_request("expand on this"));
        assert!(is_transform_request("elaborate on it"));
        // "expand on <topic>" = new content — must fall through to additive
        assert!(!is_transform_request("expand on the delegation of powers"));
        assert!(is_additive_request("expand on the delegation of powers"));
    }

    #[test]
    fn procedural_and_lookup_default_to_guidance() {
        assert_eq!(classify_capability("what is the procedure for long leave"), Capability::Guidance);
        assert_eq!(classify_capability("what does section 18 say"), Capability::Guidance);
        assert_eq!(classify_capability("hello"), Capability::Guidance);
    }

    #[test]
    fn topical_summary_without_document_scope_is_guidance() {
        // Regression (2026-06-27): a topical "summarize X" that names no
        // document must NOT trigger the whole-corpus map-reduce (was 7.1 min).
        // It is a targeted question → Guidance (fast hybrid RAG).
        assert_eq!(
            classify_capability("Summarize the approval limits of the DDWP and CDWP"),
            Capability::Guidance
        );
        assert_eq!(
            classify_capability("give me an overview of the leave sanctioning powers"),
            Capability::Guidance
        );
    }

    #[test]
    fn broad_overview_still_maps_to_summarize_doc() {
        assert_eq!(
            classify_capability("give me an overview of all documents"),
            Capability::SummarizeDoc
        );
        assert_eq!(
            classify_capability("summarize everything in the corpus"),
            Capability::SummarizeDoc
        );
    }

    #[test]
    fn summarize_request_detects_all_forms_incl_keywordless_titles() {
        assert!(is_summarize_request("Summarize the Sindh Delegation of Financial Powers 2019."));
        assert!(is_summarize_request("give me a summary of the constitution"));
        assert!(is_summarize_request("please summarise this document"));
        // Not a summary request.
        assert!(!is_summarize_request("what does Rule 4 say about penalties"));
        assert!(!is_summarize_request("how many rules are in the SPPRA Rules"));
    }

    #[test]
    fn bare_must_does_not_trigger_actionitems_mapreduce() {
        // Regression (2026-06-29): the ActionItems classifier cue matched a bare
        // "must"/"shall do", so ordinary factual questions containing "must" were
        // misrouted to the whole-corpus ActionItems map-reduce (one LLM call per
        // batch over the entire corpus) and hung past the client timeout on BOTH
        // engines. These are the exact 5 questions that timed out in the F016/F017
        // eval set (Q01/Q04/Q16/Q19/Q30) — they must route to fast single-pass
        // Guidance, never a map-reduce capability.
        for q in [
            "A development project's PC-I shows a total cost of exactly Rs 2,000 million with foreign aid equal to 25% of the total cost. Can the DDWP approve it by itself, or must it go higher? Explain the precise reason.",
            "Under the SPPRA Rules, distinguish a 'Request for Quotations' from a 'Petty Purchase', and state the rupee value above which a procurement opportunity must be advertised in newspapers.",
            "A procurement is valued at exactly Rs 1,000,000. Must it be advertised in newspapers, only on the Authority's website, or neither? Quote the boundary rule.",
            "How many members must be physically present for a valid quorum of the Sindh Cabinet under Rule 19-C of the Rules of Business 1986?",
            "A single Rs 9,500 million development project is partly foreign-aided. Two readings are possible: (a) CDWP approves because it is under Rs 10,000 million; (b) it must go to ECNEC because of the foreign aid. Which reading is correct and why?",
        ] {
            let cap = classify_capability(q);
            assert!(!cap.is_map_reduce(), "bare-\"must\" query must NOT map-reduce: {cap:?} for {q:?}");
            assert_eq!(cap, Capability::Guidance, "expected Guidance for {q:?}");
        }
    }

    #[test]
    fn genuine_action_item_requests_still_route_to_actionitems() {
        // The tightened cue must still catch real action-extraction phrasing.
        assert_eq!(classify_capability("what are the action items in this document"), Capability::ActionItems);
        assert_eq!(classify_capability("what must the secretary do under these rules"), Capability::ActionItems);
        assert_eq!(classify_capability("give me a checklist of the obligations"), Capability::ActionItems);
    }

    #[test]
    fn conflict_outranks_compare_when_both_present() {
        // "compare ... conflict" → conflict wins (checked first).
        assert_eq!(
            classify_capability("compare these acts and tell me where they conflict"),
            Capability::Conflict
        );
    }

    #[test]
    fn draft_noun_does_not_misfire_as_draft_capability() {
        // "the draft Act" is a noun, not a drafting request.
        assert_eq!(
            classify_capability("summarize the draft Local Government Act"),
            Capability::SummarizeDoc
        );
    }

    #[test]
    fn document_scope_extracts_named_instruments() {
        let scope = parse_document_scope(
            "compare the Sindh Local Government Act 2013 with the Police Order 2002",
        );
        assert!(scope.iter().any(|s| s.to_lowercase().contains("act")), "{scope:?}");
        assert!(scope.iter().any(|s| s.to_lowercase().contains("order")), "{scope:?}");
    }

    #[test]
    fn capability_flags() {
        assert!(Capability::Compare.is_document_scoped());
        assert!(Capability::Compare.is_map_reduce());
        assert!(!Capability::Guidance.is_document_scoped());
        assert!(!Capability::AmendmentChain.is_map_reduce());
    }
}
