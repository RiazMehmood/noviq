//! Web search — a SEPARATE retrieval lane, never mixed with the legal corpus.
//!
//! # Why this is its own lane
//!
//! Every answer Noviq gives is built on one promise, stated in the grounding
//! prompt: *"Your authority in every answer comes ONLY from the documents
//! retrieved for the current question — never from memory or general knowledge…
//! never cite a rule, judgment, or precedent that is not in the context."*
//!
//! Web results break that promise if they are poured into the same context. A
//! blog post and the Sindh Rules of Business would arrive with identical `[n]`
//! citation markers, and an officer acting on the answer could not tell which
//! was statute. For a product advising government officers, that is the worst
//! failure this system can produce — worse than refusing to answer.
//!
//! So web search is:
//!   * **opt-in per turn** (a composer toggle, like deep search) — never a
//!     silent fallback when the corpus finds nothing, because a refusal is a
//!     correctness feature here, not a defect to paper over;
//!   * **separately labelled** end to end — results carry their own source type
//!     so the prompt and the UI can mark them as web, not law.
//!
//! # Provider
//!
//! **SearXNG, self-hosted on this VM** (loopback only, no API key, no credits).
//! The requirement was a web search that does not meter — no per-query billing
//! and no dependence on a vendor's free-tier allowance — so a hosted search API
//! was ruled out however good its terms.
//!
//! Self-hosting removes the vendor and the billing. It does NOT make the query
//! private: the upstream engines SearXNG aggregates still see the query text,
//! from this VM's IP rather than the officer's. That is the honest boundary of
//! what this buys, and it should be settled before the feature is offered to
//! users rather than after.
//!
//! The trait keeps the choice reversible — the same seam as
//! `services::inference`, where the engine is swappable behind `TextGenerator`.

pub mod searxng;

use async_trait::async_trait;

/// One web result, already extracted to text by the provider.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WebResult {
    pub title: String,
    pub url: String,
    /// Extracted page content — the provider's summary/extract, NOT raw HTML.
    pub content: String,
    /// Provider relevance score, when supplied. Advisory only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    /// Publication date when the provider knows it. Shown to the officer because
    /// the age of a web source is part of judging it — unlike a statute, which
    /// is dated by its own text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("web search is not configured (no API key)")]
    NotConfigured,
    #[error("web search provider rejected the request: {0}")]
    Rejected(String),
    #[error("web search transport error: {0}")]
    Transport(String),
}

/// The seam. One method, deliberately: this lane retrieves, it does not generate.
#[async_trait]
pub trait WebSearcher: Send + Sync {
    /// Retrieve up to `limit` results for `query`.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<WebResult>, SearchError>;
    /// Provider name for traces and the UI ("tavily").
    fn name(&self) -> &'static str;
}

/// How many results to put in front of the model.
///
/// Raised 5 → 12 (2026-07-29). Five was an arbitrary caution that sat badly
/// beside the rest of the system: the RAG path hands the model ~35 retrieved
/// chunks and lets it choose, then web search gave it five snippets and expected
/// the same quality of judgement. More candidates is how the model finds the one
/// that matches the question — the same reasoning that sets the retrieval pool.
///
/// The block is measured into the prompt budget by `law_overhead`, so a wider
/// set shrinks the law allowance rather than overflowing it.
pub const DEFAULT_RESULT_LIMIT: usize = 12;

