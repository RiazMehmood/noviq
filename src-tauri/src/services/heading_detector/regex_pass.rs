//! Pass A — regex heading detection. Returns labelled candidates with confidence.

use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeadingCandidate {
    /// Byte offset into the source text where this candidate begins.
    pub offset: usize,
    /// The literal heading text as it appears in the source (e.g., "Section 8(2)").
    pub label: String,
    /// 'document' is reserved for the synthetic root; here only 'section' is emitted.
    pub unit_type: HeadingType,
    /// 0.0..=1.0 — high (>=0.85) skips Pass B; low (<0.85) triggers Pass B disambiguation.
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadingType { Section }

struct PatternSet {
    /// (regex, confidence)
    patterns: Vec<(Regex, f32)>,
}

fn patterns() -> &'static PatternSet {
    static P: OnceLock<PatternSet> = OnceLock::new();
    P.get_or_init(|| {
        let patterns = vec![
            // High confidence: explicit legal markers at line start.
            // "Section 8", "Section 8(2)", "Rule 12(2)(a)", "Article 7", "Chapter III", "Schedule II", "Clause 4.1"
            (Regex::new(
                r"(?m)^(?P<lbl>(?:Section|Rule|Article|Chapter|Schedule|Clause|Sub-clause|Part)\s+[\divxlcIVXLC]+(?:\([\divxlcIVXLC]+\))*(?:\([a-zA-Z]\))?)(?:[^\w(]|$)"
            ).unwrap(), 0.95),

            // High confidence: Pakistani-style notification headers (line-start, dated).
            (Regex::new(
                r"(?m)^(?P<lbl>(?:[A-Z][A-Za-z&]+\s+)*Notification\s+dated\s+\d{1,2}(?:st|nd|rd|th)?\s+[A-Z][a-z]+\s+\d{4})"
            ).unwrap(), 0.90),

            // Lower confidence: numbered outline headings ("1. Introduction", "2.1 Methodology").
            (Regex::new(
                r"(?m)^(?P<lbl>\d+(?:\.\d+){0,3}\.?\s+[A-Z][^\n]{1,80})$"
            ).unwrap(), 0.55),
        ];
        PatternSet { patterns }
    })
}

pub fn scan(text: &str) -> Vec<HeadingCandidate> {
    let mut hits: Vec<HeadingCandidate> = Vec::new();
    for (re, conf) in &patterns().patterns {
        for cap in re.captures_iter(text) {
            let m = cap.name("lbl").expect("named group missing");
            hits.push(HeadingCandidate {
                offset: m.start(),
                label: m.as_str().trim().to_string(),
                unit_type: HeadingType::Section,
                confidence: *conf,
            });
        }
    }
    // Dedup by offset; keep the highest-confidence candidate per offset.
    hits.sort_by(|a, b| {
        a.offset.cmp(&b.offset)
            .then(b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
    });
    hits.dedup_by_key(|h| h.offset);
    hits
}

// ── Feature 005 — profile-aware heading scan ─────────────────────────────────

use crate::services::doc_profiles::DocProfile;
use crate::services::structure_parser::NodeLevel;

/// A heading candidate with explicit level (six-level vocabulary).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LevelledHeading {
    pub offset: usize,
    pub label:  String,
    pub level:  NodeLevel,
}

/// Run a profile's heading_patterns over `text` and emit ordered candidates.
/// Duplicates at the same offset are deduped, keeping the deepest level
/// (most specific match wins).
pub fn scan_with_profile(text: &str, profile: &DocProfile) -> Vec<LevelledHeading> {
    scan_with_profile_unit(text, profile, None)
}

/// As `scan_with_profile`, but applies a resolved citation-unit override
/// (Phase 5 / D1). When `unit_override` is `Some(word)`, provision-level
/// (Article/Clause) labels whose leading word is the profile's default unit are
/// re-minted with `word` (e.g. `manual`'s "Section 9" → "Article 9" once the
/// resolver proves the document calls its provisions Articles). Structural
/// labels (Chapter/Part/Schedule) are never touched.
pub fn scan_with_profile_unit(
    text: &str,
    profile: &DocProfile,
    unit_override: Option<&str>,
) -> Vec<LevelledHeading> {
    let mut hits: Vec<LevelledHeading> = Vec::new();
    for hp in profile.heading_patterns {
        let re = Regex::new(hp.regex).expect("invalid heading regex in profile");
        for cap in re.captures_iter(text) {
            // US8/R1 (FR-013a): a bare 4-digit year in the `num` position is a
            // statute year ("…Rules, 2017…"), never a provision ordinal — never
            // mint "Rule 2017"/"Section 2010". Index-time mirror of the runtime
            // `decompose::looks_like_year` guard, which never saw stored labels.
            if let Some(num) = cap.name("num") {
                if looks_like_year(num.as_str()) {
                    continue;
                }
            }
            let m = cap.get(0).unwrap();
            let mut label = format_citation(hp.citation_fmt, &cap);
            if let Some(unit) = unit_override {
                if matches!(hp.level, NodeLevel::Article | NodeLevel::Clause) {
                    label = swap_unit_word(&label, profile.citation_unit, unit);
                }
            }
            hits.push(LevelledHeading { offset: m.start(), label, level: hp.level });
        }
    }
    // 2026-07-12 (item 1): gazette-footnote Part suppression. An amendment-compiled
    // instrument (E&D 1973) carries editorial footnotes like `Part IV-A, dated
    // 9-7-1985, the Sindh Govt. Gaz., …` that the Part heading pattern mis-mints as
    // a spurious container ("Part IV"), which then steals the following provision's
    // clauses. A genuine Part heading never carries a gazette/notification/date
    // citation on its own line — so drop Part-level hits whose line does.
    hits.retain(|h| {
        h.level != NodeLevel::Part || !line_is_gazette_footnote(line_at(text, h.offset))
    });
    // US6 (T038/T039): structural boundaries — `SECOND SCHEDULE`, `PART-I`,
    // `ANNEXURE`, `APPENDIX`, `FORM`. Document-CLASS grammar (NOT per-document
    // keywords). Emitted at Part level so they OPEN a new container and RESET the
    // provision-label run — schedule/annexure content no longer inherits the
    // preceding Rule's label (the live `Rule 27(5)` / `Rule 22(v)` misattribution).
    hits.extend(scan_structural_boundaries(text));

    hits.sort_by(|a, b| {
        a.offset.cmp(&b.offset)
            .then((b.level as usize).cmp(&(a.level as usize)))
    });
    hits.dedup_by_key(|h| h.offset);
    // Nesting audit 2026-07-02: an ambiguous `(i)/(v)/(x)/(ii)` marker matched
    // both the alpha SubClause and roman Item patterns; the offset-dedup above
    // kept the deeper (Item) reading. Decide each occurrence by sequence context
    // — `(i)` after `(h)` is the alpha run continuing; `(i)` under `(l)` opens a
    // roman list.
    resolve_ambiguous_sub_markers(&mut hits);
    // 013/D4 (T022) — catchline↔body collapse. A provision's catchline
    // (`9. Security of person`) and its plain-prose body (`9. No person shall…`)
    // BOTH begin with the same `N.` and so are both detected as Article
    // headings. Two nodes with an identical label collide on `unit_path` at tree
    // build and the provision is dropped entirely — this is exactly how Articles
    // 9/33/34/35/39 vanished while 8/10/… (whose body opens with `(1)`, not a
    // capital letter) survived with a single match. Collapse consecutive
    // same-label headings to the EARLIEST offset so the node spans the catchline
    // through the body as one provision. Safe: two genuinely distinct provisions
    // never share a label AND sit adjacent in reading order — only a
    // catchline/body repeat does. `dedup_by` keeps the first (lower offset) of
    // each equal run.
    //
    // ORDER MATTERS: the serial-table guard below must run BEFORE this collapse.
    // `dedup_by` keeps the EARLIEST offset of an equal run, so when a table row
    // precedes the real provision of the same number (APT's appointing-authority
    // table sits between Rules 4 and 5), collapsing first would fuse them at the
    // TABLE's offset — and suppressing the table would then delete the genuine
    // rule with it. Removing the rows first leaves the collapse only real
    // provisions to consider.
    suppress_serial_column_tables(text, &mut hits);
    hits.dedup_by(|a, b| a.label == b.label);
    // US5/US6 (Phase 1) — Schedule-Aware Heading Suppression: inside a structural
    // container that exhibits a delegation-table signature, bare-number lines
    // (`3. Leases of land`) are TABLE SERIALS, not Rule headings. Drop them so the
    // rows fall under their container (e.g. `Part-II`) instead of minting a false
    // `Rule 3`. Runs BEFORE the sequence filter (which can't tell a serial from a
    // rule). A missing row is preferable to a false legal citation.
    suppress_schedule_table_serials(text, &mut hits);
    drop_out_of_sequence_articles(&mut hits);
    // 013/iter6: dotted-leader suppression. A heading sitting on a line that
    // carries a dotted leader (`6. High treason ………… 12`) is a Table-of-Contents
    // entry or a form fill-blank — never a real provision. The bulk TOC is lifted
    // out upstream by `strip_front_matter`; this catches any stray dotted line
    // that survives in the body (e.g. mid-document form blanks). Structural
    // boundaries (Part/Chapter) are never dotted, so they are unaffected.
    hits.retain(|h| !heading_line_is_dotted(text, h.offset));
    // 013/iter9: drop committee-ROSTER rows ("2. Deputy Chairman … (Member)",
    // "5. A representative from East Pakistan (Member)") — a numbered line whose
    // body is a person/post + a parenthesised or right-aligned role, or carries
    // "ex-officio". These are membership tables, not provisions. A provision title
    // ("5. Powers of the Chairman") has the role inline (no parens / right-align /
    // ex-officio), so it is never touched. Only Article-level bare-number hits.
    hits.retain(|h| !(h.level == NodeLevel::Article && heading_line_is_roster(text, h.offset)));
    // 2026-08-04: drop office-order CIRCULATION-LIST entries ("15. The
    // Collectors/Deputy Commissioners (all) in Sindh."). Same shape as the
    // roster rule but with no role column, so `is_roster_line` cannot see them;
    // found in the Dev Projects Manual annexes by the post-re-index gap check.
    hits.retain(|h| !(h.level == NodeLevel::Article && heading_line_is_circulation(text, h.offset)));
    // 2026-07-29: second, GLOBAL sequence pass — the streaming rule above cannot
    // see a footnote number that sits near the END of a document (`Article 343`,
    // `Section 1752`, `Section 156`). Runs LAST, deliberately: it reasons about
    // the shape of the whole provision run, so it must see the final set with
    // TOC leftovers and roster rows already removed — a stray dotted contents
    // line would otherwise distort the very statistics it judges by.
    drop_footnote_tail_outliers(&mut hits);
    hits
}

/// Nesting audit 2026-07-02 — sequence-aware resolution of markers that read
/// as EITHER an alpha sub-clause or a roman item. Legal drafting reuses the
/// same glyphs at both depths: SPPRA Rule 2(1) runs `(a)…(h),(i),(j)…` where
/// `(i)` is the ninth LETTER, while Rule 2(1)(l) nests `(i)…(iv)` where `(i)`
/// is roman ONE — in the same rule. Regexes cannot decide (regex_lite has no
/// lookaround, and the reading is inherently sequential), so this pass walks
/// the offset-ordered hits tracking the active alpha run per container and the
/// active roman run per sub-clause:
///   - a marker that CONTINUES the open roman run (`(iv)` after `(iii)`) stays
///     an Item;
///   - else one that CONTINUES the sibling alpha run (`(i)` after `(h)`,
///     `(ii)` after `(hh)`, `(aa)` after `(z)`) demotes to SubClause;
///   - else a bare `(v)`/`(x)` with no run either way keeps the legacy alpha
///     reading; everything else (`(i)` list-opener, unambiguous roman shapes
///     like `(iii)`) stays an Item.
/// Any node at Article/Clause depth or shallower resets both runs.
fn resolve_ambiguous_sub_markers(hits: &mut [LevelledHeading]) {
    let mut last_alpha: Option<String> = None; // core of the open alpha run
    let mut last_roman: Option<String> = None; // core of the open roman run
    for h in hits.iter_mut() {
        match h.level {
            NodeLevel::SubClause => {
                last_alpha = Some(marker_core(&h.label));
                last_roman = None;
            }
            NodeLevel::Item => {
                let core = marker_core(&h.label);
                let continues_roman = last_roman
                    .as_deref()
                    .and_then(roman_succ)
                    .is_some_and(|s| s == core);
                let continues_alpha = last_alpha
                    .as_deref()
                    .map(alpha_succ)
                    .is_some_and(|s| s == core);
                if continues_roman {
                    last_roman = Some(core);
                } else if continues_alpha {
                    h.level = NodeLevel::SubClause;
                    last_alpha = Some(core);
                    last_roman = None;
                } else if matches!(core.as_str(), "v" | "x") {
                    // A stray single letter with no roman predecessor: the
                    // alpha reading (pre-audit behaviour) is the safer default.
                    h.level = NodeLevel::SubClause;
                    last_alpha = Some(core);
                    last_roman = None;
                } else {
                    // `(i)` opening a list, or an unambiguous roman shape.
                    last_roman = Some(core);
                }
            }
            _ => {
                // New Article/Clause/structural container — both runs reset.
                last_alpha = None;
                last_roman = None;
            }
        }
    }
}

