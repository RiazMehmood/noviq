//! 013/Iteration-6 — front-matter zoning.
//!
//! A legal PDF opens with non-law sections — Table of Contents, Preface,
//! Foreword, Abbreviations/Acronyms, Drafting Committee, List of Annexes. Their
//! lines (dotted "Title …… page" TOC entries, numbered committee rosters) match
//! the bare-number heading pattern and were being promoted to DUPLICATE
//! provisions (constitution: 297 duplicated articles; doc-06: a committee roster
//! as `Article 1..46`).
//!
//! This pass detects the initial front-matter region and lifts it OUT of the
//! body BEFORE heading detection, emitting **one retrievable chunk per section**
//! (kept for summarisation / document comparison — never discarded). Two safety
//! rails: (1) the region must START near the document top (within the first 20%
//! of lines) so a mid-body "Appendix" or a stray dotted line is never mistaken
//! for front-matter; (2) the region only extends across small gaps, so the body
//! is never swallowed. The dominant, unambiguous signal is the dotted leader
//! (`……… page`) — real law prose never looks like that.

use std::sync::OnceLock;
use regex_lite::Regex;

use crate::services::parsers::chrome::page_for;

/// One front-matter section, kept retrievable (NOT chrome).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatterBlock {
    /// `toc` | `preface` | `foreword` | `abbreviations` | `committee` |
    /// `annex_list` | `glossary` | `front_matter`.
    pub kind: String,
    pub content: String,
    pub page: u32,
}

/// Result of lifting front-matter out of the body (mirrors `strip_chrome`).
pub struct FrontMatterResult {
    pub cleaned: String,
    pub blocks: Vec<FrontMatterBlock>,
    pub cleaned_page_offsets: Vec<(u32, usize)>,
}

/// A line carrying a dotted leader (`....`/`…`) — a Table-of-Contents entry or a
/// form fill-blank, never a real provision heading.
pub fn is_dotted_line(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    // A TOC/form leader is a RUN — 4+ dots or 2+ ellipsis chars. A lone "…" in
    // ordinary prose must not count.
    RE.get_or_init(|| Regex::new(r"\.{4,}|…{2,}").unwrap()).is_match(line)
}

/// Recognise a front-matter section heading; returns its canonical kind.
fn section_kind(line: &str) -> Option<&'static str> {
    static RES: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    let res = RES.get_or_init(|| {
        vec![
            ("toc", Regex::new(r"(?i)\btable of contents\b|^\s*contents\s*$").unwrap()),
            ("preface", Regex::new(r"(?i)^\s*preface\s*$").unwrap()),
            ("foreword", Regex::new(r"(?i)^\s*fore-?word\s*$").unwrap()),
            ("abbreviations", Regex::new(r"(?i)^\s*(abbreviations?|acronyms?)\b").unwrap()),
            ("committee", Regex::new(r"(?i)\bdrafting committee\b").unwrap()),
            ("annex_list", Regex::new(r"(?i)^\s*list of (annex|appendic)").unwrap()),
            ("glossary", Regex::new(r"(?i)^\s*glossary\s*$").unwrap()),
        ]
    });
    res.iter().find(|(_, re)| re.is_match(line)).map(|(k, _)| *k)
}

/// Max gap (in lines) the front-matter region may jump across before it is
/// considered ended (the body has begun).
const FM_GAP: usize = 50;
/// Contents pages are interleaved with chapter headings, page furniture and
/// blank leading, so consecutive TOC runs sit further apart than `FM_GAP`
/// allows. Measured on the Constitution amended to the 27th, the eight numbered
/// contents runs are separated by gaps of 19–83 lines; at `FM_GAP` only the
/// first would be lifted and the rest would go on minting Articles.
const TOC_CHAIN_GAP: usize = 150;
/// A numbered contents run needs this many entries before it is believed. Body
/// provisions do not stack this densely — each is followed by its own prose.
const TOC_MIN_ENTRIES: usize = 8;
/// Contents entries are titles, not sentences.
const TOC_MAX_LINE_LEN: usize = 90;
/// Short non-entry lines tolerated inside a run (chapter headings, page numbers).
const TOC_MAX_GAP: usize = 12;
/// Percent of a run's provision numbers that must reappear below it before the
/// run is believed to be a contents list. Real contents runs measure 94–100%
/// across the corpus; a run of genuine body provisions measures 0%.
const TOC_MIN_RECURRENCE: usize = 60;
/// The region must start within this fraction of the document (top 20%).
const FM_GATE_FRAC: f32 = 0.20;
/// Hard cap (lines) on a single named front-matter section without a boundary.
const FM_SECTION_CAP: usize = 60;

