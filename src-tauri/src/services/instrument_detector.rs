//! Does a PDF hold ONE legal instrument, or several — and where does each begin?
//!
//! Deterministic, no model, `pdftotext` only, about a second on thirty pages.
//! That speed is the design constraint: this runs at UPLOAD, before indexing, and
//! a check the user waits two minutes for is a check the user skips.
//!
//! Why it exists: `Compendium Rules under SLGA 2013` is twelve separate rule-sets
//! in one file, each with its own Rule 1. Indexed as a single source it makes
//! "Rule 4" ambiguous across instruments, yields one meaningless document
//! summary, and cites answers to a book rather than to a law — which is why
//! `query_router` carries a hand-written demotion for that one document. The
//! pipeline assumes file == instrument; this reports where that is false.
//!
//! # Two signals, and neither is complete
//!
//! * **Short-title clause** — *"These rules may be called the X Rules, 2015"*.
//!   The instrument naming itself, so an excellent LOCATOR. A poor LABEL: read
//!   from `-layout` text it absorbs the marginal catchline, giving
//!   `"Sindh Local Councils Short title and (Election) Rules, 2015"`.
//! * **Table of contents** — dotted-leader rows, frequently WRAPPED across two
//!   lines. Clean titles, but it prints BOOK pages, not PDF indices.
//!
//! Measured on the Compendium (2026-07-30): the clause pass finds
//! `Sindh Councils (Budget) Rules, 2017` at p.150 which the contents list omits
//! entirely, while the contents list carries four instruments the clause pass
//! misses. So they are UNIONED, never ranked — trusting either alone loses real
//! instruments.
//!
//! # Nothing here decides anything
//!
//! Every range is proposed to a human. Disagreements between the two signals are
//! reported AS disagreements rather than quietly reconciled, because that is
//! exactly where a reviewer's attention is worth spending. A wrong split
//! silently truncates an instrument, which is worse than not splitting at all.
//!
//! Results are deliberately not cached: a re-uploaded edition is re-detected, so
//! a compendium that has gained an instrument cannot inherit last year's
//! boundaries.

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

/// Which signals vouch for a segment's start page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    /// Contents list and short-title clause agree — the safe case.
    Both,
    /// Found in the text but absent from the contents list.
    ClauseOnly,
    /// Listed in the contents but no short-title clause found; the start page is
    /// the contents' own claim and is the most likely to be off by a page.
    TocOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start: u32,
    pub end: u32,
    pub title: String,
    pub evidence: Evidence,
    /// False for front matter and closing schedules ("Introduction").
    pub is_instrument: bool,
    /// A provision-numbering restart corroborates this boundary.
    pub restart: bool,
    /// First non-blank line of the segment — the cheapest thing a human can
    /// check. One line tells a reviewer whether the boundary is right.
    pub first_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inspection {
    pub pages: u32,
    pub segments: Vec<Segment>,
    /// Page offset from the contents list's printed numbers to PDF indices.
    pub page_offset: i32,
    pub offset_derived: bool,
    pub notes: Vec<String>,
}

impl Inspection {
    pub fn instrument_count(&self) -> usize {
        self.segments.iter().filter(|s| s.is_instrument).count()
    }
    /// Two or more instruments means the file == instrument assumption is false.
    pub fn is_container(&self) -> bool {
        self.instrument_count() >= 2
    }
}

// ── patterns ────────────────────────────────────────────────────────────────

fn short_title_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?is)may\s+be\s+(?:called|cited\s+as)\s+(?:the\s+)?([^.;]{6,140}?)\s*,?\s*(1[89]\d\d|20\d\d)\s*[.,;]",
        )
        .unwrap()
    })
}

fn anchor_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^[ \t]*\[?[ \t]*(\d{1,3})[A-Z]?\.").unwrap())
}

/// A contents row OPENS with its ordinal; it CLOSES with dots and a page number.
/// Between those it may wrap across any number of lines.
fn toc_open_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[ \t]*(\d{1,2})\.[ \t]+(\S.*)$").unwrap())
}

fn toc_close_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(.*?)\.{3,}\s*(\d{1,4})\s*$").unwrap())
}

/// A title naming a legal instrument, as opposed to front or back matter
/// ("Introduction", "Functions to be Performed by Corporations…").
fn instrument_title_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(Rules|Act|Ordinance|Regulations|Order|Policy)\b[^)]*?,?\s*(1[89]\d\d|20\d\d)\s*$")
            .unwrap()
    })
}