/// The alphanumeric core of a bracketed marker label: `"(ii)"` → `"ii"`.
fn marker_core(label: &str) -> String {
    label
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Successor in an alpha sub-clause run: `h → i`, `z → aa`, `hh → ii`.
/// Doubled runs increment the (same) letter of both positions.
fn alpha_succ(core: &str) -> String {
    let chars: Vec<char> = core.chars().collect();
    match chars.as_slice() {
        ['z'] => "aa".to_string(),
        [c] if c.is_ascii_lowercase() => ((*c as u8 + 1) as char).to_string(),
        [a, b] if a == b && *a < 'z' => {
            let n = (*a as u8 + 1) as char;
            [n, n].iter().collect()
        }
        _ => String::new(), // no successor — never matches
    }
}

/// Successor in a roman item run (`i → ii → … → xv`); `None` past the table.
fn roman_succ(core: &str) -> Option<&'static str> {
    const SEQ: [&str; 15] = [
        "i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x", "xi", "xii", "xiii",
        "xiv", "xv",
    ];
    let pos = SEQ.iter().position(|r| *r == core)?;
    SEQ.get(pos + 1).copied()
}

/// The source line carrying the heading at `offset` (without the trailing
/// newline). A heading match begins at `^\s*` and `\s` includes `\n`, so the
/// match can START on a preceding blank line — advance to the first
/// non-whitespace character (the heading's number) before locating its line.
fn line_at(text: &str, offset: usize) -> &str {
    let mut o = offset.min(text.len());
    o = text[o..]
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| o + i)
        .unwrap_or(o);
    let start = text[..o].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = text[o..].find('\n').map(|i| o + i).unwrap_or(text.len());
    text.get(start..end).unwrap_or("")
}

/// True when the source line containing `offset` carries a dotted leader.
fn heading_line_is_dotted(text: &str, offset: usize) -> bool {
    crate::services::parsers::front_matter::is_dotted_line(line_at(text, offset))
}

/// True when the source line containing `offset` is a committee-roster row.
fn heading_line_is_roster(text: &str, offset: usize) -> bool {
    is_roster_line(line_at(text, offset))
}

/// True when the source line containing `offset` is a circulation-list entry.
fn heading_line_is_circulation(text: &str, offset: usize) -> bool {
    is_circulation_line(&entry_at(text, offset))
}

/// The provision's OWN opening text, not just its first physical line: from the
/// heading to the next numbered entry, the next blank line, or a bounded window
/// — whichever comes first.
///
/// ⚠️ 2026-08-05, found by re-indexing: reading one line cost the Constitution
/// **13 Articles** (57, 61, 79, 111, 119, 166, 171, 190, 205, 208, 211, 266,
/// 280 — it had been provably complete at 280/280). `pdftotext -layout` wraps a
/// provision across lines and parks its marginal catchline on the right, so
///
///     280. The Proclamation of Emergency issued on the twenty-third   Continuance of
///
/// opens with an addressee word and carries no verb until the FOURTH line
/// (`shall be deemed`) — indistinguishable, one line at a time, from
/// `20. The Private Secretary to Minister P&D, Sindh.`
///
/// "A provision STATES something" is the right discriminator; a physical line
/// was the wrong unit to test it on. A circulation entry is self-contained, so
/// widening the window to the entry cannot make one look like a provision, while
/// it lets a wrapped provision reach its verb.
fn entry_at(text: &str, offset: usize) -> String {
    /// Enough for a wrapped opening sentence; far past any addressee line.
    const MAX: usize = 400;
    let mut o = offset.min(text.len());
    o = text[o..]
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| o + i)
        .unwrap_or(o);
    let start = text[..o].rfind('\n').map(|i| i + 1).unwrap_or(0);

    let mut out = String::new();
    for (k, line) in text[start..].lines().enumerate() {
        if k == 0 {
            out.push_str(line);
            continue;
        }
        // The next numbered entry, or the end of the paragraph, ends this one.
        let t = line.trim();
        if t.is_empty() || numbered_entry_opener().is_match(line) || out.len() >= MAX {
            break;
        }
        out.push(' ');
        out.push_str(t);
    }
    out
}

/// Runs of whitespace to a single space, trimmed. `pdftotext -layout` pads
/// columns out to the page width, so a raw `.len()` measures the page, not the
/// sentence.
fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `12.` / `12A.` at the start of a line — where one numbered entry ends and the
/// next begins.
fn numbered_entry_opener() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*\d+[A-Z]?\.\s").unwrap())
}

/// A CIRCULATION-LIST entry (2026-08-04) — the addressee list at the foot of an
/// office order / notification: a numbered line naming an office or person and
/// nothing else.
///
///     15. The Collectors/Deputy Commissioners (all) in Sindh.
///     16. All Project Program Director /Coordinators under P&D.
///     20. The Private Secretary to Minister P&D, Sindh.
///
/// Found in the Manual for Development Projects 2019 (gap check, 2026-08-04):
/// its annexes carry office orders whose circulation lists were minted as
/// `Section 15…23`, manufacturing a phantom "Sections 24-29 missing" gap and,
/// worse, citable provision labels that name a *typist's distribution list*.
///
/// Why the existing guards could not see them:
///   * `is_roster_line` needs a parenthesised/right-aligned ROLE column
///     ("(Chairman)") — a circulation entry has no role column;
///   * `suppress_schedule_table_serials` needs the region to be a recognised
///     schedule span, but the annexes are delimited by a Title-Case running
///     header (`Annexes`) and `scan_structural_boundaries` requires a
///     STANDALONE ALL-CAPS line, so no span is ever opened;
///   * the sequence rules see one clean ascending run (15,16,…,23) with no
///     restart and no collision — structurally identical to real provisions.
///
/// The discriminator is that a provision STATES something and a circulation
/// entry only NAMES someone: every real provision in this corpus carries a
/// verb (`shall`, `may`, `will`, `is`, `means`, `includes` …), while an
/// addressee line is a bare noun phrase. Requiring BOTH an addressee opener
/// (`The`/`All`/`Mr`/`Ms`/`Messrs`) AND the absence of any verb keeps this
/// narrow, and it fails SAFE in the direction that matters: an entry we cannot
/// classify stays a heading (`23. Office Order File Sd/- …` survives), because
/// a spurious label is recoverable and a deleted provision is not.
///
/// Deliberately NOT matched (verified in the tests below):
///   * `13. The Secretary shall consider the matter` — has a verb;
///   * `1. RECEIPT AND DESPATCH` — no addressee opener;
///   * `30. On the expiry of the date for which anticipatory approval…` — ditto.
///
/// ⚠️ 2026-08-05 — the verb test alone is not enough, because the verb is not
/// always REACHABLE. Article 119 opens `119. The custody of the Provincial
/// Consolidated Fund, the …` and does not reach `shall be regulated` until the
/// NEXT PAGE, past a form feed and a page header; a footnote rule (`–––––`)
/// overruns `entry_at`'s window before then. Widening the window cannot fix
/// that. So there is a second, independent safeguard below: an addressee entry
/// is SHORT.
fn is_circulation_line(line: &str) -> bool {
    static NUM: OnceLock<Regex> = OnceLock::new();
    static ADDRESSEE: OnceLock<Regex> = OnceLock::new();
    static VERB: OnceLock<Regex> = OnceLock::new();

    let num = NUM.get_or_init(|| Regex::new(r"^\s*\d+[A-Z]?\.\s").unwrap());
    if !num.is_match(line) {
        return false;
    }
    // An addressee entry NAMES one office and stops — the longest real one in
    // this corpus is 68 chars (`17. The Deputy Secretary (Staff) to Chief
    // Secretary, Sindh, Karachi.`). A provision's opening sentence is far
    // longer. Measured on the collapsed entry so `-layout` column padding and
    // the marginal catchline do not inflate it. Twice the observed maximum
    // leaves room for a wrapped addressee while still failing SAFE: an entry we
    // cannot classify stays a heading, because a spurious label is recoverable
    // and a deleted provision is not.
    const MAX_ADDRESSEE_CHARS: usize = 140;
    if collapse_whitespace(line).chars().count() > MAX_ADDRESSEE_CHARS {
        return false;
    }
    // The addressee opener must follow the number directly: `N. The …`.
    let addressee =
        ADDRESSEE.get_or_init(|| Regex::new(r"^\s*\d+[A-Z]?\.\s+(The|All|Mr|Ms|Messrs)\b").unwrap());
    if !addressee.is_match(line) {
        return false;
    }
    // Any verb at all means the line STATES something — a provision, not an
    // addressee. Kept small and common on purpose: this is the safety catch,
    // so it should over-match rather than under-match.
    let verb = VERB.get_or_init(|| {
        Regex::new(
            r"(?i)\b(shall|may|must|will|would|is|are|was|were|be|been|has|have|had|means?|includes?|applies|shall\s+be)\b",
        )
        .unwrap()
    });
    !verb.is_match(line)
}

/// A committee-roster row: a numbered person/post whose line carries a role
/// designation as a parenthesised/right-aligned column ("(Chairman)", "  Member")
/// or "ex-officio". A provision title ("5. Powers of the Chairman") has the role
/// inline (no parens / right-align / ex-officio) and is never matched.
fn is_roster_line(line: &str) -> bool {
    static NUM: OnceLock<Regex> = OnceLock::new();
    static EXOFF: OnceLock<Regex> = OnceLock::new();
    static ROLE: OnceLock<Regex> = OnceLock::new();
    let num = NUM.get_or_init(|| Regex::new(r"^\s*\d+[A-Z]?\.\s").unwrap());
    if !num.is_match(line) {
        return false;
    }
    let exoff = EXOFF.get_or_init(|| Regex::new(r"(?i)\bex[-\s]?officio\b").unwrap());
    if exoff.is_match(line) {
        return true;
    }
    // A role word at the END of the line, preceded by a paren or a right-aligned
    // (2+ space) gap (the membership-table column).
    let role = ROLE.get_or_init(|| {
        Regex::new(r"(?i)(\(|\s{2,}\(?)(chairman|members?|convener|convenor|secretary)\)?\s*$").unwrap()
    });
    role.is_match(line)
}