/// System prompt for WEB-ONLY mode (2026-07-29).
///
/// When the officer switches web search on, the indexed corpus is not consulted
/// at all — this is a research mode, not a legal-answer mode. That exclusivity
/// is a safety property, not a simplification:
///
/// The previous design put web results *alongside* the legal context. The model
/// answered a "who is the current President" question from the web, tagged it
/// `[5]`, and the citation pipeline renumbered that to `[1]` and bound it to a
/// Constitution chunk that merely contained the phrase "President of Pakistan"
/// (trace 551fb7cd). A web fact shipped wearing a statutory citation — the worst
/// output this system can produce.
///
/// With no legal chunks in context there is no `[n]` to mint and nothing to
/// rebind to, so that class of failure is structurally impossible rather than
/// merely discouraged. The `strip_legal_markers` pass below enforces it even if
/// the model invents one from memory.
pub const WEB_ONLY_SYSTEM_PROMPT: &str = "\
You are a research assistant. You have been given results from a public web \
search. Answer the user's question from those results and nothing else.\n\n\
SHAPE OF THE ANSWER\n\
- Open with ONE sentence that answers the question directly. No preamble, no \
restating the question.\n\
- Then a short bold heading naming what follows, and under it the substance.\n\
- When the answer IS a set of items — amendments, requirements, steps, dates — \
write them as a bulleted list, ONE PER LINE, each with its own marker. A list \
flattened into a paragraph is the single most common way this answer becomes \
hard to read.\n\
- Name and define each instrument, rule or body the FIRST time it appears — \
\"the Sindh Public Procurement Rules, 2010 (SPPR)\" — then use the short form. \
Never assume the reader knows an abbreviation.\n\
- Where a source gives a date, a notification number or an issuing authority, \
include it. Those are what make a web claim checkable.\n\
- Keep it tight. Three short sections beat one long one, and a fact you cannot \
attribute is better left out than padded in.\n\
RULES\n\
- Cite every factual sentence with the bracketed number of the source it came \
from, in the form [W1], [W2], … matching the Web results block. Place the \
marker at the end of the sentence.\n\
- Use ONLY the numbers shown in the Web results block. NEVER write a bare \
number marker such as [1] or [2] — those mean an indexed legal document, and \
you have not been given any.\n\
- If the results do not answer the question, say so plainly. Do not fill the \
gap from memory or general knowledge.\n\
- Web sources can be wrong, outdated or contradictory. If they disagree, say so \
and cite both. If a result looks stale for a question about a current position \
or figure, note the date.\n\
- This is NOT legal advice and NOT a statement of law. Never present anything \
here as the content of a statute, rule or regulation. If the question is a \
legal one, answer what the web says and add that it must be verified against \
the indexed legal documents by asking again with web search turned off.\n\
- Think silently. Output only the final answer.";

/// Turn the officer's sentence into a search query.
///
/// The raw question is a poor query and sometimes an actively harmful one. Asked
/// "tell me about attached document details and search the net for its more
/// information also", the engine matched the words *"attached document"* and
/// returned Outlook help pages and English-grammar forums — the meta-language of
/// the request drowned out its subject.
///
/// So: drop the instruction phrases, and when a document is attached use its
/// title, which is what the officer actually means by "this document".
pub fn build_query(question: &str, doc_title: Option<&str>) -> String {
    static RE: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    // Phrases that address the ASSISTANT rather than describe the subject.
    let re = RE.get_or_init(|| {
        // NB assembled with `concat!`, not `\` line-continuations: a backslash at
        // end of line does NOT continue a RAW string, so the earlier version
        // baked literal backslash-newlines into the pattern and silently broke
        // most of the alternation ("search the net" and "also" survived).
        regex_lite::Regex::new(concat!(
            r"(?i)\b(please|kindly|tell me about|tell me|give me|can you|could you|i want to know",
            r"|search (the )?(net|web|internet)( for)?|look (it )?up",
            r"|find (out )?(more )?(information|details|info)",
            r"|more (information|details|info)|further (information|details)",
            r"|about (this|the) (attached )?(document|file|pdf|notice)",
            r"|(this|the) attached (document|file|pdf)|attached (document|file|pdf)|attachment",
            r"|details|also|as well|for its)\b",
        ))
        .unwrap()
    });
    let cleaned = re.replace_all(question, " ");
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_matches(|c: char| !c.is_alphanumeric()).trim();

    match doc_title {
        // The document IS the subject. Its title carries the names and the
        // subject matter; whatever survives of the question adds the angle.
        Some(t) => {
            let title = clean_doc_title(t);
            if cleaned.is_empty() { title } else { format!("{title} {cleaned}") }
        }
        // No attachment: fall back to the original question rather than a
        // stripped fragment that may have lost the subject entirely.
        None if cleaned.split_whitespace().count() < 2 => question.trim().to_string(),
        None => cleaned.to_string(),
    }
}

