//! Feature 022 — grounding + the officer's **reply-support brief**.
//!
//! Noviq does NOT write the officer's reply. It gives him what he cannot easily
//! get himself: the provision actually invoked against him, quoted verbatim, and
//! the provisions that **protect** him — procedure, penalty scale, hearing,
//! appeal — likewise verbatim and cited. He writes; Noviq checks (see `mod.rs`
//! `Phase::CoWrite`).
//!
//! Two precision invariants make the citations trustworthy:
//!   1. **Instrument pinning** — a bare token such as `Rule 4` occurs in many
//!      statutes. Retrieval is restricted to the statute the notice *names*
//!      (`instrument_scope`); an unresolvable statute is a MISS, never a guess.
//!   2. **Verbatim containment** — a retrieved provision counts only when its text
//!      actually contains the token (`Rule 4`), not merely ranks well for it.
//!
//! Nothing here asserts a fact the officer did not give or a rule the corpus did
//! not supply (FR-008/FR-011, Principle XI).

use std::sync::Arc;

use crate::db::Db;
use crate::services::assistant::CitationOut;
use crate::services::embedder::EmbedderState;

use super::frame::{CitedRule, DocKind, Frame, GroundOutcome, NoticeAssistState};

/// Normalize a provision reference for containment matching:
/// "Rule-12 (2)" / "rule 12(2)" → "rule12(2)".
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '\u{2011}')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Match a provision token against the way the corpus ACTUALLY prints it.
///
/// A plain `norm(content).contains(norm(token))` cannot work. Trace `fe568a07`
/// invoked `Rule 4A(1)`; the E&D Rules print it as
/// `2[4-A.-(1) If in the opinion of the authorized officer …` — there is **no
/// literal "Rule"** before the number (it lives in `chunks.unit_label`, which
/// `retrieve_for_mapreduce` does not return), and a `.` sits between the letter
/// and the clause paren, which `norm` does not strip. The provision was in the
/// corpus the whole time and the check could never see it.
///
/// So build a matcher from the token's *shape*: optional kind word, number,
/// optional letter suffix (`4-A`, `4A`), then each clause paren, tolerating the
/// `.`/`-`/space noise the gazette prints between them. `\b` before the number
/// keeps `4A(1)` from matching inside `24A(1)`.
/// Returns `(strict, loose)` patterns. `strict` wants the clause parens adjacent
/// to the number (`4-A.-(1)`); `loose` wants only the provision itself in a
/// provision-like position, because the gazette often separates a clause from its
/// number by the heading — `4. Penalties.- (1) The following are …`. A chunk IS
/// the provision, so locating `Rule 4` is enough to quote it.
fn provision_matcher(token: &str) -> Option<(regex_lite::Regex, regex_lite::Regex)> {
    static PARTS: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    let parts = PARTS.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)^(?:[a-z]+\s+)?([0-9]{1,3})\s*-?\s*([a-z]?)\s*((?:\(\s*[0-9a-z]+\s*\))*)$",
        )
        .expect("provision-token shape regex compiles")
    });
    let c = parts.captures(token.trim())?;
    let num = c.get(1)?.as_str();
    let letter = c.get(2).map(|m| m.as_str()).unwrap_or("");
    let subs = c.get(3).map(|m| m.as_str()).unwrap_or("");

    // `4` → `\b4`;  `4A` → `\b4\s*-?\s*A`
    let core = if letter.is_empty() {
        format!(r"\b{num}")
    } else {
        format!(r"\b{num}\s*-?\s*{letter}")
    };

    let mut clause = core.clone();
    let mut has_subs = false;
    for g in regex_lite::Regex::new(r"\(\s*([0-9A-Za-z]+)\s*\)").ok()?.captures_iter(subs) {
        let inner = g.get(1)?.as_str().to_string();
        clause.push_str(&format!(r"[\.\-\s]*\(\s*{inner}\s*\)"));
        has_subs = true;
    }

    // ⚠ A provision must be matched where it is DEFINED, not where it is merely
    // referred to. The E&D Rule 4-A chunk ends "…in accordance with rule 5." — a
    // cross-reference. An unanchored match would happily quote that chunk as
    // "Rule 5", which is the wrong-provision bug in miniature.
    //
    // So anchor to a defining position: start of the chunk or of a line,
    // optionally past a gazette footnote marker like `2[`, then either the kind
    // word ("Rule 4") or the number opening a clause ("4-A." / "4.").
    let anchor = r"(?:^|\n)\s*(?:[0-9]{1,2}\[)?\s*";
    let numl = if letter.is_empty() {
        num.to_string()
    } else {
        format!(r"{num}\s*-?\s*{letter}")
    };
    let defining = format!(
        r"{anchor}(?:(?:rule|section|article|clause|schedule|para(?:graph)?)\s*-?\s*{numl}\b|{numl}\s*[\.\-])"
    );

    // With clause parens: accept the adjacent clause form OR the defining position
    // (the gazette often separates them: "4. Penalties.- (1) The following…").
    // Without them: defining position only — a bare `\b4` would match any stray digit.
    let primary = if has_subs { clause } else { defining.clone() };
    Some((
        regex_lite::Regex::new(&format!("(?i){primary}")).ok()?,
        regex_lite::Regex::new(&format!("(?i){defining}")).ok()?,
    ))
}

