//! Semantic boundary chunker.
//!
//! Splits text into ≤ 512-token chunks with 64-token overlap.
//! Boundary preference order: structural header (Article/Section) → paragraph
//! → sentence → word.
//!
//! Each chunk carries its structural label: chunks that start with a header
//! paragraph keep it verbatim; chunks that continue an article across an
//! overflow flush receive a synthetic "[Article N cont.]" prefix so the LLM
//! can always ground citations to the correct structural unit.

use crate::db::repositories::chunks::NewChunk;

const MAX_TOKENS: usize = 512;
const OVERLAP_TOKENS: usize = 64;

fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}

pub struct ChunkSpec {
    pub chunk_index: i64,
    pub content: String,
    pub char_start: i64,
    pub char_end: i64,
    pub page_number: Option<i64>,
    pub token_count: i64,
    pub unit_type: String,
    pub unit_label: Option<String>,
    pub unit_path: Option<String>,
    pub parent_index: Option<i64>,
}

impl ChunkSpec {
    pub fn into_new_chunk(self, source_id: &str) -> NewChunk {
        NewChunk {
            source_id: source_id.to_string(),
            chunk_index: self.chunk_index,
            content: self.content,
            char_start: self.char_start,
            char_end: self.char_end,
            page_number: self.page_number,
            token_count: self.token_count,
            parent_chunk_id: None,
            unit_type: "leaf".into(),
            unit_label: None,
            unit_path: None,
            domain_metadata: None,
        }
    }

    /// Hierarchy-aware variant used by the multi-level chunker path (Task 7+).
    /// Carries unit_type/label/path from the spec and accepts the parent's DB id.
    pub fn into_new_chunk_with_parent(
        self,
        source_id: &str,
        parent_chunk_id: Option<String>,
    ) -> NewChunk {
        NewChunk {
            source_id: source_id.to_string(),
            chunk_index: self.chunk_index,
            content: self.content,
            char_start: self.char_start,
            char_end: self.char_end,
            page_number: self.page_number,
            token_count: self.token_count,
            parent_chunk_id,
            unit_type: self.unit_type,
            unit_label: self.unit_label,
            unit_path: self.unit_path,
            domain_metadata: None,
        }
    }
}

/// Split `text` into semantic chunks.
///
/// `page_offsets`: list of `(page_number, char_offset_start)` pairs from the parser.
pub fn chunk(text: &str, page_offsets: &[(u32, usize)]) -> Vec<ChunkSpec> {
    let paragraphs: Vec<(usize, &str)> = split_paragraphs(text);

    let mut chunks: Vec<ChunkSpec> = Vec::new();
    let mut current_text = String::new();
    let mut current_start: usize = 0;
    let mut current_tokens: usize = 0;
    let mut chunk_index: i64 = 0;
    // Label of the most recently seen structural header ("Article 7", "Section 12").
    // Propagated to continuation chunks that start via overlap rather than a header.
    let mut current_article: Option<String> = None;
    // true when the buffer starts with a header paragraph — no continuation
    // prefix is needed at flush time.
    let mut chunk_has_own_header: bool = false;

    for (par_start, par) in &paragraphs {
        if is_toc_paragraph(par) {
            continue;
        }
        let par_tokens = estimate_tokens(par);
        let new_header = detect_structural_header(par);

        // Strong break: a new article/section begins. Flush the current chunk
        // and start the next buffer clean (no overlap) so the article header
        // is always the first paragraph of its chunk.
        if new_header.is_some() && !current_text.is_empty() {
            emit(
                &mut chunks, &mut chunk_index,
                &current_text, current_start, current_tokens,
                &current_article, chunk_has_own_header,
                page_offsets,
            );
            current_text.clear();
            current_tokens = 0;
            chunk_has_own_header = false;
        }

        // Token overflow break (existing logic).
        if current_tokens + par_tokens > MAX_TOKENS && !current_text.is_empty() {
            emit(
                &mut chunks, &mut chunk_index,
                &current_text, current_start, current_tokens,
                &current_article, chunk_has_own_header,
                page_offsets,
            );
            let (ovl, ovl_start) = take_overlap(&current_text, current_start);
            current_text = ovl;
            current_start = ovl_start;
            current_tokens = estimate_tokens(&current_text);
            chunk_has_own_header = false;
        }

        // Track current article label after handling any flush.
        if let Some(ref h) = new_header {
            current_article = Some(h.clone());
        }

        // Paragraph larger than MAX_TOKENS — split on sentences.
        if par_tokens > MAX_TOKENS {
            let sentences = split_sentences(par);
            let mut first_sentence = true;
            for (sent_rel_start, sent) in sentences {
                let sent_start = par_start + sent_rel_start;
                let sent_tokens = estimate_tokens(sent);

                if current_tokens + sent_tokens > MAX_TOKENS && !current_text.is_empty() {
                    emit(
                        &mut chunks, &mut chunk_index,
                        &current_text, current_start, current_tokens,
                        &current_article, chunk_has_own_header,
                        page_offsets,
                    );
                    let (ovl, ovl_start) = take_overlap(&current_text, current_start);
                    current_text = ovl;
                    current_start = ovl_start;
                    current_tokens = estimate_tokens(&current_text);
                    chunk_has_own_header = false;
                }

                if current_text.is_empty() {
                    current_start = sent_start;
                    if first_sentence && new_header.is_some() {
                        chunk_has_own_header = true;
                    }
                } else {
                    current_text.push(' ');
                }
                current_text.push_str(sent);
                current_tokens += sent_tokens;
                first_sentence = false;
            }
        } else {
            if current_text.is_empty() {
                current_start = *par_start;
                if new_header.is_some() {
                    chunk_has_own_header = true;
                }
            } else {
                current_text.push_str("\n\n");
            }
            current_text.push_str(par);
            current_tokens += par_tokens;
        }
    }

    if !current_text.is_empty() {
        emit(
            &mut chunks, &mut chunk_index,
            &current_text, current_start, current_tokens,
            &current_article, chunk_has_own_header,
            page_offsets,
        );
    }

    chunks
}