/// Pull the legal instruments an attached document CITES, for use as the web
/// query.
///
/// A legal document's title is usually the wrong query. Asked to find reference
/// law for a show-cause notice, the title gave "Show Casue Notice to Ms Sadaf
/// Mughal lecturer in commerce" — a search for a private individual, who has no
/// web presence, returning a department homepage and a generic "how to reply to
/// a show cause notice" page. What the officer actually wants is law about the
/// INSTRUMENTS the notice rests on: the Sindh Civil Servants (Efficiency &
/// Discipline) Rules, 1973.
///
/// So take the named Acts/Rules/Ordinances out of the document's own text.
/// Deduplicated, longest first (the fuller name is the more specific query),
/// capped so one document cannot produce an unbounded query string.
pub fn instruments_in(text: &str) -> Vec<String> {
    static RE: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        // "…Rules, 1973" / "…Act, 2013" / "…Ordinance, 2000", with the words
        // before it — bounded to 6 words so a whole sentence is never captured.
        regex_lite::Regex::new(
            // Each preceding token may be a capitalised word, a parenthesised one,
            // or a bare `&` — "Sindh Civil Servants (Efficiency & Discipline)
            // Rules, 1973" walks back through all three, and requiring a capital
            // stopped at "(Efficiency" and captured only "Discipline) Rules 1973".
            r"((?:(?:[A-Z(][\w&()\-']*|&)\s+){1,8}(?:Rules|Act|Ordinance|Regulations|Order|Policy))\s*,?\s*(\d{4})",
        )
        .unwrap()
    });
    let mut out: Vec<String> = Vec::new();
    for c in re.captures_iter(text) {
        let name = c.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let year = c.get(2).map(|m| m.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let full = format!("{name} {year}");
        let norm = full.to_lowercase();
        if !out.iter().any(|e| e.to_lowercase() == norm) {
            out.push(full);
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.len()));
    out.truncate(2);
    out
}

/// A filename is not a search query. `SC03_Show_Casue_Notice_in_respect_of_Ms_Sumera_Lecturer_in_Zoolog.pdf`
/// → `Show Casue Notice in respect of Ms Sumera Lecturer in Zoolog`.
/// Typos in the original are preserved deliberately — it is the officer's file
/// name, and search engines correct spelling better than we can guess at it.
fn clean_doc_title(title: &str) -> String {
    let t = title.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(title);
    let t = t.replace(['_', '-'], " ");
    let t = t
        .split_whitespace()
        // Drop a leading filing code like `SC03`, which is noise to a search engine.
        .skip_while(|w| w.len() <= 5 && w.chars().any(|c| c.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join(" ");
    t.trim().to_string()
}

/// Make a query string safe for a search engine.
///
/// Instrument names carry punctuation that engines treat as syntax, not words:
/// "Sindh Civil Servants (Efficiency & Discipline) Rules, 1973" sent verbatim
/// returned a bank's annual report and a court cause-list for a question about
/// appeal periods. Parentheses go, `&` becomes "and", stray punctuation is
/// dropped, and the whole thing is capped — a very long query dilutes every term
/// in it.
pub fn sanitize_query(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for c in q.chars() {
        match c {
            '&' => out.push_str(" and "),
            '(' | ')' | '[' | ']' | '{' | '}' | '"' | '“' | '”' | '|' | '\\' | '/' | '*' | ':'
            | ';' | ',' | '?' | '!' => out.push(' '),
            _ => out.push(c),
        }
    }
    let words: Vec<&str> = out.split_whitespace().collect();
    // ~16 words is plenty; past that the engine is matching noise.
    words.into_iter().take(16).collect::<Vec<_>>().join(" ")
}

/// Domains whose word carries weight for Pakistani public-law research.
/// Not a filter — a ranking preference. A blog can still be useful; it should
/// just not outrank a High Court listing or a provincial assembly PDF.
const AUTHORITATIVE: &[&str] = &[
    ".gov.pk", ".gov", ".gos.pk", "sindhhighcourt", "supremecourt", "pas.gov.pk",
    "sindhlaws", "na.gov.pk", "senate.gov.pk", "elections.gov.pk", "punjablaws",
    "pakistancode", "fedlaw", "ilo.org", "unep.org",
];

/// Domains that are user-uploaded or aggregated content — usable, but never the
/// best available answer for a legal question.
const LOW_TRUST: &[&str] = &["scribd.com", "studocu", "coursehero", "slideshare", "pinterest", "quora", "reddit"];

/// Reorder results so authoritative sources lead, and drop the obviously
/// irrelevant.
///
/// SearXNG merges several engines and its ordering optimises for general web
/// relevance, which is not the same as usefulness to an officer researching a
/// statute. This does not invent relevance — it only reorders what came back and
/// removes results sharing no meaningful term with the query at all (that is how
/// an electoral-roll video arrived under a question about show-cause notices).
pub fn rank_results(query: &str, results: Vec<WebResult>) -> Vec<WebResult> {
    let terms: Vec<String> = sanitize_query(query)
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 3) // drop "the", "and", "under", short noise
        .map(str::to_string)
        .collect();

    let score = |r: &WebResult| -> i32 {
        let hay = format!("{} {}", r.title, r.content).to_lowercase();
        let url = r.url.to_lowercase();
        let mut s = 0;
        // Term overlap is the base signal.
        s += terms.iter().filter(|t| hay.contains(t.as_str())).count() as i32 * 2;
        if AUTHORITATIVE.iter().any(|d| url.contains(d)) {
            s += 6;
        }
        if LOW_TRUST.iter().any(|d| url.contains(d)) {
            s -= 3;
        }
        // A PDF from a government domain is usually the instrument itself.
        if url.ends_with(".pdf") {
            s += 2;
        }
        s
    };

    let mut scored: Vec<(i32, WebResult)> = results.into_iter().map(|r| (score(&r), r)).collect();
    // ORDER ONLY — nothing is discarded (2026-07-29). An earlier version dropped
    // zero-scoring results, which is the search deciding for the model what is
    // worth considering. If a web search has been authorised, the model should
    // see what came back and judge it; a weak-looking snippet can still carry
    // the citation that matters. Ranking puts the likely-useful first; it does
    // not withhold the rest.
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, r)| r).collect()
}