/// Phase 1 — Schedule-Aware Heading Suppression (Rule-N misattribution fix).
///
/// In delegation schedules (Sindh Delegation of Financial Powers, Second Schedule)
/// each departmental table restarts its serial numbering at `1, 2, 3 …`. The
/// generic bare-number heading pattern promotes every `"3. Leases of land"` to a
/// `Rule 3` heading — textually identical to a real rule, so neither the regex nor
/// the ascending-sequence filter can reject it.
///
/// The discriminator is **structural, not lexical**: such serials live inside a
/// container whose text carries a delegation-table signature (`To whom delegated`,
/// `Nature of power`, `Sr. #`, or a tight `(1)(2)(3)(4)` column header). We split
/// the document into the spans between structural boundaries (levels shallower than
/// Article — Part/Chapter/Schedule), and within any span that shows that signature
/// we drop the Article-level headings. The serial rows then attach to the nearest
/// container boundary (the desired fallback: `Part-II`/`Second Schedule`).
///
/// Safety: the signature never appears in ordinary rule bodies, so main-body rules
/// (and schedules without delegation tables) are untouched. The failure mode is
/// *fewer* headings inside one delegation table — never a false Rule citation, and
/// never a change outside such a table.
fn suppress_schedule_table_serials(text: &str, hits: &mut Vec<LevelledHeading>) {
    // Container boundaries = headings shallower than Article (Part/Chapter/Schedule,
    // all emitted at <= Part level), plus the document start. These delimit the
    // spans we test for a delegation-table signature.
    // Each boundary carries whether it OPENS a schedule/annexure/appendix — only
    // those spans get the RC-C serial-restart treatment (the main body and
    // Part/Chapter spans never do).
    let mut bounds: Vec<(usize, bool)> = hits
        .iter()
        .filter(|h| (h.level as usize) < (NodeLevel::Article as usize))
        .map(|h| (h.offset, is_schedule_label(&h.label)))
        .collect();
    bounds.push((0, false));
    bounds.sort_by_key(|&(o, _)| o);
    bounds.dedup_by_key(|&mut (o, _)| o);

    // Main-body provision numbers = Article serials BEFORE the first schedule.
    // A schedule serial that repeats one of these is a label collision (the live
    // `Rule/Section 30` appearing once in the body and again per schedule), not a
    // new provision — see the collision rule below.
    let first_schedule = bounds.iter().find(|&&(_, s)| s).map(|&(o, _)| o);
    let main_nums: std::collections::HashSet<u32> = match first_schedule {
        Some(fs) => hits
            .iter()
            .filter(|h| h.level == NodeLevel::Article && h.offset < fs)
            .filter_map(|h| leading_provision_number(&h.label))
            .collect(),
        None => std::collections::HashSet::new(),
    };

    // A span fully suppresses when it shows a delegation-table signature (any
    // span) OR (RC-C) it is a schedule span whose bare-number serials RESTART — a
    // multi-section distribution-of-business list, not a single Rule run.
    let mut suppress: Vec<(usize, usize)> = Vec::new();
    // Schedule spans — used for the per-heading collision rule below.
    let mut schedule_spans: Vec<(usize, usize)> = Vec::new();
    for (i, &(start, opens_schedule)) in bounds.iter().enumerate() {
        let end = bounds.get(i + 1).map(|&(o, _)| o).unwrap_or(text.len());
        if opens_schedule {
            schedule_spans.push((start, end));
        }
        if let Some(region) = text.get(start..end) {
            if is_delegation_table_context(region)
                || (opens_schedule && is_serial_restart_table(region))
            {
                suppress.push((start, end));
            }
        }
    }
    if suppress.is_empty() && schedule_spans.is_empty() {
        return;
    }

    let in_schedule = |off: usize| schedule_spans.iter().any(|&(a, b)| off >= a && off < b);

    // A serial number that recurs across schedule spans is a per-schedule index /
    // form row (each schedule renumbers from 1), not a unique provision — even
    // when no main-body rule shares the number. Count how many schedule serials
    // carry each number; >= 2 ⇒ recurring index row.
    let mut sched_num_count: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for h in hits.iter() {
        if h.level == NodeLevel::Article && in_schedule(h.offset) {
            if let Some(n) = leading_provision_number(&h.label) {
                *sched_num_count.entry(n).or_default() += 1;
            }
        }
    }

    hits.retain(|h| {
        if h.level != NodeLevel::Article {
            return true; // keep boundaries and deeper Clause/SubClause markers
        }
        // (a) full-span suppression: a delegation/restart table → all serials go.
        if suppress.iter().any(|&(a, b)| h.offset >= a && h.offset < b) {
            return false;
        }
        if in_schedule(h.offset) {
            if let Some(n) = leading_provision_number(&h.label) {
                // (b) collision with a genuine main-body provision, OR (c) a number
                // that recurs across schedules (index/form row) → drop the heading
                // so the one true `Rule N` stays unambiguous. Its content remains
                // retrievable under the schedule container. A schedule serial with
                // a UNIQUE, non-colliding number is kept.
                if main_nums.contains(&n) || sched_num_count.get(&n).copied().unwrap_or(0) >= 2 {
                    return false;
                }
            }
        }
        true
    });
}

/// True when a boundary label opens a Schedule / Annexure / Appendix container.
fn is_schedule_label(label: &str) -> bool {
    let l = label.to_ascii_lowercase();
    l.contains("schedule") || l.contains("annex") || l.contains("appendix")
}

/// Number of DECREASES required in a schedule's line-start serial sequence to
/// treat it as a multi-section table rather than a single provision run.
const SERIAL_RESTART_MIN_DECREASES: usize = 2;

/// RC-C (audit 2026-06-29) — true when a region's line-start bare-number serials
/// are NON-MONOTONIC: the sequence decreases `>= SERIAL_RESTART_MIN_DECREASES`
/// times. In Sindh Rules of Business' SCHEDULE-I (distribution of business) each
/// department renumbers its functions from 1, so `Rule/Section 5, 26, 32 …` were
/// minted once per department (Section 5 ×24, Section 32 ×14 in the live DB). A
/// genuine provision run is monotonic (0 decreases); a multi-section table
/// decreases at every department restart. Department-header lines (`4 - NAME`,
/// `4. NAME`) interleave the runs, so counting decreases — not a strict
/// drop-to-1 — is what catches them. Gated to schedule spans by the caller, so a
/// normal ascending schedule list (a First Schedule "list of laws") shows 0
/// decreases and is left untouched.
fn is_serial_restart_table(region: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?m)^[ \t]*(\d{1,3})\.[ \t]").unwrap());
    let mut prev: Option<u32> = None;
    let mut decreases = 0usize;
    for cap in re.captures_iter(region) {
        let Ok(n) = cap[1].parse::<u32>() else { continue };
        if let Some(p) = prev {
            if n < p {
                decreases += 1;
                if decreases >= SERIAL_RESTART_MIN_DECREASES {
                    return true;
                }
            }
        }
        prev = Some(n);
    }
    false
}

/// True when `region` text carries a delegation-table signature. Each marker is
/// highly specific to delegation/powers tables and absent from ordinary rule
/// prose. The `(1)(2)(3)(4)` form requires the markers to be adjacent (only
/// spaces/tabs between) so it matches a column header, NOT spread-out sub-clauses.
// ── Serial-column tables inside the BODY (2026-07-29) ────────────────────────
//
// `suppress_schedule_table_serials` above reasons at CONTAINER granularity: it
// asks whether a whole Part/Schedule span looks like a table. That is useless
// for a table sitting in the middle of a Part, which is where Sindh drafting
// usually puts one. APT Rules 1974 is the proof — Rule 4 introduces it inline:
//
//     4.- (1) The authorities competent to make appointments … shall be as follows:
//
//          S. No.        Posts                              Appointing Authority
//            1.   Posts sanctioned in Basic Pay Scales 1 to 2   Head of Office
//            2.   Posts sanctioned in Basic Pay Scales 3 to 5   …
//            3.   Posts sanctioned in Basic Pay Scales 6 to 8   …
//
// The `1. 2. 3.` are the S. No. COLUMN. They minted Rules 1/2/3 on top of the
// document's real Rules 1/2/3, so `Rule 3` named two different things and a
// query for it could be answered from the table while citing the statute.
//
// Why the container guard cannot be extended to cover it: firing at span
// granularity here would suppress every bare-number heading in `PART – I`,
// which holds the GENUINE Rules 1–5. That is exactly the poison-guard failure
// documented on `drop_out_of_sequence_articles` — one bad signal deleting real
// law. So this works at LINE granularity instead: find the table's own range
// and suppress only inside it.
//
// The range is anchored on a serial-column HEADER (`S. No.`, `Sr. #`, …) and
// extends while the ascending serial run continues, tolerating wrapped cell
// text between rows. Note the run is identified by the numbers themselves, not
// by column alignment: `normalize` collapses 3+ spaces to one, so the gutters
// that would most reliably mark a table row are already gone by the time we see
// the text.
//
// Also covers Delegation of Financial Powers and the Secretariat Instructions
// checklists, which collide the same way.

/// Rows needed before a serial run counts as a table rather than a coincidence.
const MIN_SERIAL_TABLE_ROWS: usize = 3;
/// How many non-row lines may sit between two rows (wrapped cell text) before
/// the run is considered finished.
const MAX_LINES_BETWEEN_SERIAL_ROWS: usize = 12;

/// A line whose FIRST column is a serial-number label — the header of a
/// numbered table. Matched per LINE, never over a span.
fn line_is_serial_column_header(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // `S. No.` / `S.No` / `Sr. No.` / `Sr. #` / `Serial No.`, then at least
        // one more column label — a bare "S. No." on its own is not a header.
        Regex::new(r"(?i)^\s*(?:s\.?\s*no\.?|sr\.?\s*(?:no\.?|#)|serial\s*no\.?)\b.*\w").unwrap()
    });
    re.is_match(line)
}

/// The serial at the start of a table row (`3. Posts sanctioned …`).
/// Deliberately does NOT match the catchline-dash provision form (`4.- (1) …`):
/// the dot must be followed by whitespace, so a real provision opening is never
/// read as a row serial.
fn line_initial_serial(line: &str) -> Option<u32> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^\s*(\d{1,3})\.?\s").unwrap());
    re.captures(line)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

/// Byte ranges covered by serial-column tables in `text`.
fn serial_table_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut offset = 0usize;
    let lines: Vec<(usize, &str)> = text
        .split_inclusive('\n')
        .map(|l| {
            let at = offset;
            offset += l.len();
            (at, l.trim_end_matches('\n'))
        })
        .collect();

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        if !line_is_serial_column_header(lines[i].1) {
            i += 1;
            continue;
        }
        let start = lines[i].0;
        let mut end = start + lines[i].1.len();
        let mut expected: Option<u32> = None;
        let mut rows = 0usize;
        let mut gap = 0usize;
        let mut j = i + 1;
        while j < lines.len() && gap <= MAX_LINES_BETWEEN_SERIAL_ROWS {
            let is_row = match (line_initial_serial(lines[j].1), expected) {
                // The run opens on a small serial (tables start at 1, sometimes
                // at 2 when the header row is itself numbered).
                (Some(n), None) if n <= 2 => {
                    expected = Some(n + 1);
                    true
                }
                (Some(n), Some(want)) if n == want => {
                    expected = Some(n + 1);
                    true
                }
                _ => false,
            };
            if is_row {
                rows += 1;
                end = lines[j].0 + lines[j].1.len();
                gap = 0;
            } else {
                gap += 1;
            }
            j += 1;
        }
        if rows >= MIN_SERIAL_TABLE_ROWS {
            ranges.push((start, end));
            i = j;
        } else {
            i += 1;
        }
    }
    ranges
}

/// Drop Article-level headings that are really rows of a serial-column table.
/// Fail-safe on the same principle as the guards above: if it wants to delete a
/// large share of the document's provisions it has misread the document, so it
/// keeps everything and says so.
fn suppress_serial_column_tables(text: &str, hits: &mut Vec<LevelledHeading>) {
    let ranges = serial_table_ranges(text);
    if ranges.is_empty() {
        return;
    }
    let in_table = |o: usize| ranges.iter().any(|&(a, b)| o >= a && o < b);

    let article_total = hits.iter().filter(|h| h.level == NodeLevel::Article).count();
    let doomed = hits
        .iter()
        .filter(|h| h.level == NodeLevel::Article && in_table(h.offset))
        .count();
    if doomed == 0 {
        return;
    }
    if article_total >= MIN_ARTICLES_FOR_SEQUENCE_GUARD
        && doomed * 100 > article_total * MAX_DROP_PCT
    {
        log::warn!(
            "serial-table guard disabled: would drop {doomed}/{article_total} Articles \
             ({}%) — the serial run looks like the document's real numbering, not a \
             table. Keeping all headings.",
            doomed * 100 / article_total.max(1),
        );
        return;
    }
    log::info!("serial-table guard: dropping {doomed} table-row heading(s) in {} table region(s)", ranges.len());
    hits.retain(|h| h.level != NodeLevel::Article || !in_table(h.offset));
}

fn is_delegation_table_context(region: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?i)to whom delegated|nature of power|sr\.?[ \t]*#|\(1\)[ \t]*\(2\)[ \t]*\(3\)[ \t]*\(4\)",
        )
        .unwrap()
    });
    re.is_match(region)
}

