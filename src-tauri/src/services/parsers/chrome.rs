//! Feature 012 / US1 — upstream page-chrome classification (fixes D3).
//!
//! Page chrome (mastheads, running headers/footers, page markers, contact
//! pages, form fields) must be isolated as a non-provision class **before**
//! tree construction, so it never inherits a provision label and never reaches
//! retrieval as a fake citation. (The downstream `is_boilerplate` flag was
//! proven insufficient live: junk swept under real provisions had already
//! inherited a label, so a label-based flag matched 0/4277.)
//!
//! Generic structural signals ONLY — no per-document keyword lists. In power
//! order: (1) cross-page positional repetition [strongest — kills mastheads /
//! running headers / page numbers], (2) margin geometry, (3) form-field
//! structure. Conservative keep-bias: a block with a provision identity is
//! NEVER chrome; ambiguous blocks stay `Provision` (a missed strip is
//! acceptable; stripping a real provision is silent + unrecoverable).
//!
//! Pure core — no IO. Geometry (`y_band`) is optional: signal 1 works on text
//! alone (so it runs even before `-bbox` geometry is threaded through), and the
//! margin signal activates when geometry is present.

use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Persisted classification of a pre-tree block. Mirrors `chunks.block_class`
/// (migration 003); `Provision` is the default and only retrieval-eligible class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockClass {
    Provision,
    Chrome,
    TableRow,
    /// P1/D5: page-bottom amendment-history apparatus. Removed from the body but
    /// KEPT as a separate, retrievable `block_class='footnote'` chunk.
    Footnote,
}

impl BlockClass {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockClass::Provision => "provision",
            BlockClass::Chrome => "chrome",
            BlockClass::TableRow => "table_row",
            BlockClass::Footnote => "footnote",
        }
    }
}

/// Why a block was classified as chrome (audit trail).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChromeReason {
    Repetition,
    Margin,
    FormField,
    /// P1/D6: a line that is ENTIRELY a bare integer, part of a footer
    /// page-number run that increments across pages.
    PageNumber,
}

/// Whether a chunk's label could be resolved from the hierarchy (US2). Mirrors
/// `chunks.label_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelStatus {
    Resolved,
    Unresolved,
}

impl LabelStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LabelStatus::Resolved => "resolved",
            LabelStatus::Unresolved => "unresolved",
        }
    }
}

/// A pre-tree block presented to the chrome classifier.
#[derive(Debug, Clone)]
pub struct Block {
    /// Verbatim text of the block.
    pub text: String,
    /// 1-based source page.
    pub page: u32,
    /// Normalized vertical position 0.0 (top) .. 1.0 (bottom). `None` when no
    /// geometry is available (text-only pipeline) — the margin signal is then
    /// skipped, repetition still applies.
    pub y_band: Option<f32>,
    /// True when the block carries a provision identity (a detected heading /
    /// real marker). Such a block is never chrome (keep invariant).
    pub has_provision_identity: bool,
}

/// Outcome for one block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    pub class: BlockClass,
    pub reason: Option<ChromeReason>,
}

impl Classification {
    const fn provision() -> Self {
        Classification { class: BlockClass::Provision, reason: None }
    }
    const fn chrome(reason: ChromeReason) -> Self {
        Classification { class: BlockClass::Chrome, reason: Some(reason) }
    }
    const fn footnote() -> Self {
        Classification { class: BlockClass::Footnote, reason: None }
    }
}

/// Max character length for a block to be eligible as chrome. A masthead /
/// header / page marker / form line is short; a real provision paragraph is
/// not. Guards against stripping a genuinely repeated long notice.
const CHROME_MAX_CHARS: usize = 90;

/// Margin bands: top 8% / bottom 8% of the page.
const TOP_MARGIN: f32 = 0.08;
const BOTTOM_MARGIN: f32 = 0.92;

/// Classify every block. `total_pages` scopes the cross-page repetition
/// threshold. Pure; deterministic; order-preserving (output[i] ↔ blocks[i]).
pub fn classify_blocks(blocks: &[Block], total_pages: usize) -> Vec<Classification> {
    let repeated = repeated_across_pages(blocks, total_pages);
    let page_numbers = page_number_blocks(blocks);

    blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            // Running-header banner: a BARE container heading (a `Part`/`Chapter`
            // token with its numeral and nothing else) that recurs across pages
            // is a page banner, not a real heading — strip it. A genuine heading
            // appears once at the start of its span, so it never enters `repeated`
            // (a one-off `PART VI` is kept). Checked BEFORE the keep invariant
            // because such a banner carries a (spurious) provision identity.
            if is_bare_container_heading(&b.text) && repeated.contains(&i) {
                return Classification::chrome(ChromeReason::Repetition);
            }

            // Keep invariant (strongest, overrides every chrome signal): a block
            // that names a provision is content, never chrome.
            if b.has_provision_identity {
                return Classification::provision();
            }

            // P1/D5 — footnote apparatus (amendment-history wording). Isolated
            // BEFORE chrome signals; kept retrievable as a footnote, not chrome.
            if is_footnote_apparatus(&b.text) {
                return Classification::footnote();
            }
            // P1/D6 — footer page-number run (a line that is only a bare integer,
            // several of which increment across pages). Never a regex over numbers
            // *inside* text — only a line that is ENTIRELY one integer.
            if page_numbers.contains(&i) {
                return Classification::chrome(ChromeReason::PageNumber);
            }

            let chars = b.text.chars().count();

            // Signal 1 — cross-page positional repetition (strongest).
            if chars <= CHROME_MAX_CHARS && repeated.contains(&i) {
                return Classification::chrome(ChromeReason::Repetition);
            }
            // Signal 2 — margin geometry (when geometry is present).
            if chars <= CHROME_MAX_CHARS {
                if let Some(y) = b.y_band {
                    if (y < TOP_MARGIN || y > BOTTOM_MARGIN) && !b.text.trim().is_empty() {
                        return Classification::chrome(ChromeReason::Margin);
                    }
                }
            }
            // Signal 3 — form-field / contact structure.
            if is_form_field(&b.text) {
                return Classification::chrome(ChromeReason::FormField);
            }
            Classification::provision()
        })
        .collect()
}