/// Strip legal `[n]` citation markers from a web-mode answer.
///
/// Belt and braces for the rule above. The model is told never to emit a bare
/// `[1]`, but instruction-following is not a guarantee, and in web mode such a
/// marker is *always* wrong: there is no legal context for it to refer to, and
/// the UI/citation layer would otherwise be free to bind it to something.
/// `[W1]` is preserved.
pub fn strip_legal_markers(answer: &str) -> String {
    static RE: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex_lite::Regex::new(r"\[(\d{1,3})\]").unwrap());
    re.replace_all(answer, "").to_string()
}

/// The instruction appended to the system prompt when web results are present.
///
/// This is the whole safety story of the feature, so it is deliberately blunt.
/// The base grounding prompt tells the model its authority comes ONLY from the
/// retrieved documents; web results would contradict that flatly if simply
/// dropped in. Rather than weaken the grounding rule — which protects every
/// legal answer — this carves out a clearly-fenced second section with its own
/// marker syntax, so a web claim can never be mistaken for a `[n]` citation to
/// an indexed instrument.
pub const WEB_LANE_PROMPT: &str = "\n\n\
=== WEB RESULTS — NOT LAW, NOT AUTHORITY ===\n\
The officer switched web search on, so a second block titled \"Web results\" \
follows the Context block. It comes from a public web search, NOT from the \
indexed legal corpus.\n\
PRECEDENCE: if an instruction above restricts you to the attached document or \
to the retrieved passages, that restriction governs the LEGAL answer only. It \
does NOT cancel the web section required below — those results were fetched at \
the officer's explicit request and must be reported.\n\
- The Context block is authority. Cite it with [n] exactly as instructed above.\n\
- Web results are NOT authority. Cite them ONLY as [W1], [W2], … NEVER give a \
web result an [n] marker, and never merge the two numbering schemes.\n\
- Answer the question from the Context block FIRST, in full.\n\
- THEN write a section headed \"From the web (unverified)\". This section is \
REQUIRED whenever web results are present, and it comes AFTER the answer and \
BEFORE any \"Next steps\" section.\n\
WRITING THAT SECTION:\n\
- Write two or three sentences of PROSE. Do NOT write one line per source, and \
do NOT begin a line with a marker — a marker goes at the END of the sentence it \
supports, like this [W2].\n\
- SYNTHESISE. Say what the sources collectively add, grouping agreement into one \
sentence with several markers [W1][W3] rather than restating each source in turn.\n\
- Prefer what is USEFUL to an officer answering this matter: an order or \
judgment applying the same rule, the authority that made it, the outcome.\n\
- If the results add nothing beyond the documents, say exactly that in ONE \
sentence and name what was searched for. That is a complete and acceptable \
section — padding it is worse than admitting it.\n\
- Never state a rule, section, penalty, deadline or authority on a web result \
alone. Present it as what that source says, not as the legal position.
\n- A [W] marker means THAT RESULT SAYS THIS. Never attach one to a fact taken \nfrom the documents, from an earlier turn, or from your own knowledge. Restating \nthe attached document under web markers is a false attribution, not a summary.
\n- This is a CONVERSATION. Do not restate background, allegations or framework an \nearlier turn already covered — answer what was asked THIS time.";

