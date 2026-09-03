//! T017 — heading-boundary + 2400-token-budget chunker for the LLM IR
//! builder.
//!
//! Reuses Feature 005's `heading_detector::regex_pass::scan_with_profile`
//! for boundary detection. Token budget is approximated as
//! `chars / TOKEN_CHAR_RATIO` so we don't pull in a tokenizer just for this
//! pre-chunking step.
//!
//! See research.md §R5.

use crate::services::doc_profiles::DocProfile;
use crate::services::heading_detector::regex_pass::scan_with_profile;
use crate::services::structure_parser::NodeLevel;

/// Maximum tokens per chunk. Leaves headroom in a 4096-token Mistral context
/// after system prompt (~700 tokens) + breadcrumb (~100) + JSON-schema state
/// overhead.
// Slice-B-revised (after first real run): 116-page Sindh rules PDF produced
// 275 chunks at budget=2400. To halve the chunk count without overflowing the
// IR builder's n_ctx=4096, raise to 3200 and lift IR_BUILDER_N_CTX → 6144 in
// mistral_builder so sys+chunk+gen still fits.
pub const CHUNK_TOKEN_BUDGET: usize = 3200;

/// Conservative chars-per-token estimate for English+legal-Pakistani text.
/// llama.cpp tokenisation typically lands around 3.5–4.0 chars per token; we
/// use 3.5 to stay safely under the budget.
const TOKEN_CHAR_RATIO: f32 = 3.5;

/// The active outline path from previous chunks — Vec of (heading-level,
/// number/label) pairs walked from root. Empty for the very first chunk.
///
/// Concretely: `[(Part, "III"), (Chapter, "2"), (Section, "14")]` means the
/// next chunk is inside Part III › Chapter 2 › Section 14.
pub type Breadcrumb = Vec<(NodeLevel, String)>;

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    /// Text of this chunk (raw slice from the normalized source).
    pub text: String,
    /// Breadcrumb context active at the START of this chunk.
    pub breadcrumb: Breadcrumb,
    /// Byte offset in the source where this chunk begins (0-based).
    pub offset: usize,
    /// 1-based page number (best-effort — derived from offset / total length
    /// × page_count by the caller; chunker itself only emits offsets).
    pub page_start: u32,
    pub page_end: u32,
}

/// Pre-chunk `text` along heading boundaries from `profile`, packing each
/// chunk to roughly `CHUNK_TOKEN_BUDGET` tokens.
///
/// Pure function — testable without a model.
pub fn chunk(text: &str, profile: &DocProfile, page_count: u32) -> Vec<Chunk> {
    let total_len = text.len();
    if total_len == 0 {
        return Vec::new();
    }

    let headings = scan_with_profile(text, profile);

    // Build candidate segments: each starts at one heading and ends at the
    // next (or at EOF). When no headings exist, the whole document is one
    // segment.
    let mut segments: Vec<(usize, usize, Option<&_>)> = Vec::new();
    if headings.is_empty() {
        segments.push((0, total_len, None));
    } else {
        // Prefix text before the first heading (if any) is its own segment.
        if headings[0].offset > 0 {
            segments.push((0, headings[0].offset, None));
        }
        for (i, h) in headings.iter().enumerate() {
            let end = headings
                .get(i + 1)
                .map(|n| n.offset)
                .unwrap_or(total_len);
            segments.push((h.offset, end, Some(h)));
        }
    }

    let budget_chars = (CHUNK_TOKEN_BUDGET as f32 * TOKEN_CHAR_RATIO) as usize;
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut breadcrumb: Breadcrumb = Vec::new();

    for (start, end, heading) in segments {
        // Update breadcrumb based on this segment's heading (if any). A new
        // heading at level L replaces breadcrumb entries at level >= L.
        if let Some(h) = heading {
            // Drop any breadcrumb entries at this level or deeper, then push.
            while breadcrumb
                .last()
                .map(|(l, _)| (*l as usize) >= (h.level as usize))
                .unwrap_or(false)
            {
                breadcrumb.pop();
            }
            breadcrumb.push((h.level, h.label.clone()));
        }

        let segment_text = &text[start..end];

        // If this segment alone exceeds the budget, split it at paragraph
        // boundaries.
        if segment_text.len() > budget_chars {
            for (sub_start, sub_text) in split_oversized_segment(segment_text, budget_chars) {
                push_chunk(
                    &mut chunks,
                    &breadcrumb,
                    start + sub_start,
                    sub_text.to_string(),
                    total_len,
                    page_count,
                );
            }
        } else {
            push_chunk(
                &mut chunks,
                &breadcrumb,
                start,
                segment_text.to_string(),
                total_len,
                page_count,
            );
        }
    }

    chunks
}

fn push_chunk(
    chunks: &mut Vec<Chunk>,
    breadcrumb: &Breadcrumb,
    offset: usize,
    text: String,
    total_len: usize,
    page_count: u32,
) {
    if text.trim().is_empty() {
        return;
    }
    let (page_start, page_end) = estimate_pages(offset, text.len(), total_len, page_count);
    chunks.push(Chunk {
        text,
        breadcrumb: breadcrumb.clone(),
        offset,
        page_start,
        page_end,
    });
}