/// Catchline fragments that `-layout` splices into a short-title clause.
fn catchline_noise_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(Short\s+title(\s+and)?(\s+commencement)?|commencement|Definitions?)\b[.,]?")
            .unwrap()
    })
}

/// Clause and contents row within this many pages are the same start.
const MERGE_WINDOW: i64 = 3;
/// Numbering restart within this many pages corroborates a start.
const CORROBORATION_WINDOW: i64 = 2;
/// Below this a "start" is a quotation of another instrument, not an instrument.
const MIN_INSTRUMENT_PAGES: u32 = 2;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").trim_matches(|c: char| {
        c == ',' || c == ';' || c == ':' || c == '-' || c == ' '
    }).to_string()
}

/// Distinctive lowercase words, for matching a clause to a contents row. The
/// stop list is deliberately aggressive: almost every title here contains
/// "Sindh", "Local", "Council" and "Rules", so those carry no signal.
fn tokens(title: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "of", "and", "rules", "act", "sindh", "to", "for", "by",
        "ordinance", "regulations", "order", "local", "councils", "council",
    ];
    let mut out: Vec<String> = Vec::new();
    for w in title.to_lowercase().split(|c: char| !c.is_ascii_alphabetic()) {
        if w.len() > 2 && !STOP.contains(&w) && !out.iter().any(|s| s == w) {
            out.push(w.to_string());
        }
    }
    out
}

fn overlap(a: &[String], b: &[String]) -> usize {
    a.iter().filter(|w| b.contains(w)).count()
}

// ── signals ─────────────────────────────────────────────────────────────────

/// `(pdf_page, name)` for every page where a document names itself.
fn clause_starts(pages: &[String]) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = Vec::new();
    for (i, text) in pages.iter().enumerate() {
        let page = i as u32 + 1;
        if out.iter().any(|(p, _)| *p == page) {
            continue;
        }
        if let Some(c) = short_title_re().captures(text) {
            let raw = c.get(1).map(|m| m.as_str()).unwrap_or_default();
            let year = c.get(2).map(|m| m.as_str()).unwrap_or_default();
            let stripped = catchline_noise_re().replace_all(raw, " ");
            let mut name = squash(&stripped);
            if let Some(rest) = name.strip_prefix("the ").or_else(|| name.strip_prefix("The ")) {
                name = rest.to_string();
            }
            if name.len() >= 6 && name.len() <= 140 {
                out.push((page, format!("{name}, {year}")));
            }
        }
    }
    out
}

/// Pages where provision numbering falls back to 1 after a run.
fn restart_pages(pages: &[String]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut high: u32 = 0;
    for (i, text) in pages.iter().enumerate() {
        let nums: Vec<u32> = anchor_re()
            .captures_iter(text)
            .filter_map(|c| c.get(1)?.as_str().parse().ok())
            .collect();
        let (Some(&lo), Some(&hi)) = (nums.iter().min(), nums.iter().max()) else {
            continue;
        };
        if high >= 5 && lo == 1 {
            out.push(i as u32 + 1);
            high = hi;
        } else {
            high = high.max(hi);
        }
    }
    out
}

/// `(title, printed_page)` from a dotted-leader contents list, including rows
/// wrapped over two or more lines.
fn toc_entries(pages: &[String]) -> Vec<(String, u32)> {
    let mut rows: Vec<(String, u32)> = Vec::new();
    let mut first_ordinal: Option<u32> = None;
    let scan = pages.len().div_ceil(8).max(4).min(pages.len());
    let mut pending: Option<(u32, String)> = None;

    for text in &pages[..scan] {
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(c) = toc_open_re().captures(line) {
                let n: u32 = c.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
                pending = Some((n, c.get(2).map(|m| m.as_str()).unwrap_or("").to_string()));
            } else if let Some((n, head)) = pending.take() {
                pending = Some((n, format!("{head} {}", line.trim())));
            }
            let Some((n, ref head)) = pending else { continue };
            if let Some(c) = toc_close_re().captures(head) {
                let title = squash(c.get(1).map(|m| m.as_str()).unwrap_or(""));
                let page: u32 = c.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
                if !title.is_empty() && page > 0 {
                    first_ordinal.get_or_insert(n);
                    rows.push((title, page));
                }
                pending = None;
            }
        }
    }
    // A contents list is ordered and opens at 1; anything else is a false read.
    if rows.len() < 2 || first_ordinal != Some(1) {
        return Vec::new();
    }
    // And it must index INSTRUMENTS, not sections. `Sindh Local Government Act
    // 2013` opens with 102 dotted-leader rows naming its own sections ("Short
    // title, extent and commencement…… 1", "Exception…… 1") — dozens of them on
    // one printed page. Read as instrument starts those collapse into
    // zero-length overlapping segments. A list where almost nothing looks like
    // "<Something> Rules, 2016" is a section index and carries no signal here.
    let instrument_rows = rows
        .iter()
        .filter(|(title, _)| instrument_title_re().is_match(title))
        .count();
    if instrument_rows < 2 {
        return Vec::new();
    }
    rows
}