/// Render the retrieved results as the fenced block the prompt above refers to.
/// Numbered `W1…Wn` — a separate namespace from the `[n]` legal citations, so
/// the two can never collide in the model's output.
pub fn format_web_block(results: &[WebResult]) -> String {
    let mut out = String::from("\n\nWeb results (public web search — NOT the indexed legal corpus):\n");
    for (i, r) in results.iter().enumerate() {
        out.push_str(&format!("[W{}] {}\n{}\n", i + 1, r.title, r.url));
        if let Some(d) = &r.published {
            out.push_str(&format!("published: {d}\n"));
        }
        if !r.content.trim().is_empty() {
            // Snippet only — see the searxng module docs on why there is no
            // full-page extraction.
            let snippet: String = r.content.chars().take(600).collect();
            out.push_str(&snippet);
            out.push('\n');
        }
        out.push_str("---\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(title: &str, url: &str, content: &str) -> WebResult {
        WebResult {
            title: title.into(),
            url: url.into(),
            content: content.into(),
            score: None,
            published: None,
        }
    }

    /// The numbering namespace is the safety property: a web source must never
    /// be renderable as `[1]`, which the officer reads as an indexed instrument.
    #[test]
    fn web_block_uses_a_separate_citation_namespace() {
        let block = format_web_block(&[r("A", "https://x/1", "one"), r("B", "https://x/2", "two")]);
        assert!(block.contains("[W1]"));
        assert!(block.contains("[W2]"));
        assert!(!block.contains("[1]"), "web results must never use the legal [n] namespace");
        assert!(block.contains("NOT the indexed legal corpus"));
    }

    /// The lane prompt must keep saying the two things that make this safe, so a
    /// future edit that softens either one fails here rather than in production.
    #[test]
    fn lane_prompt_forbids_merging_the_two_sources() {
        assert!(WEB_LANE_PROMPT.contains("[W1]"));
        assert!(WEB_LANE_PROMPT.contains("NOT authority"));
        assert!(
            WEB_LANE_PROMPT.contains("Context block FIRST"),
            "the law must be answered from first"
        );
    }

    /// Regression for trace 551fb7cd. The model wrote a web fact tagged `[5]`;
    /// the citation pipeline renumbered it to `[1]` and bound it to a
    /// Constitution chunk. In web-only mode a bare `[n]` is ALWAYS wrong — there
    /// is no legal context for it to refer to — so it is stripped before the
    /// citation layer can ever see it.
    #[test]
    fn legal_markers_are_stripped_from_a_web_answer() {
        let raw = "The current President of Pakistan is Asif Ali Zardari [5]. \
                   He took office on 10 March 2024 [W2]. Earlier he served 2008-2013 [1].";
        let out = strip_legal_markers(raw);
        assert!(!out.contains("[5]"), "legal marker survived: {out}");
        assert!(!out.contains("[1]"), "legal marker survived: {out}");
        assert!(out.contains("[W2]"), "web marker must be preserved: {out}");
    }

    /// The web-only prompt must keep forbidding the bare-number form and keep
    /// saying this is not law, so a future edit that softens either fails here.
    #[test]
    fn web_only_prompt_forbids_legal_markers_and_legal_authority() {
        assert!(WEB_ONLY_SYSTEM_PROMPT.contains("[W1]"));
        assert!(
            WEB_ONLY_SYSTEM_PROMPT.contains("NEVER write a bare"),
            "the bare-[n] prohibition is the fix for trace 551fb7cd"
        );
        assert!(WEB_ONLY_SYSTEM_PROMPT.contains("NOT legal advice"));
        assert!(
            WEB_ONLY_SYSTEM_PROMPT.contains("Do not fill the gap from memory"),
            "answering from training data is how the wrong fact arrived in the first place"
        );
    }

    /// The web answer must be SHAPED, not a wall of prose. Prod 2026-07-30: a
    /// question whose answer was four amended rules came back as two flat
    /// paragraphs with the rules buried mid-sentence, while the equivalent
    /// corpus answer is headed and itemised.
    #[test]
    fn web_only_prompt_asks_for_a_shaped_answer() {
        assert!(
            WEB_ONLY_SYSTEM_PROMPT.contains("bulleted list"),
            "a list of amendments flattened into a paragraph is the readability defect"
        );
        assert!(
            WEB_ONLY_SYSTEM_PROMPT.contains("bold heading"),
            "the officer asked for a proper heading"
        );
        assert!(
            WEB_ONLY_SYSTEM_PROMPT.contains("define each instrument"),
            "abbreviations must be defined on first use"
        );
    }

    /// The live failure: the meta-language of the request became the search.
    #[test]
    fn query_drops_instruction_phrases_and_uses_the_document_title() {
        let q = "tell me about attached document details and search the net for its more information also";
        let doc = "SC03_Show_Casue_Notice_in_respect_of_Ms_Sumera_Lecturer_in_Zoolog.pdf";
        let out = build_query(q, Some(doc));
        assert!(out.contains("Show Casue Notice"), "document subject missing: {out}");
        assert!(out.contains("Sumera"), "the person the notice concerns is the subject: {out}");
        assert!(!out.to_lowercase().contains("attached document"), "meta-language survived: {out}");
        assert!(!out.to_lowercase().contains("search the net"), "meta-language survived: {out}");
        assert!(!out.contains("SC03"), "filing code is noise to a search engine: {out}");
        assert!(!out.contains(".pdf"), "extension is noise: {out}");
    }

    /// With no attachment an ordinary question must pass through intact — the
    /// cleaner must never strip a question down to nothing.
    #[test]
    fn query_without_attachment_keeps_the_question() {
        assert_eq!(
            build_query("who is current finance minister of pakistan?", None),
            "who is current finance minister of pakistan"
        );
        // Nearly all meta-language: falling back to the original beats sending
        // an empty or one-word query.
        let out = build_query("tell me more information", None);
        assert!(out.len() > 3, "must not collapse to nothing: {out:?}");
    }

    /// The live case: a show-cause notice. Its TITLE names a private individual
    /// (no web presence — the search returned a department homepage and a
    /// how-to article). The instruments it CITES are what reference law exists
    /// about.
    #[test]
    fn instruments_are_pulled_from_the_document_text() {
        let body = "SHOW CAUSE NOTICE
Whereas you are accused of misconduct under Rule 4 of                     the Sindh Civil Servants (Efficiency & Discipline) Rules, 1973 read with                     sub-rule (3) of Rule 5 of the said Rules, and the Sindh Civil Servants Act, 1973                     applies to your service.";
        let found = instruments_in(body);
        assert!(!found.is_empty(), "no instrument found in a notice that names two");
        let joined = found.join(" | ").to_lowercase();
        assert!(joined.contains("efficiency"), "the E&D Rules are the subject: {found:?}");
        assert!(joined.contains("1973"), "the year is part of the instrument name: {found:?}");
        assert!(found.len() <= 2, "query must stay bounded: {found:?}");
    }

    #[test]
    fn instruments_dedupe_and_ignore_prose_without_a_year() {
        let body = "the Sindh Civil Servants Act, 1973 ... again the Sindh Civil Servants Act, 1973                     ... and some rules that carry no year at all";
        let found = instruments_in(body);
        assert_eq!(found.len(), 1, "duplicates must collapse: {found:?}");
    }

    /// The prompt must now REQUIRE the web section. Silently omitting it is what
    /// produced an answer with web results attached but no [W] markers in the
    /// prose, after the officer had deliberately switched web search on.
    /// The section must be required AND readable. The first version produced one
    /// line per source, each opening and closing with the same marker
    /// ("[W1] The Rules ... [W1]."), which reads as a dump of results rather
    /// than an answer.
    #[test]
    fn lane_prompt_requires_a_readable_web_section() {
        assert!(WEB_LANE_PROMPT.contains("REQUIRED whenever web results are present"));
        // The attachment scope override says "use ONLY the attached document"
        // and "overrides any earlier instruction" — without an explicit
        // precedence line the model obeys it and drops the web section entirely.
        assert!(WEB_LANE_PROMPT.contains("PRECEDENCE"));
        assert!(WEB_LANE_PROMPT.contains("governs the LEGAL answer only"));
        // and the two end-of-answer sections must not compete for last place
        assert!(WEB_LANE_PROMPT.contains("BEFORE any \"Next steps\" section"));
        assert!(WEB_LANE_PROMPT.contains("[W1]"));
        assert!(WEB_LANE_PROMPT.contains("NOT authority"));
        // formatting rules that fix the one-line-per-source dump
        assert!(WEB_LANE_PROMPT.contains("Do NOT write one line per source"));
        assert!(WEB_LANE_PROMPT.contains("do NOT begin a line with a marker"));
        assert!(WEB_LANE_PROMPT.contains("SYNTHESISE"));
        // and the escape hatch, so "nothing useful" is an acceptable answer
        assert!(WEB_LANE_PROMPT.contains("padding it is worse"));
        // A [W] on a fact taken from the DOCUMENT is a false attribution — the
        // mirror of the [1]-on-a-web-fact bug (trace 551fb7cd).
        assert!(WEB_LANE_PROMPT.contains("false attribution"));
        assert!(WEB_LANE_PROMPT.contains("THAT RESULT SAYS THIS"));
        assert!(WEB_LANE_PROMPT.contains("This is a CONVERSATION"));
    }

    /// Instrument punctuation is search-engine syntax, not words. Sent verbatim,
    /// "Sindh Civil Servants (Efficiency & Discipline) Rules, 1973" returned a
    /// bank annual report and a court cause-list.
    #[test]
    fn query_is_sanitised_for_a_search_engine() {
        let out = sanitize_query("Sindh Civil Servants (Efficiency & Discipline) Rules, 1973 appeal period under Rule 10?");
        assert!(!out.contains('('), "parens are engine syntax: {out}");
        assert!(!out.contains('&'), "ampersand is engine syntax: {out}");
        assert!(!out.contains(','), "{out}");
        assert!(!out.contains('?'), "{out}");
        assert!(out.contains("Efficiency and Discipline"), "meaning must survive: {out}");
        assert!(out.contains("1973"));
        // long queries dilute every term
        let long = sanitize_query(&"word ".repeat(40));
        assert!(long.split_whitespace().count() <= 16, "query not capped");
    }

    /// A government order citing the same Rules must outrank a Scribd upload and
    /// an unrelated video — the live result set had exactly that inversion.
    #[test]
    fn authoritative_sources_outrank_uploads_and_noise() {
        let q = "Sindh Civil Servants Efficiency and Discipline Rules 1973 show cause notice";
        let results = vec![
            WebResult { title: "Show Cause Notice".into(), url: "https://www.scribd.com/doc/1".into(),
                        content: "show cause notice discipline".into(), score: None, published: None },
            WebResult { title: "How To Fill SIR_2002 FORM".into(), url: "https://youtube.com/x".into(),
                        content: "electoral roll draft".into(), score: None, published: None },
            WebResult { title: "Order vacating notice".into(), url: "https://sindheducation.gov.pk/a.pdf".into(),
                        content: "Rule 5(4)(b) of the Sindh Civil Servants Efficiency and Discipline Rules 1973".into(),
                        score: None, published: None },
        ];
        let out = rank_results(q, results);
        assert!(out[0].url.contains("gov.pk"), "government order must lead: {:?}", out[0].url);
        // Ranking ORDERS, it does not withhold: the model decides what is useful.
        assert_eq!(out.len(), 3, "no result may be discarded");
        assert!(
            out.iter().position(|r| r.url.contains("youtube")).unwrap()
                > out.iter().position(|r| r.url.contains("gov.pk")).unwrap(),
            "the unrelated result must rank last, not vanish"
        );
    }

    /// A thin result set must never be emptied — some sources beat none.
    #[test]
    fn ranking_never_empties_a_weak_result_set() {
        let results = vec![
            WebResult { title: "a".into(), url: "https://x/1".into(), content: "".into(), score: None, published: None },
            WebResult { title: "b".into(), url: "https://x/2".into(), content: "".into(), score: None, published: None },
        ];
        let out = rank_results("something entirely unrelated", results);
        assert_eq!(out.len(), 2, "nothing scored, so nothing may be dropped");
    }

    #[test]
    fn stripping_leaves_ordinary_brackets_alone() {
        // Only numeric markers go; bracketed words are content.
        let out = strip_legal_markers("Section 9-A [as amended] applies [3].");
        assert!(out.contains("[as amended]"));
        assert!(!out.contains("[3]"));
    }

    #[test]
    fn long_content_is_truncated_per_result() {
        let long = "x".repeat(5000);
        let block = format_web_block(&[r("A", "https://x/1", &long)]);
        assert!(block.len() < 1200, "one result must not swallow the budget: {}", block.len());
    }
}