/// Does this chunk actually contain the provision? Strict clause match first,
/// then the provision-number match, then the original normalised containment for
/// any token whose shape we cannot parse.
fn chunk_contains_provision(content: &str, token: &str) -> bool {
    match provision_matcher(token) {
        Some((strict, loose)) => strict.is_match(content) || loose.is_match(content),
        None => norm(content).contains(&norm(token)),
    }
}

/// Pull the individual provision tokens out of a notice's rule reference.
///
/// The model reports what the notice *says*, e.g. `"Rule 4-A (1) read with
/// sub-rule (3) of rule 5 of the Sindh Civil Servants (Efficiency & Discipline)
/// Rules, 1973"`. Matching that whole sentence verbatim inside a chunk can never
/// succeed — the bug that made every rule come back `found = false`. Retrieval and
/// verbatim checking run per **token**: `Rule 4-A(1)`, `Rule 5(3)`, `Rule 4`, each
/// of which really does appear in the corpus (`chunks.unit_label` is "Rule 4").
pub fn provision_tokens(reference: &str) -> Vec<String> {
    // Pass A — "sub-rule (3) of rule 5" means Rule 5(3), NOT "Rule 3". Consume the
    // whole phrase so pass B cannot re-read "rule 5" as a bare token.
    static SUB: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    let sub = SUB.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)\bsub-?(rule|section)s?\s*\(?\s*([0-9]+[A-Za-z]?)\s*\)?\s*(?:of|to)?\s*(?:the\s+)?(?:rule|section)s?[-\s]*([0-9]+(?:-?[A-Za-z])?)",
        )
        .expect("sub-provision regex compiles")
    });
    // Pass B — plain "Rule 4", "Rule 4-A (1)", "Section 5(4)". `\d{1,3}` deliberately
    // excludes 4-digit years: "…(Efficiency & Discipline) Rules-1973" must NOT yield
    // "Rule 1973". `paragraph|para` (longer alternative first) is included because
    // GFR numbers its clauses as *paras*, not rules — audit paras cite e.g.
    // "Para 213(5) of GFR" / "Para 149 of GFR Vol-I"; without it those citations
    // never tokenize and every GFR reference is a false miss. The trailing
    // `[-\s]*[0-9]` requirement keeps words like "parameter" from matching.
    static RE: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)\b(rule|section|article|clause|schedule|paragraph|para)s?[-\s]*([0-9]{1,3}(?:-?[A-Za-z])?)\b((?:\s*\(\s*[0-9A-Za-z]+\s*\))*)",
        )
        .expect("provision-token regex compiles")
    });

    let titlecase = |kind: &str| -> String {
        let mut s = String::new();
        let mut ch = kind.chars();
        if let Some(f) = ch.next() {
            s.extend(f.to_uppercase());
            s.push_str(ch.as_str());
        }
        s
    };

    let mut out: Vec<String> = Vec::new();
    let mut push = |label: String, out: &mut Vec<String>| {
        if !out.iter().any(|x| norm(x) == norm(&label)) {
            out.push(label);
        }
    };

    // Blank out pass-A spans so pass B doesn't double-count them.
    let mut rest = reference.to_string();
    for c in sub.captures_iter(reference) {
        let kind = c.get(1).map(|m| m.as_str()).unwrap_or("rule").to_lowercase();
        let sub_num = c.get(2).map(|m| m.as_str()).unwrap_or("");
        let parent = c.get(3).map(|m| m.as_str()).unwrap_or("").replace('-', "");
        if sub_num.is_empty() || parent.is_empty() {
            continue;
        }
        push(format!("{} {}({})", titlecase(&kind), parent, sub_num), &mut out);
        if let Some(m) = c.get(0) {
            rest.replace_range(m.range(), &" ".repeat(m.len()));
        }
    }

    for c in re.captures_iter(&rest) {
        let kind = c.get(1).map(|m| m.as_str()).unwrap_or("rule").to_lowercase();
        let Some(num) = c.get(2).map(|m| m.as_str().replace('-', "")).filter(|n| !n.is_empty()) else {
            continue;
        };
        let subs = c.get(3).map(|m| m.as_str()).unwrap_or("").replace(' ', "");
        push(format!("{} {}{}", titlecase(&kind), num, subs), &mut out);
    }
    out
}