/// Lift the initial front-matter region out of `text`, returning the cleaned
/// body, one block per section, and page offsets realigned to the cleaned text.
/// Returns the text unchanged when no front-matter region is found near the top.
pub fn strip_front_matter(text: &str, page_offsets: &[(u32, usize)]) -> FrontMatterResult {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let n = lines.len();

    // Per-line byte offset (for page lookup + realignment).
    let mut line_off: Vec<usize> = Vec::with_capacity(n);
    let mut off = 0usize;
    for l in &lines {
        line_off.push(off);
        off += l.len();
    }

    // Lines to lift = the UNION of two detectors (run both):
    //   (1) roman-paginated front-matter — the constitution numbers its front
    //       pages i, ii, … (footer) before the body's arabic page 1; this removes
    //       the WHOLE TOC, including the wrapped-dot lines the dotted detector
    //       misses. Most documents have no roman pagination ⇒ this is a no-op.
    //   (2) the dotted/named-section front cluster — works on every document.
    let mut removed: std::collections::HashSet<usize> = std::collections::HashSet::new();
    if let Some(roman_end) = roman_front_matter_end(&lines) {
        removed.extend(0..roman_end);
    }
    if let Some(kept) = front_cluster(&lines, n) {
        for (a, b, _) in kept {
            removed.extend(a..b);
        }
    }
    if removed.is_empty() {
        return passthrough(text, page_offsets);
    }

    // Blocks: one per maximal contiguous removed run, split at named headings.
    let mut blocks: Vec<FrontMatterBlock> = Vec::new();
    let flush = |blocks: &mut Vec<FrontMatterBlock>, a: usize, b: usize, kind: &str| {
        let body: String = lines[a..b].concat();
        let trimmed = body.trim();
        if !trimmed.is_empty() {
            blocks.push(FrontMatterBlock {
                kind: kind.to_string(),
                content: trimmed.to_string(),
                page: page_for(line_off[a], page_offsets),
            });
        }
    };
    let mut li = 0usize;
    while li < n {
        if !removed.contains(&li) {
            li += 1;
            continue;
        }
        let run_start = li;
        while li < n && removed.contains(&li) {
            li += 1;
        }
        let run_end = li;
        let mut seg_start = run_start;
        let mut seg_kind = section_kind(lines[run_start]).unwrap_or("toc");
        for j in (run_start + 1)..run_end {
            if let Some(k) = section_kind(lines[j]) {
                flush(&mut blocks, seg_start, j, seg_kind);
                seg_start = j;
                seg_kind = k;
            }
        }
        flush(&mut blocks, seg_start, run_end, seg_kind);
    }

    // Rebuild the body without the removed lines, realigning page offsets.
    let mut cleaned = String::with_capacity(text.len());
    let mut cleaned_page_offsets: Vec<(u32, usize)> = Vec::new();
    let mut last_page: Option<u32> = None;
    let mut cleaned_off = 0usize;
    for (li, line) in lines.iter().enumerate() {
        if removed.contains(&li) {
            // Keep the page break even though the line goes — a later stage
            // re-derives the page map from form feeds, so dropping one silently
            // freezes the page counter. See the note in `chrome::strip_chrome`.
            for _ in line.matches('\u{0c}') {
                cleaned.push('\u{0c}');
                cleaned_off += '\u{0c}'.len_utf8();
            }
            continue;
        }
        let page = page_for(line_off[li], page_offsets);
        if last_page != Some(page) {
            cleaned_page_offsets.push((page, cleaned_off));
            last_page = Some(page);
        }
        cleaned.push_str(line);
        cleaned_off += line.len();
    }
    if cleaned_page_offsets.is_empty() {
        cleaned_page_offsets.push((1, 0));
    }
    FrontMatterResult { cleaned, blocks, cleaned_page_offsets }
}