/// PDF page minus printed page, taken from rows both signals agree on. Derived
/// rather than assumed, so a differently-bound edition still resolves.
fn derive_offset(toc: &[(String, u32)], clauses: &[(u32, String)]) -> (i32, bool) {
    let mut diffs: Vec<i32> = Vec::new();
    for (title, printed) in toc {
        let tt = tokens(title);
        if tt.is_empty() {
            continue;
        }
        for (pdf_page, name) in clauses {
            let d = *pdf_page as i32 - *printed as i32;
            if overlap(&tt, &tokens(name)) >= 2 && d.abs() < 40 {
                diffs.push(d);
                break;
            }
        }
    }
    if diffs.is_empty() {
        return (0, false);
    }
    diffs.sort_unstable();
    (diffs[diffs.len() / 2], true)
}

fn first_line_of(page: Option<&String>) -> String {
    page.and_then(|t| t.lines().map(str::trim).find(|l| l.len() > 3))
        .unwrap_or_default()
        .chars()
        .take(120)
        .collect()
}

/// Inspect already-extracted page texts. Pure — the testable half.
pub fn inspect_pages(pages: &[String]) -> Inspection {
    let clauses = clause_starts(pages);
    let restarts = restart_pages(pages);
    let toc = toc_entries(pages);
    let (offset, offset_derived) = derive_offset(&toc, &clauses);
    let npages = pages.len() as u32;

    // (start, title, evidence, is_instrument)
    let mut raw: Vec<(i64, String, Evidence, bool)> = toc
        .iter()
        .map(|(title, printed)| {
            (
                *printed as i64 + offset as i64,
                title.clone(),
                Evidence::TocOnly,
                instrument_title_re().is_match(title),
            )
        })
        .collect();

    for (pdf_page, name) in &clauses {
        // Merge on TITLE, not proximity. Two contents rows can sit one page
        // apart ("Introduction" p.5, the Election Rules p.6); merging on distance
        // attaches the clause to whichever came first, which promotes front
        // matter to an instrument and inverts both ranges.
        let nt = tokens(name);
        let mut best: Option<usize> = None;
        let mut best_overlap = 0usize;
        for (i, seg) in raw.iter().enumerate() {
            if (seg.0 - *pdf_page as i64).abs() > MERGE_WINDOW {
                continue;
            }
            let o = overlap(&nt, &tokens(&seg.1));
            if o > best_overlap {
                best = Some(i);
                best_overlap = o;
            }
        }
        match best {
            Some(i) if best_overlap >= 1 => {
                raw[i].0 = *pdf_page as i64; // clause knows the exact page
                raw[i].2 = Evidence::Both; // contents keeps the clean title
                raw[i].3 = true;
            }
            _ => raw.push((*pdf_page as i64, name.clone(), Evidence::ClauseOnly, true)),
        }
    }

    raw.sort_by_key(|s| s.0);
    // Two starts on one page cannot both be right, and emitting both produces a
    // zero-length segment that overlaps its neighbour. Keep the better-evidenced
    // one; an instrument beats front matter, and corroborated beats not.
    raw.dedup_by(|b, a| {
        if a.0 != b.0 {
            return false;
        }
        let rank = |e: Evidence, inst: bool| match (e, inst) {
            (Evidence::Both, _) => 3,
            (Evidence::ClauseOnly, _) => 2,
            (Evidence::TocOnly, true) => 1,
            (Evidence::TocOnly, false) => 0,
        };
        if rank(b.2, b.3) > rank(a.2, a.3) {
            *a = b.clone();
        }
        true
    });

    let mut segments: Vec<Segment> = Vec::new();
    for i in 0..raw.len() {
        let start = raw[i].0.clamp(1, npages.max(1) as i64) as u32;
        let end = if i + 1 < raw.len() {
            (raw[i + 1].0 - 1).clamp(start as i64, npages as i64) as u32
        } else {
            npages
        };
        if end.saturating_sub(start) + 1 < MIN_INSTRUMENT_PAGES && raw[i].2 == Evidence::ClauseOnly {
            continue; // a quotation of another instrument, not an instrument
        }
        segments.push(Segment {
            start,
            end,
            title: raw[i].1.clone(),
            evidence: raw[i].2,
            is_instrument: raw[i].3,
            restart: restarts
                .iter()
                .any(|r| (*r as i64 - start as i64).abs() <= CORROBORATION_WINDOW),
            first_line: first_line_of(pages.get(start.saturating_sub(1) as usize)),
        });
    }

    let mut out = Inspection {
        pages: npages,
        segments,
        page_offset: offset,
        offset_derived,
        notes: Vec::new(),
    };
    add_notes(&mut out, &toc);
    out
}