/// Detect structural boundary headings (schedules / parts / annexures /
/// appendices / forms). Heading-vs-reference disambiguation (FINAL §C.3): the
/// keyword must be a STANDALONE, ALL-CAPS line — so a mid-sentence reference like
/// "…in column 2 of the Second Schedule…" (mixed case) is never a boundary.
/// Returns Part-level headings (shallower than Article ⇒ resets inheritance).
/// True when a line carries an editorial gazette/notification/date citation — the
/// signature of an amendment footnote, never of a genuine structural heading.
fn line_is_gazette_footnote(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)dated\s+\d|gaz\.|notification\s+no|extra-?ordinary").unwrap()
    });
    re.is_match(line)
}

fn scan_structural_boundaries(text: &str) -> Vec<LevelledHeading> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            // RC-C (audit 2026-06-29): a SCHEDULE/ANNEXURE/APPENDIX may carry its
            // own roman/numeric suffix on the heading line — `SCHEDULE-I`,
            // `SCHEDULE - III`, `SCHEDULE– VI` (Sindh Rules of Business). The bare
            // word form (`SECOND SCHEDULE`) still matches via the optional suffix.
            // Without this the schedule never opens a container, so its
            // distribution-of-business serial lists were minted as duplicate Rules.
            // 2026-07-12 (item 1): tolerate a leading amendment-footnote marker
            // (`(?:\d*\[)?`, e.g. E&D `1[SCHEDULE`) before the boundary word — the
            // marker is consumed, not captured, so the label stays clean.
            r"(?m)^[ \t]*(?:\d*\[)?[ \t]*(?P<lbl>(?:(?:FIRST|SECOND|THIRD|FOURTH|FIFTH|SIXTH|SEVENTH|EIGHTH|NINTH|TENTH)[ \t]+)?(?:SCHEDULE|ANNEXURE|APPENDIX)(?:[ \t]*[-–—]?[ \t]*[IVXLC0-9]+)?|PART[ \t]*[-–—]?[ \t]*[IVXLC0-9]+|FORM[ \t]+[A-Z0-9-]+)[ \t]*$",
        )
        .unwrap()
    });
    // A `FIRST/SECOND SCHEDULE` / `PART-I` heading frequently sits at the TOP of
    // a page, immediately after a formfeed (`\x0C`) rather than a newline. The
    // `(?m)^` anchor matches after `\n` but NOT after `\x0C`, so those top-of-page
    // boundaries were missed and their rows inherited the preceding Rule's label.
    // Normalize formfeed → newline before scanning. Both are single UTF-8 bytes,
    // so every `m.start()` offset remains valid against the ORIGINAL `text`.
    let scan_src = text.replace('\u{0c}', "\n");
    let mut out = Vec::new();
    for cap in re.captures_iter(&scan_src) {
        let m = cap.name("lbl").expect("named group missing");
        let raw = m.as_str().trim();
        // The pattern's literals are uppercase, but guard against a stray
        // lowercase (e.g. a profile-specific token) sneaking into the match.
        if raw.chars().any(|c| c.is_lowercase()) {
            continue;
        }
        out.push(LevelledHeading {
            offset: m.start(),
            label: titlecase_boundary(raw),
            level: NodeLevel::Part,
        });
    }
    // Running-header banner guard: a genuine structural boundary (a Part /
    // Schedule / Form heading) appears ONCE. The same boundary label recurring
    // many times is a page running-header (e.g. a gazette's "PART-IV", OCR'd
    // "PART-1V", printed atop every page) that slipped past chrome stripping —
    // minting one empty Part shell per page. Drop every label that recurs
    // `>= RUNNING_HEADER_MIN` times; the text stays in the body (only the false
    // container node is suppressed). A real one-off boundary is untouched.
    const RUNNING_HEADER_MIN: usize = 3;
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for h in &out {
        *counts.entry(h.label.clone()).or_default() += 1;
    }
    out.retain(|h| counts[&h.label] < RUNNING_HEADER_MIN);
    out
}