fn estimate_pages(offset: usize, len: usize, total: usize, page_count: u32) -> (u32, u32) {
    if total == 0 || page_count == 0 {
        return (1, 1);
    }
    let pc = page_count as f32;
    let start = ((offset as f32 / total as f32) * pc).floor() as u32 + 1;
    let end_off = (offset + len).min(total);
    let end = ((end_off as f32 / total as f32) * pc).ceil() as u32;
    let end = end.max(start).min(page_count);
    (start.max(1).min(page_count), end)
}

/// Split a segment that exceeds the token budget into sub-pieces at the
/// nearest paragraph boundary, falling back to character-cut when no
/// paragraph break exists. Returns `(relative_offset_within_segment, text)`.
fn split_oversized_segment(segment: &str, budget_chars: usize) -> Vec<(usize, &str)> {
    let mut out: Vec<(usize, &str)> = Vec::new();
    let mut cursor = 0usize;

    while cursor < segment.len() {
        let remaining = &segment[cursor..];
        if remaining.len() <= budget_chars {
            out.push((cursor, remaining));
            break;
        }

        // Find the last paragraph break within budget.
        let window = &remaining[..budget_chars.min(remaining.len())];
        let break_at = window
            .rfind("\n\n")
            .map(|p| p + 2)
            .or_else(|| window.rfind('\n').map(|p| p + 1))
            .unwrap_or(budget_chars);
        // Snap to a UTF-8 char boundary so we don't slice mid-character.
        let safe_break = snap_to_char_boundary(remaining, break_at);
        if safe_break == 0 {
            // Pathological: extremely long single line. Hard cut at budget.
            let hard = snap_to_char_boundary(remaining, budget_chars);
            out.push((cursor, &remaining[..hard]));
            cursor += hard;
        } else {
            out.push((cursor, &remaining[..safe_break]));
            cursor += safe_break;
        }
    }
    out
}

fn snap_to_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    let mut i = idx;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::doc_profiles::REGISTRY;

    fn rules_profile() -> &'static DocProfile {
        REGISTRY
            .iter()
            .find(|p| p.doc_type == "rules")
            .copied()
            .unwrap()
    }

    fn generic_profile() -> &'static DocProfile {
        REGISTRY
            .iter()
            .find(|p| p.doc_type == "generic")
            .copied()
            .unwrap()
    }

    #[test]
    fn empty_text_returns_empty() {
        let out = chunk("", rules_profile(), 0);
        assert!(out.is_empty());
    }

    #[test]
    fn no_headings_returns_single_chunk_when_under_budget() {
        let txt = "Just one short paragraph of plain text.";
        let out = chunk(txt, generic_profile(), 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, txt);
        assert!(out[0].breadcrumb.is_empty());
    }

    #[test]
    fn headings_become_chunk_boundaries() {
        let txt = "\
Rule 1. Short title.\n\nBody text for rule one.\n\n\
Rule 2. Definitions.\n\nBody text for rule two.\n\n\
Rule 3. Procedure.\n\nBody text for rule three.\n";
        let out = chunk(txt, rules_profile(), 1);
        assert!(out.len() >= 3, "got {} chunks", out.len());
        // Each chunk should carry a breadcrumb pointing at its rule.
        let breadcrumbs: Vec<_> = out
            .iter()
            .filter(|c| !c.breadcrumb.is_empty())
            .map(|c| c.breadcrumb.last().unwrap().1.clone())
            .collect();
        assert!(
            breadcrumbs.iter().any(|b| b.contains("1") || b.contains("Rule 1")),
            "expected breadcrumb with rule 1, got: {breadcrumbs:?}"
        );
    }

    #[test]
    fn oversized_segment_splits_on_paragraph_boundaries() {
        // One single huge segment of ~50k chars with paragraph breaks every
        // 1000 chars — must produce multiple chunks under the budget.
        let mut s = String::new();
        for _ in 0..50 {
            s.push_str(&"x".repeat(1000));
            s.push_str("\n\n");
        }
        let out = chunk(&s, generic_profile(), 10);
        assert!(out.len() > 1, "oversized segment should split");
        let budget_chars = (CHUNK_TOKEN_BUDGET as f32 * TOKEN_CHAR_RATIO) as usize;
        for c in &out {
            assert!(
                c.text.len() <= budget_chars + 4096,
                "chunk overshoots budget by too much: {} chars",
                c.text.len()
            );
        }
    }

    #[test]
    fn breadcrumb_propagates_across_chunks_within_same_section() {
        // A rule with a paragraph that forces a split.
        let mut body = String::new();
        for _ in 0..40 {
            body.push_str(&"y".repeat(500));
            body.push_str("\n\n");
        }
        let txt = format!("Rule 5. Long rule.\n\n{body}");
        let out = chunk(&txt, rules_profile(), 5);
        assert!(out.len() > 1);
        // All chunks after the heading should carry a breadcrumb pointing at
        // Rule 5.
        let with_crumb: Vec<_> = out
            .iter()
            .skip_while(|c| c.breadcrumb.is_empty())
            .collect();
        assert!(!with_crumb.is_empty());
        for c in &with_crumb {
            assert!(
                c.breadcrumb.iter().any(|(_, label)| label.contains("5")),
                "breadcrumb should mention rule 5: {:?}",
                c.breadcrumb
            );
        }
    }

    #[test]
    fn page_estimate_is_within_range() {
        let mut s = String::new();
        for _ in 0..1000 {
            s.push('a');
        }
        let out = chunk(&s, generic_profile(), 10);
        for c in &out {
            assert!(c.page_start >= 1 && c.page_start <= 10);
            assert!(c.page_end >= c.page_start && c.page_end <= 10);
        }
    }
}