fn add_notes(ins: &mut Inspection, toc: &[(String, u32)]) {
    let instruments = ins.instrument_count();
    if !toc.is_empty() {
        ins.notes.push(format!(
            "contents list: {} rows; page offset {:+} {}",
            toc.len(),
            ins.page_offset,
            if ins.offset_derived { "(derived)" } else { "(NOT derived — assumed 0)" }
        ));
        let toc_only = ins
            .segments
            .iter()
            .filter(|s| s.is_instrument && s.evidence == Evidence::TocOnly)
            .count();
        let clause_only = ins
            .segments
            .iter()
            .filter(|s| s.is_instrument && s.evidence == Evidence::ClauseOnly)
            .count();
        if toc_only > 0 {
            ins.notes.push(format!(
                "{toc_only} instrument(s) listed in the contents with no short-title \
                 clause found — their start pages are the contents' own claim, verify them"
            ));
        }
        if clause_only > 0 {
            ins.notes.push(format!(
                "{clause_only} instrument(s) found in the text but ABSENT from the \
                 contents list — the book's own index is incomplete"
            ));
        }
    } else if instruments >= 2 {
        ins.notes.push(
            "no contents list; names come from short-title clauses and may carry \
             marginal catchline text"
                .into(),
        );
    }

    // Every page must end up somewhere. Unassigned pages are how content
    // disappears silently. Only meaningful on a container: a single instrument
    // is never cut, so its front matter travels with it.
    if instruments >= 2 {
        if let Some(first) = ins.segments.first() {
            if first.start > 1 {
                ins.notes.push(format!(
                    "pp.1–{} assigned to nothing (cover / contents / front matter) — \
                     confirm they are meant to be dropped",
                    first.start - 1
                ));
            }
        }
        for pair in ins.segments.windows(2) {
            if pair[1].start > pair[0].end + 1 {
                ins.notes.push(format!(
                    "pp.{}–{} fall between two instruments — UNASSIGNED",
                    pair[0].end + 1,
                    pair[1].start - 1
                ));
            }
        }
    }
}