/// Every provision the notice invokes, expanded to tokens and de-duplicated.
/// Each entry is `(token, full_reference)`: the reference (carrying the statute
/// name) drives retrieval; the token is what must appear verbatim.
fn invoked_rules(frame: &Frame) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for a in &frame.allegations {
        let Some(reference) = a.invoked_rule.as_ref().map(|r| r.trim()) else { continue };
        if reference.is_empty() {
            continue;
        }
        for token in provision_tokens(reference) {
            if !out.iter().any(|(t, _)| norm(t) == norm(&token)) {
                out.push((token, reference.to_string()));
            }
        }
    }
    out
}

/// Words carrying no instrument identity — dropped when matching a notice's
/// statute name against corpus document names.
const STOPWORDS: [&str; 16] = [
    "the", "of", "and", "read", "with", "sub", "rule", "rules", "section", "sections", "act",
    "under", "said", "in", "exercise", "powers",
];

/// Score a corpus document name against the statute a notice names.
/// Exposed for tests; the year match is deliberately weighted (E&D **1973** vs
/// APT **1974** differ only there).
pub fn instrument_score(reference: &str, document_name: &str) -> usize {
    let name = document_name.to_lowercase();
    let wanted: std::collections::HashSet<String> = reference
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_lowercase())
        .filter(|w| !STOPWORDS.contains(&w.as_str()) && !w.chars().all(|c| c.is_ascii_digit()))
        .collect();
    if wanted.is_empty() {
        return 0;
    }
    let hits = wanted.iter().filter(|w| name.contains(w.as_str())).count();
    let year_hit = reference
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w.len() == 4 && w.chars().all(|c| c.is_ascii_digit()) && name.contains(w));
    hits + if year_hit { 2 } else { 0 }
}

/// Source ids of the **instrument the notice actually names**.
///
/// Without this, `Rule 4` is grounded against whichever indexed document ranks
/// first — quoting a rule the notice never invoked (the reported bug). `None`
/// when nothing clears the bar: the caller then reports a miss rather than
/// citing the wrong Act.
async fn instrument_scope(pool: &Db, reference: &str, scope_ids: &[String]) -> Option<Vec<String>> {
    if reference.trim().is_empty() || scope_ids.is_empty() {
        return None;
    }
    let placeholders = crate::db::pg_in(scope_ids.len(), 1);
    let sql = format!("SELECT id, file_name FROM sources WHERE id IN ({placeholders})");
    let mut q = sqlx::query_as::<_, (String, String)>(&sql);
    for id in scope_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await.ok()?;

    let mut best: Vec<(usize, String)> = Vec::new();
    for (id, file_name) in rows {
        let name = crate::services::assistant::clean_document_name(&file_name);
        let score = instrument_score(reference, &name);
        if score >= 2 {
            best.push((score, id));
        }
    }
    if best.is_empty() {
        log::warn!("notice_assist: could not resolve the instrument named in {reference:?}");
        return None;
    }
    best.sort_by(|a, b| b.0.cmp(&a.0));
    let top = best[0].0;
    Some(best.into_iter().filter(|(s, _)| *s == top).map(|(_, id)| id).collect())
}

/// Ground the invoked provisions, then the officer's safeguards.
pub async fn ground(pool: &Db, embedder: &Arc<EmbedderState>, frame: &mut Frame, scope_ids: &[String]) {
    frame.cited_rule_text.clear();
    for (token, reference) in invoked_rules(frame) {
        let cited = ground_one(pool, embedder, &token, &reference, scope_ids).await;
        frame.cited_rule_text.push(cited);
    }
    ground_support(pool, embedder, frame, scope_ids).await;
}

