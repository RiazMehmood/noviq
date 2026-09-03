//! 013/Iteration-8 — back-matter zoning (symmetric to `front_matter`).
//!
//! A legal PDF often ends with Schedules / Annexes / Appendices. Their numbered
//! lists (e.g. the constitution's First Schedule "list of laws":
//! `4. The Life Insurance (Nationalization) Order, 1972`) were promoted to FALSE
//! provisions (`Article 4`), inflating the corpus with wrong citations.
//!
//! This pass lifts the **contiguous tail cluster** of Schedule/Annex/Appendix
//! sections out of the body BEFORE heading detection — so those lines never reach
//! the provision detector — and keeps each section as a retrievable
//! `block_class='back_matter'` chunk (schedules are operative legal content).
//!
//! Safety: only the cluster that reaches EOF and STARTS in the last 20% of the
//! document is taken; a mid-body schedule (Compendium / Rules-of-Business) is
//! never touched. The cluster chains backward across schedule-content gaps but
//! stops at the first gap larger than 15% of the document (= the body resumes).

use std::sync::OnceLock;
use regex_lite::Regex;

use crate::services::parsers::chrome::page_for;
use crate::services::parsers::front_matter::is_dotted_line;

/// One back-matter section, kept retrievable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackMatterBlock {
    /// `schedule` | `annex` | `appendix`.
    pub kind: String,
    pub content: String,
    pub page: u32,
}

pub struct BackMatterResult {
    pub cleaned: String,
    pub blocks: Vec<BackMatterBlock>,
    pub cleaned_page_offsets: Vec<(u32, usize)>,
}

/// Max gap (fraction of document) the tail cluster may chain across before it is
/// considered ended (the main body has resumed).
const BM_GAP_FRAC: f32 = 0.15;
/// The cluster START must sit within the last 20% of the document.
const BM_GATE_FRAC: f32 = 0.20;

/// A standalone Schedule / Annex / Appendix heading — centered ALL-CAPS,
/// tolerating a leading `[` (amendment bracket, as in the constitution's
/// `[FIRST SCHEDULE`). Returns the section kind. A dotted TOC entry
/// (`THIRD SCHEDULE …… 186`) is NOT a boundary. Case-sensitive uppercase, so a
/// mid-sentence "…the Second Schedule…" never matches.
fn boundary_kind(line: &str) -> Option<&'static str> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"^[ \t]*(?:\d*\[)?(?:(?:FIRST|SECOND|THIRD|FOURTH|FIFTH|SIXTH|SEVENTH|EIGHTH|NINTH|TENTH)[ \t]+)?(SCHEDULE|ANNEXURE|ANNEX|APPENDIX)\b",
        )
        .unwrap()
    });
    if is_dotted_line(line) {
        return None;
    }
    let c = re.captures(line)?;
    Some(match &c[1] {
        "SCHEDULE" => "schedule",
        "APPENDIX" => "appendix",
        _ => "annex",
    })
}