/// Indices of blocks whose normalized text recurs on `>= K` distinct pages.
/// K = max(3, ceil(0.4 * total_pages)) — conservative so a one-off repeat is
/// never chrome. Digits are stripped in the normal form so "Page 3"/"Page 4"
/// and "P a g e 12" collapse to the same key and count as one recurring header.
fn repeated_across_pages(blocks: &[Block], total_pages: usize) -> std::collections::HashSet<usize> {
    use std::collections::HashMap;
    let k = repetition_threshold(total_pages);

    // normalized text -> set of pages it appears on, and the block indices.
    let mut pages_by_key: HashMap<String, std::collections::HashSet<u32>> = HashMap::new();
    let mut idxs_by_key: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, b) in blocks.iter().enumerate() {
        // A bare container heading is allowed into the repetition count despite
        // its provision identity, so a recurring `PART-IV` page banner can be
        // detected as a running header (see `classify_blocks`).
        if b.has_provision_identity && !is_bare_container_heading(&b.text) {
            continue;
        }
        if b.text.chars().count() > CHROME_MAX_CHARS {
            continue;
        }
        let key = normalize_for_repetition(&b.text);
        if key.is_empty() {
            continue;
        }
        pages_by_key.entry(key.clone()).or_default().insert(b.page);
        idxs_by_key.entry(key).or_default().push(i);
    }

    let mut out = std::collections::HashSet::new();
    for (key, pages) in &pages_by_key {
        if pages.len() >= k {
            if let Some(idxs) = idxs_by_key.get(key) {
                out.extend(idxs.iter().copied());
            }
        }
    }
    out
}

fn repetition_threshold(total_pages: usize) -> usize {
    let forty_pct = ((total_pages as f32) * 0.4).ceil() as usize;
    forty_pct.max(3)
}

/// Lowercase, drop digits and punctuation, collapse whitespace. So a running
/// header differing only by page number (and the spaced "P a g e" marker) maps
/// to one key.
fn normalize_for_repetition(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = true; // trims leading space
    for c in text.chars() {
        if c.is_ascii_digit() {
            continue;
        }
        if c.is_alphabetic() {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_space = false;
        } else {
            // any non-alnum (space, punctuation) → a single separator
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        }
    }
    out.trim().to_string()
}

/// True when a line is ENTIRELY a bare integer (≤4 digits) — the shape of a
/// printed footer page number. A clause marker ("(1)") or a legal phrase
/// ("fourteen years", "Article 253") is never a digit-only line, so this never
/// touches real content.
fn is_bare_integer(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && t.len() <= 4 && t.bytes().all(|c| c.is_ascii_digit())
}

/// P1/D6: indices of bare-integer lines that form a footer page-number run.
/// Requires such lines on ≥2 distinct pages (a run that increments) so a lone
/// stray integer is left as content (conservative keep-bias).
fn page_number_blocks(blocks: &[Block]) -> std::collections::HashSet<usize> {
    let candidates: Vec<(usize, u32)> = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| !b.has_provision_identity && is_bare_integer(&b.text))
        .map(|(i, b)| (i, b.page))
        .collect();
    let distinct_pages: std::collections::HashSet<u32> =
        candidates.iter().map(|(_, p)| *p).collect();
    if distinct_pages.len() >= 2 {
        candidates.iter().map(|(i, _)| *i).collect()
    } else {
        std::collections::HashSet::new()
    }
}