#[allow(clippy::too_many_arguments)]
fn emit(
    chunks: &mut Vec<ChunkSpec>,
    chunk_index: &mut i64,
    current_text: &str,
    current_start: usize,
    current_tokens: usize,
    current_article: &Option<String>,
    chunk_has_own_header: bool,
    page_offsets: &[(u32, usize)],
) {
    let char_end = current_start + current_text.len();
    let display_content = if !chunk_has_own_header {
        match current_article {
            Some(a) => format!("[{} cont.]\n\n{}", a, current_text),
            None    => current_text.to_string(),
        }
    } else {
        current_text.to_string()
    };
    chunks.push(make_chunk(
        *chunk_index,
        &display_content,
        current_start as i64,
        char_end as i64,
        current_tokens as i64,
        page_offsets,
    ));
    *chunk_index += 1;
}

fn take_overlap(current_text: &str, current_start: usize) -> (String, usize) {
    let char_end = current_start + current_text.len();
    let overlap = overlap_text(current_text, OVERLAP_TOKENS);
    let overlap_start = char_end - overlap.len();
    (overlap, overlap_start)
}

fn make_chunk(
    idx: i64,
    content: &str,
    char_start: i64,
    char_end: i64,
    token_count: i64,
    page_offsets: &[(u32, usize)],
) -> ChunkSpec {
    // Resolve page via the chunk's midpoint offset — more robust than
    // char_start, which can point into overlap that was carried across a
    // page boundary.
    let mid = ((char_start + char_end) / 2) as usize;
    let page_number = resolve_page(mid, page_offsets);
    ChunkSpec {
        chunk_index: idx,
        content: content.to_string(),
        char_start,
        char_end,
        page_number,
        token_count,
        unit_type: "leaf".into(),
        unit_label: None,
        unit_path: None,
        parent_index: None,
    }
}

/// Return the 1-based page number for a given character offset.
fn resolve_page(char_offset: usize, page_offsets: &[(u32, usize)]) -> Option<i64> {
    if page_offsets.is_empty() {
        return None;
    }
    let page = page_offsets
        .iter()
        .rev()
        .find(|(_, start)| *start <= char_offset)
        .map(|(p, _)| *p as i64)
        .unwrap_or(1);
    Some(page)
}

/// Extract the last `target_tokens` worth of text from `s` for overlap.
fn overlap_text(s: &str, target_tokens: usize) -> String {
    let target_chars = target_tokens * 4;
    if s.len() <= target_chars {
        return s.to_string();
    }
    let raw_start = s.len() - target_chars;
    let start = (0..=raw_start).rev().find(|&i| s.is_char_boundary(i)).unwrap_or(0);
    if let Some(boundary) = s[start..].find(' ') {
        s[start + boundary + 1..].to_string()
    } else {
        s[start..].to_string()
    }
}