async fn ground_one(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    token: &str,
    reference: &str,
    scope_ids: &[String],
) -> CitedRule {
    if scope_ids.is_empty() {
        return CitedRule::missed(token, GroundOutcome::NotInScope);
    }
    // Pin to the statute the notice names when we can resolve it. When we cannot,
    // fall back to the full scope rather than declaring the rule missing —
    // "I could not identify the Act" is NOT "this rule does not exist" (trace
    // 020ac9ac told an officer Rule 4 was unindexed while 29448bd5 was quoting it).
    // The result is flagged `InstrumentUnresolved` so the brief says so out loud
    // instead of silently passing off another statute's Rule 4 as his.
    let (search_ids, instrument_confirmed) =
        match instrument_scope(pool, reference, scope_ids).await {
            Some(ids) => (ids, true),
            None => {
                log::warn!(
                    "notice_assist: could not resolve the instrument named in {reference:?} — \
                     widening to full scope, result will be flagged unconfirmed"
                );
                (scope_ids.to_vec(), false)
            }
        };

    let query = format!("{token} {reference}");
    let hits = match crate::services::query_router::retrieve_for_mapreduce(
        pool, embedder, &query, &search_ids,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            log::warn!("notice_assist grounding retrieval failed for {token}: {e}");
            return CitedRule::missed(token, GroundOutcome::LookupMissed);
        }
    };

    // Match on the chunk's unit label as well as its body: `chunks.unit_label` is
    // the bare provision ("Rule 4") while the body often opens "4. Penalties.-",
    // so a body-only containment check misses provisions that ARE present.
    let mut candidates: Vec<(usize, String, Option<i64>, String)> = Vec::new();
    for (_chunk_id, _source_id, content, page, file_name) in hits {
        if chunk_contains_provision(&content, token) {
            // When the instrument is unconfirmed, prefer the document whose name
            // best matches the notice's reference rather than whichever Act
            // happened to rank first.
            let affinity = if instrument_confirmed {
                1
            } else {
                instrument_score(reference, &crate::services::assistant::clean_document_name(&file_name))
            };
            candidates.push((affinity, content, page, file_name));
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let Some((_, content, page, file_name)) = candidates.into_iter().next() else {
        return CitedRule::missed(token, GroundOutcome::LookupMissed);
    };
    let instrument = crate::services::assistant::clean_document_name(&file_name);
    let text = content.trim().to_string();
    CitedRule::grounded(
        token,
        text.clone(),
        CitationOut {
            n: 0, // renumbered by `citations()`
            instrument,
            path: token.to_string(),
            verbatim: text,
            page,
            ..Default::default()
        },
        instrument_confirmed,
    )
}

/// The provisions that work **in the officer's favour**. The notice cites the rule
/// used *against* him; nobody hands him the safeguards. Retrieved verbatim from the
/// SAME statute so he can cite them while writing his own reply. A topic the corpus
/// cannot support is omitted — never invented.
async fn ground_support(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    frame: &mut Frame,
    scope_ids: &[String],
) {
    frame.support_rules.clear();
    if scope_ids.is_empty() {
        return;
    }
    let instrument = frame.allegations.iter().find_map(|a| a.invoked_rule.clone()).unwrap_or_default();
    let Some(instrument_ids) = instrument_scope(pool, &instrument, scope_ids).await else {
        log::warn!("notice_assist: no instrument resolved — skipping support provisions");
        return;
    };

    const TOPICS: [(&str, &str); 5] = [
        ("Inquiry", "inquiry officer or inquiry committee shall be appointed"),
        ("Right of hearing", "opportunity of hearing personal hearing right of defence"),
        ("Penalty scale", "minor penalties and major penalties prescribed"),
        ("Appeal", "appeal or representation against the order of penalty"),
        ("Conduct alleged", "absence from duty leave without pay condonation joining"),
    ];

    let mut seen: std::collections::HashSet<String> = frame
        .cited_rule_text
        .iter()
        .filter_map(|c| c.verbatim.as_deref().map(norm))
        .collect();

    for (label, topic) in TOPICS {
        let query = format!("{topic} {instrument}");
        let Ok(hits) = crate::services::query_router::retrieve_for_mapreduce(
            pool, embedder, &query, &instrument_ids,
        )
        .await
        else {
            continue;
        };
        for (_chunk_id, _source_id, content, page, file_name) in hits {
            let body = content.trim();
            if body.len() < 40 || !seen.insert(norm(body)) {
                continue;
            }
            let instrument_name = crate::services::assistant::clean_document_name(&file_name);
            frame.support_rules.push(CitedRule::grounded(
                label,
                body.to_string(),
                CitationOut {
                    n: 0,
                    instrument: instrument_name,
                    path: label.to_string(),
                    verbatim: body.to_string(),
                    page,
                    ..Default::default()
                },
                // Support provisions are already scoped to the resolved instrument
                // by `ground_support`, so the statute is confirmed by construction.
                true,
            ));
            break; // best hit per topic
        }
    }
}

/// Every grounded provision — invoked-against-him first, then the safeguards.
/// Defines the `[n]` order used everywhere.
fn all_grounded(frame: &Frame) -> Vec<&CitedRule> {
    frame
        .cited_rule_text
        .iter()
        .filter(|c| c.is_quotable())
        .chain(frame.support_rules.iter().filter(|c| c.is_quotable()))
        .collect()
}

/// Citations for the turn, renumbered 1..n over every grounded provision.
pub fn citations(frame: &Frame) -> Vec<CitationOut> {
    all_grounded(frame)
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| {
            c.citation.clone().map(|mut cit| {
                cit.n = (i + 1) as i64;
                cit
            })
        })
        .collect()
}