/// P1/D5: true when a line is footnote/amendment-history apparatus. Footnotes
/// are intrinsically identified by amendment vocabulary (the only place this
/// vocabulary co-occurs with an authority clause). Conservative: requires an
/// amendment verb AND an instrument/effect token, so ordinary prose that merely
/// says "substituted" is untouched.
fn is_footnote_apparatus(text: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        // The verb may be an abbreviation ending in a dot ("Subs.", "Ins.",
        // "Ord.") or a full word ("inserted", "substituted"). Matching `subs\.`
        // then `\b` is broken: a word boundary cannot fall between the literal
        // `.` and the following space (both non-word), so "Subs. by …" never
        // matched and only lines carrying "w.e.f" slipped through the fallback.
        // Match the bare stem with `\b…\b` and consume an optional trailing dot.
        Regex::new(
            r"(?i)\b(?:subs|ins|ord|added|inserted|substituted|omitted|deleted|repealed|amended)\b\.?.*\b(?:by|ibid|act|ordinance|amendment|order|gazette|w\.e\.f|with effect)\b",
        )
        .unwrap()
    });
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if re.is_match(t) || t.to_lowercase().contains("w.e.f") {
        return true;
    }
    // Marginal amendment-annotation labels (gazette side-notes). On a scanned
    // two-column page these wrap into SHORT fragments that OCR interleaves into
    // the body — "Amendment of", "Omission of section", "section 3 of Sindh",
    // "Act No.XLII", "of 2013." They annotate which provision an amendment
    // touches, so (like other amendment apparatus) they are KEPT as retrievable
    // footnote chunks, not left to splice into provision prose. Matched as
    // SHORT, ^-anchored fragments so an ordinary sentence is never lifted; any
    // misfire lands in the recoverable footnote class, never silently dropped.
    static MARGIN: OnceLock<Regex> = OnceLock::new();
    let margin = MARGIN.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(?:(?:amendment|omission|insertion|substitution|addition|deletion|repeal|re-?numbering|re-?placement)\s+of\b|(?:new\s+)?sections?\s+[0-9ivxlc][0-9a-z-]*\s+of\b|(?:sindh\s+)?act\s+no\.?\s*[ivxlcdm]+\b|of\s*[12][0-9]{3}\.?\s*$)",
        )
        .unwrap()
    });
    if t.chars().count() <= 40 && margin.is_match(t) {
        return true;
    }
    // Editorial / reference footnotes (not amendments): "…see ibid., pp. 2479-2482",
    // "See footnote 6 on page 3, supra". These markers are distinctly footnote
    // apparatus — they never appear in provision prose (a provision cross-ref
    // says "see Article 199", not "see ibid"/"supra"/"op. cit.").
    static REF: OnceLock<Regex> = OnceLock::new();
    let refre = REF.get_or_init(|| {
        Regex::new(r"(?i)\bibid\b|\bsupra\b|\bop\.?\s*cit\b|see footnote\b").unwrap()
    });
    refre.is_match(t)
}

/// True when a line is ENTIRELY a structural container heading — a bare
/// `Part`/`Chapter` token plus its numeral and nothing else (no title, no
/// trailing prose). A genuine such heading appears once at the start of its
/// span; the SAME bare token recurring on many pages is a running-header banner
/// (e.g. a gazette's "PART-IV" page banner). Used only in tandem with the
/// cross-page repetition signal — a one-off bare heading is never stripped, and
/// a heading carrying a title ("PART VI PROVINCIAL ASSEMBLY") is not bare so it
/// is never touched (keep-bias). Roman-numeral OCR slips (IV→"1V") still match.
fn is_bare_container_heading(text: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        Regex::new(r"(?i)^\s*(?:part|chapter)\b[\s.:)\-–—]*[0-9ivxlcdm]+\s*$").unwrap()
    });
    re.is_match(text)
}

/// Structural form-field / contact-line detector — NO keyword list. Catches
/// fill-blanks, dotted leaders, isolated date lines, lone email/URL/phone lines,
/// and all-caps masthead/instruction blocks. Conservative: only fires on short,
/// low-prose lines so a real terse rule is unaffected.
fn is_form_field(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.chars().count() > CHROME_MAX_CHARS {
        return false;
    }

    // Fill-blanks / dotted leaders: 3+ consecutive underscores or dots.
    if has_run(t, '_', 3) || has_run(t, '.', 4) {
        return true;
    }
    // Lone contact tokens: a short line that is essentially one email / URL /
    // phone number (structural shape, not a vocabulary match).
    if is_contact_line(t) {
        return true;
    }
    // Isolated date line ("27th February 1984", "12-03-2019") with little else.
    if is_date_line(t) {
        return true;
    }
    // NOTE: an "all-caps short line" signal was deliberately removed — it
    // over-stripped real all-caps headings (e.g. "PART VI", "CHAPTER-I",
    // "MANAGEMENT AND ADMINISTRATION OF THE AUTHORITY", "FORMS OF
    // COMMUNICATIONS"). Recurring all-caps mastheads ("GOVERNMENT OF SINDH",
    // "NOTIFICATION") are still caught by the stronger cross-page repetition
    // signal; a one-off all-caps heading must be kept (keep-bias).
    false
}