fn split_paragraphs(text: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start = 0;

    for part in text.split("\n\n") {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            result.push((start, trimmed));
        }
        start += part.len() + 2;
    }
    result
}

/// Detect whether a paragraph starts with a structural header. Returns a
/// short label like `"Article 7"` or `"Section 12"` usable as a citation marker.
///
/// Recognized forms at the very start of the paragraph:
///   - `N.` (1–3 digits, period, space, uppercase letter)  → `"Article N"`
///   - `Article N` / `Section N` / `Chapter N` / `Part N` / `Schedule N`
fn detect_structural_header(par: &str) -> Option<String> {
    let first_line = par.lines().next()?.trim_start();

    // Form 1: "N. Title..." — the Pakistan Constitution style.
    {
        let bytes = first_line.as_bytes();
        let mut i = 0;
        while i < bytes.len() && i < 3 && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i > 0 && i + 2 < bytes.len() && bytes[i] == b'.' {
            // Require ". " or ".\t" followed by an uppercase ASCII letter or
            // bracket (footnote-prefixed headers like "7.  [The State..." exist).
            let after_dot = &first_line[i + 1..];
            let after_ws = after_dot.trim_start_matches(|c: char| c == ' ' || c == '\t');
            let first_char = after_ws.chars().next();
            let looks_like_header = first_char.map_or(false, |c| {
                c.is_ascii_uppercase() || c == '[' || c == '('
            });
            if looks_like_header {
                let num = &first_line[..i];
                return Some(format!("Article {}", num));
            }
        }
    }

    // Form 2: explicit keyword + number.
    for kw in &["Article", "Section", "Chapter", "Part", "Schedule"] {
        if let Some(rest) = strip_prefix_ci(first_line, kw) {
            let rest = rest.trim_start();
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                return Some(format!("{} {}", kw, digits));
            }
        }
    }

    None
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    // `.get()` returns None if `prefix.len()` bytes don't land on a UTF-8
    // char boundary (e.g. the paragraph starts with a multi-byte char like
    // "—"), avoiding the slicing panic that previously crashed the indexer
    // mid-scan on documents containing non-ASCII leaders.
    let head = s.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Detect table-of-contents paragraphs by dot-leader density.
fn is_toc_paragraph(text: &str) -> bool {
    let non_ws: usize = text.chars().filter(|c| !c.is_whitespace()).count();
    if non_ws < 10 {
        return false;
    }
    let dots: usize = text.chars().filter(|&c| c == '.').count();
    (dots * 100 / non_ws) > 15
}

fn split_sentences(text: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start = 0;

    for part in text.split(". ") {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            result.push((start, trimmed));
        }
        start += part.len() + 2;
    }
    result
}

const SECTION_TOKEN_CAP: usize = 1500;
pub const LEAF_SOFT_CAP: usize = 512;

/// Walk the document tree and emit one ChunkSpec per node (multi-granular).
/// Document chunk content = title + first ~1000 chars (rough rollup).
/// Section content = full section text, truncated at SECTION_TOKEN_CAP.
/// Leaf content = section body, split at paragraph boundaries when > LEAF_SOFT_CAP.
pub fn chunk_tree(
    text: &str,
    tree: &crate::services::heading_detector::tree::DocNode,
) -> Vec<ChunkSpec> {
    use crate::services::heading_detector::tree::NodeKind;
    let mut out: Vec<ChunkSpec> = Vec::new();
    let mut next_index: i64 = 0;

    // Document chunk first.
    let doc_label = tree.label.clone();
    let doc_content = build_document_content(text, &doc_label);
    let doc_index = next_index;
    out.push(ChunkSpec {
        chunk_index: next_index,
        content: doc_content.clone(),
        char_start: 0,
        char_end: text.len() as i64,
        page_number: None,
        token_count: estimate_tokens(&doc_content) as i64,
        unit_type: "document".into(),
        unit_label: doc_label.clone(),
        unit_path: Some(String::new()),
        parent_index: None,
    });
    next_index += 1;

    for section in &tree.children {
        match section.kind {
            NodeKind::Leaf => {
                // Two-level fallback: document -> leaf directly (no section node).
                emit_leaves(text, section, doc_index, &mut next_index, &mut out);
            }
            NodeKind::Section => {
                let sec_text_raw = &text[section.byte_start..section.byte_end];
                // Strip TOC/dot-leader paragraphs; if nothing substantive remains
                // (cover/preface/contents pages), skip this section entirely so it
                // can't dominate retrieval with verbatim-title false positives.
                let (cleaned, keep) = strip_toc_paragraphs(sec_text_raw);
                if !keep {
                    continue;
                }
                let sec_text = cleaned.as_str();
                let sec_content = if estimate_tokens(sec_text) > SECTION_TOKEN_CAP {
                    let truncated_chars = SECTION_TOKEN_CAP * 4;
                    let cut = (0..=truncated_chars.min(sec_text.len())).rev()
                        .find(|i| sec_text.is_char_boundary(*i)).unwrap_or(0);
                    format!(
                        "{}\n[…section truncated for embedding; full text in leaf chunks…]",
                        &sec_text[..cut]
                    )
                } else { sec_text.to_string() };
                let sec_index = next_index;
                out.push(ChunkSpec {
                    chunk_index: next_index,
                    content: sec_content.clone(),
                    char_start: section.byte_start as i64,
                    char_end: section.byte_end as i64,
                    page_number: None,
                    token_count: estimate_tokens(&sec_content) as i64,
                    unit_type: "section".into(),
                    unit_label: section.label.clone(),
                    unit_path: Some(section.path.clone()),
                    parent_index: Some(doc_index),
                });
                next_index += 1;
                for leaf in &section.children {
                    emit_leaves(text, leaf, sec_index, &mut next_index, &mut out);
                }
            }
            NodeKind::Document => unreachable!("nested documents not supported"),
        }
    }
    out
}