/// Normalize an ALL-CAPS boundary label to a readable citation: each word
/// Title-Cased, but pure roman-numeral / numeric segments kept upper
/// (`"SECOND SCHEDULE"` → `"Second Schedule"`, `"PART-I"` → `"Part-I"`).
fn titlecase_boundary(raw: &str) -> String {
    // RC-C: collapse "KEYWORD <ws/dash> <numeral>" → "KEYWORD-<numeral>" so the
    // spacing/dash variants of one heading (`SCHEDULE-I`, `SCHEDULE - III`,
    // `SCHEDULE– VI`) normalize to a single hyphenated label. A bare
    // `SECOND SCHEDULE` (no trailing numeral) is left untouched.
    static SUFFIX: OnceLock<Regex> = OnceLock::new();
    let suffix = SUFFIX.get_or_init(|| {
        Regex::new(r"(?P<kw>SCHEDULE|ANNEXURE|APPENDIX|PART)[ \t]*[-–—]?[ \t]*(?P<n>[IVXLC0-9]+)").unwrap()
    });
    let raw = suffix.replace(raw, "$kw-$n");
    let raw = raw.as_ref();
    let seg = |s: &str| -> String {
        if !s.is_empty() && s.chars().all(|c| "IVXLC0123456789".contains(c)) {
            s.to_string()
        } else {
            let mut cs = s.chars();
            match cs.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase(),
                None => String::new(),
            }
        }
    };
    // Split each word on ANY dash (ASCII hyphen, en-dash, em-dash) and re-join
    // with a canonical ASCII hyphen, so `PART-I` and `PART–I` (en-dash, common
    // in this corpus) normalize to the SAME label `Part-I` instead of diverging
    // into `Part-I` vs `Part–i`.
    raw.split(' ')
        .map(|word| {
            word.split(|c| c == '-' || c == '\u{2013}' || c == '\u{2014}')
                .map(seg)
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse the leading provision number from a citation label (`"Rule 149"` →
/// `149`, `"Rule 25A"` → `25`, `"Section 9.11"` → `9`). Returns `None` when the
/// label carries no leading number (e.g. `"Part III"`, `"(2)"`).
fn leading_provision_number(label: &str) -> Option<u32> {
    let digits: String = label
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Generic footnote/form discriminator (Heading-mislabel defect, 2026-06-16).
///
/// The general bare-number heading pattern is high-recall: it also matches
/// schedule form-fields (`1. Name`, `2. Address`) and amendment footnotes
/// (`3. Subs. Ibid.`) that are NOT provision headings. The structural property
/// that separates them — with no vocabulary list — is that real provision
/// headings form an *ascending* numeric run within their container, while
/// form/footnote clusters *restart* at a low number deep inside a high run.
///
/// We drop an Article-level bare-number heading only when its number jumps
/// **far backwards** (to at most half of the highest number accepted since the
/// last structural reset). The half-of-max ratio tolerates ordinary
/// subsection/amendment jitter (e.g. `9.11` after `9.`) while catching genuine
/// restarts (`1`/`2` amid rules `149`/`150`). A Part/Chapter heading (level
/// shallower than Article) resets the run so legitimately renumbered sub-rule
/// sets are preserved.
/// Below this many Articles the drop-share signal is noise (a 3-provision form
/// can legitimately lose 2), so the guard does not engage.
const MIN_ARTICLES_FOR_SEQUENCE_GUARD: usize = 12;
/// Above this share of Articles discarded, the sequence rule is judged wrong
/// about the document rather than right about the headings.
const MAX_DROP_PCT: usize = 30;

fn drop_out_of_sequence_articles(hits: &mut Vec<LevelledHeading>) {
    let mut max_num: u32 = 0;
    let drop: Vec<bool> = hits
        .iter()
        .map(|h| {
            if (h.level as usize) < (NodeLevel::Article as usize) {
                max_num = 0; // new container — reset the expected run
                return false;
            }
            if h.level != NodeLevel::Article {
                return false; // Clause / SubClause markers are not sequenced
            }
            match leading_provision_number(&h.label) {
                Some(n) if n.saturating_mul(2) < max_num => true, // far-backward restart → drop
                // Far-FORWARD jump: a number that leaps hundreds above the
                // established run is not the next provision — it is a footnote /
                // reference number (the constitution numbers footnotes into the
                // thousands; "Article 2454"/"Article 773" amid Articles ≤280).
                // Guarded so a normal ascent (281 after 280) and small gaps never
                // trip it: requires an established run AND a >2× AND >300 jump.
                Some(n)
                    if max_num >= 20
                        && n > max_num.saturating_mul(2)
                        && n - max_num > 300 =>
                {
                    true
                }
                Some(n) => {
                    if n > max_num {
                        max_num = n;
                    }
                    false
                }
                None => false,
            }
        })
        .collect();
    // ── poison guard (2026-07-27, trace a72f1539) ───────────────────────────
    // The backward-restart rule assumes `max_num` was established by REAL
    // provisions. When a contents page leaks past front-matter stripping, every
    // listing is minted as an Article first, so `max_num` is already past 280
    // before the body starts — and then this rule deletes the genuine articles
    // for being "out of sequence" (Article 63 → 126 < 280 → dropped). One bad
    // input destroyed ~100 labels with no error anywhere.
    //
    // A rule that silently deletes law must fail safe. If it wants to discard a
    // large share of the Articles it was given, it is not disambiguating noise —
    // it is wrong about this document. Keep everything and say so.
    let article_total = hits
        .iter()
        .filter(|h| h.level == NodeLevel::Article)
        .count();
    let article_dropped = hits
        .iter()
        .zip(drop.iter())
        .filter(|(h, d)| **d && h.level == NodeLevel::Article)
        .count();
    if article_total >= MIN_ARTICLES_FOR_SEQUENCE_GUARD
        && article_dropped * 100 > article_total * MAX_DROP_PCT
    {
        log::warn!(
            "heading sequence guard disabled: would drop {}/{} Articles ({}%) — \
             the numbering run looks poisoned (contents page leaked into the body?). \
             Keeping all headings; labels may be duplicated but none are lost.",
            article_dropped,
            article_total,
            article_dropped * 100 / article_total.max(1),
        );
        return;
    }

    let mut idx = 0;
    hits.retain(|_| {
        let keep = !drop[idx];
        idx += 1;
        keep
    });
}

// ── Footnote false labels: the TRAILING-OUTLIER rule (2026-07-29) ────────────
//
// `drop_out_of_sequence_articles` is a STREAMING rule — it carries a running
// max and can only judge a number against what it has already seen. That leaves
// a whole family of false labels untouched, because a footnote number sitting
// near the END of a document is never far enough ahead of the run at that point
// to trip the far-forward test:
//
//   * Constitution `Article 343` (footnote 343, pp.168-170) — by p.168 the run
//     is already ~230, so 343 is neither >2× the max nor >300 above it. It then
//     swallowed ~14 chunks of genuine Article-232 text under a label that does
//     not exist in the statute.
//   * Civil Servants Act `Section 1752` (footnote 1752, p.8) — the far-forward
//     test needs `max_num >= 20`, but on p.8 the run has only reached ~9.
//     (It is below the 1800 floor, so `looks_like_year` cannot see it either.)
//   * Dev Projects Manual `Section 156` (p.109) — actually *Article 156 of the
//     Constitution* quoted inside the Manual; the jump is 133, under the 300 cut.
//
// The signal those three share is only visible with the WHOLE document in hand:
// real provisions form one dense run, and a footnote number is an isolated
// island above its top. So this is a second, GLOBAL pass — it says nothing
// about numbers inside the run, only about the tail.
//
// Deliberately conservative, per the poison-guard lesson above (a rule that
// silently deletes law must fail safe):
//   * needs a real run to reason about (`MIN_DISTINCT_FOR_TAIL_RULE`);
//   * only ever inspects the TOP of the run, peeling one number at a time;
//   * the gap must beat BOTH an absolute floor and a fifth of the span below,
//     so a long statute is not judged by the same yardstick as a short one;
//   * caps total removals — if it wants to peel much of the tail it is wrong
//     about the document, not right about the headings.
//
// Verified against all 19 prod documents (2026-07-29): fires on exactly the
// three false labels above and on nothing else. Every legitimate document in
// the corpus has a top-of-run gap of exactly 1.

/// Below this many distinct provision numbers there is no run to reason about
/// (a 12-provision rules annex can legitimately end at 40), so the rule is off.
const MIN_DISTINCT_FOR_TAIL_RULE: usize = 20;
/// A tail gap must clear this many ordinals regardless of document length —
/// stops a short document's tiny span from making an ordinary gap look damning.
const MIN_TAIL_GAP: u32 = 25;
/// …and this fraction (1/N) of the span below it, so a 280-Article statute is
/// not judged by the same absolute yardstick as a 27-section act.
const TAIL_GAP_SPAN_DIVISOR: u32 = 5;
/// Never peel more than this share of the distinct numbers.
const MAX_TAIL_DROP_PCT: usize = 10;

/// Drop Article-level labels whose number is an isolated island ABOVE the
/// document's provision run — footnote/reference numbers mis-minted as
/// provisions. See the block comment above for why the streaming rule cannot.
fn drop_footnote_tail_outliers(hits: &mut Vec<LevelledHeading>) {
    let mut nums: Vec<u32> = hits
        .iter()
        .filter(|h| h.level == NodeLevel::Article)
        .filter_map(|h| leading_provision_number(&h.label))
        .collect();
    nums.sort_unstable();
    nums.dedup();
    if nums.len() < MIN_DISTINCT_FOR_TAIL_RULE {
        return;
    }

    let max_peel = (nums.len() * MAX_TAIL_DROP_PCT / 100).max(1);
    let mut doomed: Vec<u32> = Vec::new();
    while nums.len() >= 2 && doomed.len() < max_peel {
        let top = nums[nums.len() - 1];
        let prev = nums[nums.len() - 2];
        let span_below = prev.saturating_sub(nums[0]);
        let threshold = MIN_TAIL_GAP.max(span_below / TAIL_GAP_SPAN_DIVISOR);
        if top - prev <= threshold {
            break; // the top belongs to the run — nothing above it to peel
        }
        doomed.push(top);
        nums.pop();
    }
    if doomed.is_empty() {
        return;
    }

    log::info!(
        "footnote tail guard: dropping {} out-of-run provision label(s) {:?} \
         (run tops out at {})",
        doomed.len(),
        doomed,
        nums.last().copied().unwrap_or(0),
    );
    hits.retain(|h| {
        h.level != NodeLevel::Article
            || leading_provision_number(&h.label)
                .is_none_or(|n| !doomed.contains(&n))
    });
}

/// A bare 4-digit number in the calendar range 1800–2099 (no trailing letter):
/// a statute year, not a provision ordinal (FR-013a). Mirrors the runtime guard
/// in `intent_router::decompose` so index-time labels are guarded too.
fn looks_like_year(num: &str) -> bool {
    num.len() == 4
        && num.bytes().all(|b| b.is_ascii_digit())
        && matches!(num.parse::<u32>(), Ok(y) if (1800..=2099).contains(&y))
}

/// Re-mint a provision label under a resolved citation unit (Phase 5 / D1).
/// Replaces only the leading unit word when it equals the profile default
/// (`"Section 9"` → `"Article 9"`); a label that doesn't start with the default
/// word, or a no-op swap, is returned unchanged.
fn swap_unit_word(label: &str, from: &str, to: &str) -> String {
    if from.eq_ignore_ascii_case(to) {
        return label.to_string();
    }
    let prefix = format!("{from} ");
    match label.strip_prefix(&prefix) {
        Some(rest) => format!("{to} {rest}"),
        None => label.to_string(),
    }
}

fn format_citation(template: &str, cap: &regex_lite::Captures<'_>) -> String {
    let mut out = template.to_string();
    for name in ["num", "title", "sub", "clause"] {
        if let Some(g) = cap.name(name) {
            out = out.replace(&format!("{{{name}}}"), g.as_str().trim());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Serial-column body tables (2026-07-29) ───────────────────────────────

    /// APT Rules 1974, reproduced at the shape that caused the collision: a
    /// serial-column table introduced INSIDE Part I by Rule 4, whose `1. 2. 3.`
    /// column minted Rules 1/2/3 on top of the document's real Rules 1/2/3.
    /// The genuine provisions in the same Part must survive — suppressing the
    /// whole span (which is what the container guard would do) would delete them.
    #[test]
    fn serial_column_table_rows_are_dropped_but_real_rules_survive() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "\
PART - I GENERAL

1.- These rules may be called the Sind Civil Servants Appointment Rules 1974.

2.- In these rules, unless there is anything repugnant in the subject, the
following expressions shall have the meanings assigned to them.

3.- (1) Appointments to posts shall be made by any of the following methods.

4.- (1) The authorities competent to make appointments shall be as follows:

S. No. Posts Appointing Authority
1. Posts sanctioned in Basic Pay Scales 1 to 2 Head of Office
2. Posts sanctioned in Basic Pay Scales 3 to 5 Head of Department
3. Posts sanctioned in Basic Pay Scales 6 to 8 Appointing Authority
4. Posts sanctioned in Basic Pay Scales 9 to 11 Secretary of Department
5. Posts sanctioned in Basic Pay Scales 12 to 15 Minister Incharge

5.- Where an appointing authority is absent the next senior officer shall act.
";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        for want in ["Rule 1", "Rule 2", "Rule 3", "Rule 4", "Rule 5"] {
            assert!(labels.contains(&want), "real provision {want} dropped: {labels:?}");
        }
        // …and each appears exactly ONCE — the table twin is gone. This is the
        // property that makes "search for Rule 3" answerable at all.
        for want in ["Rule 1", "Rule 2", "Rule 3"] {
            assert_eq!(
                labels.iter().filter(|l| **l == want).count(),
                1,
                "{want} is still ambiguous: {labels:?}"
            );
        }
    }

    #[test]
    fn serial_table_range_needs_a_header_and_a_real_run() {
        // No header ⇒ no range, however many numbered lines follow.
        assert!(serial_table_ranges("1. Alpha\n2. Beta\n3. Gamma\n4. Delta\n").is_empty());
        // Header but a run too short to be a table.
        assert!(serial_table_ranges("S. No. Posts Authority\n1. Alpha\n2. Beta\n").is_empty());
        // Header plus a genuine run.
        assert_eq!(
            serial_table_ranges("S. No. Posts Authority\n1. Alpha\n2. Beta\n3. Gamma\n").len(),
            1
        );
        // A broken run stops the table: 1,2,3 then an unrelated number, which
        // must fall OUTSIDE the range (otherwise a real provision sitting after
        // a table would be suppressed with it).
        let text = "S. No. Posts Authority\n1. A\n2. B\n3. C\n97. Unrelated\n";
        let r = serial_table_ranges(text);
        assert_eq!(r.len(), 1);
        let unrelated_at = text.find("97.").expect("fixture");
        assert!(
            r[0].1 <= unrelated_at,
            "range {:?} swallowed text past the ascending run (at {unrelated_at})",
            r[0]
        );
    }

    /// The catchline-dash provision form must never be read as a row serial —
    /// `4.- (1) …` opens a rule, `4. Posts sanctioned …` is a table row.
    #[test]
    fn a_dashed_provision_is_not_a_row_serial() {
        assert_eq!(line_initial_serial("4. Posts sanctioned in Basic Pay Scales"), Some(4));
        assert_eq!(line_initial_serial("4.- (1) The authorities competent to make"), None);
        assert_eq!(line_initial_serial("10.- Initial appointments to posts in Basic"), None);
    }

    /// Fail-safe: if the "table" would swallow most of the document's
    /// provisions, the guard has misread the document and must stand down.
    #[test]
    fn serial_table_guard_stands_down_rather_than_eat_the_document() {
        let mut hits: Vec<LevelledHeading> = (1..=30)
            .map(|n| LevelledHeading {
                offset: 30 + n as usize, // all inside the "table" range below
                label: format!("Rule {n}"),
                level: NodeLevel::Article,
            })
            .collect();
        let before = hits.len();
        let text = "S. No. Posts Authority\n1. A\n2. B\n3. C\n4. D\n5. E\n6. F\n7. G\n8. H\n";
        suppress_serial_column_tables(text, &mut hits);
        assert_eq!(hits.len(), before, "guard must keep everything when it would over-delete");
    }

    // ── Footnote tail-outlier rule (2026-07-29) ──────────────────────────────

    /// Build Article-level hits for a run of provision numbers, in order.
    fn article_hits(unit: &str, nums: &[u32]) -> Vec<LevelledHeading> {
        nums.iter()
            .enumerate()
            .map(|(i, n)| LevelledHeading {
                offset: i * 100,
                label: format!("{unit} {n}"),
                level: NodeLevel::Article,
            })
            .collect()
    }

    fn surviving_nums(hits: &[LevelledHeading]) -> Vec<u32> {
        hits.iter()
            .filter_map(|h| leading_provision_number(&h.label))
            .collect()
    }

    /// The Constitution's footnote 343 (pp.168-170). The streaming rule cannot
    /// catch it: by then the run is ~230, so 343 is neither >2× nor >300 above.
    #[test]
    fn footnote_tail_drops_constitution_article_343() {
        let mut nums: Vec<u32> = (1..=280).collect();
        nums.push(343);
        let mut hits = article_hits("Article", &nums);
        drop_footnote_tail_outliers(&mut hits);
        let kept = surviving_nums(&hits);
        assert_eq!(kept.len(), 280, "only the footnote label should go");
        assert_eq!(kept.last(), Some(&280));
        assert!(!kept.contains(&343));
    }

    /// `Section 1752` (Civil Servants Act, footnote 1752 on p.8). Below the
    /// 1800 floor so `looks_like_year` is blind to it, and the run at that point
    /// is under the far-forward rule's `max_num >= 20` precondition.
    #[test]
    fn footnote_tail_drops_section_1752_below_the_year_floor() {
        assert!(!looks_like_year("1752"), "precondition: the year guard cannot see it");
        let mut nums: Vec<u32> = (1..=26).collect();
        nums.push(1752);
        let mut hits = article_hits("Section", &nums);
        drop_footnote_tail_outliers(&mut hits);
        assert_eq!(surviving_nums(&hits), (1..=26).collect::<Vec<_>>());
    }

    /// `Section 156` in the Dev Projects Manual — Article 156 of the
    /// Constitution quoted inside it. Jump of 126, under the 300 far-forward cut.
    /// `Section 30` (gap 7) is ordinary run jitter and must survive.
    #[test]
    fn footnote_tail_drops_quoted_provision_but_keeps_run_jitter() {
        let mut nums: Vec<u32> = (1..=23).collect();
        nums.push(30);
        nums.push(156);
        let mut hits = article_hits("Section", &nums);
        drop_footnote_tail_outliers(&mut hits);
        let kept = surviving_nums(&hits);
        assert!(kept.contains(&30), "a 7-ordinal gap is jitter, not a footnote");
        assert!(!kept.contains(&156));
    }

    /// Every legitimate document in the prod corpus (2026-07-29) has a
    /// top-of-run gap of exactly 1. None of them may lose a label.
    #[test]
    fn footnote_tail_leaves_legitimate_documents_alone() {
        for (name, top) in [
            ("Sindh Local Government Act 2013", 161u32),
            ("Govt. of Sindh Manual of Secretariat instruction 2010", 187),
            ("Compendium Rules under SLGA 2013", 84),
            ("Sindh Rules of Business 1986", 60),
            ("Sindh Delegation of Financial Powers 2019", 37),
            ("Sindh Public Procurement Act 2009", 27),
            ("SPPRA Policy Letter 16 Oct 2023", 21),
        ] {
            let nums: Vec<u32> = (1..=top).collect();
            let mut hits = article_hits("Section", &nums);
            drop_footnote_tail_outliers(&mut hits);
            assert_eq!(
                surviving_nums(&hits).len(),
                top as usize,
                "{name} must keep every provision",
            );
        }
    }

    /// SPPRA Rules 2010: real internal gaps (56, 59-60, 69, 86 absent) with a
    /// contiguous top. Internal holes are none of this rule's business.
    #[test]
    fn footnote_tail_ignores_internal_gaps() {
        let nums: Vec<u32> = (1..=91).filter(|n| ![56, 59, 60, 69, 86].contains(n)).collect();
        let want = nums.len();
        let mut hits = article_hits("Rule", &nums);
        drop_footnote_tail_outliers(&mut hits);
        assert_eq!(surviving_nums(&hits).len(), want);
    }

    /// Fail-safe: a short document has no run to reason about, so the rule
    /// stays out of it entirely — even when the tail gap looks enormous.
    #[test]
    fn footnote_tail_is_off_for_short_documents() {
        let mut nums: Vec<u32> = (1..=12).collect();
        nums.push(900);
        let mut hits = article_hits("Rule", &nums);
        drop_footnote_tail_outliers(&mut hits);
        assert!(
            surviving_nums(&hits).contains(&900),
            "under {MIN_DISTINCT_FOR_TAIL_RULE} distinct numbers the rule must not fire",
        );
    }

    /// Fail-safe: it must never peel a large share of the tail. A document
    /// numbered in wide steps is a document this rule misunderstands.
    #[test]
    fn footnote_tail_never_eats_the_document() {
        let nums: Vec<u32> = (1..=30).map(|n| n * 100).collect(); // 100, 200, … 3000
        let mut hits = article_hits("Section", &nums);
        drop_footnote_tail_outliers(&mut hits);
        let kept = surviving_nums(&hits).len();
        assert!(
            kept >= nums.len() - nums.len() * MAX_TAIL_DROP_PCT / 100,
            "peeled {} of {} — the cap did not hold",
            nums.len() - kept,
            nums.len(),
        );
    }

    /// Non-Article levels are not sequenced and must pass through untouched.
    #[test]
    fn footnote_tail_leaves_structural_headings_alone() {
        let mut hits = article_hits("Article", &(1..=280).collect::<Vec<_>>());
        hits.push(LevelledHeading {
            offset: 99_999,
            label: "Part XII".into(),
            level: NodeLevel::Part,
        });
        hits.push(LevelledHeading {
            offset: 100_000,
            label: "Article 343".into(),
            level: NodeLevel::Article,
        });
        drop_footnote_tail_outliers(&mut hits);
        assert!(hits.iter().any(|h| h.label == "Part XII"));
        assert!(!hits.iter().any(|h| h.label == "Article 343"));
    }

    /// Nesting audit 2026-07-02 — the SPPRA Rule 2(1) double-reading: `(i)`
    /// after `(h)` is the ninth alpha clause; `(i)…(iv)` under `(l)` is a
    /// nested roman list. Both in one document, resolved by sequence.
    #[test]
    fn ambiguous_markers_resolve_by_sequence() {
        let mk = |label: &str, level: NodeLevel, offset: usize| LevelledHeading {
            offset,
            label: label.to_string(),
            level,
        };
        // Offset-dedup keeps the deeper Item reading for (i)/(v)/(x)/(ii);
        // the resolver then decides. Simulate that post-dedup state.
        let mut hits = vec![
            mk("Rule 2", NodeLevel::Article, 0),
            mk("(1)", NodeLevel::Clause, 10),
            mk("(h)", NodeLevel::SubClause, 20),
            mk("(i)", NodeLevel::Item, 30), // alpha: continues (h)
            mk("(j)", NodeLevel::SubClause, 40),
            mk("(l)", NodeLevel::SubClause, 50),
            mk("(i)", NodeLevel::Item, 60),   // roman: opens a list under (l)
            mk("(ii)", NodeLevel::Item, 70),  // roman continuation
            mk("(iii)", NodeLevel::Item, 80),
            mk("(iv)", NodeLevel::Item, 90),
            mk("(v)", NodeLevel::Item, 100), // roman: continues (iv)
            mk("(m)", NodeLevel::SubClause, 110),
            mk("(z)", NodeLevel::SubClause, 120),
            mk("(aa)", NodeLevel::SubClause, 130), // doubled alpha run
            mk("(hh)", NodeLevel::SubClause, 140),
            mk("(ii)", NodeLevel::Item, 150), // alpha: continues (hh)
        ];
        resolve_ambiguous_sub_markers(&mut hits);
        let levels: Vec<(&str, NodeLevel)> =
            hits.iter().map(|h| (h.label.as_str(), h.level)).collect();
        assert_eq!(levels[3], ("(i)", NodeLevel::SubClause), "ninth letter after (h)");
        assert_eq!(levels[6], ("(i)", NodeLevel::Item), "roman list opener under (l)");
        assert_eq!(levels[7], ("(ii)", NodeLevel::Item));
        assert_eq!(levels[10], ("(v)", NodeLevel::Item), "continues (iv), not a letter");
        assert_eq!(levels[15], ("(ii)", NodeLevel::SubClause), "doubled run after (hh)");
    }

    #[test]
    fn stray_v_and_x_keep_the_alpha_reading() {
        let mut hits = vec![
            LevelledHeading { offset: 0, label: "Rule 7".into(), level: NodeLevel::Article },
            LevelledHeading { offset: 10, label: "(u)".into(), level: NodeLevel::SubClause },
            LevelledHeading { offset: 20, label: "(v)".into(), level: NodeLevel::Item },
        ];
        resolve_ambiguous_sub_markers(&mut hits);
        assert_eq!(hits[2].level, NodeLevel::SubClause, "(v) after (u) is a letter");
    }

    #[test]
    fn alpha_and_roman_successors() {
        assert_eq!(alpha_succ("h"), "i");
        assert_eq!(alpha_succ("z"), "aa");
        assert_eq!(alpha_succ("hh"), "ii");
        assert_eq!(alpha_succ("iv"), ""); // not an alpha shape
        assert_eq!(roman_succ("i"), Some("ii"));
        assert_eq!(roman_succ("iv"), Some("v"));
        assert_eq!(roman_succ("xv"), None);
    }

    #[test]
    fn detects_legal_section_headers_high_confidence() {
        let text = "Some preamble.\n\nSection 8. Recruitment. Text...\n\nRule 12(2) provides...";
        let hits = scan(text);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Section 8"));
        assert!(labels.contains(&"Rule 12(2)"));
        assert!(hits.iter().all(|h| h.confidence >= 0.85));
    }

    #[test]
    fn detects_numbered_outline_low_confidence() {
        let text = "1. Introduction\n\nBody text...\n\n2.1 Methodology\n\nMore body.";
        let hits = scan(text);
        assert!(hits.iter().any(|h| h.label == "1. Introduction" && h.confidence < 0.85));
        assert!(hits.iter().any(|h| h.label == "2.1 Methodology" && h.confidence < 0.85));
    }

    #[test]
    fn ignores_inline_section_mentions() {
        let text = "The applicant must comply with see section 8 of the Act.";
        let hits = scan(text);
        assert!(hits.is_empty(), "got: {hits:?}");
    }

    #[test]
    fn detects_pakistani_notification_header() {
        let text = "SGA&CD Notification dated 27th February 1984\n\nThe minimum length of service shall be...";
        let hits = scan(text);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].label, "SGA&CD Notification dated 27th February 1984");
        assert!(hits[0].confidence >= 0.85);
    }

    /// RC-C (audit 2026-06-29): a `SCHEDULE-<num>` heading with a roman/numeric
    /// suffix on the line (with or without surrounding dash/space) is a boundary,
    /// normalized to one hyphenated label; the bare `SECOND SCHEDULE` word form
    /// still matches.
    #[test]
    fn detects_schedule_with_numeric_suffix_boundary() {
        let b = scan_structural_boundaries(
            "Rule 5. X.\nbody\n\nSCHEDULE-I\nrows\n\nSCHEDULE - III\nrows\n\nSCHEDULE\u{2013} VI\nrows",
        );
        let labels: Vec<&str> = b.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Schedule-I"), "got {labels:?}");
        assert!(labels.contains(&"Schedule-III"), "spacing/dash not normalized: {labels:?}");
        assert!(labels.contains(&"Schedule-VI"), "en-dash not normalized: {labels:?}");
        assert!(b.iter().all(|h| h.level == NodeLevel::Part));
    }

    /// RC-C: the distribution-of-business schedule restarts numbering per
    /// department, re-minting `Rule 30/31/32`. Inside a SCHEDULE container whose
    /// serials reset, those Article-level serials must be SUPPRESSED — leaving the
    /// one genuine main-body Rule 30 — while the main body (a single ascending
    /// run) is untouched.
    #[test]
    fn schedule_serial_restart_suppressed_main_body_kept() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "\
30. The Planning and Development Board shall coordinate development.\n\n\
SCHEDULE-I\n\
3 - FINANCE DEPARTMENT\n\
1. Budget.\n\
2. Audit.\n\
30. Waste lands.\n\
31. Service matters.\n\
4 - CHIEF MINISTER'S SECRETARIAT\n\
1. Administrative matters.\n\
2. Service matters.\n\
30. Other duties.\n\
31. Residual matters.\n";
        let hits = scan_with_profile_unit(text, &RULES, None);
        let rule30: Vec<&LevelledHeading> =
            hits.iter().filter(|h| h.label == "Rule 30").collect();
        assert_eq!(
            rule30.len(), 1,
            "exactly one genuine Rule 30 must survive (schedule serials suppressed); got {:?}",
            hits.iter().map(|h| &h.label).collect::<Vec<_>>()
        );
        // The surviving Rule 30 is the main-body one (before the schedule).
        let sched_off = text.find("SCHEDULE-I").unwrap();
        assert!(rule30[0].offset < sched_off, "surviving Rule 30 is not the main-body one");
        // The schedule boundary itself is kept.
        assert!(hits.iter().any(|h| h.label == "Schedule-I" && h.level == NodeLevel::Part));
    }

    /// US6 (T038): an ALL-CAPS standalone `SECOND SCHEDULE` / `PART-I` line is a
    /// structural boundary (Part level); a mid-sentence mixed-case reference is
    /// NOT — so schedule content stops inheriting the preceding Rule's label.
    #[test]
    fn detects_schedule_boundaries_not_inline_references() {
        let boundaries = scan_structural_boundaries(
            "Rule 27. Police.\nbody\n\nSECOND SCHEDULE\n(See Rule-3)\nPART-I\nPOWERS COMMON\n1. Creation of Post",
        );
        let labels: Vec<&str> = boundaries.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Second Schedule"), "got {labels:?}");
        assert!(labels.contains(&"Part-I"), "got {labels:?}");
        assert!(boundaries.iter().all(|h| h.level == NodeLevel::Part));
        // mid-sentence mixed-case reference must NOT be a boundary
        let refs = scan_structural_boundaries(
            "The powers in column 2 of the Second Schedule shall be exercised by the authority.",
        );
        assert!(refs.is_empty(), "inline reference wrongly treated as boundary: {refs:?}");
    }

    /// US6 fix: a `SECOND SCHEDULE` heading at the TOP of a page sits right after
    /// a formfeed (`\x0C`), not a newline. The `(?m)^` anchor matches after `\n`
    /// but not after `\x0C`, so the boundary was missed and the schedule's rows
    /// inherited the preceding Rule's label. Formfeed must be normalized so the
    /// boundary fires — and its offset must still point into the ORIGINAL text.
    #[test]
    fn detects_schedule_boundary_after_formfeed() {
        let text = "Rule 27. Police.\nbody text\x0CSECOND SCHEDULE\n(See Rule-3)\n1. Creation of Post";
        let boundaries = scan_structural_boundaries(text);
        let labels: Vec<&str> = boundaries.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Second Schedule"), "got {labels:?}");
        // offset must index back into the original (un-normalized) text
        let b = boundaries.iter().find(|h| h.label == "Second Schedule").unwrap();
        assert!(text[b.offset..].starts_with("SECOND SCHEDULE"), "offset misaligned");
    }

    /// US6 fix: `PART-I` (hyphen) and `PART–I` (en-dash — common in this corpus)
    /// must normalize to the SAME label, not diverge into `Part-I` vs `Part–i`.
    #[test]
    fn part_boundary_endash_normalizes_to_hyphen() {
        assert_eq!(titlecase_boundary("PART-I"), "Part-I");
        assert_eq!(titlecase_boundary("PART\u{2013}I"), "Part-I"); // en-dash
        assert_eq!(titlecase_boundary("PART\u{2014}II"), "Part-II"); // em-dash
        assert_eq!(titlecase_boundary("SECOND SCHEDULE"), "Second Schedule");
    }

    /// Running-header banner guard: a gazette's "PART-IV" (OCR'd "PART-1V")
    /// printed atop every page recurs many times → it is dropped (no false Part
    /// shells), while genuine one-off boundaries ("PART-I", "SECOND SCHEDULE")
    /// are kept.
    #[test]
    fn recurring_part_banner_is_dropped_one_off_boundaries_kept() {
        let mut text = String::new();
        for _ in 0..7 {
            text.push_str("PART-1V\nsome page body line\n\x0C");
        }
        text.push_str("PART-I\nPOWERS COMMON\n1. Creation of Post\n\nSECOND SCHEDULE\n(See Rule-3)\n");
        let labels: Vec<String> =
            scan_structural_boundaries(&text).into_iter().map(|h| h.label).collect();
        assert!(!labels.iter().any(|l| l == "Part-1V"), "banner not dropped: {labels:?}");
        assert!(labels.iter().any(|l| l == "Part-I"), "real Part-I lost: {labels:?}");
        assert!(labels.iter().any(|l| l == "Second Schedule"), "schedule lost: {labels:?}");
    }

    #[test]
    fn scan_with_profile_uses_rules_patterns() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "Rule 1. Short title.\n\nbody\n\nRule 2. Definitions.\n\nbody";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.iter().any(|l| l.starts_with("Rule 1")));
        assert!(labels.iter().any(|l| l.starts_with("Rule 2")));
    }

    /// item-1: an amendment-inserted rule whose OCR glued a footnote digit onto the
    /// bracket (`5[12. …`) must still be detected as `Rule 12` (its text used to be
    /// absorbed into Rule 11).
    #[test]
    fn amendment_footnote_digit_before_bracket_detected() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "Rule 11. Penalties. --- The following apply.\n\n\
                    5[12. Constitution of anti-corruption council. --- There shall be a council.";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.iter().any(|l| l.starts_with("Rule 12")), "got {labels:?}");
    }

    /// item-1: a gazette-footnote line (`Part IV-A, dated …, the Sindh Govt. Gaz.`)
    /// must NOT mint a spurious `Part IV` container that steals following clauses.
    #[test]
    fn gazette_footnote_part_is_suppressed() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "Rule 12. Council. --- There shall be a council.\n\
                    Part IV-A, dated 9-7-1985, the Sindh Govt. Gaz., Part IV-A.\n\
                    (2) The council shall inquire.";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(
            !labels.iter().any(|l| l.starts_with("Part IV") || l.starts_with("Part-IV")),
            "gazette footnote wrongly minted a Part container: {labels:?}"
        );
    }

    /// Phase 1 — Schedule-Aware Heading Suppression. Inside a `Part-II` container
    /// whose text carries a delegation-table signature (`To whom delegated`), the
    /// bare-number serials `3. Leases of land` / `17. WORKS AND SERVICES …` are
    /// table rows, not rules — they must NOT become `Rule 3` / `Rule 17`. The rows
    /// fall under the `Part-II` container instead (safe fallback).
    #[test]
    fn delegation_table_serials_not_promoted_to_rules() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "\
SECOND SCHEDULE
PART-II
SPECIAL POWERS TO CERTAIN DEPARTMENTS
Sr. #	Nature of Power	To whom delegated	Extent
(1)	(2)	(3)	(4)
3. Leases of land pertaining to Government buildings.
17. WORKS AND SERVICES DEPARTMENT";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(
            !labels.iter().any(|l| l == &"Rule 3" || l == &"Rule 17"),
            "schedule table serials wrongly promoted to Rules: {labels:?}"
        );
        // The container boundaries must still be present.
        assert!(labels.contains(&"Second Schedule"), "got {labels:?}");
        assert!(labels.contains(&"Part-II"), "got {labels:?}");
    }

    /// Suppression must be narrow: real rules in ordinary legislative text (no
    /// delegation-table signature) are still detected, even when bare-numbered.
    #[test]
    fn real_bare_number_rules_outside_table_still_detected() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "\
PART-I
PRELIMINARY
1. Short title and commencement.
2. Definitions.
3. Grounds for penalty.";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Rule 1"), "got {labels:?}");
        assert!(labels.contains(&"Rule 2"), "got {labels:?}");
        assert!(labels.contains(&"Rule 3"), "got {labels:?}");
    }

    /// Mixed document: Rules → Schedule(table) → must suppress ONLY the schedule
    /// table serials, and leave the main-body rules that precede the schedule.
    #[test]
    fn rules_then_delegation_schedule_suppresses_only_table() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "\
1. Short title.
2. Definitions.
SECOND SCHEDULE
PART-II
Sr. #	Nature of Power	To whom delegated	Extent
3. Leases of land.
4. Abolition of posts.";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        // main-body rules kept
        assert!(labels.contains(&"Rule 1"), "got {labels:?}");
        assert!(labels.contains(&"Rule 2"), "got {labels:?}");
        // table serials (after the schedule, in the delegation span) suppressed
        assert!(
            !labels.iter().any(|l| l == &"Rule 3" || l == &"Rule 4"),
            "table serials in schedule wrongly kept: {labels:?}"
        );
    }

    /// A schedule WITHOUT a delegation-table signature must NOT suppress its
    /// numbered headings — proves the discriminator is the table signature, not
    /// merely being inside a schedule.
    #[test]
    fn schedule_without_table_context_keeps_numbered_headings() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "\
FIRST SCHEDULE
PART-I
1. Form of application for registration.
2. Particulars to be furnished.";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Rule 1"), "got {labels:?}");
        assert!(labels.contains(&"Rule 2"), "got {labels:?}");
    }

    /// FR-013a: a bare-number heading whose number is a year must NOT become a
    /// Rule ordinal; genuine ordinals (e.g. 260) still parse.
    #[test]
    fn year_numbered_heading_is_not_minted_as_a_rule() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "\
260. Powers of the competent authority to sanction.