/// Inspect a PDF on disk. Shells out to `pdftotext -layout`, exactly as
/// `parsers::pdf` already does for the extraction tiers.
pub fn inspect_pdf(path: &Path) -> Result<Inspection, String> {
    let out = Command::new("pdftotext")
        .arg("-layout")
        .arg(path)
        .arg("-")
        .output()
        .map_err(|e| format!("pdftotext unavailable: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "pdftotext failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut pages: Vec<String> = text.split('\u{c}').map(str::to_string).collect();
    // `pdftotext` TERMINATES each page with a form feed, including the last, so
    // splitting yields a trailing empty element that is not a page. Left in, it
    // inflates the page count by one and pushes the final segment's end page
    // past the real end of the document — which made the split endpoint refuse
    // the very plan this detector had just proposed (found end-to-end, where
    // `pdfinfo` said 40 pages and this said 41).
    // Exactly ONE, not all of them: `Sindh Local Government Act 2013` genuinely
    // ends on a blank page, and popping every empty trailing element lost it,
    // reporting 149 pages where the document has 150.
    if pages.last().is_some_and(|p| p.trim().is_empty()) {
        pages.pop();
    }
    Ok(inspect_pages(&pages))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg(s: &str) -> String {
        s.to_string()
    }

    /// The shape of an ordinary single statute: one short-title clause, one run
    /// of provisions. It must never be proposed for splitting.
    #[test]
    fn a_single_instrument_is_not_a_container() {
        let pages = vec![
            pg("THE SINDH CIVIL SERVANTS ACT, 1973\n\n1. (1) This Act may be called the Sindh Civil Servants Act, 1973.\n2. Definitions.\n3. Terms of service.\n"),
            pg("4. Appointments.\n5. Probation.\n6. Seniority.\n7. Promotion.\n"),
        ];
        let ins = inspect_pages(&pages);
        assert!(!ins.is_container(), "single statute proposed for splitting");
        assert_eq!(ins.instrument_count(), 1);
    }

    /// A Schedule restarts its numbering at 1. Every instrument in the corpus
    /// has one, so a restart ALONE must never open a new instrument — measured
    /// on the Constitution (restarts on six pages) and RoB 1986 (six).
    #[test]
    fn a_numbering_restart_alone_is_a_schedule_not_an_instrument() {
        let mut pages = vec![pg(
            "1. (1) These rules may be called the Sindh Rules of Business, 1986.\n2. Definitions.\n",
        )];
        for n in 3..=30 {
            pages.push(pg(&format!("{n}. Provision {n} text here.\n")));
        }
        pages.push(pg("SCHEDULE I\n1. First entry.\n2. Second entry.\n3. Third.\n"));
        let ins = inspect_pages(&pages);
        assert!(!ins.is_container(), "a Schedule was treated as an instrument");
    }

    /// The Compendium's shape: two rule-sets, each naming itself, each numbering
    /// from 1.
    #[test]
    fn two_self_naming_rule_sets_are_a_container() {
        let mut pages = vec![
            pg("1. (1) These rules may be called the Sindh Local Councils (Election) Rules, 2015.\n2. Definitions.\n"),
        ];
        for n in 3..=20 {
            pages.push(pg(&format!("{n}. Election provision {n}.\n")));
        }
        pages.push(pg(
            "1. (1) These rules may be called the Sindh Local Council (Contract) Rules, 2016.\n2. Definitions.\n",
        ));
        for n in 3..=12 {
            pages.push(pg(&format!("{n}. Contract provision {n}.\n")));
        }
        let ins = inspect_pages(&pages);
        assert!(ins.is_container());
        assert_eq!(ins.instrument_count(), 2);
        assert!(ins.segments[0].title.contains("Election"));
        assert!(ins.segments[1].title.contains("Contract"));
        // ranges must tile without overlap or gap
        assert_eq!(ins.segments[0].end + 1, ins.segments[1].start);
        assert_eq!(ins.segments[1].end, ins.pages);
    }

    /// The catchline leaks into the clause on a two-column statute reprint:
    /// `19. (1) These rules may be called the X Rules, 2015   Short title and`.
    /// The name must not carry it, because the name becomes the citation string.
    #[test]
    fn catchline_text_is_stripped_from_the_name() {
        let pages = vec![
            pg("1. (1) These rules may be called the Sindh Local Councils Short title and (Election) Rules, 2015.\n"),
            pg("2. Definitions.\n3. More.\n"),
        ];
        let ins = inspect_pages(&pages);
        let t = &ins.segments[0].title;
        assert!(!t.contains("Short title"), "catchline survived in {t:?}");
        assert!(t.contains("Election"), "instrument name lost: {t:?}");
    }

    /// Contents rows wrap across two lines (three of the Compendium's thirteen
    /// did), and the page numbers they print are BOOK pages needing an offset.
    #[test]
    fn wrapped_contents_rows_are_read_whole() {
        let toc = pg(
            "CONTENTS\n\
             1.   Introduction..................................................... 04\n\
             2.   The Sindh Local Councils (Election) Rules, 2015 ................ 05\n\
             3.   The Sindh Councils (Imposition, Assessment, Collection and\n\
                  Administration of Taxes) Rules, 2016........................... 87\n",
        );
        let rows = toc_entries(&[toc]);
        assert_eq!(rows.len(), 3, "wrapped row not reassembled: {rows:?}");
        assert!(rows[2].0.contains("Administration of Taxes"), "{:?}", rows[2].0);
        assert_eq!(rows[2].1, 87);
    }

    /// A contents list that does not open at 1 is something else — a numbered
    /// list inside the body — and must not be read as one.
    #[test]
    fn a_list_not_starting_at_one_is_not_a_contents_list() {
        let page = pg("4.  Some heading................. 12\n5.  Another heading.......... 20\n");
        assert!(toc_entries(&[page]).is_empty());
    }

    /// `Sindh Local Government Act 2013` opens with 102 dotted-leader rows
    /// naming its own SECTIONS, dozens sharing one printed page. Read as
    /// instrument starts they collapsed into zero-length overlapping segments —
    /// caught by `corpus_audit`, which is why that assertion exists.
    #[test]
    fn a_section_index_is_not_an_instrument_index() {
        let page = pg(
            "CONTENTS\n\
             1.   Short title, extent and commencement............................ 1\n\
             2.   Exception...................................................... 1\n\
             3.   Definitions.................................................... 1\n\
             4.   Local councils to be body corporate............................ 2\n",
        );
        assert!(
            toc_entries(&[page]).is_empty(),
            "a section index was read as a list of instruments"
        );
    }

    /// Whatever the signals say, the proposed ranges must tile the document:
    /// no overlap, no page claimed twice. Overlapping ranges would silently
    /// assign the same text to two instruments.
    #[test]
    fn proposed_ranges_never_overlap() {
        let toc = pg(
            "CONTENTS\n\
             1.   The Sindh Local Councils (Election) Rules, 2015 ................ 01\n\
             2.   The Sindh Local Council (Contract) Rules, 2016 ................. 01\n",
        );
        let mut pages = vec![toc];
        for n in 1..=6 {
            pages.push(pg(&format!("{n}. Provision {n}.\n")));
        }
        let ins = inspect_pages(&pages);
        for pair in ins.segments.windows(2) {
            assert!(
                pair[1].start > pair[0].end,
                "overlap: {:?} then {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    /// Front matter and closing schedules are segments (so no page is
    /// unaccounted for) but are NOT instruments. Two rule-sets, because a
    /// contents list needs at least two instrument-looking rows before it is
    /// read as an index of instruments at all.
    #[test]
    fn front_matter_is_a_segment_but_not_an_instrument() {
        let toc = pg(
            "CONTENTS\n\
             1.   Introduction..................................................... 01\n\
             2.   The Sindh Local Council (Contract) Rules, 2016 ................. 02\n\
             3.   The Sindh Local Councils (Property) Rules, 2017 ................ 12\n",
        );
        let mut pages = vec![toc, pg("INTRODUCTION\nSome prose about local government.\n")];
        pages.push(pg("1. (1) These rules may be called the Sindh Local Council (Contract) Rules, 2016.\n"));
        for n in 2..=10 {
            pages.push(pg(&format!("{n}. Contract provision {n}.\n")));
        }
        pages.push(pg("1. (1) These rules may be called the Sindh Local Councils (Property) Rules, 2017.\n"));
        for n in 2..=6 {
            pages.push(pg(&format!("{n}. Property provision {n}.\n")));
        }
        let ins = inspect_pages(&pages);
        let intro = ins.segments.iter().find(|s| s.title.contains("Introduction"));
        assert!(intro.is_some(), "front matter dropped from the segment list");
        assert!(!intro.unwrap().is_instrument, "Introduction counted as an instrument");
        assert_eq!(ins.instrument_count(), 2, "front matter inflated the count");
    }

    /// An empty or unreadable document must not panic or claim a container.
    #[test]
    fn empty_input_is_inert() {
        let ins = inspect_pages(&[]);
        assert!(!ins.is_container());
        assert_eq!(ins.instrument_count(), 0);
        assert!(ins.segments.is_empty());
    }

    /// The last segment must end on the document's real last page.
    ///
    /// `pdftotext` terminates every page with a form feed, so a naive split
    /// leaves a trailing empty element. Counting it as a page made the detector
    /// propose a range one past the end, which the split endpoint then refused —
    /// a review flow whose own output it would not accept. Verified against
    /// `pdfinfo`, which is what the executor counts with.
    #[test]
    #[ignore]
    fn page_count_agrees_with_pdfinfo() {
        let Ok(dir) = std::env::var("NOVIQ_CORPUS_DIR") else {
            eprintln!("skip: set NOVIQ_CORPUS_DIR");
            return;
        };
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).expect("corpus dir").flatten() {
            let p = entry.path();
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")) {
                continue;
            }
            let Ok(ins) = inspect_pdf(&p) else { continue };
            let truth = std::process::Command::new("pdfinfo")
                .arg(&p)
                .output()
                .ok()
                .and_then(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .find_map(|l| l.strip_prefix("Pages:")?.trim().parse::<u32>().ok())
                });
            let Some(truth) = truth else { continue };
            assert_eq!(
                ins.pages,
                truth,
                "{:?}: detector says {} pages, pdfinfo says {truth}",
                p.file_name().unwrap(),
                ins.pages
            );
            if let Some(last) = ins.segments.last() {
                assert!(
                    last.end <= truth,
                    "{:?}: last segment ends on p.{} but the document has {truth} pages",
                    p.file_name().unwrap(),
                    last.end
                );
            }
            checked += 1;
        }
        println!("page counts agree with pdfinfo on {checked} documents");
        assert!(checked > 0, "no documents checked");
    }

    /// Audit the real corpus. Ignored by default — it needs the PDFs and
    /// `pdftotext`, so it only runs on the VM:
    ///
    /// ```text
    /// NOVIQ_CORPUS_DIR=/opt/noviq/data/uploads/admin \
    ///   cargo test --lib --no-default-features --features server \
    ///   instrument_detector::tests::corpus_audit -- --ignored --nocapture
    /// ```
    ///
    /// Measured 2026-07-30: exactly ONE of nineteen documents is a container.
    /// Over-splitting is the dangerous failure here, so the assertion that
    /// matters is the count of containers, not the contents of any one.
    #[test]
    #[ignore]
    fn corpus_audit() {
        let Ok(dir) = std::env::var("NOVIQ_CORPUS_DIR") else {
            eprintln!("skip: set NOVIQ_CORPUS_DIR");
            return;
        };
        let mut pdfs: Vec<_> = std::fs::read_dir(&dir)
            .expect("corpus dir")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")))
            .collect();
        pdfs.sort();

        let mut containers = 0;
        for p in &pdfs {
            let ins = match inspect_pdf(p) {
                Ok(i) => i,
                Err(e) => panic!("{}: {e}", p.display()),
            };
            let name = p.file_name().unwrap().to_string_lossy();
            if ins.is_container() {
                containers += 1;
                println!("CONTAINER ({} instruments)  {name}", ins.instrument_count());
                for s in &ins.segments {
                    println!(
                        "    {:?} pp.{}-{} {} {}",
                        s.evidence,
                        s.start,
                        s.end,
                        &s.title.chars().take(58).collect::<String>(),
                        if s.is_instrument { "" } else { "[not an instrument]" }
                    );
                }
                for n in &ins.notes {
                    println!("    - {n}");
                }
            } else {
                println!("single                     {name}");
            }
            // Ranges must tile: no overlap, no page counted twice.
            for pair in ins.segments.windows(2) {
                assert!(
                    pair[1].start > pair[0].end,
                    "{name}: overlapping ranges {:?} / {:?}",
                    pair[0],
                    pair[1]
                );
            }
        }
        println!("\n{containers} of {} documents are containers.", pdfs.len());
        assert_eq!(containers, 1, "corpus container count changed — investigate");
    }

    /// The reviewer's cheapest check: one line of text per proposed boundary.
    #[test]
    fn every_segment_carries_its_opening_line() {
        let mut pages = vec![pg(
            "1. (1) These rules may be called the Sindh Local Councils (Election) Rules, 2015.\n",
        )];
        for n in 2..=8 {
            pages.push(pg(&format!("{n}. Election provision {n}.\n")));
        }
        pages.push(pg("1. (1) These rules may be called the Sindh Local Council (Contract) Rules, 2016.\n"));
        for n in 2..=6 {
            pages.push(pg(&format!("{n}. Contract provision {n}.\n")));
        }
        let ins = inspect_pages(&pages);
        for s in &ins.segments {
            assert!(!s.first_line.is_empty(), "segment {s:?} has no opening line");
        }
    }
}