fn build_document_content(text: &str, label: &Option<String>) -> String {
    // Title only. Body preview was indexed as embedding content and consistently
    // ranked #1 for unrelated queries because PDF front-matter (cover + TOC)
    // contains every article title verbatim — a strong false-positive signal
    // for the cross-encoder. The document chunk's job is hierarchical navigation,
    // not retrieval. See CHANGELOG_DEV.md 2026-05-16.
    let title = label.as_deref().unwrap_or("(untitled)");
    let _ = text;
    title.to_string()
}

/// Filter TOC/dot-leader paragraphs from a body. Returns the cleaned text
/// and a flag indicating whether anything substantive remains (≥80 non-ws chars).
fn strip_toc_paragraphs(body: &str) -> (String, bool) {
    let paras: Vec<(usize, &str)> = split_paragraphs(body);
    let mut out = String::with_capacity(body.len());
    for (_, p) in &paras {
        if is_toc_paragraph(p) { continue; }
        if !out.is_empty() { out.push_str("\n\n"); }
        out.push_str(p);
    }
    let non_ws: usize = out.chars().filter(|c| !c.is_whitespace()).count();
    (out, non_ws >= 80)
}

fn emit_leaves(
    text: &str,
    leaf_node: &crate::services::heading_detector::tree::DocNode,
    parent_index: i64,
    next_index: &mut i64,
    out: &mut Vec<ChunkSpec>,
) {
    let body_raw = &text[leaf_node.byte_start..leaf_node.byte_end];
    // Drop dot-leader TOC paragraphs before sizing/splitting. Cover/contents
    // leaves collapse to <80 non-ws chars and are skipped entirely.
    let (cleaned, keep) = strip_toc_paragraphs(body_raw);
    if !keep {
        return;
    }
    let body = cleaned.as_str();
    if estimate_tokens(body) <= LEAF_SOFT_CAP {
        out.push(ChunkSpec {
            chunk_index: *next_index,
            content: body.to_string(),
            char_start: leaf_node.byte_start as i64,
            char_end: leaf_node.byte_end as i64,
            page_number: None,
            token_count: estimate_tokens(body) as i64,
            unit_type: "leaf".into(),
            unit_label: None,
            unit_path: Some(leaf_node.path.clone()),
            parent_index: Some(parent_index),
        });
        *next_index += 1;
        return;
    }
    let paragraphs: Vec<(usize, &str)> = split_paragraphs(body);
    let mut buf = String::new();
    let mut buf_start = leaf_node.byte_start;
    let mut buf_tokens = 0usize;
    for (rel_start, par) in &paragraphs {
        if is_toc_paragraph(par) { continue; }
        let pt = estimate_tokens(par);

        // ⚠️ A SINGLE paragraph bigger than the cap used to be appended whole and
        // shipped as one chunk: the flush below is guarded by `!buf.is_empty()`,
        // so with an empty buffer an oversized paragraph sailed straight past it.
        // `pdftotext -layout` output often has no blank lines at all, which makes
        // an ENTIRE statute one "paragraph" — that is how the Sindh Water
        // Management Ordinance became a single 125,405-token chunk, past any
        // context window and therefore unretrievable. `chunk_text` has always had
        // this sentence-level fallback; the leaf path did not.
        if pt > LEAF_SOFT_CAP {
            if !buf.is_empty() {
                push_leaf(out, next_index, parent_index, &buf, buf_start, buf_start + buf.len(), &leaf_node.path);
                buf.clear();
                buf_tokens = 0;
            }
            let par_start = leaf_node.byte_start + rel_start;
            for (piece, piece_start) in split_oversized(par, par_start) {
                let plen = piece.len();
                push_leaf(out, next_index, parent_index, &piece, piece_start, piece_start + plen, &leaf_node.path);
            }
            continue;
        }

        if buf_tokens + pt > LEAF_SOFT_CAP && !buf.is_empty() {
            push_leaf(out, next_index, parent_index, &buf, buf_start, buf_start + buf.len(), &leaf_node.path);
            buf.clear();
            buf_tokens = 0;
        }
        if buf.is_empty() { buf_start = leaf_node.byte_start + rel_start; }
        else { buf.push_str("

"); }
        buf.push_str(par);
        buf_tokens += pt;
    }
    if !buf.is_empty() {
        push_leaf(out, next_index, parent_index, &buf, buf_start, buf_start + buf.len(), &leaf_node.path);
    }
}

/// Split one oversized paragraph into <= `LEAF_SOFT_CAP` pieces.
///
/// Sentences first: a sentence boundary is the least damaging place to cut a
/// provision. ⚠️ A sentence can ALSO exceed the cap — a table rendered as one
/// line, or OCR text with no terminators at all — so the word-level backstop is
/// not defensive padding. Without it this function would reproduce the exact bug
/// it exists to fix, one level down.
/// Re-split text that is already stored, for the maintenance repair path.
///
/// Same splitting as indexing uses — deliberately the SAME function, so a
/// repaired chunk is indistinguishable from a freshly indexed one. If these
/// ever diverge, repaired documents start behaving differently from new ones.
pub fn split_oversized_text(text: &str) -> Vec<String> {
    if estimate_tokens(text) <= LEAF_SOFT_CAP {
        return vec![text.to_string()];
    }
    split_oversized(text, 0).into_iter().map(|(piece, _)| piece).collect()
}

fn split_oversized(par: &str, par_start: usize) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut buf = String::new();
    let mut buf_start = par_start;
    let mut buf_tokens = 0usize;

    for (sent_rel, sent) in split_sentences(par) {
        let st = estimate_tokens(sent);
        if st > LEAF_SOFT_CAP {
            if !buf.is_empty() {
                out.push((std::mem::take(&mut buf), buf_start));
                buf_tokens = 0;
            }
            let mut w_buf = String::new();
            let mut w_tokens = 0usize;
            let w_start = par_start + sent_rel;
            for word in sent.split_whitespace() {
                let wt = estimate_tokens(word).max(1);
                if w_tokens + wt > LEAF_SOFT_CAP && !w_buf.is_empty() {
                    out.push((std::mem::take(&mut w_buf), w_start));
                    w_tokens = 0;
                }
                if !w_buf.is_empty() { w_buf.push(' '); }
                w_buf.push_str(word);
                w_tokens += wt;
            }
            if !w_buf.is_empty() { out.push((w_buf, w_start)); }
            continue;
        }
        if buf_tokens + st > LEAF_SOFT_CAP && !buf.is_empty() {
            out.push((std::mem::take(&mut buf), buf_start));
            buf_tokens = 0;
        }
        if buf.is_empty() { buf_start = par_start + sent_rel; }
        else { buf.push(' '); }
        buf.push_str(sent);
        buf_tokens += st;
    }
    if !buf.is_empty() { out.push((buf, buf_start)); }
    out
}

fn push_leaf(
    out: &mut Vec<ChunkSpec>, next_index: &mut i64, parent_index: i64,
    content: &str, start: usize, end: usize, path: &str,
) {
    out.push(ChunkSpec {
        chunk_index: *next_index,
        content: content.to_string(),
        char_start: start as i64,
        char_end: end as i64,
        page_number: None,
        token_count: estimate_tokens(content) as i64,
        unit_type: "leaf".into(),
        unit_label: None,
        unit_path: Some(path.to_string()),
        parent_index: Some(parent_index),
    });
    *next_index += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: ONE paragraph larger than the cap must still be split.
    ///
    /// This is the shape that produced a single 125,405-token chunk in
    /// production (`SINDH_WATER_MANAGEMENT_ORDINANCE`): `pdftotext -layout`
    /// output with no blank lines, so the whole statute is one "paragraph".
    /// The old loop appended it whole because its flush was guarded by
    /// `!buf.is_empty()`.
    #[test]
    fn oversized_single_paragraph_is_split() {
        let sentence = "The competent authority shall record reasons in writing. ";
        let par = sentence.repeat(400); // ~2,800 tokens, no blank lines at all
        assert!(estimate_tokens(&par) > LEAF_SOFT_CAP * 4, "test input too small");

        let pieces = split_oversized(&par, 0);
        assert!(pieces.len() > 1, "oversized paragraph was not split");
        for (piece, _) in &pieces {
            assert!(
                estimate_tokens(piece) <= LEAF_SOFT_CAP,
                "piece of {} tokens exceeds the {LEAF_SOFT_CAP} cap",
                estimate_tokens(piece)
            );
        }
    }

    /// The word-level backstop: text with NO sentence terminators (OCR output,
    /// a table flattened to one line) must still come back inside the cap.
    #[test]
    fn oversized_text_without_sentence_boundaries_is_still_split() {
        let par = "alpha ".repeat(3000); // no '.', '?' or '!' anywhere
        let pieces = split_oversized(&par, 0);
        assert!(pieces.len() > 1, "unpunctuated text was not split");
        for (piece, _) in &pieces {
            assert!(
                estimate_tokens(piece) <= LEAF_SOFT_CAP,
                "piece of {} tokens exceeds the cap",
                estimate_tokens(piece)
            );
        }
    }

    #[test]
    fn detects_n_dot_title_header() {
        assert_eq!(
            detect_structural_header("7. Definition of the State.- The State means..."),
            Some("Article 7".to_string())
        );
        assert_eq!(
            detect_structural_header("32. Promotion of local Government institutions"),
            Some("Article 32".to_string())
        );
    }

    #[test]
    fn detects_keyword_form() {
        assert_eq!(
            detect_structural_header("Article 91. The Federal Government"),
            Some("Article 91".to_string())
        );
        assert_eq!(
            detect_structural_header("Section 3 Application"),
            Some("Section 3".to_string())
        );
    }

    #[test]
    fn ignores_non_headers() {
        assert_eq!(detect_structural_header("The State means the Federal Government"), None);
        assert_eq!(detect_structural_header("1.2.3 version string"), None);
    }

    #[test]
    fn continuation_prefix_applied_on_overflow() {
        // Realistic layout: short article header paragraph followed by many
        // body paragraphs from the same article — forces a mid-article flush.
        let header = "7. Definition of the State.- The State means the Federal Government.";
        let body_para = "Further, the State shall ensure justice for its citizens.";
        let mut paragraphs = vec![header.to_string()];
        for _ in 0..40 {
            paragraphs.push(body_para.to_string());
        }
        let text = paragraphs.join("\n\n");
        let chunks = chunk(&text, &[(1, 0)]);

        assert!(chunks.len() >= 2, "expected multi-chunk output, got {}", chunks.len());
        assert!(
            chunks[0].content.starts_with("7. Definition of the State"),
            "first chunk should start with the article header, got: {:?}",
            &chunks[0].content[..chunks[0].content.len().min(60)]
        );
        assert!(
            chunks[1].content.starts_with("[Article 7 cont.]"),
            "continuation chunk should carry the article label, got: {:?}",
            &chunks[1].content[..chunks[1].content.len().min(60)]
        );
    }

    #[test]
    fn strong_break_on_new_article_boundary() {
        // Two short articles back-to-back must produce two chunks, not one
        // packed chunk — so article headers are never orphaned from their body.
        let text = "\
6. Safeguard against discrimination. No citizen shall be discriminated.

7. Definition of the State. The State means the Federal Government.

8. Laws inconsistent with rights. Any law inconsistent shall be void.";
        let chunks = chunk(text, &[(1, 0)]);

        assert!(chunks.len() >= 3, "expected 3 chunks for 3 articles, got {}", chunks.len());
        assert!(chunks[0].content.starts_with("6."), "chunk 0: {:?}", chunks[0].content);
        assert!(chunks[1].content.starts_with("7."), "chunk 1: {:?}", chunks[1].content);
        assert!(chunks[2].content.starts_with("8."), "chunk 2: {:?}", chunks[2].content);
    }
}