fn has_run(s: &str, ch: char, n: usize) -> bool {
    let mut run = 0usize;
    for c in s.chars() {
        if c == ch {
            run += 1;
            if run >= n {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn is_contact_line(t: &str) -> bool {
    let has_at = t.contains('@') && t.contains('.');
    let has_url = t.contains("http://") || t.contains("https://") || t.contains("www.");
    // phone: a line that is mostly digits / phone punctuation and has >= 7 digits.
    let digits = t.chars().filter(|c| c.is_ascii_digit()).count();
    let phoneish = digits >= 7
        && t.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '(' | ')' | ' ' | '.' ));
    has_at || has_url || phoneish
}

fn is_date_line(t: &str) -> bool {
    let words = t.split_whitespace().count();
    if words > 5 {
        return false;
    }
    // dd-mm-yyyy / dd/mm/yyyy
    let slashy = t.chars().filter(|c| matches!(c, '/' | '-')).count() >= 2
        && t.chars().filter(|c| c.is_ascii_digit()).count() >= 4
        && t.chars().all(|c| c.is_ascii_digit() || matches!(c, '/' | '-' | ' ' | '.'));
    if slashy {
        return true;
    }
    // "<num><ord> <Month> <year>" e.g. "27th February 1984"
    let lower = t.to_lowercase();
    const MONTHS: [&str; 12] = [
        "january", "february", "march", "april", "may", "june", "july", "august",
        "september", "october", "november", "december",
    ];
    let has_month = MONTHS.iter().any(|m| lower.contains(m));
    let has_year = t
        .split(|c: char| !c.is_ascii_digit())
        .any(|d| d.len() == 4 && matches!(d.parse::<u32>(), Ok(y) if (1800..=2099).contains(&y)));
    has_month && has_year && words <= 5
}


// ── Text-pipeline integration (US1 wiring) ───────────────────────────────────

/// A page-chrome block lifted out of the document body, kept (not deleted) so
/// it can be persisted as a separate auditable `chrome` chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromeBlock {
    pub content: String,
    pub page: u32,
}

/// P1/D5: a footnote/amendment-history block lifted out of the body, kept and
/// persisted as a separate, retrievable `block_class='footnote'` chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteBlock {
    pub content: String,
    pub page: u32,
}

/// Result of stripping chrome from a document's normalized text.
pub struct StripResult {
    /// Body text with chrome lines removed, ready for heading detection +
    /// tree-build. Byte offsets are self-consistent (offsets into `cleaned`).
    pub cleaned: String,
    /// The removed chrome blocks (kept for separate persistence).
    pub chrome: Vec<ChromeBlock>,
    /// P1/D5: removed footnote blocks (kept, retrievable, persisted separately).
    pub footnotes: Vec<FootnoteBlock>,
    /// Page offsets `(page, byte_start)` re-aligned to `cleaned` (013/D12).
    /// Chrome removal shifts byte positions, so the caller's original
    /// `page_offsets` no longer map correctly onto the cleaned body; downstream
    /// page-number stamping MUST use these instead.
    pub cleaned_page_offsets: Vec<(u32, usize)>,
}

/// True when a line independently names a provision (strong legal marker or a
/// `N. Title` heading). Such a line is content and must never be stripped —
/// the keep invariant, applied at the text-pipeline boundary where we do not
/// yet have the full heading tree.
fn line_is_provision(line: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        // Case-insensitive + punctuation/dash tolerant so all-caps structural
        // headings ("PART VI", "CHAPTER-I", "SCHEDULE – II") and ordinary
        // markers ("Rule 5", "11. Power") all count as a provision identity.
        Regex::new(
            r"(?i)^\s*(?:(?:section|rule|article|chapter|schedule|clause|sub-clause|part|regulation)\b[\s.:)\-–—]*[0-9ivxlc]|[0-9]+[a-z]?[.)]\s+\S)",
        )
        .unwrap()
    });
    re.is_match(line)
}

/// Which 1-based page a byte offset falls on, per `page_offsets`
/// (`(page, byte_start)`, ascending). Defaults to page 1 when offsets are absent.
pub fn page_for(byte_off: usize, page_offsets: &[(u32, usize)]) -> u32 {
    let mut page = page_offsets.first().map(|(p, _)| *p).unwrap_or(1);
    for (p, start) in page_offsets {
        if *start <= byte_off {
            page = *p;
        } else {
            break;
        }
    }
    page
}