/// Lift the tail Schedule/Annex/Appendix cluster out of `text`, returning the
/// cleaned body, one block per section, and realigned page offsets. Returns the
/// text unchanged when there is no qualifying tail cluster.
pub fn strip_back_matter(text: &str, page_offsets: &[(u32, usize)]) -> BackMatterResult {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let n = lines.len();
    if n == 0 {
        return passthrough(text, page_offsets);
    }

    let mut line_off: Vec<usize> = Vec::with_capacity(n);
    let mut off = 0usize;
    for l in &lines {
        line_off.push(off);
        off += l.len();
    }

    let bounds: Vec<usize> = (0..n).filter(|&i| boundary_kind(lines[i]).is_some()).collect();
    if bounds.is_empty() {
        return passthrough(text, page_offsets);
    }

    // Chain backward from the last boundary across schedule-content gaps; stop at
    // the first gap > BM_GAP_FRAC of the document (the main body has resumed).
    let gap_max = ((n as f32) * BM_GAP_FRAC) as usize;
    let mut start = *bounds.last().unwrap();
    for &b in bounds.iter().rev() {
        if start - b <= gap_max {
            start = b;
        } else {
            break;
        }
    }

    // The cluster must start in the last BM_GATE_FRAC of the document, else it is
    // a mid-body schedule — leave it in the body.
    let gate = ((n as f32) * (1.0 - BM_GATE_FRAC)) as usize;
    if start < gate {
        return passthrough(text, page_offsets);
    }

    // One block per section, splitting [start, n) at each boundary heading.
    let mut blocks: Vec<BackMatterBlock> = Vec::new();
    let flush = |blocks: &mut Vec<BackMatterBlock>, a: usize, b: usize, kind: &str| {
        let body: String = lines[a..b].concat();
        let trimmed = body.trim();
        if !trimmed.is_empty() {
            blocks.push(BackMatterBlock {
                kind: kind.to_string(),
                content: trimmed.to_string(),
                page: page_for(line_off[a], page_offsets),
            });
        }
    };
    let mut seg_start = start;
    let mut seg_kind = boundary_kind(lines[start]).unwrap_or("schedule");
    for j in (start + 1)..n {
        if let Some(k) = boundary_kind(lines[j]) {
            flush(&mut blocks, seg_start, j, seg_kind);
            seg_start = j;
            seg_kind = k;
        }
    }
    flush(&mut blocks, seg_start, n, seg_kind);

    // Rebuild the body without [start, n), realigning page offsets.
    let mut cleaned = String::with_capacity(text.len());
    let mut cleaned_page_offsets: Vec<(u32, usize)> = Vec::new();
    let mut last_page: Option<u32> = None;
    let mut cleaned_off = 0usize;
    for (li, line) in lines.iter().enumerate() {
        if li >= start {
            break;
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
    BackMatterResult { cleaned, blocks, cleaned_page_offsets }
}

fn passthrough(text: &str, page_offsets: &[(u32, usize)]) -> BackMatterResult {
    BackMatterResult {
        cleaned: text.to_string(),
        blocks: Vec::new(),
        cleaned_page_offsets: page_offsets.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(provisions: usize) -> String {
        let mut s = String::new();
        for k in 1..=provisions {
            s.push_str(&format!("{k}. Provision {k}. Some real legislative prose for this article.\n"));
        }
        s
    }

    #[test]
    fn lifts_tail_schedule_keeps_body() {
        // body of 40 articles, then a tail FIRST SCHEDULE listing laws (1..3).
        let text = format!(
            "{}\n[FIRST SCHEDULE\n1. The Life Insurance (Nationalization) Order, 1972.\n\
             2. The Co-operative Banks Act.\n3. The Income Tax Order.\n",
            body(40)
        );
        let r = strip_back_matter(&text, &[(1, 0)]);
        assert!(r.cleaned.contains("Provision 40"), "body lost: {:?}", r.cleaned);
        assert!(!r.cleaned.contains("Life Insurance"), "schedule leaked into body");
        assert_eq!(r.blocks.len(), 1, "{:?}", r.blocks);
        assert_eq!(r.blocks[0].kind, "schedule");
        assert!(r.blocks[0].content.contains("Life Insurance"));
    }

    #[test]
    fn mid_body_schedule_is_not_back_matter() {
        // SCHEDULE near the FRONT (37%) followed by lots more body → not back-matter.
        let text = format!("{}\nSCHEDULE-I\nForm stuff.\n{}", body(15), body(40));
        let r = strip_back_matter(&text, &[(1, 0)]);
        assert!(r.blocks.is_empty(), "mid-body schedule wrongly lifted: {:?}", r.blocks);
        assert!(r.cleaned.contains("SCHEDULE-I"));
    }

    #[test]
    fn no_schedule_passthrough() {
        let text = body(30);
        let r = strip_back_matter(&text, &[(1, 0)]);
        assert!(r.blocks.is_empty());
        assert_eq!(r.cleaned, text);
    }

    #[test]
    fn dotted_toc_schedule_line_is_not_a_boundary() {
        // a TOC dotted entry must not be read as a real schedule boundary
        assert!(boundary_kind("THIRD SCHEDULE ............................ 186").is_none());
        assert_eq!(boundary_kind("        [FIRST SCHEDULE"), Some("schedule"));
        // item-1: OCR'd amendment-footnote digit before the bracket (E&D `1[SCHEDULE`)
        assert_eq!(boundary_kind("1[SCHEDULE (See Rule 10)"), Some("schedule"));
        assert_eq!(boundary_kind("   APPENDIX 'A'"), Some("appendix"));
        assert_eq!(boundary_kind("        [ANNEX"), Some("annex"));
        // mid-sentence mixed-case reference is never a boundary
        assert!(boundary_kind("in column 2 of the Second Schedule shall apply").is_none());
    }
}