2017. The rules framed thereunder shall apply mutatis mutandis.";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Rule 260"), "real ordinal must parse: {labels:?}");
        assert!(
            !labels.iter().any(|l| l.contains("2017")),
            "a year must never be minted as a Rule: {labels:?}"
        );
    }

    /// Heading-mislabel defect (2026-06-16): SPPRA/SPSC/Delegation number their
    /// rules `N. Title - body` (single dash) and `N. Title.` — none of the four
    /// narrow `rules` patterns match these, so the headings were never detected
    /// and their text was swept under the preceding `(N)` sub-clause, mislabeled.
    #[test]
    fn rules_profile_detects_bare_number_title_headings() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "\
1. Short Title and Commencement - These rules may be called the Rules.

2. Definitions - In these rules, unless the context otherwise requires.

3. Scope and Applicability - Save as otherwise provided, these rules apply.

4. Powers of Higher authority.

5. Conflict with International Agreements - In the event of a conflict.";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        for n in ["Rule 1", "Rule 2", "Rule 3", "Rule 4", "Rule 5"] {
            assert!(labels.contains(&n), "missing {n} in {labels:?}");
        }
    }

    /// Generic footnote/form discriminator: real rule headings form an ASCENDING
    /// provision run; schedule form-fields restart low (`1. Name`, `2. Address`)
    /// amid high rule numbers. The sequence filter must drop the restart cluster
    /// so forms are not promoted into fake rule headings (Compendium regression).
    #[test]
    fn rules_profile_rejects_form_field_restart_among_high_rules() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "\
149. Detection of errors in merit list - If an error is found.