/// Detect page-chrome line-by-line over the normalized text and return the body
/// with chrome removed plus the removed blocks. Generic structural signals only
/// (cross-page repetition, form-field structure); geometry-free so it runs on
/// the text pipeline. Conservative keep-bias — a provision line is never
/// stripped. Removing a repeated masthead that split a provision across a page
/// break also rejoins that provision (desirable).
pub fn strip_chrome(text: &str, page_offsets: &[(u32, usize)]) -> StripResult {
    let total_pages = page_offsets
        .iter()
        .map(|(p, _)| *p as usize)
        .max()
        .unwrap_or(1)
        .max(1);

    // Split keeping newlines so the cleaned text reconstructs faithfully.
    let lines: Vec<&str> = text.split_inclusive('\n').collect();

    // Per-line page + the block model for non-blank lines.
    let mut blocks: Vec<Block> = Vec::new();
    let mut block_line: Vec<usize> = Vec::new();
    // Original byte offset of each line — needed to recompute pages on `cleaned`
    // after chrome lines are dropped (013/D12).
    let mut line_off: Vec<usize> = Vec::with_capacity(lines.len());
    let mut off = 0usize;
    for (i, line) in lines.iter().enumerate() {
        line_off.push(off);
        let trimmed = line.trim_end_matches('\n');
        // A `⟦TBL:n⟧` region marker is never a block, so it can never be
        // classified — or removed — here. It looks EXACTLY like a masthead to
        // signal 1: `normalize_for_repetition` strips digits, so every marker in
        // the document collapses to the one key "tbl" and recurs on page after
        // page. Measured live (2026-08-05): Delegation of Financial Powers has 62
        // tables, its markers were all stripped as repeated chrome, and
        // `strip_table_regions` then found nothing to lift — leaving 105
        // linearised table rows in the body to be minted as provisions. RoB, with
        // 2 tables, stayed under the threshold and came out clean, which is what
        // made the defect look document-specific.
        if !trimmed.trim().is_empty()
            && !crate::services::parsers::table::is_table_marker_line(trimmed)
        {
            let page = page_for(off, page_offsets);
            blocks.push(Block {
                text: trimmed.to_string(),
                page,
                y_band: None,
                has_provision_identity: line_is_provision(trimmed),
            });
            block_line.push(i);
        }
        off += line.len();
    }

    let cls = classify_blocks(&blocks, total_pages);

    // Lines removed from the body: chrome AND footnotes (P1/D5). Footnotes are
    // collected separately so they persist as retrievable `footnote` chunks.
    let mut removed_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut chrome: Vec<ChromeBlock> = Vec::new();
    let mut footnotes: Vec<FootnoteBlock> = Vec::new();
    for (bi, c) in cls.iter().enumerate() {
        let li = block_line[bi];
        match c.class {
            BlockClass::Chrome => {
                removed_lines.insert(li);
                chrome.push(ChromeBlock {
                    content: blocks[bi].text.clone(),
                    page: blocks[bi].page,
                });
            }
            BlockClass::Footnote => {
                removed_lines.insert(li);
                footnotes.push(FootnoteBlock {
                    content: blocks[bi].text.clone(),
                    page: blocks[bi].page,
                });
            }
            _ => {}
        }
    }

    if chrome.is_empty() && footnotes.is_empty() {
        // No lines removed — `cleaned` == `text`, so offsets are unchanged.
        return StripResult {
            cleaned: text.to_string(),
            chrome,
            footnotes,
            cleaned_page_offsets: page_offsets.to_vec(),
        };
    }

    let mut cleaned = String::with_capacity(text.len());
    let mut cleaned_page_offsets: Vec<(u32, usize)> = Vec::new();
    let mut last_page: Option<u32> = None;
    let mut cleaned_off = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if !removed_lines.contains(&i) {
            let page = page_for(line_off[i], page_offsets);
            if last_page != Some(page) {
                cleaned_page_offsets.push((page, cleaned_off));
                last_page = Some(page);
            }
            cleaned.push_str(line);
            cleaned_off += line.len();
        } else {
            // Removing a line must not remove the PAGE BREAK on it. A form feed
            // sits at the start of the first line of each page, and that first
            // line is exactly what a repeated running header is — so stripping
            // headers silently deleted page boundaries.
            //
            // `cleaned_page_offsets` below stays correct either way, but a later
            // stage (`structured_importer::strip_table_regions`) RE-DERIVES the
            // page map from the form feeds still present in the text. Once they
            // were gone the page counter stopped advancing, and every provision
            // after that point kept an earlier page number. Measured on the
            // Manual for Development Projects: 238 real pages collapsed to 149,
            // with 341 chunks stamped p137 whose text carries the printed
            // footers -143- through -169-.
            for _ in line.matches('\u{0c}') {
                cleaned.push('\u{0c}');
                cleaned_off += '\u{0c}'.len_utf8();
            }
        }
    }
    StripResult { cleaned, chrome, footnotes, cleaned_page_offsets }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(text: &str, page: u32) -> Block {
        Block { text: text.to_string(), page, y_band: None, has_provision_identity: false }
    }
    fn provision(text: &str, page: u32) -> Block {
        Block { text: text.to_string(), page, y_band: None, has_provision_identity: true }
    }

    fn classes(blocks: &[Block], pages: usize) -> Vec<BlockClass> {
        classify_blocks(blocks, pages).into_iter().map(|c| c.class).collect()
    }

    #[test]
    fn masthead_repeated_across_pages_is_chrome() {
        // Same masthead on 5 pages → chrome via repetition.
        let blocks: Vec<Block> = (1..=5)
            .map(|p| block("Government of Sindh Services General Administration", p))
            .collect();
        let out = classes(&blocks, 5);
        assert!(out.iter().all(|c| *c == BlockClass::Chrome), "{out:?}");
    }

    #[test]
    fn page_number_markers_collapse_by_digit_strip() {
        // "P a g e 3", "P a g e 4"… differ only by digits → one recurring key.
        let blocks: Vec<Block> = (1..=4)
            .map(|p| block(&format!("P a g e {}", p + 10), p))
            .collect();
        let out = classes(&blocks, 4);
        assert!(out.iter().all(|c| *c == BlockClass::Chrome), "{out:?}");
    }

    #[test]
    fn one_off_short_line_is_not_chrome() {
        // A short line appearing on a single page is NOT repetition-chrome.
        let blocks = vec![block("Short note appearing once", 1)];
        assert_eq!(classes(&blocks, 6), vec![BlockClass::Provision]);
    }

    #[test]
    fn provision_identity_overrides_every_signal() {
        // Even if a provision-bearing block repeats across pages, keep it.
        let blocks: Vec<Block> = (1..=5).map(|p| provision("1. Definitions", p)).collect();
        let out = classes(&blocks, 5);
        assert!(out.iter().all(|c| *c == BlockClass::Provision), "{out:?}");
    }

    #[test]
    fn margin_geometry_short_text_is_chrome() {
        let b = Block { text: "Annual Report 2023".into(), page: 1, y_band: Some(0.97), has_provision_identity: false };
        let out = classify_blocks(&[b], 1);
        assert_eq!(out[0].class, BlockClass::Chrome);
        assert_eq!(out[0].reason, Some(ChromeReason::Margin));
    }

    #[test]
    fn contact_line_is_form_field_chrome() {
        let blocks = vec![
            block("info@spsc.gov.pk", 1),
            block("www.spsc.gov.pk", 1),
            block("042-99200500", 1),
        ];
        let out = classes(&blocks, 1);
        assert!(out.iter().all(|c| *c == BlockClass::Chrome), "{out:?}");
    }

    #[test]
    fn fill_blanks_and_dotted_leaders_are_chrome() {
        let blocks = vec![
            block("Name of candidate: ____________", 1),
            block("Signature .........................", 1),
        ];
        let out = classes(&blocks, 1);
        assert!(out.iter().all(|c| *c == BlockClass::Chrome), "{out:?}");
    }

    #[test]
    fn one_off_all_caps_heading_is_kept_not_chrome() {
        // A standalone all-caps heading must NOT be stripped (real headings like
        // "MANAGEMENT AND ADMINISTRATION OF THE AUTHORITY" were over-stripped).
        let blocks = vec![
            block("MANAGEMENT AND ADMINISTRATION OF THE AUTHORITY", 1),
            block("FORMS OF COMMUNICATIONS", 2),
        ];
        let out = classes(&blocks, 6);
        assert!(out.iter().all(|c| *c == BlockClass::Provision), "{out:?}");
    }

    #[test]
    fn structural_heading_is_provision_identity() {
        // Case-insensitive + dash-tolerant: PART/CHAPTER/SCHEDULE headings are
        // protected even when they appear in a repeated band.
        assert!(line_is_provision("PART VI"));
        assert!(line_is_provision("CHAPTER-I"));
        assert!(line_is_provision("SCHEDULE – II"));
        assert!(line_is_provision("Rule 5. Commencement."));
        assert!(!line_is_provision("GOVERNMENT OF SINDH"));
    }

    #[test]
    fn repeated_all_caps_masthead_still_chrome_via_repetition() {
        // Recurring mastheads remain chrome through the repetition signal.
        let blocks: Vec<Block> = (1..=4).map(|p| block("GOVERNMENT OF SINDH", p)).collect();
        let out = classes(&blocks, 4);
        assert!(out.iter().all(|c| *c == BlockClass::Chrome), "{out:?}");
    }

    #[test]
    fn isolated_date_line_is_chrome() {
        let blocks = vec![block("27th February 1984", 1), block("12-03-2019", 1)];
        let out = classes(&blocks, 1);
        assert!(out.iter().all(|c| *c == BlockClass::Chrome), "{out:?}");
    }

    #[test]
    fn real_provision_prose_is_kept() {
        // A normal substantive rule paragraph — no signal fires.
        let blocks = vec![block(
            "Direct contracting may be resorted to when the goods are available only \
             from a single source and no reasonable alternative exists.",
            1,
        )];
        assert_eq!(classes(&blocks, 10), vec![BlockClass::Provision]);
    }

    #[test]
    fn threshold_scales_with_page_count() {
        assert_eq!(repetition_threshold(2), 3); // floor
        assert_eq!(repetition_threshold(5), 3); // ceil(2.0) = 2 -> floor 3
        assert_eq!(repetition_threshold(20), 8); // ceil(8.0)
    }

    #[test]
    fn strip_chrome_removes_repeated_masthead_keeps_body() {
        // A masthead recurs on each of 4 pages; the rule body appears once.
        let header = "Government of Sindh — Services General Administration\n";
        // Distinct body content per page (not differing only by a digit, which
        // would legitimately collapse under digit-stripped repetition).
        let bodies = [
            "The competent authority may sanction expenditure within limits.\n",
            "Procurement shall follow open competitive bidding ordinarily.\n",
            "Direct contracting is permitted from a single proprietary source.\n",
            "Appeals lie to the higher authority within the prescribed period.\n",
        ];
        let mut text = String::new();
        let mut offsets = Vec::new();
        for (i, body) in bodies.iter().enumerate() {
            offsets.push((i as u32 + 1, text.len()));
            text.push_str(header);
            text.push_str(body);
        }
        let r = strip_chrome(&text, &offsets);
        assert!(!r.cleaned.contains("Services General Administration"), "masthead survived: {:?}", r.cleaned);
        assert!(r.cleaned.contains("competent authority may sanction"), "body lost: {:?}", r.cleaned);
        assert_eq!(r.chrome.len(), 4, "four masthead copies lifted");
    }

    /// Stripping a repeated header must not strip the PAGE BREAK riding on it.
    ///
    /// A form feed sits at the start of the first line of each page, and that
    /// first line is exactly what a running header is — so removing headers used
    /// to delete page boundaries. `cleaned_page_offsets` stayed correct, but
    /// `structured_importer::strip_table_regions` re-derives the page map from
    /// the form feeds still in the text, so the counter froze and every later
    /// provision kept an earlier page number. Measured on the Manual for
    /// Development Projects: 238 real pages collapsed to 149 stamped ones.
    #[test]
    fn strip_chrome_keeps_page_breaks_of_removed_headers() {
        let header = "Government of Sindh — Services General Administration\n";
        let bodies = [
            "The competent authority may sanction expenditure within limits.\n",
            "Procurement shall follow open competitive bidding ordinarily.\n",
            "Direct contracting is permitted from a single proprietary source.\n",
            "Appeals lie to the higher authority within the prescribed period.\n",
        ];
        // Build it the way the extractor does: a form feed introduces each page
        // after the first, so it leads that page's header line.
        let mut text = String::new();
        let mut offsets = Vec::new();
        for (i, body) in bodies.iter().enumerate() {
            if i > 0 {
                text.push('\u{0c}');
            }
            offsets.push((i as u32 + 1, text.len()));
            text.push_str(header);
            text.push_str(body);
        }
        assert_eq!(text.matches('\u{0c}').count(), 3, "fixture should hold 3 page breaks");

        let r = strip_chrome(&text, &offsets);
        assert!(!r.cleaned.contains("Services General Administration"), "masthead survived");
        assert_eq!(
            r.cleaned.matches('\u{0c}').count(),
            3,
            "page breaks were deleted along with the headers: {:?}",
            r.cleaned
        );
        // And the page map re-derived from those form feeds still finds 4 pages —
        // this is the derivation that was silently collapsing.
        let rederived = crate::services::parsers::pdf::pdftotext_page_offsets(&r.cleaned);
        assert_eq!(rederived.len(), 4, "re-derived page map lost pages: {rederived:?}");
    }

    #[test]
    fn strip_chrome_keeps_provision_line_even_if_repeated() {
        // A provision-bearing line is never stripped (keep invariant).
        let line = "Rule 5. Commencement.\n";
        let text = line.repeat(4);
        let offsets: Vec<(u32, usize)> = (0..4).map(|i| (i + 1, i as usize * line.len())).collect();
        let r = strip_chrome(&text, &offsets);
        assert!(r.chrome.is_empty(), "provision line stripped: {:?}", r.chrome);
        assert_eq!(r.cleaned, text);
    }

    #[test]
    fn strip_chrome_no_offsets_treats_as_single_page() {
        // No page geometry → single page → cross-page repetition can't fire;
        // a one-off contact line still goes via the form-field signal.
        let text = "Some real provision text that is long enough to be body content.\ninfo@example.gov.pk\n";
        let r = strip_chrome(text, &[]);
        assert!(r.cleaned.contains("real provision text"));
        assert!(!r.cleaned.contains("info@example.gov.pk"), "contact line not stripped");
    }

    // ---- P1/D6: footer page-number run -------------------------------------

    #[test]
    fn bare_page_number_run_across_pages_is_chrome() {
        // Digit-only lines on distinct pages (a footer page-number run) → chrome.
        let blocks = vec![block("6", 1), block("7", 2), block("8", 3)];
        assert!(classes(&blocks, 3).iter().all(|c| *c == BlockClass::Chrome));
    }

    #[test]
    fn lone_bare_integer_survives_as_content() {
        // T010a (spec edge case): a single bare integer is NOT a run → kept as
        // content, never stripped (conservative keep-bias).
        let blocks = vec![block("14", 1)];
        assert_eq!(classes(&blocks, 1), vec![BlockClass::Provision]);
    }

    #[test]
    fn legal_phrase_with_a_number_is_never_a_page_number() {
        // "fourteen years" and a numeric cross-reference are not digit-only lines.
        let blocks = vec![
            block("imprisonment for fourteen years", 1),
            block("subject to Article 253", 2),
        ];
        assert!(classes(&blocks, 2).iter().all(|c| *c == BlockClass::Provision));
    }

    // ---- P1/D5: footnote apparatus -----------------------------------------

    #[test]
    fn footnote_apparatus_is_classified_footnote() {
        let blocks = vec![
            block("Subs. by the Constitution (Eighteenth Amdt.) Act, 2010, s. 4.", 1),
            block("Ins. by Ordinance XX of 2002, w.e.f. 21st August 2002.", 2),
        ];
        assert!(classes(&blocks, 2).iter().all(|c| *c == BlockClass::Footnote));
    }

    #[test]
    fn footnotes_are_isolated_from_body_but_kept() {
        let text = "8. Laws inconsistent with rights are void.\nSubs. by the Constitution (Eighteenth Amdt.) Act, 2010, s. 4.\n";
        let r = strip_chrome(text, &[(1, 0)]);
        assert!(r.cleaned.contains("Laws inconsistent"), "body lost: {:?}", r.cleaned);
        assert!(!r.cleaned.contains("Eighteenth Amdt"), "footnote left in body: {:?}", r.cleaned);
        assert_eq!(r.footnotes.len(), 1, "footnote not captured: {:?}", r.footnotes);
    }

    #[test]
    fn reference_footnotes_are_classified_footnote() {
        // Editorial/reference footnotes (not amendments) must also be isolated, so
        // their leading number (e.g. footnote #2454) never becomes a provision.
        let blocks = vec![
            block("For the Senate (Finance Committee) Rules, 1973, see ibid., pp. 2479-2482.", 1),
            block("See footnote 6 on page 3, supra", 2),
        ];
        assert!(classes(&blocks, 2).iter().all(|c| *c == BlockClass::Footnote));
        // a real provision cross-reference ("see Article 199") is NOT a footnote
        assert_eq!(
            classes(&[block("Subject to Article 199 the High Court may act.", 1)], 1),
            vec![BlockClass::Provision]
        );
    }

    // ---- running-header banner (bare container heading) --------------------

    #[test]
    fn repeated_bare_part_banner_is_chrome() {
        // A gazette page banner "PART-IV" (OCR'd "PART-1V") alone on its line,
        // recurring on every inner page, is a running header → chrome — even
        // though it carries a (spurious) Part provision identity.
        let blocks: Vec<Block> = (2..=8).map(|p| provision("PART-1V", p)).collect();
        let out = classes(&blocks, 8);
        assert!(out.iter().all(|c| *c == BlockClass::Chrome), "{out:?}");
    }

    #[test]
    fn strip_chrome_repro_paddle_banner() {
        // Reproduce the PaddleOCR doc-04 layout: page 1 has the real "PART VI";
        // pages 2..8 carry the running-header banner "PART-1V" (standalone line)
        // plus the recurring masthead. Built exactly like the paddle path
        // (per-page text, formfeed separators, page_offsets at page starts).
        let pages = [
            "The Sindh Gobernment Gazette\nPART VI\nWHEREAS it is expedient to amend the Act.\n",
            "(648)\nTHE SINDH GOVERNMENT GAZETTE. Nov. 02, 2013\nPART-1V\n2.\nIn the said Act body two.\n",
            "PART-1V\nTHE SINDH GOVERNMENT GAZETTE. Nov. 02, 2013 (649)\nbody three text here.\n",
            "PART-1V\nTHE SINDH GOVERNMENT GAZETTE. Nov. 02, 2013 (650)\nbody four text here.\n",
            "PART-1V\nTHE SINDH GOVERNMENT GAZETTE. Nov. 02, 2013\nbody five text here.\n",
            "PART-1V\nTHE SINDH GOVERNMENT GAZETTE. Nov. 02, 2013 (652)\nbody six text here.\n",
            "PART-1V\nTHE SINDH GOVERNMENT GAZETTE.Nov. 02, 2013 (653)\nbody seven text here.\n",
            "PART-1V\nTHE SINDH GOVERNMENT GAZETTE. Nov. 02, 2013 (654)\nbody eight text here.\n",
        ];
        let mut text = String::new();
        let mut offsets: Vec<(u32, usize)> = Vec::new();
        for (i, p) in pages.iter().enumerate() {
            offsets.push((i as u32 + 1, text.len()));
            text.push_str(p);
            if i + 1 < pages.len() {
                text.push('\u{0c}');
            }
        }
        let r = strip_chrome(&text, &offsets);
        assert!(r.cleaned.contains("PART VI"), "real Part VI lost: {:?}", r.cleaned);
        assert!(
            !r.cleaned.contains("PART-1V"),
            "running-header PART-1V survived strip: {:?}",
            r.cleaned
        );
    }

    #[test]
    fn one_off_part_heading_is_kept() {
        // A real "PART VI" heading appears once → never repeated → kept.
        let blocks = vec![provision("PART VI", 1)];
        assert_eq!(classes(&blocks, 8), vec![BlockClass::Provision]);
    }

    #[test]
    fn part_heading_with_title_is_kept_even_if_repeated() {
        // A heading carrying a title is NOT a bare container token, so the banner
        // rule never touches it (keep-bias) regardless of repetition.
        let blocks: Vec<Block> =
            (1..=5).map(|p| provision("PART VI PROVINCIAL ASSEMBLY OF SINDH", p)).collect();
        let out = classes(&blocks, 5);
        assert!(out.iter().all(|c| *c == BlockClass::Provision), "{out:?}");
    }

    // ---- marginal amendment-annotation fragments ---------------------------

    #[test]
    fn marginal_amendment_annotations_are_footnote() {
        // The shattered side-note fragments OCR interleaves into the body.
        let blocks = vec![
            block("Amendment of", 2),
            block("Omission of section", 2),
            block("section 3 of Sindh", 2),
            block("Act No.XLII", 2),
            block("of 2013.", 2),
            block("of2013.", 2),
            block("Insertion of new", 7),
        ];
        let out = classes(&blocks, 8);
        assert!(out.iter().all(|c| *c == BlockClass::Footnote), "{out:?}");
    }

    #[test]
    fn amendment_body_sentence_is_not_a_margin_note() {
        // The real amendment body (item text) must stay provision, never lifted.
        let blocks = vec![
            block("In the said Act, section 6 shall be omitted.", 2),
            block("In the said Act, in sub-section (4) of section 8, for", 2),
        ];
        let out = classes(&blocks, 8);
        assert!(out.iter().all(|c| *c == BlockClass::Provision), "{out:?}");
    }

    #[test]
    fn ordinary_prose_saying_substituted_is_not_a_footnote() {
        // Guard: an amendment verb alone (no instrument/effect token) stays body.
        let blocks = vec![block("the goods may be substituted at the buyer's option", 1)];
        assert_eq!(classes(&blocks, 1), vec![BlockClass::Provision]);
    }
}