/// Gate haystack: the verbatim corpus text backing the turn.
pub fn gate_context(frame: &Frame) -> String {
    all_grounded(frame)
        .into_iter()
        .filter_map(|c| c.verbatim.clone())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Numbered provisions handed to the model when it critiques the officer's draft.
/// `[n]` markers match [`citations`].
pub fn numbered_provisions(frame: &Frame) -> String {
    all_grounded(frame)
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "[{n}] {label}{instrument}\n{text}",
                n = i + 1,
                label = c.invoked_ref,
                instrument = c
                    .citation
                    .as_ref()
                    .map(|x| format!(" — {}", x.instrument))
                    .unwrap_or_default(),
                text = c.verbatim.clone().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn quote(text: &str) -> String {
    format!("> {}", text.replace('\n', "\n> "))
}

/// The **reply-support brief** — what the officer gets immediately after
/// attaching the notice. Deterministic assembly (no LLM): the charge as the
/// notice states it, the rule invoked (verbatim), the provisions in his favour
/// (verbatim), and what he still needs. He writes the reply; this arms him.
pub fn support_brief(state: &NoticeAssistState) -> String {
    let f = &state.frame;
    let audit = f.document_kind == DocKind::AuditPara;
    let mut out = String::new();
    let mut n = 0usize;

    out.push_str(if audit {
        "## Audit para — support for your response\n\n"
    } else {
        "## Show-cause notice — support for your reply\n\n"
    });

    // 1. The charge, in the notice's own words.
    out.push_str(if audit { "### The objection\n" } else { "### What you are charged with\n" });
    if f.allegations.is_empty() || f.allegation_details_missing() {
        out.push_str("⚠ The specific allegation could not be read from the attached copy.\n");
    } else {
        for a in &f.allegations {
            out.push_str(&format!("- {}\n", a.text));
        }
    }
    if let Some(d) = &f.deadline {
        out.push_str(&format!("\n**Deadline:** {d}\n"));
    }
    if let Some(forum) = &f.forum {
        out.push_str(&format!("**Reply to:** {forum}\n"));
    }
    out.push('\n');

    // 2. The provision invoked against him.
    out.push_str("### The provision invoked against you\n");
    if f.cited_rule_text.is_empty() {
        out.push_str(
            "⚠ The notice names no provision, or it could not be identified. Ask the issuing \
             authority to specify the rule relied upon.\n\n",
        );
    }
    // Each outcome says only what the code actually knows. "Not found in the
    // indexed corpus" is a claim about the CORPUS and was wrong in trace 020ac9ac
    // — the lookup missed; the rule was there all along.
    for c in &f.cited_rule_text {
        match c.outcome {
            GroundOutcome::Found | GroundOutcome::NotAttempted if c.is_quotable() => {
                n += 1;
                out.push_str(&format!(
                    "**{r}** [{n}]\n\n{q}\n\n",
                    r = c.invoked_ref,
                    n = n,
                    q = quote(c.verbatim.as_deref().unwrap_or_default())
                ));
            }
            GroundOutcome::InstrumentUnresolved => {
                n += 1;
                let instrument = c
                    .citation
                    .as_ref()
                    .map(|x| x.instrument.as_str())
                    .unwrap_or("an indexed document");
                out.push_str(&format!(
                    "**{r}** [{n}]\n\n{q}\n\n⚠ This text is from **{instrument}**, but the \
                     statute your notice names could not be matched to an indexed document — \
                     confirm this is the instrument the notice relies on before quoting it.\n\n",
                    r = c.invoked_ref,
                    n = n,
                    q = quote(c.verbatim.as_deref().unwrap_or_default())
                ));
            }
            GroundOutcome::LookupMissed => out.push_str(&format!(
                "⚠ **{r}** — could not be located in the indexed text of that statute. Obtain \
                 the official text before relying on it; do not paraphrase it in your reply.\n\n",
                r = c.invoked_ref
            )),
            GroundOutcome::NotInScope => out.push_str(&format!(
                "⚠ **{r}** — no documents were in scope to search. Index the governing statute, \
                 or attach it, then run `/reply` again.\n\n",
                r = c.invoked_ref
            )),
            GroundOutcome::Found | GroundOutcome::NotAttempted => out.push_str(&format!(
                "⚠ **{r}** — grounding did not complete for this provision. Obtain the official \
                 text before relying on it.\n\n",
                r = c.invoked_ref
            )),
        }
    }

    // 3. The provisions that protect him — the point of this feature.
    out.push_str("### Provisions in your favour\n");
    if f.support_rules.is_empty() {
        out.push_str(
            "⚠ No safeguard provisions could be retrieved from the indexed corpus for this \
             statute. Index the governing Rules/Ordinance, or supply them.\n\n",
        );
    } else {
        out.push_str(
            "_Quote these verbatim in your reply — each is taken from the same statute the notice \
             relies on._\n\n",
        );
        for c in &f.support_rules {
            n += 1;
            out.push_str(&format!(
                "**{label}** [{n}]\n\n{q}\n\n",
                label = c.invoked_ref,
                n = n,
                q = quote(c.verbatim.as_deref().unwrap_or_default())
            ));
        }
    }

    // 4. What to put on the record — no invented facts, just the checklist.
    out.push_str("### What to put on the record\n");
    out.push_str("- The sanction, approval or authority you acted under\n");
    out.push_str("- Any **written** order or instruction of a superior (and its number/date)\n");
    out.push_str("- File notings, approvals, registers or leave records that support you\n");
    out.push_str("- Whether an inquiry officer was appointed, and whether you were heard\n");
    out.push_str("- That no personal benefit accrued and no loss was caused, if that is the case\n\n");

    if f.poorly_read() || f.allegation_details_missing() {
        out.push_str(
            "⚠ The attached copy was hard to read. Attach a clearer scan (or paste the allegation) \
             so the charge can be addressed precisely.\n\n",
        );
    }

    out.push_str(
        "---\n**Now write your reply here, a paragraph at a time.** I'll check each paragraph \
         against the provisions above: I'll flag any claim you haven't evidenced, tell you which \
         `[n]` supports a point, and warn you if you cite a rule that isn't in the corpus. I will \
         not write the reply for you.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::assistant::capabilities::notice_assist::frame::Allegation;

    fn cited(reference: &str, found: bool, text: &str) -> CitedRule {
        if found {
            CitedRule::grounded(
                reference,
                text.to_string(),
                CitationOut {
                    n: 0,
                    instrument: "Sindh Civil Servants Efficiency and Discipline Rules 1973".into(),
                    path: reference.into(),
                    verbatim: text.into(),
                    page: Some(2),
                    ..Default::default()
                },
                true,
            )
        } else {
            CitedRule::missed(reference, GroundOutcome::LookupMissed)
        }
    }

    fn base_frame() -> Frame {
        Frame {
            document_kind: DocKind::ShowCause,
            allegations: vec![Allegation {
                id: 1,
                text: "You remained absent from 01.04.2019 till 21.12.2020".into(),
                invoked_rule: Some(
                    "Rule 4-A (1) read with sub-rule (3) of rule 5 of the Sindh Civil Servants \
                     (Efficiency & Discipline) Rules, 1973"
                        .into(),
                ),
                invoked_authority: None,
                questions: vec![],
            }],
            deadline: Some("within 14 days".into()),
            ..Default::default()
        }
    }

    #[test]
    fn provision_tokens_split_a_compound_reference() {
        let toks = provision_tokens(
            "Rule 4-A (1) read with sub-rule (3) of rule 5 of the Sindh Civil Servants \
             (Efficiency & Discipline) Rules, 1973",
        );
        // "Rule 4-A (1)" → Rule 4A(1); "sub-rule (3) of rule 5" → Rule 5(3), never "Rule 3".
        assert!(toks.iter().any(|t| norm(t) == norm("Rule 4A(1)")), "got {toks:?}");
        assert!(toks.iter().any(|t| norm(t) == norm("Rule 5(3)")), "got {toks:?}");
        assert!(!toks.iter().any(|t| norm(t) == norm("Rule 3")), "sub-rule number is not a rule: {toks:?}");
        // The statute's year must never become a provision token.
        assert!(!toks.iter().any(|t| t.contains("1973")), "year leaked as a rule: {toks:?}");
    }

    #[test]
    fn provision_tokens_handle_the_simple_sc02_reference() {
        let toks =
            provision_tokens("Rule-4 of the Sindh Civil Servants (Efficiency & Discipline)Rules-1973");
        assert_eq!(toks.len(), 1, "got {toks:?}");
        assert_eq!(norm(&toks[0]), norm("Rule 4"));
    }

    #[test]
    fn provision_tokens_recognise_gfr_para_citations() {
        // GFR numbers its clauses as paras — audit paras cite them this way.
        let a = provision_tokens("Para 213 (5) of GFR");
        assert!(a.iter().any(|t| norm(t) == norm("Para 213(5)")), "got {a:?}");
        let b = provision_tokens("Para 149 of General Financial Rules (GFR) Vol-I");
        assert!(b.iter().any(|t| norm(t) == norm("Para 149")), "got {b:?}");
        // "paragraph" (the long form) must also tokenise, not half-match to "para".
        let c = provision_tokens("paragraph 23 of GFR");
        assert!(c.iter().any(|t| norm(t) == norm("Paragraph 23")), "got {c:?}");
        // A word that merely starts with "para" is not a provision.
        assert!(provision_tokens("this is of paramount importance").is_empty());
    }

    #[test]
    fn instrument_score_prefers_the_named_statute_and_year() {
        let reference = "Rule 4 of the Sindh Civil Servants (Efficiency & Discipline) Rules, 1973";
        let ed = instrument_score(reference, "Sindh Civil Servants Efficiency and Discipline Rules 1973");
        let apt = instrument_score(reference, "Sindh Civil Servants APT Rules 1974");
        let sppra = instrument_score(reference, "SPPRA Rules 2010 Revised 2013");
        assert!(ed > apt, "E&D {ed} must beat APT {apt}");
        assert!(ed > sppra, "E&D {ed} must beat SPPRA {sppra}");
        assert!(sppra < 2, "an unrelated statute must not clear the bar (got {sppra})");
    }

    #[test]
    fn brief_quotes_invoked_rule_and_the_safeguards_with_stable_numbering() {
        let mut f = base_frame();
        f.cited_rule_text = vec![cited(
            "Rule 4",
            true,
            "4. Penalties.- (1) The following are the minor and major penalties",
        )];
        f.support_rules = vec![
            cited("Inquiry", true, "5. (1) The authorised officer shall appoint an Inquiry Officer"),
            cited("Right of hearing", true, "The accused shall be given an opportunity of hearing"),
        ];
        let mut s = NoticeAssistState::new();
        s.frame = f;
        let out = support_brief(&s);

        assert!(out.contains("Penalties"), "invoked rule quoted");
        assert!(out.contains("[1]"), "invoked rule numbered first");
        assert!(out.contains("Provisions in your favour"));
        assert!(out.contains("Inquiry Officer") && out.contains("[2]"), "safeguard quoted + numbered");
        assert!(out.contains("opportunity of hearing") && out.contains("[3]"));
        assert!(out.contains("Now write your reply"), "co-writing invitation present");
        assert!(!out.contains("Relief sought"), "no ready-made letter");

        let cits = citations(&s.frame);
        assert_eq!(cits.len(), 3);
        assert_eq!(cits[0].n, 1);
        assert_eq!(cits[2].n, 3);
        assert!(gate_context(&s.frame).contains("Inquiry Officer"));
        assert!(numbered_provisions(&s.frame).contains("[3]"));
    }

    #[test]
    fn missing_rule_is_flagged_never_quoted_from_memory() {
        let mut f = base_frame();
        f.cited_rule_text = vec![cited("Rule 99", false, "")];
        let mut s = NoticeAssistState::new();
        s.frame = f;
        let out = support_brief(&s);
        // The message must claim only what the code knows: the lookup missed
        // inside that statute — NOT that the corpus lacks the rule (trace
        // 020ac9ac made the stronger claim and it was false).
        assert!(out.contains("could not be located in the indexed text of that statute"));
        assert!(!out.contains("not found in the indexed corpus"),
                "must not make a corpus-wide claim the lookup cannot support");
        assert!(out.contains("do not paraphrase"));
        assert!(citations(&s.frame).is_empty(), "an ungrounded rule cites nothing");
    }

    #[test]
    fn provision_matcher_finds_the_gazette_rendering() {
        // VERBATIM from the E&D Rules 1973 (pdftotext -layout, line 184). This is
        // the exact text that made trace fe568a07 report Rule 4A(1) as missing:
        // no literal "Rule", a footnote marker, "4-A", and ".-" before "(1)".
        const REAL: &str = "2[4-A.-(1) If in the opinion of the authorized officer there are \
                            sufficient grounds for proceeding against the civil servant under \
                            the rules, he shall take action in accordance with rule 5.";
        assert!(chunk_contains_provision(REAL, "Rule 4A(1)"),
                "the old norm-containment check missed this — the rule was there all along");
        assert!(chunk_contains_provision(REAL, "Rule 4A"));

        // Plain provisions still match the ordinary rendering.
        const PLAIN: &str = "Rule 4\n4. Penalties.- (1) The following are the minor and major \
                             penalties, namely-";
        assert!(chunk_contains_provision(PLAIN, "Rule 4"));
        assert!(chunk_contains_provision(PLAIN, "Rule 4(1)"));

        // And it must not fire on a different provision that merely shares digits.
        assert!(!chunk_contains_provision("24-A.-(1) something else", "Rule 4A(1)"),
                "\\b must stop 4A matching inside 24A");
        assert!(!chunk_contains_provision(REAL, "Rule 5(2)"));
    }

    #[test]
    fn unconfirmed_instrument_is_quoted_but_flagged() {
        // The text was located, but the notice's statute could not be matched to
        // an indexed document. It must be QUOTED (it is real corpus text) and
        // simultaneously flagged — the officer needs both facts.
        let mut f = base_frame();
        f.cited_rule_text = vec![CitedRule::grounded(
            "Rule 4",
            "4. Penalties.- minor and major".into(),
            CitationOut {
                n: 0,
                instrument: "Sindh Civil Servants APT Rules 1974".into(),
                path: "Rule 4".into(),
                verbatim: "4. Penalties.- minor and major".into(),
                page: Some(2),
                ..Default::default()
            },
            false, // instrument NOT confirmed
        )];
        let mut s = NoticeAssistState::new();
        s.frame = f;
        let out = support_brief(&s);
        assert!(out.contains("4. Penalties.- minor and major"), "text must still be quoted");
        assert!(out.contains("could not be matched to an indexed document"));
        assert!(out.contains("Sindh Civil Servants APT Rules 1974"),
                "must name the document the text actually came from");
        assert_eq!(citations(&s.frame).len(), 1, "an unconfirmed quote still carries its citation");
    }

    #[test]
    fn ground_outcome_and_found_cannot_drift() {
        // `found` is kept only for session compatibility and is derived by the
        // constructors — the two must never disagree.
        let g = CitedRule::grounded(
            "Rule 4", "t".into(),
            CitationOut { n: 0, instrument: "i".into(), path: "p".into(), verbatim: "t".into(), page: None , ..Default::default() },
            true,
        );
        assert!(g.found && g.outcome == GroundOutcome::Found && g.is_quotable());
        let u = CitedRule::grounded(
            "Rule 4", "t".into(),
            CitationOut { n: 0, instrument: "i".into(), path: "p".into(), verbatim: "t".into(), page: None , ..Default::default() },
            false,
        );
        assert!(u.found && u.outcome == GroundOutcome::InstrumentUnresolved && u.is_quotable());
        for o in [GroundOutcome::LookupMissed, GroundOutcome::NotInScope] {
            let m = CitedRule::missed("Rule 4", o);
            assert!(!m.found && m.outcome == o && !m.is_quotable());
        }
    }

    #[test]
    fn no_safeguards_retrieved_is_stated_not_invented() {
        let mut f = base_frame();
        f.cited_rule_text = vec![cited("Rule 4", true, "4. Penalties.- minor and major")];
        let mut s = NoticeAssistState::new();
        s.frame = f;
        let out = support_brief(&s);
        assert!(out.contains("No safeguard provisions could be retrieved"));
    }

    #[test]
    fn illegible_scan_is_surfaced_in_the_brief() {
        let mut f = base_frame();
        f.allegations[0].text = "Misconduct".into();
        f.source_chars = 300;
        let mut s = NoticeAssistState::new();
        s.frame = f;
        let out = support_brief(&s);
        assert!(out.contains("could not be read"));
        assert!(out.contains("clearer scan"));
    }
}