/// The dotted/named-section front cluster (works on every document): TOC dotted
/// runs (≥3 dotted lines) + named section headings, chained from the first span
/// near the top, stopping at the first big gap (= the body began). Returns the
/// kept spans, or None when there is no qualifying cluster near the top.
fn front_cluster(lines: &[&str], n: usize) -> Option<Vec<(usize, usize, &'static str)>> {
    let gate = ((n as f32) * FM_GATE_FRAC) as usize;
    let mut spans: Vec<(usize, usize, &'static str)> = Vec::new();

    // (a) dotted runs: ≥3 dotted lines, blanks allowed between.
    let mut i = 0;
    while i < n {
        if is_dotted_line(lines[i]) {
            let run_start = i;
            let mut last_dotted = i;
            let mut j = i + 1;
            while j < n {
                if is_dotted_line(lines[j]) {
                    last_dotted = j;
                    j += 1;
                } else if lines[j].trim().is_empty() && j - last_dotted <= 2 {
                    j += 1;
                } else {
                    break;
                }
            }
            let dotted = (run_start..=last_dotted).filter(|&x| is_dotted_line(lines[x])).count();
            if dotted >= 3 {
                spans.push((run_start, last_dotted + 1, "toc"));
            }
            i = last_dotted + 1;
        } else {
            i += 1;
        }
    }

    // (b) named sections, bounded by the next heading / structural boundary / cap.
    for idx in 0..n {
        if let Some(k) = section_kind(lines[idx]) {
            let cap = (idx + FM_SECTION_CAP).min(n);
            let mut end = cap;
            for j in (idx + 1)..cap {
                if section_kind(lines[j]).is_some() || (is_struct_boundary(lines[j]) && !is_dotted_line(lines[j])) {
                    end = j;
                    break;
                }
            }
            spans.push((idx, end, k));
        }
    }

    // (c) numbered contents runs with NO dot leaders.
    spans.extend(numbered_toc_runs(lines));

    if spans.is_empty() {
        return None;
    }
    spans.sort();
    if spans[0].0 >= gate.max(1) {
        return None;
    }
    let mut kept: Vec<(usize, usize, &'static str)> = vec![spans[0]];
    for &s in &spans[1..] {
        let (last_end, last_kind) = {
            let l = kept.last().unwrap();
            (l.1, l.2)
        };
        // Contents runs chain across a wider gap than other front matter — see
        // TOC_CHAIN_GAP.
        let gap_allowed = if last_kind == "toc" && s.2 == "toc" { TOC_CHAIN_GAP } else { FM_GAP };
        if s.0 <= last_end {
            kept.last_mut().unwrap().1 = last_end.max(s.1);
        } else if s.0 - last_end <= gap_allowed {
            kept.push(s);
        } else {
            break;
        }
    }
    Some(kept)
}

/// A numbered contents entry: `192. Constitution of High Court.` — a provision
/// number and a short TITLE, with no sentence body.
fn is_toc_entry_line(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let t = line.trim();
    t.chars().count() <= TOC_MAX_LINE_LEN
        && RE
            .get_or_init(|| Regex::new(r"^\[?\s*\d{1,3}[A-Z]?\.\s+\S").unwrap())
            .is_match(t)
}

/// Does the line TERMINATE like a contents entry rather than running on into
/// prose? A trailing page number is normal in a contents table and is ignored.
///
/// This is what separates a contents entry from a body provision that happens to
/// be short, and it is load-bearing: `65. A person elected to a House shall not
/// sit` is line-shaped like an entry but does not end like one.
fn is_toc_title_shaped(line: &str) -> bool {
    static PAGE_NO: OnceLock<Regex> = OnceLock::new();
    static LEAD: OnceLock<Regex> = OnceLock::new();
    let stripped = PAGE_NO
        .get_or_init(|| Regex::new(r"\s+\d{1,4}$").unwrap())
        .replace(line.trim(), "");
    let t = stripped.trim_end();
    if !(t.ends_with('.') || t.ends_with(']')) {
        return false;
    }
    // A contents entry is ONE title. A terse provision that states its rule on
    // the same line — `1. Short title and commencement. These rules may be
    // called the Sindh Rules.` — also ends in a period, so the ending alone is
    // not enough; what gives it away is the sentence break in the middle.
    let rest = LEAD
        .get_or_init(|| Regex::new(r"^\[?\s*\d{1,3}[A-Z]?\.\s+").unwrap())
        .replace(t, "");
    !rest.contains(". ")
}

/// Runs of numbered contents entries that carry NO dot leaders.
///
/// The dotted-leader detector (`is_dotted_line`) is the unambiguous signal, but
/// the Constitution amended to the 27th typesets its contents WITHOUT leaders —
/// entries extract as a bare `146. Power of Federation to confer powers`. Those
/// matched the bare-number heading pattern, were minted as Articles, and pushed
/// the running maximum past 280 before the body began; `drop_out_of_sequence_
/// articles` then deleted every genuine Article below ~140 as an out-of-sequence
/// restart. That is how Article 63(1)(e) came to be labelled Article 154(e)
/// (trace a72f1539).
///
/// Density plus title-shape is the discriminator: verified across all 18 corpus
/// documents, this fires on the Constitution's eight contents runs and on the
/// numbered officer roster in the Delegation of Financial Powers (which this
/// module exists to strip — a roster is not a provision), and on nothing else.
fn numbered_toc_runs(lines: &[&str]) -> Vec<(usize, usize, &'static str)> {
    let mut out: Vec<(usize, usize, &'static str)> = Vec::new();
    let mut start: Option<usize> = None;
    let mut end = 0usize;
    let mut nums: Vec<String> = Vec::new();
    let mut gap = 0usize;

    let mut flush = |start: Option<usize>, end: usize, nums: &[String], out: &mut Vec<_>| {
        if let Some(s) = start {
            if nums.len() >= TOC_MIN_ENTRIES && entries_recur_below(lines, end, nums) {
                out.push((s, end + 1, "toc"));
            }
        }
    };

    for (i, line) in lines.iter().enumerate() {
        let entry = is_toc_entry_line(line);
        if entry && is_toc_title_shaped(line) {
            if start.is_none() {
                start = Some(i);
            }
            if let Some(n) = provision_number(line) {
                nums.push(n);
            }
            end = i;
            gap = 0;
        } else if start.is_some()
            && !entry
            && line.trim().chars().count() <= TOC_MAX_LINE_LEN
            && gap < TOC_MAX_GAP
        {
            // Short interleaved lines (chapter headings, page numbers) do not end
            // a contents region; prose, or a numbered line that runs on instead of
            // reading as a title, does — that is where the body has begun.
            gap += 1;
        } else {
            flush(start, end, &nums, &mut out);
            start = None;
            nums.clear();
            gap = 0;
        }
    }
    flush(start, end, &nums, &mut out);
    out
}

/// The provision number a line opens with (`191A` from `191A. [Omitted.]`).
fn provision_number(line: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\[?\s*(\d{1,3}[A-Z]?)\.").unwrap())
        .captures(line.trim())
        .map(|c| c[1].to_string())
}

/// Fraction of a candidate run's numbers that appear again BELOW it as a
/// line-initial provision number, against [`TOC_MIN_RECURRENCE`].
///
/// This is what a table of contents actually IS: a list that points at
/// provisions occurring later in the document. Shape alone cannot carry the
/// decision — a run of terse one-line provisions (`1. Provision number 1 with
/// real legislative prose here.`) is indistinguishable from a contents list by
/// length, punctuation or density, and an earlier version of this detector
/// lifted exactly that out of the body. Measured over the corpus: real contents
/// runs recur at 94–100%, a body run at 0%.
fn entries_recur_below(lines: &[&str], end: usize, nums: &[String]) -> bool {
    let below: std::collections::HashSet<String> = lines
        .iter()
        .skip(end + 1)
        .filter_map(|l| provision_number(l))
        .collect();
    let hits = nums.iter().filter(|n| below.contains(*n)).count();
    hits * 100 >= nums.len() * TOC_MIN_RECURRENCE
}

/// A standalone lowercase roman numeral page footer (`i`, `ii`, … `xlv`): 1–5
/// chars over `{i,v,x,l}` with at least one `i/v/x`.
///
/// Accepts a PARENTHESISED footer — `(vii)` — on a line of its own (2026-07-27,
/// trace a72f1539). Parentheses were originally excluded to keep a clause marker
/// `(i)` from being read as a page footer, and that concern is real: this
/// decides where the body begins. But the Constitution amended to the 27th
/// paginates its front matter `(i)`, `(ii)`, `(iii)` where the previous edition
/// used bare `i`, `ii`, `iii`, so its whole table of contents flowed into the
/// body, was minted as Articles, and pushed the expected article number past 280
/// before the real text started — after which the sequence guard deleted every
/// genuine Article below ~140.
///
/// Being alone on the line is necessary but NOT sufficient. What actually
/// separates a page footer from a clause marker is that footers **ascend**
/// (i, ii, iii, …) across page-spaced intervals, while a clause list restarts.
/// [`roman_front_matter_end`] enforces the ascent; this function only recognises
/// the shape.
///
/// An earlier attempt keyed on indentation — a footer is centred, a marker sits
/// at the left margin — which reads correctly in the raw PDF text but is useless
/// here: `text_normalizer::collapse_columns_per_line` squashes multi-space runs
/// to a single space BEFORE this pass runs, so by now every line is flush left.
fn is_roman_footer_line(line: &str) -> bool {
    let t = line.trim();
    match t.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        Some(inner) => is_roman_footer(inner),
        None => is_roman_footer(t),
    }
}

/// Numeric value of a lowercase roman numeral over `{i,v,x,l}`, honouring
/// subtractive pairs (`ix` = 9). `None` for anything unparseable.
fn roman_value(s: &str) -> Option<u32> {
    let digit = |c: u8| match c {
        b'i' => Some(1),
        b'v' => Some(5),
        b'x' => Some(10),
        b'l' => Some(50),
        _ => None,
    };
    let b = s.as_bytes();
    if b.is_empty() {
        return None;
    }
    // Signed accumulator: a subtractive pair (`ix`) subtracts before it adds, so
    // an unsigned running total underflows on the very first character.
    let mut total: i64 = 0;
    for i in 0..b.len() {
        let v = digit(b[i])? as i64;
        let next = b.get(i + 1).copied().and_then(digit).unwrap_or(0) as i64;
        if v < next {
            total -= v;
        } else {
            total += v;
        }
    }
    u32::try_from(total).ok().filter(|n| *n > 0)
}

/// Roman numeral of a footer line, parenthesised or bare.
fn roman_footer_value(line: &str) -> Option<u32> {
    let t = line.trim();
    let inner = t.strip_prefix('(').and_then(|s| s.strip_suffix(')')).unwrap_or(t);
    if is_roman_footer(inner) {
        roman_value(inner)
    } else {
        None
    }
}

fn is_roman_footer(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= 5
        && t.bytes().all(|b| matches!(b, b'i' | b'v' | b'x' | b'l'))
        && t.bytes().any(|b| matches!(b, b'i' | b'v' | b'x'))
}

/// Constitution-style front-matter paginated with roman numerals (footers i, ii,
/// …). Returns the line index where the body begins = just after the last footer
/// of the leading, page-spaced roman run. Requires ≥3 such footers, so the
/// (vast majority of) documents without roman front-pagination return None. The
/// body's own arabic footers are removed earlier by chrome, so we anchor on the
/// roman run rather than the first arabic "1".
fn roman_front_matter_end(lines: &[&str]) -> Option<usize> {
    let footers: Vec<usize> = (0..lines.len())
        .filter(|&i| is_roman_footer_line(lines[i].trim_end_matches('\n')))
        .collect();
    if footers.len() < 3 {
        return None;
    }
    // Keep only the LEADING run whose footers are page-spaced (≤120 lines apart)
    // AND whose numerals ASCEND. The ascent is what makes the parenthesised form
    // safe to accept: real pagination climbs i → ii → iii across pages, whereas a
    // clause list `(i) (ii) (iii)` restarts at every provision, so a run of them
    // cannot keep climbing across page-length gaps. Without this the body would
    // be cut at the first `(i)` that happened to sit alone on a line.
    let mut end = footers[0];
    let mut run = 1usize;
    let mut last_val = roman_footer_value(lines[footers[0]]);
    for w in footers.windows(2) {
        let val = roman_footer_value(lines[w[1]]);
        let ascends = match (last_val, val) {
            (Some(a), Some(b)) => b > a,
            // Unparseable on either side → fall back to the spacing rule alone,
            // preserving the pre-2026-07-27 behaviour for bare numerals.
            _ => true,
        };
        if w[1] - w[0] <= 120 && ascends {
            end = w[1];
            run += 1;
            last_val = val;
        } else {
            break;
        }
    }
    if run >= 3 {
        Some((end + 1).min(lines.len()))
    } else {
        None
    }
}

/// A structural boundary line (Part / Chapter / Preamble / Schedule / Annexure)
/// — marks where front-matter ends and the body begins.
fn is_struct_boundary(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^\s*(part\s+[ivx0-9]|chapter[\s-]+[ivx0-9]|preamble|schedule|annex)").unwrap()
    })
    .is_match(line)
}

fn passthrough(text: &str, page_offsets: &[(u32, usize)]) -> FrontMatterResult {
    FrontMatterResult {
        cleaned: text.to_string(),
        blocks: Vec::new(),
        cleaned_page_offsets: page_offsets.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_line_detected_real_prose_not() {
        assert!(is_dotted_line("6. High treason ............... 12"));
        assert!(is_dotted_line("Preamble …………… 3"));
        assert!(!is_dotted_line("6. High treason shall be punishable with death."));
    }

    #[test]
    fn lifts_toc_and_keeps_body() {
        let text = "\
TABLE OF CONTENTS
1. Short title ................... 1
2. Definitions .................. 2
6. High treason ................. 5

PART I
1. Short title and commencement. These rules may be called the Rules.
2. Definitions. In these rules unless the context otherwise requires.";
        let r = strip_front_matter(text, &[(1, 0)]);
        // TOC removed from body, real provisions kept
        assert!(!r.cleaned.contains("...............")); // dotted TOC gone
        assert!(r.cleaned.contains("PART I"));
        assert!(r.cleaned.contains("may be called"));
        // one retrievable toc block holding the contents verbatim
        assert_eq!(r.blocks.len(), 1, "{:?}", r.blocks);
        assert_eq!(r.blocks[0].kind, "toc");
        assert!(r.blocks[0].content.contains("High treason"));
    }

    /// Regression, trace a72f1539 (2026-07-27). The Constitution amended to the
    /// 27th paginates its front matter `(i)`, `(ii)`, `(iii)` where the previous
    /// edition used bare `i`, `ii`, `iii`. `is_roman_footer` rejected the
    /// parenthesised form, so the whole table of contents flowed into the body,
    /// was minted as Articles, and pushed the expected number past 280 — after
    /// which the sequence guard deleted every real Article below ~140.
    ///
    /// Footer text and indentation copied from the live extraction.
    #[test]
    fn parenthesised_roman_footers_lift_the_front_matter() {
        let text = "CONSTITUTION OF PAKISTAN
    146.   Power of Federation to confer powers, etc., on
           Provinces, in certain cases.
    147.   Power of the Provinces to entrust functions.
                                   (vi)
    148.   Obligation of Provinces and Federation.
    154.   Functions and rules of procedure.
                                   (vii)
    155.   Complaints as to interference with water supplies
    156.   National Economic Council.
                                   (viii)
CHAPTER 2.-THE MAJLIS-E-SHOORA (PARLIAMENT)
54. (1) The President may, from time to time, summon either
House or both Houses in joint sitting to meet at such time and place.";
        let r = strip_front_matter(text, &[(1, 0)]);
        assert!(
            !r.cleaned.contains("Functions and rules of procedure"),
            "contents listing leaked into the body: {:?}",
            r.cleaned
        );
        assert!(
            r.cleaned.contains("The President may, from time to time"),
            "the real provision was lost: {:?}",
            r.cleaned
        );
        assert!(!r.blocks.is_empty(), "front matter must stay retrievable");
    }

    #[test]
    fn roman_values_parse_including_subtractive_pairs() {
        assert_eq!(roman_value("i"), Some(1));
        assert_eq!(roman_value("iv"), Some(4));
        assert_eq!(roman_value("vii"), Some(7));
        assert_eq!(roman_value("ix"), Some(9));
        assert_eq!(roman_value("xl"), Some(40));
        assert_eq!(roman_value("xlv"), Some(45));
        assert_eq!(roman_value("z"), None);
        assert_eq!(roman_value(""), None);
        assert_eq!(roman_footer_value("  (vii)  "), Some(7));
        assert_eq!(roman_footer_value("  iii  "), Some(3));
        assert_eq!(roman_footer_value("(iv) the authority shall decide;"), None);
    }

    /// A clause list `(i) (ii) (iii)` RESTARTS at every provision, so it cannot
    /// keep ascending across page-length gaps. That is what stops the body being
    /// cut at the first marker sitting alone on a line — indentation cannot be
    /// used, because normalisation flattens it before this pass runs.
    #[test]
    fn repeating_clause_markers_do_not_end_the_front_matter() {
        let mut text = String::new();
        for _ in 0..4 {
            // Each provision restarts its own (i)/(ii)/(iii) list.
            for m in ["(i)", "(ii)", "(iii)"] {
                text.push_str(m);
                text.push('\n');
                text.push_str("the competent authority shall decide the matter;\n");
            }
        }
        text.push_str("PART I\n1. Short title. These rules may be called the Rules.\n");
        let r = strip_front_matter(&text, &[(1, 0)]);
        assert!(
            r.cleaned.contains("Short title"),
            "body was cut by a restarting clause list: {:?}",
            r.cleaned
        );
    }

    /// Real provisions carry body text on the same line. A document of terse
    /// one-line provisions must not be mistaken for a contents listing.
    #[test]
    fn numbered_provisions_with_bodies_are_not_mistaken_for_contents() {
        let text = "PART I
1. Short title and commencement. These rules may be called the Sindh Rules.
2. Definitions. In these rules, unless there is anything repugnant in the
subject or context, the following expressions shall have the meanings hereby
respectively assigned to them.
3. Application. These rules shall apply to all departments of Government.
4. Powers. The competent authority may exercise such powers as are conferred.";
        let r = strip_front_matter(text, &[(1, 0)]);
        assert!(r.cleaned.contains("Short title and commencement"));
        assert!(r.cleaned.contains("Application"));
        assert!(
            r.blocks.is_empty(),
            "real provisions must not be lifted as front matter: {:?}",
            r.blocks
        );
    }

    /// Trace a72f1539: the Constitution amended to the 27th typesets its contents
    /// WITHOUT dot leaders, so `is_dotted_line` saw nothing and every entry was
    /// minted as an Article. Those fake Articles pushed the running maximum past
    /// 280 before the body began, and the out-of-sequence guard then deleted the
    /// real Articles below ~140 — which is how Article 63(1)(e) was stored as
    /// Article 154(e).
    #[test]
    fn dot_leader_less_contents_do_not_mint_articles() {
        let text = "\
CHAPTER 2.-THE SUPREME COURT
176. Constitution of Supreme Court.
177. Appointment of Chief Justice.
178. Oath of Office.
179. Retiring age.
180. Acting Chief Justice.
181. Acting Judges.
182. Ad hoc Judges.
183. Original jurisdiction.
184. Advisory jurisdiction.
191A. [Omitted.]
176. There shall be a Supreme Court of Pakistan consisting of a Chief Justice
and so many other Judges as may be determined by Act of Majlis-e-Shoora.
177. The Chief Justice of Pakistan shall be appointed by the President, and
each of the other Judges shall be appointed by the President after consultation.
178. Before entering upon office, the Chief Justice of Pakistan shall make
before the President, and any other Judge shall make before the Chief Justice.
179. A Judge of the Supreme Court shall hold office until he attains the age of
sixty-five years, unless he sooner resigns or is removed from office.
180. At any time when the office of Chief Justice of Pakistan is vacant, the
President shall appoint the most senior of the other Judges of the Supreme Court.
181. At any time when a Judge of the Supreme Court is absent or is unable to
perform the functions of his office, the President may appoint a Judge.
182. Where the Supreme Court considers that it is necessary to increase the
number of Judges, the Chief Justice may request the President to appoint one.
183. The Supreme Court shall, to the exclusion of every other court, have
original jurisdiction in any dispute between two or more Governments.
184. If, at any time, the President considers that it is desirable to obtain the
opinion of the Supreme Court on any question of law, he may refer the question.
191A. [Omitted.]";
        let r = strip_front_matter(text, &[(1, 0)]);
        assert!(
            r.blocks.iter().any(|b| b.kind == "toc"),
            "contents run was not detected: {:?}",
            r.blocks.iter().map(|b| &b.kind).collect::<Vec<_>>()
        );
        // The contents entries must be gone from the body...
        assert!(!r.cleaned.contains("176. Constitution of Supreme Court"));
        assert!(!r.cleaned.contains("183. Original jurisdiction."));
        // ...but kept as retrievable front matter, never discarded.
        assert!(r.blocks.iter().any(|b| b.content.contains("Retiring age")));
        // ...and the real provisions they point at must survive untouched.
        assert!(r.cleaned.contains("There shall be a Supreme Court of Pakistan"));
        assert!(r.cleaned.contains("original jurisdiction in any dispute"));
    }

    /// The shape test, not just the count, is what keeps terse one-line
    /// provisions out: they end in a period like a contents entry, but carry a
    /// sentence break in the middle. Eight of them in a row must still survive.
    #[test]
    fn terse_one_line_provisions_are_not_contents() {
        let text = "\
THE SINDH RULES
1. Short title. These rules may be called the Sindh Rules, 1986.
2. Definitions. In these rules the following expressions shall apply.
3. Application. These rules shall apply to all departments of Government.
4. Powers. The competent authority may exercise such powers as conferred.
5. Delegation. The Secretary may delegate any power to a subordinate.
6. Records. Every department shall maintain a register of all cases.
7. Returns. Each department shall submit a monthly return to the Board.
8. Inspection. The Board may inspect any record maintained under these rules.
9. Repeal. The rules previously in force are hereby repealed.";
        let r = strip_front_matter(text, &[(1, 0)]);
        assert!(
            !r.blocks.iter().any(|b| b.kind == "toc"),
            "terse provisions wrongly lifted as contents: {:?}",
            r.blocks
        );
        assert!(r.cleaned.contains("1. Short title"));
        assert!(r.cleaned.contains("9. Repeal"));
    }

    #[test]
    fn splits_named_sections_into_their_own_blocks() {
        let text = "\
Table of Contents
Intro ................. 1
Scope ................. 2
Abbreviations and Acronyms
PEC  Pakistan Engineering Council
P&D  Planning and Development
Drafting Committee
1. Mr A Sheikh Chairman
2. Mr B Khan Member

CHAPTER 1
1. Application. These apply to all development projects in the Province.";
        let r = strip_front_matter(text, &[(1, 0)]);
        let kinds: Vec<&str> = r.blocks.iter().map(|b| b.kind.as_str()).collect();
        assert!(kinds.contains(&"toc"), "{kinds:?}");
        assert!(kinds.contains(&"abbreviations"), "{kinds:?}");
        assert!(kinds.contains(&"committee"), "{kinds:?}");
        // the committee roster is lifted out of the body (no fake provisions)
        assert!(!r.cleaned.contains("Mr A Sheikh"), "roster leaked into body: {:?}", r.cleaned);
        assert!(r.cleaned.contains("CHAPTER 1"));
        assert!(r.cleaned.contains("Application"));
    }

    #[test]
    fn roman_paginated_front_matter_is_lifted() {
        // 3 front pages footered i / ii / iii (constitution-style), then the body.
        // The TOC line "40. Strengthening bonds…" (no dots on its own line — the
        // case the dotted detector misses) must be lifted out via the roman path.
        let text = "\
TABLE OF CONTENTS
1. The Republic and its territories ......... 3
                              i
more contents listing
40. Strengthening bonds with Muslim world and promoting
                              ii
yet more contents
                              iii
PART I
1. The Republic and its territories. Pakistan shall be a Federal Republic.";
        let r = strip_front_matter(text, &[(1, 0)]);
        assert!(r.cleaned.contains("PART I"), "body lost: {:?}", r.cleaned);
        assert!(r.cleaned.contains("Federal Republic"));
        assert!(!r.cleaned.contains("Strengthening bonds"), "TOC survivor leaked: {:?}", r.cleaned);
        assert!(!r.cleaned.contains("TABLE OF CONTENTS"));
        assert!(!r.blocks.is_empty());
    }

    #[test]
    fn mid_body_dotted_line_is_not_front_matter() {
        // A single dotted form-blank deep in the body must NOT trigger stripping.
        let mut text = String::from("PART I\n");
        for k in 1..=40 {
            text.push_str(&format!("{k}. Provision number {k} with real legislative prose here.\n"));
        }
        text.push_str("Name of officer ................................\n"); // lone form blank, ~85% down
        let r = strip_front_matter(&text, &[(1, 0)]);
        assert!(r.blocks.is_empty(), "must not strip mid-body: {:?}", r.blocks);
        assert!(r.cleaned.contains("Provision number 40"));
    }
}