150. Basis for allocation of service - The allocation shall be made.

1. Name and No. of Union Council.

2. Address of the candidate.

151. Right of representation and appeal - A candidate may appeal.";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        for n in ["Rule 149", "Rule 150", "Rule 151"] {
            assert!(labels.contains(&n), "missing real heading {n} in {labels:?}");
        }
        assert!(!labels.contains(&"Rule 1"), "form field 1 promoted: {labels:?}");
        assert!(!labels.contains(&"Rule 2"), "form field 2 promoted: {labels:?}");
    }

    /// 013/D4 (T022): an Article whose body is plain prose repeats its number
    /// on both the catchline (`9. Security of person`) and the body (`9. No
    /// person shall…`). Both match the bare-number pattern; without collapse the
    /// two same-label nodes collide and the provision is dropped (Articles
    /// 9/33/34/35/39). They must collapse to ONE heading at the catchline offset.
    #[test]
    fn catchline_and_prose_body_collapse_to_one_article() {
        use crate::services::doc_profiles::manual::MANUAL;
        let text = "\
9.          Security of person

9.     No person shall be deprived of life or liberty save in accordance
with law.

10.         Safeguards as to arrest and detention

10.     (1)     No person who is arrested shall be detained in custody";
        let hits = scan_with_profile(text, &MANUAL);
        assert_eq!(
            hits.iter().filter(|h| h.label == "Section 9").count(),
            1,
            "catchline+body must collapse to one Section 9: {hits:?}"
        );
        assert!(hits.iter().any(|h| h.label == "Section 10"), "got {hits:?}");
        // the surviving Section 9 must anchor at the catchline (earliest offset)
        let nine = hits.iter().find(|h| h.label == "Section 9").unwrap();
        assert!(text[nine.offset..].starts_with("9.          Security"), "offset wrong: {hits:?}");
    }

    /// 013/D3 (T019/T021): alpha-suffixed ordinals (10A/19A/25A) and arabic
    /// chapter numbers (`CHAPTER 1.`) must be detected by the manual profile —
    /// a bare `\d+` drops the suffix and a roman-only chapter pattern misses the
    /// arabic form.
    #[test]
    fn manual_profile_captures_alpha_suffix_articles_and_arabic_chapters() {
        use crate::services::doc_profiles::manual::MANUAL;
        // CHAPTER lines are CENTERED (leading whitespace) as in the real doc.
        let text = "\
                        CHAPTER 1. – FUNDAMENTAL RIGHTS

10A.        Right to fair trial

19A.       Right to information.

25A.        Equality of citizens

                        CHAPTER 2. – PRINCIPLES OF POLICY";
        let hits = scan_with_profile(text, &MANUAL);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        for l in ["Section 10A", "Section 19A", "Section 25A", "Chapter 1", "Chapter 2"] {
            assert!(labels.contains(&l), "missing {l} in {labels:?}");
        }
    }

    /// Phase 5 / D1: with a resolved unit override, manual "Section N" provision
    /// labels are re-minted as "Article N", while structural Chapter labels are
    /// untouched.
    #[test]
    fn unit_override_remints_provision_labels_only() {
        use crate::services::doc_profiles::manual::MANUAL;
        let text = "\
CHAPTER 1. – FUNDAMENTAL RIGHTS

9.          Security of person

10A.        Right to fair trial";
        let hits = scan_with_profile_unit(text, &MANUAL, Some("Article"));
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Article 9"), "got {labels:?}");
        assert!(labels.contains(&"Article 10A"), "got {labels:?}");
        assert!(!labels.iter().any(|l| l.starts_with("Section")), "Section leaked: {labels:?}");
        // structural label is NOT re-minted to a provision unit
        assert!(labels.contains(&"Chapter 1"), "got {labels:?}");
    }

    /// Far-forward jump: a footnote/reference number that leaps hundreds above the
    /// established provision run (the constitution numbers footnotes into the
    /// Regression, trace a72f1539 (2026-07-27). When a contents page leaks into
    /// the body, `max_num` is pushed past 280 before the real provisions start —
    /// and the backward-restart rule then deletes every genuine provision below
    /// ~140 (Article 63 → 126 < 280). One hundred chunks ended up labelled
    /// `Article 154`, including the text of Article 63.
    ///
    /// The guard must notice it is about to discard most of the document and keep
    /// everything instead. Duplicate labels are recoverable; deleted law is not.
    #[test]
    fn poisoned_numbering_run_disables_the_sequence_guard() {
        use crate::services::doc_profiles::rules::RULES;
        // A contents listing (ascending, high) followed by the real body (low).
        let mut text = String::new();
        for n in [146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 280] {
            text.push_str(&format!("{n}. Contents catchline for provision {n}\n\n"));
        }
        for n in [54, 55, 56, 57, 58, 59, 60, 61, 62, 63] {
            text.push_str(&format!(
                "{n}. The competent authority shall pass such orders as it deems fit.\n\n"
            ));
        }
        let hits = scan_with_profile(text.as_str(), &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        for n in [54, 59, 62, 63] {
            assert!(
                labels.iter().any(|l| *l == format!("Rule {n}")),
                "real provision {n} was deleted by the sequence guard: {labels:?}"
            );
        }
    }

    /// The guard must still engage on a genuinely noisy tail — the poison check
    /// only fires when a LARGE share would be dropped.
    /// thousands) must NOT become a provision — `773`/`2454` amid Rules ≤280.
    #[test]
    fn footnote_number_far_forward_jump_is_dropped() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "\
278. Penultimate provision with real legislative text here.

279. Last provision before the footnote apparatus begins.

280. Final article of the body carrying substantive prose.

773. A stray footnote reference number that leapt forward.

2454. Another footnote reference number entirely out of range.";
        let hits = scan_with_profile(text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Rule 280"), "real provision dropped: {labels:?}");
        assert!(!labels.iter().any(|l| l.contains("773")), "773 footnote-number kept: {labels:?}");
        assert!(!labels.iter().any(|l| l.contains("2454")), "2454 kept: {labels:?}");
    }

    /// End-to-end through `scan_with_profile`: the Constitution's shape, where
    /// the far-forward rule provably cannot help. With the run at 280, footnote
    /// 343 is only 63 ahead and nowhere near 2× — so it survives every earlier
    /// filter and only the tail rule removes it. Guards the wiring, not just the
    /// helper: this is the defect that put ~14 chunks of Article-232 text under
    /// a label absent from the statute.
    #[test]
    fn constitution_shaped_footnote_survives_every_rule_but_the_tail_guard() {
        use crate::services::doc_profiles::rules::RULES;
        let mut text = String::new();
        for n in 1..=280 {
            text.push_str(&format!(
                "{n}. Provision {n} carrying substantive legislative prose here.\n\n"
            ));
        }
        text.push_str("343. The joint sitting passed the following resolution on 10-6-1998.\n");

        let hits = scan_with_profile(&text, &RULES);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Rule 280"), "real final provision dropped");
        assert!(labels.contains(&"Rule 1"), "real first provision dropped");
        assert!(
            !labels.iter().any(|l| l.contains("343")),
            "footnote 343 kept: {:?}",
            &labels[labels.len().saturating_sub(4)..],
        );
    }

    #[test]
    fn is_roster_line_direct() {
        for l in [
            "1.   The President of Pakistan                                   (Chairman)",
            "2.   Deputy Chairman (with ex-officio status of a Minister without Cabinet rank)",
            "5.   A representative from the East Pakistan                     (Member)",
            "6.   Minister for Finance and Economic Affairs                   Chairman ex-officio",
            "2.   Deputy Chairman Planning Commission                        Member",
        ] {
            assert!(is_roster_line(l), "roster row not detected: {l:?}");
        }
        for l in ["5.   Powers of the Chairman", "6.   Rights of a member to vote", "1.2  Despite grave problems"] {
            assert!(!is_roster_line(l), "false roster: {l:?}");
        }
    }

    /// 013/iter9: committee-roster rows (a numbered person/post + a parenthesised
    /// or right-aligned role, or "ex-officio") are NOT provisions and must be
    /// dropped; a real provision title that merely mentions a role is kept.
    #[test]
    fn committee_roster_rows_are_suppressed() {
        use crate::services::doc_profiles::manual::MANUAL;
        let text = "\
1.   The President of Pakistan                                   (Chairman)

2.   Deputy Chairman (with ex-officio status of a Minister without Cabinet rank)

5.   A representative from the East Pakistan                     (Member)

6.   Minister for Finance and Economic Affairs                   Chairman ex-officio";
        let hits = scan_with_profile(text, &MANUAL);
        assert!(
            !hits.iter().any(|h| h.level == NodeLevel::Article),
            "roster rows wrongly kept as provisions: {hits:?}"
        );
    }

    /// 2026-08-04 — circulation-list entries, verbatim from the live Manual for
    /// Development Projects 2019 extraction (page 126, under the Title-Case
    /// running header `Annexes`). These were minted as `Section 15…23`, which
    /// manufactured a phantom "Sections 24-29 missing" gap in the corpus gap
    /// check and made a typist's distribution list citable as a provision.
    #[test]
    fn is_circulation_line_direct() {
        for l in [
            "15. The Collectors/Deputy Commissioners (all) in Sindh.",
            "16. All Project Program Director /Coordinators under P&D.",
            "17. The Deputy Secretary (Staff) to Chief Secretary, Sindh, Karachi.",
            "19. The Publisher, Sindh Govt. Printing Press, Karachi.",
            "20. The Private Secretary to Minister P&D, Sindh.",
            "22. The Private Secretary to Chairman, P&D Board.",
        ] {
            assert!(is_circulation_line(l), "circulation entry not detected: {l:?}");
        }
        // A provision STATES something — the verb is what saves it. These must
        // all survive, and the first is a live Rules of Business rule.
        for l in [
            "13. The Secretary shall consider the matter and decide",
            "30. On the expiry of the date for which anticipatory approval has been granted",
            "1. RECEIPT AND DESPATCH",
            "5. The Chairman may delegate any of these powers",
            "7. All appointments are subject to the approval of the Board",
        ] {
            assert!(!is_circulation_line(l), "provision wrongly read as circulation: {l:?}");
        }
    }

    /// ⚠️ The 2026-08-05 regression, verbatim from `Constitution.pdf` via
    /// `pdftotext -layout`. Reading ONE line cost 13 Articles: each opens with
    /// an addressee word and does not reach its verb until a later line, with
    /// the marginal catchline parked on the right. Testing the whole ENTRY
    /// instead lets the verb be found. (The corpus was provably complete at
    /// 280/280 before this; it came back 267 with max 279.)
    #[test]
    fn a_wrapped_provision_reaches_its_verb() {
        let const_280 = "\
280. The Proclamation of Emergency issued on the twenty-third                Continuance of
day of November, one thousand nine hundred and seventy-one,                  Proclamation
                                                                             of Emergency.
shall be deemed to be a Proclamation of Emergency issued under
Article 232, and for the purposes of clause (7) and clause (8)
";
        assert!(
            !heading_line_is_circulation(const_280, 0),
            "Article 280 must survive — its verb is four lines down"
        );

        let const_61 = "\
61.     The provisions of clauses (2) to (7) of Article 53, clauses          Other
(2) and (3) of Article 54 and Articles 55, 63 to 67, 69, 71, 72 and          provisions
77 shall also apply to the Senate.
";
        assert!(!heading_line_is_circulation(const_61, 0), "Article 61 must survive");

        let const_266 = "\
266. The Interim Constitution of the Islamic Republic of                     Repeal.
Pakistan is hereby repealed.
";
        assert!(!heading_line_is_circulation(const_266, 0), "Article 266 must survive");

        // ...and the circulation list it was written for is still suppressed:
        // each entry is self-contained, so widening the window changes nothing.
        let office_order = "\
19. The Publisher, Sindh Govt. Printing Press, Karachi.
20. The Private Secretary to Minister P&D, Sindh.
22. The Private Secretary to Chairman, P&D Board.
";
        let twenty = office_order.find("20.").expect("entry present");
        assert!(
            heading_line_is_circulation(office_order, 0)
                && heading_line_is_circulation(office_order, twenty),
            "a circulation entry must still be detected when its neighbours are read"
        );
    }

    /// ⚠️ The 13th Article, the one the entry-window fix could NOT reach.
    /// Verbatim from `Constitution.pdf` via `pdftotext -layout`: Article 119
    /// opens with an addressee word, and its verb (`shall be regulated`) sits on
    /// the NEXT PAGE — a footnote rule overruns the window long before then. No
    /// window size saves it; the length cap does, because the entry is ~300
    /// collapsed chars where a real addressee entry is 68.
    #[test]
    fn a_provision_whose_verb_is_on_the_next_page_survives() {
        let const_119 = "\
119. The custody of the Provincial Consolidated Fund, the                    Custody, etc.,
payment of moneys into that Fund, the withdrawal of moneys                   of Provincial
therefrom, the custody of other moneys received by or on behalf              Consolidated
of the Provincial Government, their payment into, and                        Fund and
                                                                             Public
–––––––––––––––––––––––––––––––––––––––––––––––––––––––––––                 Account.
";
        assert!(
            !heading_line_is_circulation(const_119, 0),
            "Article 119 must survive — its verb is on the next page"
        );
    }

    /// The length cap must not swallow the list it was written beside: every
    /// real circulation entry stays comfortably under the bound.
    #[test]
    fn real_addressee_entries_stay_under_the_length_cap() {
        for l in [
            "15. The Collectors/Deputy Commissioners (all) in Sindh.",
            "17. The Deputy Secretary (Staff) to Chief Secretary, Sindh, Karachi.",
        ] {
            assert!(
                collapse_whitespace(l).chars().count() <= 140,
                "addressee entry exceeds the cap ({} chars): {l:?}",
                collapse_whitespace(l).chars().count()
            );
        }
    }

    /// End to end: the circulation list is dropped, and a real provision sitting
    /// in the same document is kept. Asserting only the first half would pass
    /// for a rule that deletes everything.
    #[test]
    fn circulation_list_is_suppressed_but_provisions_survive() {
        use crate::services::doc_profiles::manual::MANUAL;
        let text = "\
30. On the expiry of the date for which anticipatory approval has been granted,
the case will have to be resubmitted to the competent authority.

Annexes

15. The Collectors/Deputy Commissioners (all) in Sindh.

16. All Project Program Director /Coordinators under P&D.

20. The Private Secretary to Minister P&D, Sindh.

22. The Private Secretary to Chairman, P&D Board.";
        let hits = scan_with_profile(text, &MANUAL);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Section 30"), "real provision dropped: {labels:?}");
        for gone in ["Section 15", "Section 16", "Section 20", "Section 22"] {
            assert!(!labels.contains(&gone), "circulation entry kept: {labels:?}");
        }
    }

    /// Guard: a real provision title that names a role inline is NOT a roster.
    #[test]
    fn provision_title_naming_a_role_is_not_a_roster() {
        use crate::services::doc_profiles::manual::MANUAL;
        let text = "\
5.   POWERS OF THE CHAIRMAN

body text body text body text

6.   RIGHTS OF A MEMBER TO VOTE";
        let hits = scan_with_profile(text, &MANUAL);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Section 5"), "real provision dropped: {labels:?}");
        assert!(labels.contains(&"Section 6"), "real provision dropped: {labels:?}");
    }

    #[test]
    fn scan_with_profile_uses_act_patterns_and_ignores_rules_lines() {
        use crate::services::doc_profiles::act::ACT;
        let text = "Section 3. Authority.\n\nRule 9. (this is not an act heading)\n\nSection 4. Procedure.";
        let hits = scan_with_profile(text, &ACT);
        assert!(hits.iter().any(|h| h.label.starts_with("Section 3")));
        assert!(hits.iter().any(|h| h.label.starts_with("Section 4")));
        assert!(!hits.iter().any(|h| h.label.starts_with("Rule")));
    }

    /// RC-A (chunking audit 2026-06-29): an amendment-INSERTED rule whose heading
    /// is wrapped in brackets — `[18. Response Time – ...]14` — must be detected as
    /// Rule 18. The leading `[` previously defeated the bare-number pattern, so
    /// SPPRA Rules 18 and 32 were absorbed verbatim into the preceding rule.
    #[test]
    fn amendment_bracket_inserted_rule_detected_rc_a() {
        use crate::services::doc_profiles::rules::RULES;
        let text = "17. Methods of Notification and Advertisement\n\nbody of rule seventeen.\n\n\
                    [18. Response Time – The procuring agency shall give due consideration to the scope.]14\n\n\
                    19. Exceptions – Under following circumstances deviation is permissible.";
        let labels: Vec<String> = scan_with_profile(text, &RULES).into_iter().map(|h| h.label).collect();
        assert!(labels.iter().any(|l| l == "Rule 18"), "bracketed Rule 18 missed: {labels:?}");
    }

    /// RC-A (chunking audit 2026-06-29): a bare-number provision whose body opens
    /// with a roman/alpha sub-clause marker on the same line — `3. (i) There shall
    /// be ...` — must be detected. The `[A-Z]`-after-`N.` patterns miss a `(`
    /// opener, so Sindh Rules of Business Rules 3-8/13/14 were absorbed into §2.
    #[test]
    fn bare_number_with_paren_sub_opener_detected_rc_a() {
        use crate::services::doc_profiles::manual::MANUAL;
        let text = "2. Definitions. In these rules the following apply throughout.\n\n\
                    3. (i) There shall be a Secretariat comprising the Departments specified.\n\n\
                    4. (i) Each Department shall consist of a Secretary and other officers.";
        let labels: Vec<String> = scan_with_profile(text, &MANUAL).into_iter().map(|h| h.label).collect();
        assert!(labels.iter().any(|l| l == "Section 3"), "roman-sub Section 3 missed: {labels:?}");
        assert!(labels.iter().any(|l| l == "Section 4"), "roman-sub Section 4 missed: {labels:?}");
    }
}
