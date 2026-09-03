//! Structure-aware parser (feature 003, task T011): turns the S1
//! `ConstitutionIr` into an L0–L5 `DocNode` tree, driven by a `DocProfile`.
//!
//! Provisos and explanations are attached **in-leaf** to their parent
//! clause/sub-clause (or Article, for flat articles) — never modelled as
//! standalone nodes (design decision D3). This keeps a negation-bearing
//! proviso physically inside the unit it qualifies.

use anyhow::Result;
use regex_lite::Regex;
use serde::Serialize;
use std::sync::OnceLock;
use crate::db::Db;
use uuid::Uuid;

use super::constitution_ir::{
    Article, Chapter, Clause, ConstitutionIr, Explanation, Part, Proviso, SubClause,
};
use super::doc_profiles::DocProfile;
use crate::db::repositories::chunks::{self, Chunk, NewChunk};

/// Depth of a node in the hierarchy: L0 `Document` … L5 `SubClause`, plus the
/// L6 `Item` for a roman list nested INSIDE an alpha sub-clause — the SPPRA
/// shape `Rule 16(1)(a)(ii)` / `Rule 2(1)(q)(iii)` (nesting audit 2026-07-02:
/// romans were previously flattened to L5 siblings, colliding on `(1)/(i)` and
/// losing their parent clause). The discriminant doubles as the index into
/// `DocProfile.level_vocab` (L6 reuses the L5 name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum NodeLevel {
    Document = 0,
    Part = 1,
    Chapter = 2,
    Article = 3,
    Clause = 4,
    SubClause = 5,
    Item = 6,
}

impl NodeLevel {
    /// The profile-defined name for this level (e.g. `"article"`). `Item` has
    /// no vocab slot of its own — it displays under the L5 name; the extra
    /// depth is expressed by `unit_path`/citation, not by the level word.
    pub fn name(self, profile: &DocProfile) -> &'static str {
        profile.level_vocab[(self as usize).min(NodeLevel::SubClause as usize)]
    }

    /// Canonical `chunks.unit_type` value for this level — the fixed storage
    /// vocabulary set by the migration 013 CHECK, independent of any profile's
    /// display vocabulary. `Item` persists as `subclause` (the CHECK has no
    /// deeper name); its real depth lives in `unit_path`.
    pub fn unit_type(self) -> &'static str {
        match self {
            NodeLevel::Document => "document",
            NodeLevel::Part => "part",
            NodeLevel::Chapter => "chapter",
            NodeLevel::Article => "article",
            NodeLevel::Clause => "clause",
            NodeLevel::SubClause | NodeLevel::Item => "subclause",
        }
    }
}

/// A proviso or explanation carried in-leaf by its parent node.
#[derive(Debug, Clone, Serialize)]
pub struct Attached {
    pub kind: AttachedKind,
    /// Explanation label (`"I"`, `"II"`); empty for a proviso.
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AttachedKind {
    Proviso,
    Explanation,
}

/// A node of the L0–L5 hierarchy tree.
#[derive(Debug, Clone, Serialize)]
pub struct DocNode {
    pub level: NodeLevel,
    /// Label as printed: `"II"`, `"1A"`, `"19A"`, `"(1)"`, `"(a)"`. Empty for
    /// the document root.
    pub label: String,
    /// Part/Chapter title, or an Article's marginal note.
    pub title: Option<String>,
    /// The node's own text — chapeau for an Article/Clause, full text for a
    /// sub-clause leaf. Empty for structural levels (document/part/chapter).
    pub text: String,
    /// Human citation, e.g. `"Article 19A(1)(a)"`.
    pub citation: String,
    /// Canonical citation path, e.g. `"PartII/Ch1/Art-19A/cl-(1)/sub-(a)"`.
    pub unit_path: String,
    /// Provisos/explanations attached in-leaf (D3).
    pub attached: Vec<Attached>,
    /// Footnote ids (amendment markers / editorial notes) anchored in this node.
    pub footnote_refs: Vec<String>,
    pub children: Vec<DocNode>,
}

impl DocNode {
    /// Count of descendant nodes (including self) at the given level.
    pub fn count(&self, level: NodeLevel) -> usize {
        let here = usize::from(self.level == level);
        here + self.children.iter().map(|c| c.count(level)).sum::<usize>()
    }

    /// Maximum depth of this subtree (self counts as 1).
    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(DocNode::depth).max().unwrap_or(0)
    }

    /// First node in the subtree whose `citation` equals `citation`.
    pub fn find(&self, citation: &str) -> Option<&DocNode> {
        if self.citation == citation {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(citation))
    }
}

/// Build the L0–L5 `DocNode` tree from the S1 IR.
pub fn parse_tree(ir: &ConstitutionIr, profile: &DocProfile) -> DocNode {
    // The Preamble lives as a top-level node alongside Parts so it is
    // chunked, embedded, FTS-indexed, and reachable by the citation
    // fast-path. It was silently dropped before T028 fix: the IR carried
    // `ir.preamble` but `parse_tree` ignored it, so facts only stated in the
    // Preamble (e.g. "Founder of Pakistan, Quaid-i-Azam Mohammad Ali Jinnah")
    // had no retrievable chunk. Reused `NodeLevel::Article` to avoid a
    // schema/enum change — Preamble behaves citation-equivalently to an
    // article (no clauses, no sub-tree).
    let mut children: Vec<DocNode> = Vec::new();
    if let Some(text) = ir
        .preamble
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        children.push(build_preamble(text));
    }
    children.extend(ir.parts.iter().map(|p| build_part(p, profile)));
    let root = DocNode {
        level: NodeLevel::Document,
        label: String::new(),
        title: Some(ir.title.clone()),
        text: String::new(),
        citation: ir.title.clone(),
        unit_path: "document".to_string(),
        attached: Vec::new(),
        footnote_refs: Vec::new(),
        children,
    };
    // Apply the generic long-leaf splitter. The 010 coalescing pass is NOT
    // applied here: `parse_tree` ingests the pre-structured constitution IR,
    // which is clean (not OCR-fragmented) and must not be re-merged (SC-005).
    // Coalescing runs only on the heading-detected path (`build_tree_from_headings`),
    // which is where the fragmentation originates.
    split_oversized_leaves(root, long_chunk_threshold())
}

/// Threshold above which a single chunk is split into paragraph-sized
/// children at indexing time. Default 500 chars — sized so retrieved
/// chunks fit comfortably in local Qwen 0.5B/1.5B context (n_ctx≈3200)
/// alongside TOP_K=12 neighbours. Lowered from 1000 after the fresh-DB
/// audit showed APT Rule 12(3)=14k and E&D Rule_14=7.8k chunks (the
/// internal-node leakage fixed in `split_oversized_leaves`).
///
/// Override via `ECHO_LONG_CHUNK_THRESHOLD` for A/B tuning without a
/// recompile. Smaller threshold = more splits, finer-grained vector +
/// FTS5 recall at the cost of more chunks.
fn long_chunk_threshold() -> usize {
    std::env::var("ECHO_LONG_CHUNK_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &usize| n >= 100) // floor: 100 chars min, prevents pathological splits
        // 2026-06: raised 500→1500. A 500-char ceiling shredded ordinary legal
        // provisions (CDWP §2.6 ~900c) into 3-4 chunks and severed numbers from
        // their lead-in ("Rs." | "10,000 million"). 1500c (~375 tok) keeps a
        // whole provision — subject + figure + qualifier — in ONE chunk so
        // retrieval can't grab the subject without the value. Only genuinely
        // oversized bodies still split (and Fix A keeps numbers intact there).
        .unwrap_or(1500)
}

/// RC-D ceiling: a structural container (Document/Part/Chapter) normally holds
/// only a short enumerated child-index (a few hundred chars) which `split_
/// oversized_leaves` deliberately leaves intact. Above this ceiling the text is
/// plainly NOT a clean index — the live audit (2026-06-29) found 35k–42k-char
/// container/internal-node bodies (a table or absorbed provision wrongly hung
/// off the container) shipping as ONE uncitable chunk. Such a body is split into
/// citable `… part N` pieces. 8000c (~2000 tok) sits far above any real index
/// yet well below the pathological bodies. Override via `ECHO_OVERSIZE_CEILING`.
fn oversize_hard_ceiling() -> usize {
    std::env::var("ECHO_OVERSIZE_CEILING")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &usize| n >= 1000) // never below the normal leaf threshold band
        .unwrap_or(8000)
}

/// Apply the tree's oversize policy to a standalone block body (front-matter /
/// back-matter) that is persisted OUTSIDE the DocNode tree and therefore never
/// passes through `split_oversized_leaves`. A TOC / schedule / annex that fits
/// under `oversize_hard_ceiling()` is kept whole (its structure is the point);
/// a pathological body above the ceiling (e.g. the constitution TOC at 42,658c,
/// schedules at 17,708c) is split into context-sized `≤ long_chunk_threshold()`
/// pieces instead of shipping one uncitable 40k blob. Always returns ≥1 piece.
pub(crate) fn split_block_for_persist(content: &str) -> Vec<String> {
    if content.chars().count() <= oversize_hard_ceiling() {
        return vec![content.to_string()];
    }
    split_long_text(content, long_chunk_threshold())
}

/// Split `text` into focused pieces when it exceeds `threshold` chars.
/// Tries semantic boundaries in order:
///   1. paragraphs (split on `\n\n`)
///   2. sentences  (split on `. `, `? `, `! `)
///   3. character-cut at `threshold` (last resort, may break a sentence)
///
/// Always returns at least one piece. Returns the input unchanged when
/// it fits in `threshold` chars. Empty / whitespace-only pieces dropped.
///
/// Generic over chunk kind — applies wherever long-text dilution hurts
/// retrieval (Preamble today; any future doc with a long opening).
pub(crate) fn split_long_text(text: &str, threshold: usize) -> Vec<String> {
    if text.chars().count() <= threshold {
        return vec![text.to_string()];
    }
    // Recursively enforce the threshold on every piece a semantic split yields.
    // 2026-06-30 (oversize leak fix): branches 0/1/2 previously returned their
    // pieces AS-IS — only the final char-cut (branch 3) honoured `threshold`.
    // So a body with ≥2 paragraphs where ONE paragraph was itself 35k (an OCR
    // run / bullet list with no internal \n\n or sentence break) shipped that
    // 35k piece whole (live defect: Manual `Section_30/p1` = 35,604c). Re-split
    // any piece still over `threshold`; each recursion operates on a strictly
    // smaller slice and ultimately bottoms out at the char-cut, so it always
    // terminates with pieces ≤ `threshold`.
    fn enforce(pieces: Vec<String>, threshold: usize) -> Vec<String> {
        pieces
            .into_iter()
            .flat_map(|p| {
                if p.chars().count() > threshold {
                    split_long_text(&p, threshold)
                } else {
                    vec![p]
                }
            })
            .collect()
    }
    // 0. Sub-clause marker split (013/D7+D8) — when a provision body carries
    //    line-start legal markers "(2)", "(a)", "(i)" (tolerating an amendment
    //    bracket "2\n[(b) …"), split there FIRST so a sub-clause never swallows
    //    the next. Preferred over blank-line paragraphs because legal sub-clauses
    //    are frequently NOT \n\n-separated (e.g. "(a) …; or" runs straight into
    //    "[(b) …"). Each piece stays a verbatim contiguous slice (G1 holds).
    let markers = split_at_clause_markers(text);
    if markers.len() >= 2 {
        return enforce(markers, threshold);
    }
    // 1. Paragraph split — works for the Preamble (14 paragraphs separated by \n\n).
    let paragraphs: Vec<String> = text
        .split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if paragraphs.len() >= 2 {
        return enforce(paragraphs, threshold);
    }
    // 2. Sentence split — single long paragraph; cut on `.` `?` `!` followed by
    //    whitespace. ABBREVIATION/NUMBER-AWARE (2026-06): a `.` does NOT end a
    //    sentence when the next non-space char is a DIGIT (so "Rs. 10,000 million",
    //    "Section 2. 5", decimals stay joined) or when the token ending at the `.`
    //    is a known abbreviation (Rs/No/Sec/Art/Annex/i.e/e.g/viz/vs…). This fixes
    //    the live defect where "…costing upto Rs. 10,000 million" was severed into
    //    "…upto Rs." | "10,000 million…", stranding the operative figure in a
    //    separate chunk (CDWP §2.6 → ECNEC).
    let mut sentences: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        current.push(chars[i]);
        let mut ends_sentence = matches!(chars[i], '.' | '?' | '!')
            && chars.get(i + 1).is_some_and(|c| c.is_whitespace());
        if ends_sentence && chars[i] == '.' {
            let next_non_ws = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if next_non_ws.is_some_and(|c| c.is_ascii_digit()) || ends_with_abbreviation(&current) {
                ends_sentence = false;
            }
        }
        if ends_sentence {
            let s = current.trim().to_string();
            if !s.is_empty() { sentences.push(s); }
            current.clear();
        }
    }
    let tail = current.trim().to_string();
    if !tail.is_empty() { sentences.push(tail); }
    if sentences.len() >= 2 {
        return enforce(sentences, threshold);
    }
    // 3. Character-cut — single sentence longer than threshold (rare).
    chars
        .chunks(threshold)
        .map(|c| c.iter().collect::<String>())
        .filter(|s| !s.trim().is_empty())
        .collect()
}

/// True when `buf` (ends in '.') closes on a known abbreviation whose period is
/// NOT a sentence terminator — currency/enumeration/reference shorthands and
/// initials pervasive in legal text. Paired with the "next char is a digit"
/// rule in `split_long_text`, this stops the sentence splitter from severing a
/// provision (e.g. "Rs. 10,000 million", "i.e. the Board", "Annex. II").
fn ends_with_abbreviation(buf: &str) -> bool {
    let trimmed = buf.trim_end().trim_end_matches('.');
    // Last alphabetic run immediately preceding the trailing '.'.
    let last_word: String = trimmed
        .chars()
        .rev()
        .take_while(|c| c.is_alphabetic())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if last_word.is_empty() {
        return false;
    }
    // A single capital letter is an initial ("Mr. A. Khan", "S. No.").
    if last_word.chars().count() == 1 && last_word.chars().all(|c| c.is_ascii_uppercase()) {
        return true;
    }
    const ABBREVS: &[&str] = &[
        "rs", "no", "nos", "etc", "viz", "vs", "annex", "annexure", "sr", "mr",
        "mrs", "ms", "dr", "govt", "ltd", "co", "sec", "art", "cl", "para",
        "pp", "vol", "fig", "approx", "rev", "ed", "sch", "ch", "addl", "asstt",
        "i", "e", "g",
    ];
    ABBREVS.contains(&last_word.to_ascii_lowercase().as_str())
}

/// 013/D7+D8 — split a provision body at line-start sub-clause markers
/// (`(2)`, `(a)`, `(i)`), the deterministic boundary the blank-line splitter
/// misses. Tolerates a leading amendment bracket (the live `2\n[(b) …` shape)
/// and leading whitespace, so an amendment-inserted sub-clause opens its own
/// piece instead of being absorbed by the previous one. The inserted text and
/// its brackets are preserved verbatim — pieces are contiguous slices of `text`,
/// so the verbatim-substring invariant (G1) holds. Any lead-in before the first
/// marker (catchline + clause `(1)`, which sits on the article-number line, not
/// at a line start) becomes the first piece. Returns a single-element vec when
/// fewer than two markers are present (caller falls back to paragraph/sentence).
fn split_at_clause_markers(text: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    // `^[ \t]*` line start, optional `[` amendment bracket, then a parenthesised
    // ordinal/letter/roman marker. A mid-line "clause (3)" reference never
    // matches (not at line start); the article's own `(1)` stays with the lead.
    let re = RE.get_or_init(|| {
        Regex::new(r"(?m)^[ \t]*\[?\((?:[0-9]{1,3}|[a-z]|[ivxlc]{1,4})\)").unwrap()
    });
    let starts: Vec<usize> = re.find_iter(text).map(|m| m.start()).collect();
    if starts.len() < 2 {
        return vec![text.to_string()];
    }
    let mut pieces: Vec<String> = Vec::new();
    if starts[0] > 0 {
        let lead = text[..starts[0]].trim();
        if !lead.is_empty() {
            pieces.push(lead.to_string());
        }
    }
    for (i, &s) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(text.len());
        let piece = text.get(s..end).unwrap_or("").trim();
        if !piece.is_empty() {
            pieces.push(piece.to_string());
        }
    }
    pieces
}

/// Wrap the IR's preamble text as a single article-level leaf with the
/// canonical citation `"Preamble"` and top-level `unit_path` `"Preamble"`
/// (a sibling of Parts, not nested inside one).
///
/// Splitting of long content is **not** done here — it is a separate,
/// generic tree-level transform (`split_oversized_leaves`) applied at
/// the end of `parse_tree`. This function knows nothing about chunking
/// thresholds or paragraph boundaries; it just constructs the node the
/// IR describes.
fn build_preamble(text: &str) -> DocNode {
    DocNode {
        level: NodeLevel::Article,
        label: "Preamble".to_string(),
        title: None,
        text: text.to_string(),
        citation: "Preamble".to_string(),
        unit_path: "Preamble".to_string(),
        attached: Vec::new(),
        footnote_refs: Vec::new(),
        children: Vec::new(),
    }
}

/// Idea 1 (F5-imperative fix, 2026-05-23) — generic tree transform.
///
/// Walks the tree post-order. For any node whose own `text` exceeds
/// `threshold` chars, splits the text via `split_long_text` and appends
/// paragraph children (one per part), then empties the parent's own text
/// so its persisted body becomes just the heading.
///
/// Applies to BOTH true leaves AND internal nodes carrying long own-text
/// (e.g. APT Rule 12(3) where annexure paragraphs bled into a clause that
/// also had detected sub-clauses (i)/(a)/(b)). Splitting only leaves
/// missed those cases and let 14k-char chunks through.
///
/// Paragraph children are APPENDED after any existing structural children
/// so detected sub-clauses keep their place at the front of `chunk_index`
/// order; paragraph-fallback content (typically the leftover the profile
/// couldn't classify) follows.
///
/// **Doc-agnostic** — does not know about "Preamble" or any specific
/// label. Children derive their label / citation / unit_path entirely
/// from the parent node:
///   - `label`           = `format!("{} part {N}", parent.label)`
///   - `citation`        = parent.citation (unchanged)
///   - `unit_path`       = `format!("{}/p{N}", parent.unit_path)`
///   - `level`           = `NodeLevel::Clause` (sub-unit of an article)
///
/// Same `unit_label` across parent + children means the citation
/// fast-path and salience boost keep applying. Same `unit_path` prefix
/// keeps the `LIKE 'X/%'` sub-tree lookup intact.
fn split_oversized_leaves(mut node: DocNode, threshold: usize) -> DocNode {
    // Post-order: transform children first.
    node.children = node
        .children
        .into_iter()
        .map(|c| split_oversized_leaves(c, threshold))
        .collect();

    // 2026-06-03 Fix B: structural containers (Document / Part / Chapter)
    // now carry an enumerated index of their children as `text`. That text
    // is the entire point of the chunk — it must NOT be lifted into Clause
    // children. The original splitter targeted oversized Article/Rule body
    // text (E&D Rule_14 at 7.8k chars). Container-level text never has
    // that pathology — caps at a few hundred chars for the largest
    // chapters — so the splitter's invariant ("split long leaves into
    // paragraph-clause children") doesn't apply.
    if matches!(
        node.level,
        NodeLevel::Document | NodeLevel::Part | NodeLevel::Chapter
    ) {
        // RC-D (2026-06-30): keep a normal short child-index intact, but a
        // pathologically large container body (≫ index size) is a table or
        // absorbed provision wrongly attached here — fall through and split it
        // into citable `… part N` pieces rather than ship one 40k uncitable
        // chunk. Below the ceiling the Fix-B index invariant is preserved.
        if node.text.chars().count() <= oversize_hard_ceiling() {
            return node;
        }
    }

    if node.text.chars().count() <= threshold {
        return node;
    }
    let parts = split_long_text(&node.text, threshold);
    if parts.len() < 2 {
        // split_long_text decided the text doesn't split usefully — keep as-is.
        return node;
    }
    let para_children: Vec<DocNode> = parts
        .into_iter()
        .enumerate()
        .map(|(i, part)| DocNode {
            level: NodeLevel::Clause,
            label: format!("{} part {}", node.label, i + 1),
            title: None,
            text: part,
            citation: node.citation.clone(),
            unit_path: format!("{}/p{}", node.unit_path, i + 1),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: Vec::new(),
        })
        .collect();
    // Parent becomes a citation marker: empty body, persist renders just
    // the heading; full content lives in the paragraph children. Append
    // after any existing structural sub-clauses so chunk_index order
    // remains: detected children first, paragraph fallback after.
    node.text = String::new();
    node.children.extend(para_children);
    node
}

// ── 010 re-chunk extension — coalesce undersized leaf fragments ─────────────

/// Coalescing thresholds (010 re-chunk extension, FR-001/002/003).
///
/// `min` — a leaf shorter than this is treated as a fragmentation scrap. The
/// live corpus had a 41-char median (55% < 50c): provisions were shredded one
/// OCR/marker line per chunk (e.g. the SPPRA Direct-Contracting rule existed
/// only as "(1) Direct Contracting" + scattered scraps). `min` is deliberately
/// LOW (120, not the ~300 the spec floated): legitimate short clauses (the
/// Constitution) cluster ~80–300c, so a higher `min` would merge real distinct
/// clauses and regress SC-005. Only RUNS of ≥2 consecutive scrap leaves merge;
/// an isolated short clause and any leaf ≥ `min` are left untouched.
///
/// `target`/`hard_max` — merged units accumulate up to `target` and never
/// exceed `hard_max`. Override via env for A/B tuning without a recompile.
fn coalesce_params() -> (usize, usize, usize) {
    let env = |k: &str, d: usize| {
        std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(d)
    };
    (
        env("ECHO_COALESCE_MIN", 120),
        // 2026-06: target 600→900, hard_max 800→1500 to match the raised split
        // threshold — merged scrap-runs grow to provision size instead of
        // stopping at ~600c. MIN stays 120 (a higher floor would merge real
        // distinct Constitution clauses and regress SC-005).
        env("ECHO_COALESCE_TARGET", 900),
        env("ECHO_COALESCE_HARD_MAX", 1500),
    )
}

/// One chunking path: split oversized leaves, THEN coalesce undersized ones.
/// Every tree-finalize site routes through here so split-then-coalesce is
/// applied uniformly (no third chunking path).
fn finalize_tree(root: DocNode) -> DocNode {
    let split = split_oversized_leaves(root, long_chunk_threshold());
    let (min, target, hard_max) = coalesce_params();
    coalesce_undersized_leaves(split, min, target, hard_max)
}

fn is_structural_level(level: NodeLevel) -> bool {
    matches!(
        level,
        NodeLevel::Document | NodeLevel::Part | NodeLevel::Chapter
    )
}

/// A leaf is a fragmentation "scrap" if it is a true leaf (no children), not a
/// structural container, carries no attached provisos/explanations, and its
/// own text is under `min` chars.
///
/// Discrimination is by SIZE, not by whether the leaf is a `/p{N}` paragraph
/// child of `split_oversized_leaves`. The live re-index proved the dominant
/// Sindh fragmentation IS over-split `/p` fragments: OCR text full of newline
/// breaks gets chopped into dozens of ~20–60c paragraph children (e.g. the
/// Direct-Contracting rule at `Part_II/(1)/(d)/p1..p26`). A substantial
/// paragraph (≥ `min`) is left intact; a tiny one is a scrap to be re-merged.
/// The clean Constitution is unaffected because `parse_tree` does not coalesce.
fn is_scrap(c: &DocNode, min: usize) -> bool {
    c.children.is_empty()
        && !is_structural_level(c.level)
        && c.attached.is_empty()
        && c.text.chars().count() < min
}

/// A leaf carrying no real content — either empty/whitespace-only, or whose
/// entire text is just an echo of its own label ("Rule 2" body == "2"/"Rule 2").
/// These are uncitable noise: the live audit found empty chunks and chunks whose
/// only content is a bare provision number. Such a leaf is DROPPED in coalescing
/// (its label/citation already live in metadata; nothing is lost). Conservative:
/// only fires on a true leaf with no attachments and very short text, so a
/// genuinely short clause (which carries clause text beyond its number) is kept.
fn is_empty_or_label_only_leaf(c: &DocNode) -> bool {
    if !c.children.is_empty() || is_structural_level(c.level) || !c.attached.is_empty() {
        return false;
    }
    let body = c.text.trim();
    if body.is_empty() {
        return true;
    }
    // Pure label echo: normalize away spaces/punctuation and compare to the
    // node's own label (and to the numeric tail of a "Rule 2"/"Article 22" label).
    let norm = |s: &str| -> String {
        s.chars().filter(|ch| ch.is_alphanumeric()).flat_map(char::to_lowercase).collect()
    };
    let body_n = norm(body);
    if body_n.is_empty() {
        return true; // text was only punctuation/whitespace
    }
    let label_n = norm(&c.label);
    body_n.chars().count() <= 8 && (body_n == label_n || (!label_n.is_empty() && label_n.ends_with(&body_n)))
}

/// The provision a citation INDEPENDENTLY names, normalized ("Rule 42(1)" →
/// "rule 42"), or `None` for a bare sub-marker ("(d)", "(1)") or paragraph
/// fragment that only identifies a sub-part. Used as a provision-boundary
/// signal (FR-002): two adjacent scrap leaves that name DIFFERENT provisions
/// must not be merged, so a genuinely short standalone rule keeps its own unit
/// (spec edge case). Same-named fragments and bare-marker scraps still coalesce
/// (the live Direct-Contracting fix: its scraps carry bare markers "(d)"/"(s)").
fn named_provision(citation: &str) -> Option<String> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(article|section|clause|rule|regulation|chapter|part|schedule)\s+([0-9]+[A-Za-z]?|[IVXLCDM]+)\b",
        )
        .unwrap()
    });
    let c = re.captures(citation)?;
    Some(format!(
        "{} {}",
        c.get(1)?.as_str().to_lowercase(),
        c.get(2)?.as_str().to_lowercase()
    ))
}

/// Build a merged unit from a scrap, inheriting the PARENT's citation so the
/// coherent unit is labelled by its provision (not the bare scrap marker).
fn new_merged(scrap: &DocNode, parent_citation: &str, parent_unit_path: &str, seq: usize) -> DocNode {
    DocNode {
        level: NodeLevel::Clause,
        label: scrap.label.clone(),
        title: None,
        text: scrap.text.trim().to_string(),
        citation: parent_citation.to_string(),
        unit_path: format!("{parent_unit_path}/m{seq}"),
        attached: Vec::new(),
        footnote_refs: Vec::new(),
        children: Vec::new(),
    }
}

/// Flush a run of consecutive scrap leaves. A run of <2 is NOT a fragmentation
/// signature (an isolated short clause) — emitted untouched. A run of ≥2 is
/// merged into coherent units (≤ `hard_max`, stop growing past `target`).
fn flush_scrap_run(
    run: &mut Vec<DocNode>,
    parent_citation: &str,
    parent_unit_path: &str,
    target: usize,
    hard_max: usize,
    seq: &mut usize,
    out: &mut Vec<DocNode>,
) {
    if run.len() < 2 {
        out.append(run);
        return;
    }
    let mut buf: Option<DocNode> = None;
    for scrap in run.drain(..) {
        match buf.as_mut() {
            None => {
                *seq += 1;
                buf = Some(new_merged(&scrap, parent_citation, parent_unit_path, *seq));
            }
            Some(m) => {
                let cur = m.text.chars().count();
                let combined = cur + 1 + scrap.text.trim().chars().count();
                if cur >= target || combined > hard_max {
                    out.push(buf.take().expect("buf set"));
                    *seq += 1;
                    buf = Some(new_merged(&scrap, parent_citation, parent_unit_path, *seq));
                } else {
                    m.text.push('\n');
                    m.text.push_str(scrap.text.trim());
                }
            }
        }
    }
    if let Some(m) = buf.take() {
        out.push(m);
    }
}

/// Inverse of `split_oversized_leaves`: post-order, merge runs of adjacent
/// under-`min` leaf siblings under the same parent into coherent ~`target`
/// units (≤ `hard_max`). Same-parent siblings only; a non-scrap child (longer
/// leaf or a structural/sub-tree node) breaks the run; an isolated short leaf
/// is preserved. Idempotent on already-coherent trees.
fn coalesce_undersized_leaves(mut node: DocNode, min: usize, target: usize, hard_max: usize) -> DocNode {
    // Post-order: transform descendants first.
    node.children = node
        .children
        .into_iter()
        .map(|c| coalesce_undersized_leaves(c, min, target, hard_max))
        // RC-D/2026-06-30: drop empty and pure-label-only leaves outright. These
        // are uncitable noise (the live audit found empty chunks and chunks whose
        // only content is a bare provision number); the label/citation already
        // live in metadata so nothing is lost, and removing them keeps TOP_K
        // slots filled with real content instead of blanks.
        .filter(|c| !is_empty_or_label_only_leaf(c))
        .collect();
    if node.children.len() < 2 {
        return node;
    }

    let parent_citation = node.citation.clone();
    let parent_unit_path = node.unit_path.clone();
    let children = std::mem::take(&mut node.children);
    let mut out: Vec<DocNode> = Vec::with_capacity(children.len());
    let mut run: Vec<DocNode> = Vec::new();
    let mut run_prov: Option<String> = None;
    let mut seq = 0usize;

    for child in children {
        if is_scrap(&child, min) {
            // FR-002 provision boundary: a scrap that names a DIFFERENT provision
            // than the current run ends it, so distinct short rules stay separate
            // (bare-marker scraps name nothing → run continues; same-named
            // fragments → run continues; the live Direct-Contracting fix intact).
            let child_prov = named_provision(&child.citation);
            if !run.is_empty() && child_prov != run_prov {
                flush_scrap_run(
                    &mut run, &parent_citation, &parent_unit_path, target, hard_max, &mut seq, &mut out,
                );
            }
            if run.is_empty() {
                run_prov = child_prov;
            }
            run.push(child);
        } else {
            flush_scrap_run(
                &mut run, &parent_citation, &parent_unit_path, target, hard_max, &mut seq, &mut out,
            );
            run_prov = None;
            out.push(child);
        }
    }
    flush_scrap_run(
        &mut run, &parent_citation, &parent_unit_path, target, hard_max, &mut seq, &mut out,
    );
    node.children = out;
    node
}

// ── Feature 005 — build tree directly from regex-detected headings ──────────

/// Build a `DocNode` tree from regex-detected `LevelledHeading`s + raw text.
///
/// The function assigns each heading's slice of `text` to a node at its
/// declared `NodeLevel`, nesting deeper headings under shallower ancestors via
/// a level stack. Text before the first heading becomes a single Preamble-like
/// leaf at Article level (so the existing Preamble salience boost applies).
///
/// After tree construction, applies `split_oversized_leaves` so any
/// >threshold leaf is split into paragraph-sized children with `/p1`, `/p2`
/// suffixes on `unit_path` (same machinery used for the Constitution
/// Preamble).
///
/// Returns a `DocNode` ready for `persist_tree` + `graph_builder::build_graph`.
pub fn build_tree_from_headings(
    text: &str,
    headings: &[crate::services::heading_detector::regex_pass::LevelledHeading],
    doc_title: &str,
    _profile: &DocProfile,
) -> (DocNode, std::collections::HashMap<String, usize>) {
    use crate::services::heading_detector::regex_pass::LevelledHeading;

    // 013/D12: record each heading node's source byte offset keyed by unit_path,
    // so the caller can map provisions to physical pages. Nodes created later by
    // finalize_tree (paragraph splits `/pN`, merged scraps `/mN`) are absent here
    // and inherit their ancestor's page at persist time.
    let mut offset_by_path: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    let root = DocNode {
        level: NodeLevel::Document,
        label: String::new(),
        title: Some(doc_title.to_string()),
        text: String::new(),
        citation: doc_title.to_string(),
        unit_path: "document".to_string(),
        attached: Vec::new(),
        footnote_refs: Vec::new(),
        children: Vec::new(),
    };
    if text.is_empty() || headings.is_empty() {
        // No structure detected — return a single-leaf document. Caller will
        // typically mark structure_status='failed' and fall back to size-based.
        let mut r = root;
        if !text.is_empty() {
            r.children.push(DocNode {
                level: NodeLevel::Article,
                label: "body".to_string(),
                title: None,
                text: text.to_string(),
                citation: "body".to_string(),
                unit_path: "body".to_string(),
                attached: Vec::new(),
                footnote_refs: Vec::new(),
                children: Vec::new(),
            });
        }
        return (finalize_tree(r), offset_by_path);
    }

    // Preamble — any text before headings[0].
    let mut top_children: Vec<DocNode> = Vec::new();
    if headings[0].offset > 0 {
        let preamble_text = text[..headings[0].offset].trim().to_string();
        if !preamble_text.is_empty() {
            offset_by_path.insert("Preamble".to_string(), 0);
            top_children.push(DocNode {
                level: NodeLevel::Article,
                label: "Preamble".to_string(),
                title: None,
                text: preamble_text,
                citation: "Preamble".to_string(),
                unit_path: "Preamble".to_string(),
                attached: Vec::new(),
                footnote_refs: Vec::new(),
                children: Vec::new(),
            });
        }
    }

    // Walk headings; nest by NodeLevel. Stack tracks pointer paths into the
    // tree-under-construction. The top of stack is always the deepest open
    // container at the current level.
    let make_node = |h: &LevelledHeading, slice: String, path: String| DocNode {
        level: h.level,
        label: h.label.clone(),
        title: None,
        text: slice,
        citation: h.label.clone(),
        unit_path: path,
        attached: Vec::new(),
        footnote_refs: Vec::new(),
        children: Vec::new(),
    };

    // Index path into `top_children` / nested children for each open level.
    // index_path[i] is the position to descend at depth i (0-based after the
    // root container which lives in `top_children`).
    let mut index_path: Vec<usize> = Vec::new();
    let mut level_stack: Vec<NodeLevel> = Vec::new();

    for (i, h) in headings.iter().enumerate() {
        let end = headings.get(i + 1).map(|n| n.offset).unwrap_or(text.len());
        let body_start = h.offset;  // include heading line in this node's text
        let body = text.get(body_start..end).unwrap_or("").trim().to_string();

        // Pop until top-of-stack is strictly shallower than this heading.
        while let Some(&top) = level_stack.last() {
            if (top as usize) < (h.level as usize) { break; }
            level_stack.pop();
            index_path.pop();
        }

        // Build the new node. unit_path = parent.unit_path + "/" + h.label_path.
        let parent_path: String = if index_path.is_empty() {
            "document".to_string()
        } else {
            // Walk down to the parent to read its unit_path.
            let mut cur: &DocNode = &top_children[index_path[0]];
            for &ix in &index_path[1..] {
                cur = &cur.children[ix];
            }
            cur.unit_path.clone()
        };
        let safe_label = h.label.replace(' ', "_");
        let path = if parent_path == "document" {
            safe_label
        } else {
            format!("{parent_path}/{safe_label}")
        };

        offset_by_path.insert(path.clone(), h.offset);
        let node = make_node(h, body, path);

        // Attach to parent.
        if index_path.is_empty() {
            top_children.push(node);
            level_stack.push(h.level);
            index_path.push(top_children.len() - 1);
        } else {
            let mut cur: &mut DocNode = &mut top_children[index_path[0]];
            for &ix in &index_path[1..] {
                cur = &mut cur.children[ix];
            }
            cur.children.push(node);
            let new_ix = cur.children.len() - 1;
            level_stack.push(h.level);
            index_path.push(new_ix);
        }
    }

    let mut tree = root;
    tree.children = top_children;
    // US2 (D5): resolve bare sub-marker citations against the hierarchy
    // ("(i)" under "Rule 27" → "Rule 27(i)") BEFORE coalescing, so merged
    // scrap units inherit a resolved parent citation rather than a bare marker.
    resolve_bare_citations(&mut tree, None);
    (finalize_tree(tree), offset_by_path)
}

/// True when a label is a bare sub-marker SHAPE — "(1)", "(a)", "(iv)", "i.",
/// "a)", "3" — that only makes sense qualified by an ancestor provision. Used by
/// the persist-time `label_status='unresolved'` flag. Deliberately narrow: a
/// descriptive label ("Preamble", "body"), a document title, or a filename root
/// is NOT a bare marker and must never be flagged unresolved.
pub fn is_bare_marker(label: &str) -> bool {
    let t = label.trim();
    if t.is_empty() || named_provision(t).is_some() {
        return false;
    }
    // The alphanumeric core, stripped of surrounding (), [], ., spaces.
    let core: String = t.chars().filter(|c| c.is_alphanumeric()).collect();
    if core.is_empty() || core.chars().count() > 4 {
        return false; // words / titles / filenames are not markers
    }
    let lc = core.to_lowercase();
    let is_number = lc.chars().all(|c| c.is_ascii_digit());
    let is_single_letter = lc.chars().count() == 1 && lc.chars().all(|c| c.is_ascii_alphabetic());
    let is_roman = lc.chars().all(|c| "ivxlc".contains(c));
    is_number || is_single_letter || is_roman
}

/// US2 (D5): walk the tree top-down, qualifying any node whose citation is a
/// bare sub-marker with the nearest ancestor that names a provision. A node
/// with no named ancestor keeps its bare citation and is flagged
/// `label_status='unresolved'` at persist — never a confident-but-wrong label.
/// Idempotent: a node that already names a provision is left untouched.
fn resolve_bare_citations(node: &mut DocNode, nearest_named: Option<&str>) {
    let own_named = named_provision(&node.citation).is_some();
    if !own_named {
        if let Some(anc) = nearest_named {
            if !node.citation.trim().is_empty() {
                node.citation = format!("{anc}{}", node.citation);
            }
        }
    }
    let pass: Option<String> = if named_provision(&node.citation).is_some() {
        Some(node.citation.clone())
    } else {
        nearest_named.map(str::to_string)
    };
    for child in &mut node.children {
        resolve_bare_citations(child, pass.as_deref());
    }
}

fn build_part(part: &Part, profile: &DocProfile) -> DocNode {
    let path = format!("Part{}", part.number);
    let citation = format!("Part {}", part.number);
    // Direct articles first, then chapters — IR document order.
    let mut children: Vec<DocNode> = part
        .articles
        .iter()
        .map(|a| build_article(a, &path, profile))
        .collect();
    children.extend(
        part.chapters
            .iter()
            .map(|c| build_chapter(c, &path, &citation, profile)),
    );
    // 2026-06-03 chapter-index injection (Fix B): the Part-level chunk
    // explicitly enumerates its direct articles and child chapters with
    // titles, so structural questions ("Which Articles fall under Part II?")
    // can retrieve a chunk that actually answers them. Without this the
    // part chunk content was just "Fundamental Rights and Principles of
    // Policy" — usable for semantic match but unable to ground enumerated
    // answers, causing the model to confabulate on Q11/Q12.
    let text = build_part_index_text(part);
    DocNode {
        level: NodeLevel::Part,
        label: part.number.clone(),
        title: Some(part.title.clone()),
        text,
        citation,
        unit_path: path,
        attached: Vec::new(),
        footnote_refs: Vec::new(),
        children,
    }
}

/// Build the enumerated structural index text for a Part. The text reads
/// like a section header from the document itself ("Part II — Fundamental
/// Rights and Principles of Policy comprises: Chapter 1 — Fundamental
/// Rights (Articles 8–28); Chapter 2 — Principles of Policy (Articles
/// 29–40)."), so the embedder treats it as natural prose and the LLM
/// reads it as authoritative.
fn build_part_index_text(part: &Part) -> String {
    let mut out = format!("Part {} — {}", part.number, part.title);
    if !part.articles.is_empty() || !part.chapters.is_empty() {
        out.push_str(" comprises:\n");
    }
    for ch in &part.chapters {
        out.push_str(&format!(
            "  Chapter {} — {}: Articles {}.\n",
            ch.number,
            ch.title,
            article_range(&ch.articles)
        ));
    }
    if !part.articles.is_empty() {
        out.push_str(&format!(
            "  Direct articles in this Part: {}.\n",
            article_range(&part.articles)
        ));
    }
    out
}

/// Render a compact list of article numbers — collapsed into "first–last"
/// when the run is contiguous and >= 3 long, otherwise as a comma list of
/// `Article N` tokens that match the slot extractor's regex.
fn article_range(articles: &[Article]) -> String {
    if articles.is_empty() {
        return "(none)".to_string();
    }
    let nums: Vec<String> = articles.iter().map(|a| a.number.clone()).collect();
    let are_simple = nums
        .iter()
        .all(|n| n.chars().all(|c| c.is_ascii_digit()));
    let parsed: Option<Vec<u32>> = if are_simple {
        nums.iter().map(|n| n.parse().ok()).collect()
    } else {
        None
    };
    let contiguous = parsed
        .as_ref()
        .map(|v| v.windows(2).all(|w| w[1] == w[0] + 1) && v.len() >= 3)
        .unwrap_or(false);
    if contiguous {
        format!("{} to {}", nums.first().unwrap(), nums.last().unwrap())
    } else {
        nums.iter()
            .map(|n| format!("Article {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn build_chapter(
    ch: &Chapter,
    part_path: &str,
    part_citation: &str,
    profile: &DocProfile,
) -> DocNode {
    let path = format!("{}/Ch{}", part_path, ch.number);
    let children = ch
        .articles
        .iter()
        .map(|a| build_article(a, &path, profile))
        .collect();
    // 2026-06-03 Fix B: enumerate child articles with marginal notes so
    // the chapter chunk is a real index entry, not just a heading. This
    // is the chunk that answers "What are the X listed in Chapter Y" —
    // the prior empty-text path produced a chunk whose only content was
    // the chapter title, which let the 3B model confabulate generic
    // values when asked to enumerate (Q1 / Q12 regression evidence).
    let text = build_chapter_index_text(ch, part_citation);
    DocNode {
        level: NodeLevel::Chapter,
        label: ch.number.clone(),
        title: Some(ch.title.clone()),
        text,
        citation: format!("{}, Chapter {}", part_citation, ch.number),
        unit_path: path,
        attached: Vec::new(),
        footnote_refs: Vec::new(),
        children,
    }
}

fn build_chapter_index_text(ch: &Chapter, part_citation: &str) -> String {
    let mut out = format!(
        "{}, Chapter {} — {} contains the following articles:\n",
        part_citation, ch.number, ch.title,
    );
    for art in &ch.articles {
        let note = if art.marginal_note.trim().is_empty() {
            String::new()
        } else {
            format!(" — {}", art.marginal_note.trim())
        };
        out.push_str(&format!("  Article {}{}\n", art.number, note));
    }
    out
}

fn build_article(art: &Article, parent_path: &str, profile: &DocProfile) -> DocNode {
    let path = format!("{}/Art-{}", parent_path, art.number);
    let citation = format!("{} {}", profile.citation_unit, art.number);
    let children = art
        .clauses
        .iter()
        .map(|c| build_clause(c, &path, &citation))
        .collect();
    DocNode {
        level: NodeLevel::Article,
        label: art.number.clone(),
        title: Some(art.marginal_note.clone()),
        text: art.text.clone().unwrap_or_default(),
        citation,
        unit_path: path,
        // A flat article's proviso/explanation has no clause to hang off.
        attached: attachments(&art.provisos, &art.explanations),
        footnote_refs: art.footnote_refs.clone(),
        children,
    }
}

fn build_clause(cl: &Clause, parent_path: &str, article_citation: &str) -> DocNode {
    let path = format!("{}/cl-{}", parent_path, cl.id);
    let citation = format!("{}{}", article_citation, cl.id);
    let children = cl
        .subclauses
        .iter()
        .map(|s| build_subclause(s, &path, &citation))
        .collect();
    DocNode {
        level: NodeLevel::Clause,
        label: cl.id.clone(),
        title: None,
        text: cl.text.clone(),
        citation,
        unit_path: path,
        attached: attachments(&cl.provisos, &cl.explanations),
        footnote_refs: cl.footnote_refs.clone(),
        children,
    }
}

fn build_subclause(sub: &SubClause, parent_path: &str, clause_citation: &str) -> DocNode {
    DocNode {
        level: NodeLevel::SubClause,
        label: sub.id.clone(),
        title: None,
        text: sub.text.clone(),
        citation: format!("{}{}", clause_citation, sub.id),
        unit_path: format!("{}/sub-{}", parent_path, sub.id),
        attached: attachments(&sub.provisos, &sub.explanations),
        footnote_refs: sub.footnote_refs.clone(),
        children: Vec::new(),
    }
}

/// Flatten provisos + explanations into the in-leaf `attached` list.
fn attachments(provisos: &[Proviso], explanations: &[Explanation]) -> Vec<Attached> {
    let mut out: Vec<Attached> = provisos
        .iter()
        .map(|p| Attached {
            kind: AttachedKind::Proviso,
            label: String::new(),
            text: p.text.clone(),
        })
        .collect();
    out.extend(explanations.iter().map(|e| Attached {
        kind: AttachedKind::Explanation,
        label: e.label.clone(),
        text: e.text.clone(),
    }));
    out
}

// ── S3: tree persistence into `chunks` (task T015) ───────────────────────────

/// `domain_metadata` JSON payload for a persisted hierarchy node
/// (data-model.md §1). `legal_domain` enrichment is deferred to S5.
#[derive(Debug, Serialize)]
struct NodeMetadata<'a> {
    /// Human citation, e.g. `"Article 19A(1)(a)"`.
    citation: &'a str,
    /// Canonical level name (`document` … `subclause`).
    level: &'a str,
    /// Part/Chapter title or an Article's marginal note.
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    /// Provisos/explanations carried in this leaf (D3).
    attached: &'a [Attached],
    /// Amendment-marker / editorial footnote ids anchored in this node.
    footnote_refs: &'a [String],
}

/// A heading-mislabel invariant violation (Fix-2, heading-mislabel defect
/// 2026-06-16). The chunk's content begins with an `N. Title` provision heading
/// whose number `N` is absent from the chunk's `unit_label` — i.e. a real
/// heading was absorbed into a neighbouring unit and inherited its label.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelMismatch {
    pub unit_label: String,
    pub content_number: String,
    pub snippet: String,
}

/// `^N. Title` — content that begins with a bare-number provision heading.
fn content_heading_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(?P<n>\d{1,3})[A-Z]?\.\s+[A-Z]").unwrap())
}

/// Leading citation prefix on stored content (`"Rule 54 "`, `"Section 9 "`).
fn label_prefix_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)^\s*(?:Rule|Section|Part|Chapter)\s+[0-9IVXLC]+[A-Z]?\s*").unwrap())
}

/// Index-time invariant guard (Fix-2, recurrence-proof). Given persisted
/// `(unit_label, content)` pairs, return the chunks whose content begins with a
/// provision heading whose number is absent from the label — the exact
/// "absorbed-heading mislabel" the corpus diagnosis SQL detects
/// (`docs/heading-mislabel-defect-diagnosis.md`). Pure: callers log or assert.
///
/// Generic by construction — no per-doc vocabulary. Conservative: the
/// substring test mirrors the diagnosis metric (`position(num in lbl)`), so a
/// label that legitimately contains the number (`Rule 155` vs content `55`) is
/// never flagged.
pub fn detect_label_mismatches<'a>(
    chunks: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<LabelMismatch> {
    let mut out = Vec::new();
    for (label, content) in chunks {
        // Collapse newlines so an absorbed heading on the content's first
        // non-empty line is seen even when the raw label line precedes it.
        let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
        let stripped = label_prefix_re().replace(&flat, "");
        let stripped = stripped.trim_start();
        if let Some(c) = content_heading_re().captures(stripped) {
            let n = c.name("n").unwrap().as_str();
            if !label.contains(n) {
                out.push(LabelMismatch {
                    unit_label: label.to_string(),
                    content_number: n.to_string(),
                    snippet: stripped.chars().take(60).collect(),
                });
            }
        }
    }
    out
}

/// Persist a `DocNode` tree into the `chunks` table for `source_id`.
///
/// Every node becomes one chunk: `unit_type` = the canonical level, `unit_path`
/// = the canonical citation path, `parent_chunk_id` linking each node to its
/// parent, and `domain_metadata` carrying the citation, attached provisos and
/// footnote refs. Rows are inserted pre-order in a single transaction, so each
/// child's `parent_chunk_id` always resolves to an already-inserted row.
pub async fn persist_tree(
    pool: &Db,
    source_id: &str,
    root: &DocNode,
) -> Result<Vec<Chunk>> {
    persist_tree_with_pages(pool, source_id, root, &std::collections::HashMap::new()).await
}

/// Like [`persist_tree`] but stamps each chunk's `page_number` from a
/// `unit_path -> page` map (013/D12). A node absent from the map (paragraph
/// splits, merged scraps, the document root) inherits its nearest ancestor's
/// page; a node with no mapped ancestor keeps `page_number = NULL`.
pub async fn persist_tree_with_pages(
    pool: &Db,
    source_id: &str,
    root: &DocNode,
    page_by_path: &std::collections::HashMap<String, i64>,
) -> Result<Vec<Chunk>> {
    let mut prepared: Vec<(String, NewChunk)> = Vec::new();
    let mut index: i64 = 0;
    collect_chunks(root, None, source_id, "", None, page_by_path, &mut index, &mut prepared);
    chunks::bulk_insert_with_ids(pool, prepared).await
}

/// The chunk `content` for a node, plus the context string its children
/// inherit (task T022 — multi-level embedding).
///
/// A bare leaf like `"(b) detention exceeding three months"` is context-poor;
/// for retrieval-quality embeddings each clause/sub-clause gets its Article
/// heading and clause chapeau prepended. Structural roll-up nodes
/// (document/part/chapter) keep their plain title and do not propagate context.
fn node_content(node: &DocNode, inherited: &str) -> (String, String) {
    match node.level {
        NodeLevel::Document | NodeLevel::Part | NodeLevel::Chapter => {
            let own = if node.text.is_empty() {
                node.title.clone().unwrap_or_else(|| node.citation.clone())
            } else {
                node.text.clone()
            };
            (own, String::new())
        }
        NodeLevel::Article => {
            // "Article 10 — Safeguards as to arrest and detention"
            let heading = match &node.title {
                Some(t) if !t.is_empty() => format!("{} — {}", node.citation, t),
                _ => node.citation.clone(),
            };
            let own = if node.text.is_empty() {
                heading.clone()
            } else {
                format!("{heading}\n{}", node.text)
            };
            // Clauses inherit the Article heading.
            (own, heading)
        }
        NodeLevel::Clause | NodeLevel::SubClause | NodeLevel::Item => {
            let own = if inherited.is_empty() {
                node.text.clone()
            } else {
                format!("{inherited}\n{}", node.text)
            };
            // Children inherit heading + this clause's CHAPEAU (its first
            // non-empty line), NOT the full clause body. Without this cap a
            // Rule's clause body propagated verbatim into every sub-clause
            // child, producing ~400 identical 28-k-char chunks under one
            // parent (22-prompt eval root cause, 2026-05-25).
            let chapeau = node
                .text
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim())
                .unwrap_or("");
            let next_inherited = match (inherited.is_empty(), chapeau.is_empty()) {
                (true,  true)  => String::new(),
                (true,  false) => chapeau.to_string(),
                (false, true)  => inherited.to_string(),
                (false, false) => format!("{inherited}\n{chapeau}"),
            };
            (own, next_inherited)
        }
    }
}

/// Pre-order walk: assign a UUID per node and build its `NewChunk`. `context`
/// is the ancestor text prepended to this node's content (empty at the root).
fn collect_chunks(
    node: &DocNode,
    parent_id: Option<&str>,
    source_id: &str,
    context: &str,
    parent_page: Option<i64>,
    page_by_path: &std::collections::HashMap<String, i64>,
    index: &mut i64,
    out: &mut Vec<(String, NewChunk)>,
) {
    let id = Uuid::new_v4().to_string();

    let (content, child_context) = node_content(node, context);
    let char_end = content.chars().count() as i64;
    let token_count = content.split_whitespace().count() as i64;

    // 013/D12: this node's own mapped page, else inherit the ancestor's page.
    let page = page_by_path.get(&node.unit_path).copied().or(parent_page);

    let meta = NodeMetadata {
        citation: &node.citation,
        level: node.level.unit_type(),
        title: node.title.as_deref(),
        attached: &node.attached,
        footnote_refs: &node.footnote_refs,
    };

    out.push((
        id.clone(),
        NewChunk {
            source_id: source_id.to_string(),
            chunk_index: *index,
            char_start: 0,
            char_end,
            page_number: page,
            token_count,
            parent_chunk_id: parent_id.map(str::to_string),
            unit_type: node.level.unit_type().to_string(),
            unit_label: Some(node.citation.clone()),
            unit_path: Some(node.unit_path.clone()),
            domain_metadata: Some(serde_json::to_string(&meta).unwrap_or_default()),
            content,
        },
    ));
    *index += 1;

    for child in &node.children {
        collect_chunks(child, Some(&id), source_id, &child_context, page, page_by_path, index, out);
    }
}

#[cfg(test)]
mod label_invariant_tests {
    use super::*;

    /// Fix-2 (heading-mislabel defect 2026-06-16). The absorbed-heading shape
    /// from the live DB — content `"55. Entry into Force…"` stored under label
    /// `"Rule 54"` — must be flagged, while a correctly-labeled chunk and a
    /// label that merely contains the number as a substring must NOT.
    #[test]
    fn detects_absorbed_heading_mislabel() {
        let pairs = vec![
            // Defect: label 54, content begins with provision 55.
            ("Rule 54", "Rule 54 55. Entry into Force - These rules shall come into force at once."),
            // Defect: sub-clause label, content begins with rule 50.
            ("Part II(3)", "50. Publication of the Award shall be made within thirty days."),
            // Correct: label number equals content heading number.
            ("Rule 55", "Rule 55 55. Entry into Force - These rules shall come into force."),
            // Not a mislabel: clause body, no leading provision heading.
            ("(a)", "(a) the applicant must submit the form in duplicate."),
            // Substring guard: 'Rule 155' contains '55' → must not false-flag.
            ("Rule 155", "Rule 155 55. (this number is a substring of the label)."),
        ];
        let hits = detect_label_mismatches(pairs);
        let bad: Vec<&str> = hits.iter().map(|m| m.unit_label.as_str()).collect();
        assert!(bad.contains(&"Rule 54"), "missed Rule 54 mislabel: {bad:?}");
        assert!(bad.contains(&"Part II(3)"), "missed Part II(3) mislabel: {bad:?}");
        assert!(!bad.contains(&"Rule 55"), "false-flagged correct chunk: {bad:?}");
        assert!(!bad.contains(&"(a)"), "false-flagged clause body: {bad:?}");
        assert!(!bad.contains(&"Rule 155"), "false-flagged substring label: {bad:?}");
        assert_eq!(hits.len(), 2, "exactly two real mislabels expected: {hits:?}");
    }
}

#[cfg(test)]
mod nesting_tests {
    use super::*;
    use crate::services::doc_profiles::rules::RULES;
    use crate::services::heading_detector::regex_pass::scan_with_profile;

    /// Nesting audit 2026-07-02 — the SPPRA Rule 16(1)(a)(ii) shape end-to-end:
    /// romans nest UNDER their alpha sub-clause (previously flattened to
    /// colliding `(1)/(i)` siblings), and the full citation chain resolves.
    #[test]
    fn roman_items_nest_under_alpha_subclauses_with_full_chain() {
        // Bodies must exceed ECHO_COALESCE_MIN (120c) or finalize_tree merges
        // them as scraps — real SPPRA romans run 200–450c.
        let pad = " The procuring agency shall record the reasons and justifications in writing for resorting to this method and shall place the record before the authority.";
        let text = format!(
            "Rule 16. Alternate Methods of Procurements\n\
(1) A procuring agency may utilize following alternative methods of procurement of goods, services and works after recording reasons, namely:-{pad}\n\
(a) Request for Quotations shall be the first alternative method available.{pad}\n\
(i) request for quotation is the method based on comparing price quotations obtained from at least three suppliers to assure competitive prices;{pad}\n\
(ii) a procuring agency shall engage in this method of procurement only if the prescribed conditions exist and are documented;{pad}\n\
(b) Direct Contracting means procurement from a single source without competition and shall only apply in the prescribed circumstances.{pad}\n",
        );
        let text = text.as_str();
        let headings = scan_with_profile(text, &RULES);
        let (tree, _) = build_tree_from_headings(text, &headings, "SPPRA Rules", &RULES);

        let rule = tree.find("Rule 16").expect("Rule 16 node");
        assert_eq!(rule.children.len(), 1, "one sub-rule (1)");
        let sub_rule = &rule.children[0];
        assert_eq!(sub_rule.citation, "Rule 16(1)");
        let alphas: Vec<&str> = sub_rule.children.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(alphas, vec!["(a)", "(b)"], "alpha clauses are siblings");

        let a = &sub_rule.children[0];
        assert_eq!(a.citation, "Rule 16(1)(a)");
        let romans: Vec<&str> = a.children.iter().map(|c| c.citation.as_str()).collect();
        assert_eq!(
            romans,
            vec!["Rule 16(1)(a)(i)", "Rule 16(1)(a)(ii)"],
            "romans nest under (a) with the full citation chain",
        );
        assert_eq!(a.children[1].unit_path, "Rule_16/(1)/(a)/(ii)");
        assert_eq!(a.children[1].level, NodeLevel::Item);
    }

    /// The definitions shape: `(h),(i),(j)` stay an alpha run — `(i)` must NOT
    /// nest under `(h)` — while `(l)`'s roman list nests under `(l)`.
    #[test]
    fn ninth_alpha_clause_stays_a_sibling() {
        // Bodies must exceed ECHO_COALESCE_MIN (120c) — see the test above.
        let pad = " This definition applies throughout these rules unless the subject or context otherwise requires a different construction of the defined expression.";
        let text = format!(
            "Rule 2. Definitions\n\
(1) In these rules, unless there is anything repugnant in the subject or context, the following expressions shall carry the meanings assigned below;{pad}\n\
(h) \"Bidding Process\" means the procurement procedure under which sealed bids are invited, received, opened, examined and evaluated;{pad}\n\
(i) \"Blacklisting\" means barring a bidder, contractor, consultant or supplier from participating in any future procurement proceedings;{pad}\n\
(j) \"Calendar Days\" means days including all holidays observed by the government of Sindh from time to time;{pad}\n\
(l) \"Conflict of Interest\" means any of the following circumstances arising in relation to a procurement;{pad}\n\
(i) where a contractor, supplier or consultant provides, or could be perceived as providing, biased professional advice to a procuring agency;{pad}\n\
(ii) receiving or giving any remuneration directly or indirectly in connection with the assignment except as provided in the contract;{pad}\n",
        );
        let text = text.as_str();
        let headings = scan_with_profile(text, &RULES);
        let (tree, _) = build_tree_from_headings(text, &headings, "SPPRA Rules", &RULES);

        let sub_rule = &tree.find("Rule 2").expect("Rule 2").children[0];
        let clauses: Vec<&str> = sub_rule.children.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(clauses, vec!["(h)", "(i)", "(j)", "(l)"], "alpha run intact");
        let l = sub_rule.children.last().unwrap();
        let romans: Vec<&str> = l.children.iter().map(|c| c.citation.as_str()).collect();
        assert_eq!(romans, vec!["Rule 2(1)(l)(i)", "Rule 2(1)(l)(ii)"]);
    }
}

#[cfg(test)]
mod persist_tests {
    use super::*;
    use crate::services::constitution_ir;
    use crate::services::doc_profiles::constitution::CONSTITUTION;
    use std::path::PathBuf;

    fn ir_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tools/constitution_import/constitute_IR/constitution_ir.json")
    }

    /// Fix B (2026-06-03): Chapter-level chunks must carry the enumerated
    /// list of child articles + marginal notes, so structural questions
    /// ("What are the Principles of Policy listed in Chapter 2 of Part
    /// II?") can be grounded against a real index chunk rather than
    /// extrapolated from sibling article text.
    #[tokio::test]
    async fn chapter_chunk_enumerates_its_articles_with_titles() {
        let ir = constitution_ir::load(&ir_path()).expect("IR must load");
        let tree = parse_tree(&ir, &CONSTITUTION);

        // Walk to Part II → Chapter 2 (Principles of Policy).
        let part_ii = tree
            .children
            .iter()
            .find(|n| n.level == NodeLevel::Part && n.label == "II")
            .expect("Part II not found");
        let chap_2 = part_ii
            .children
            .iter()
            .find(|n| n.level == NodeLevel::Chapter && n.label == "2")
            .expect("Chapter 2 not found");

        assert!(
            !chap_2.text.is_empty(),
            "Chapter 2 chunk text must NOT be empty — it must enumerate children"
        );
        assert!(
            chap_2.text.contains("Article 29"),
            "Chapter 2 text must list Article 29 — got {:?}",
            chap_2.text
        );
        assert!(
            chap_2.text.contains("Article 40"),
            "Chapter 2 text must list Article 40 (last Principle) — got {:?}",
            chap_2.text
        );
        assert!(
            chap_2.text.contains("Principles of Policy"),
            "Chapter 2 text must reference the chapter title — got {:?}",
            chap_2.text
        );

        // Part II text must also enumerate its chapters.
        assert!(
            part_ii.text.contains("Chapter 1") && part_ii.text.contains("Chapter 2"),
            "Part II text must list both chapters — got {:?}",
            part_ii.text
        );
    }

    async fn pool_with_source() -> Db {
        let pool = crate::db::test_pool().await;
        sqlx::query(
            "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
             VALUES('c1','/c','constitution','pdf',1,'h',0,0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn persist_tree_writes_every_node_with_resolving_parents() {
        let ir = constitution_ir::load(&ir_path()).unwrap();
        let tree = parse_tree(&ir, &CONSTITUTION);
        let pool = pool_with_source().await;

        let inserted = persist_tree(&pool, "c1", &tree).await.unwrap();

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total as usize, inserted.len());

        // A1 structural: exactly one document root with a NULL parent.
        let roots: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE parent_chunk_id IS NULL")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(roots, 1, "one document root");
        let root_type: String =
            sqlx::query_scalar("SELECT unit_type FROM chunks WHERE parent_chunk_id IS NULL")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(root_type, "document");

        // A1: zero orphans — every parent_chunk_id resolves to a real chunk.
        let orphans: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chunks c WHERE c.parent_chunk_id IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM chunks p WHERE p.id = c.parent_chunk_id)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(orphans, 0, "no orphan parent links");

        // A1: 360 articles from the importer (312 originally; +48 from the
        // F007 IR data patch f53b4e6a — missing-article + chapter-index
        // injection), plus the Preamble parent (article-level, citation
        // "Preamble") so it is retrievable.
        // Idea 1: the Preamble's text is now split into clause-level
        // paragraph children; the parent stays at article level.
        let articles: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE unit_type = 'article'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(articles, 361, "360 article chunks + 1 Preamble parent");

        // Preamble parent: exactly one row, citation marker only, top-level path.
        let parent: (String, String, Option<String>) = sqlx::query_as(
            "SELECT content, unit_path, unit_label FROM chunks \
             WHERE unit_label = 'Preamble' AND unit_path = 'Preamble'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(parent.1, "Preamble", "top-level unit_path");
        assert_eq!(parent.0, "Preamble", "parent body is the citation marker");

        // Preamble paragraph children: same label, prefixed path, content
        // contains the canonical text. The "Quaid-i-Azam" sentence must
        // live in exactly one of them (the Founder paragraph).
        let child_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chunks WHERE unit_label = 'Preamble' AND unit_path LIKE 'Preamble/p%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            child_count >= 2,
            "Preamble must split into 2+ paragraph children, got {child_count}"
        );
        let founder_hits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chunks WHERE unit_label='Preamble' AND content LIKE '%Quaid-i-Azam%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            founder_hits, 1,
            "exactly one Preamble paragraph carries the Founder-of-Pakistan sentence"
        );
        // Children link back to the parent (no orphan).
        let parent_id: String = sqlx::query_scalar(
            "SELECT id FROM chunks WHERE unit_label='Preamble' AND unit_path='Preamble'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let children_under_parent: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chunks WHERE unit_label='Preamble' AND parent_chunk_id = $1",
        )
        .bind(&parent_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            children_under_parent, child_count,
            "every Preamble paragraph child links to the Preamble parent"
        );
    }

    // ── Idea 1 — split_long_text unit tests ──────────────────────────────────

    #[test]
    fn split_returns_input_unchanged_when_under_threshold() {
        let r = split_long_text("short text", 1000);
        assert_eq!(r, vec!["short text".to_string()]);
    }

    #[test]
    fn split_uses_paragraph_boundaries_when_available() {
        let text = "Para one.\n\nPara two.\n\nPara three.";
        // Threshold sits above each paragraph (≤11c) but below the whole (32c):
        // forces the split, and each paragraph is under the limit so the
        // recursive threshold-enforcement (oversize-leak fix) leaves it intact.
        let r = split_long_text(text, 15);
        assert_eq!(r, vec!["Para one.".to_string(), "Para two.".to_string(), "Para three.".to_string()]);
    }

    #[test]
    fn split_falls_back_to_sentences_when_no_paragraphs() {
        // Single paragraph (no \n\n), multiple sentences.
        let text = "First sentence. Second sentence. Third sentence.";
        // Above each sentence (≤16c), below the whole (48c) — each sentence stays
        // whole under the recursive threshold-enforcement (oversize-leak fix).
        let r = split_long_text(text, 20);
        assert!(r.len() >= 3, "got {} pieces: {:?}", r.len(), r);
        assert!(r[0].starts_with("First"));
        assert!(r[1].starts_with("Second"));
    }

    /// Fix A (2026-06): the sentence splitter must NOT sever a figure from its
    /// "Rs."/abbreviation lead-in. Regression guard for the live CDWP §2.6 defect
    /// where "…costing upto Rs. 10,000 million" split into "…Rs." | "10,000 …",
    /// stranding the operative figure in a separate chunk.
    #[test]
    fn split_keeps_currency_figure_with_lead_in() {
        // Single paragraph (no \n\n) so the sentence splitter runs; > threshold.
        let text = "The first body sentence provides context for the rule. \
                    The Authority has authorised the forum to approve schemes \
                    costing upto Rs. 10,000 million and no more than that limit. \
                    Beyond this cost the matter is referred to the higher forum.";
        // Above the longest sentence (~117c) but below the whole (~230c) so the
        // sentence splitter runs and its abbreviation-protection is exercised,
        // without the recursive threshold-enforcement char-cutting a sentence.
        let r = split_long_text(text, 150);
        assert!(r.len() >= 2, "should split into sentences: {r:?}");
        assert!(
            !r.iter().any(|p| p.trim_end().ends_with("Rs.")),
            "currency abbrev severed from its figure: {r:?}"
        );
        assert!(
            r.iter().any(|p| p.contains("Rs. 10,000 million")),
            "Rs. 10,000 million not kept together: {r:?}"
        );
    }

    #[test]
    fn split_falls_back_to_char_chunks_for_single_huge_sentence() {
        let text = "a".repeat(2500);
        let r = split_long_text(&text, 1000);
        assert!(r.len() >= 2, "must split a 2500-char unbroken run at 1000 threshold");
        for piece in &r {
            assert!(piece.chars().count() <= 1000, "piece too large: {}", piece.chars().count());
        }
    }

    #[test]
    fn split_drops_empty_pieces() {
        // Double-blanks should not become empty entries. Threshold above each
        // paragraph (9c) but below the whole (22c) so neither is recursively
        // re-split (oversize-leak fix) — the run of blank lines is what's tested.
        let text = "Para one.\n\n\n\nPara two.";
        let r = split_long_text(text, 12);
        assert_eq!(r.len(), 2);
    }

    /// Oversize-leak fix (2026-06-30): when a body has ≥2 paragraphs but ONE
    /// paragraph is itself far larger than the threshold (an OCR run / bullet
    /// list with no internal blank line or sentence break — the live Manual
    /// `Section_30/p1` = 35,604c defect), the giant paragraph must be recursively
    /// re-split, not returned whole. Every piece must respect the threshold.
    #[test]
    fn split_long_text_enforces_threshold_on_giant_paragraph() {
        let giant = "x".repeat(35_000); // no spaces, no '.', no '\n\n', no markers
        let text = format!("Short intro paragraph.\n\n{giant}");
        let r = split_long_text(&text, 1000);
        assert!(r.len() >= 2, "must split: {}", r.len());
        for piece in &r {
            assert!(
                piece.chars().count() <= 1000,
                "piece exceeds threshold: {}",
                piece.chars().count()
            );
        }
    }

    /// `split_block_for_persist` keeps a normal-sized block whole but splits a
    /// pathological front/back-matter body (constitution TOC 42k, schedule 17k)
    /// above the oversize ceiling into context-sized pieces.
    #[test]
    fn split_block_for_persist_splits_above_ceiling_keeps_small_whole() {
        let small = "A short table of contents entry.";
        assert_eq!(split_block_for_persist(small), vec![small.to_string()]);

        let giant = "y".repeat(20_000); // > oversize_hard_ceiling() default 8000
        let pieces = split_block_for_persist(&giant);
        assert!(pieces.len() >= 2, "giant block must split: {}", pieces.len());
        for p in &pieces {
            assert!(
                p.chars().count() <= long_chunk_threshold(),
                "piece exceeds long_chunk_threshold: {}",
                p.chars().count()
            );
        }
    }

    /// 013/D7 (T030): sub-clause markers split at correct boundaries — `(a)` does
    /// not swallow the start of `(b)` (even when `(b)` is amendment-bracketed and
    /// NOT blank-line-separated), and `(3)` is its own piece.
    #[test]
    fn clause_marker_split_keeps_subclauses_separate() {
        let text = "\
        (3)         The Provisions of this Article shall not apply to—

                   (a) any law relating to members of the Armed Forces, or of
                        the police, for the purpose of ensuring discipline; or
                   2
                       [(b) any of the —

                          (i) laws specified in the First Schedule as in force
                              immediately before the commencing day;

                           (ii) other laws specified in Part I of the First Schedule;]";
        let r = split_long_text(text, 100);
        let a_piece = r.iter().find(|p| p.starts_with("(a)")).expect(&format!("(a) piece missing: {r:?}"));
        assert!(!a_piece.contains("(b)"), "(a) swallowed (b): {a_piece:?}");
        assert!(r.iter().any(|p| p.starts_with("(3)")), "(3) not its own piece: {r:?}");
        assert!(r.iter().any(|p| p.starts_with("[(b)")), "amendment-bracketed (b) not split: {r:?}");
        assert!(r.iter().any(|p| p.starts_with("(i)")) && r.iter().any(|p| p.starts_with("(ii)")), "got {r:?}");
    }

    /// 013/D8 (T031): an inline amendment insertion `1[ … ]` is preserved exactly
    /// (footnote-ref digit + brackets + inserted text) and every split piece is a
    /// verbatim contiguous slice of the source.
    #[test]
    fn amendment_bracket_preserved_verbatim_through_split() {
        let text = "\
        (4)    Notwithstanding anything, within a period of two years from the
commencing day, the appropriate Legislature shall bring the laws specified in
1[Part II of the First Schedule] into conformity with the rights conferred.

        (5)     The rights conferred by this Chapter shall not be suspended
except as expressly provided by the Constitution and any related instrument.";
        let r = split_long_text(text, 100);
        assert!(
            r.iter().any(|p| p.contains("1[Part II of the First Schedule]")),
            "amendment bracket not preserved verbatim: {r:?}"
        );
        assert!(r.iter().any(|p| p.starts_with("(4)")) && r.iter().any(|p| p.starts_with("(5)")), "got {r:?}");
        for p in &r {
            assert!(text.contains(p.as_str()), "piece is not a verbatim slice of source: {p:?}");
        }
    }

    /// Doc-agnostic shape test — given a leaf with any label/citation/path,
    /// the generic splitter produces parent + clause children where every
    /// child's label/citation/path is **derived from the parent**, never
    /// from a hardcoded literal.
    #[test]
    fn split_oversized_leaves_derives_child_metadata_from_parent() {
        // Synthetic leaf — could be "Preamble", "Abstract", "Foreword",
        // any future doc-type's long opening. We use a non-Preamble label
        // here to prove the splitter is generic.
        let long_text = "Para A.\n\nPara B.\n\nPara C.\n\nPara D.";
        let leaf = DocNode {
            level: NodeLevel::Article,
            label: "Foreword".to_string(),
            title: None,
            text: long_text.to_string(),
            citation: "Foreword".to_string(),
            unit_path: "Foreword".to_string(),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: Vec::new(),
        };
        let split = split_oversized_leaves(leaf, 10); // tiny threshold forces split

        // Parent: same metadata, empty text, has children.
        assert_eq!(split.label, "Foreword");
        assert_eq!(split.citation, "Foreword");
        assert_eq!(split.unit_path, "Foreword");
        assert_eq!(split.text, "");
        assert_eq!(split.children.len(), 4);

        // Children: metadata DERIVED FROM PARENT — no literals.
        for (i, c) in split.children.iter().enumerate() {
            assert_eq!(c.level, NodeLevel::Clause);
            assert_eq!(c.label, format!("Foreword part {}", i + 1));
            assert_eq!(c.citation, "Foreword"); // same as parent
            assert_eq!(c.unit_path, format!("Foreword/p{}", i + 1));
            assert!(!c.text.is_empty());
            assert!(c.children.is_empty());
        }
    }

    /// Splitter is a no-op for leaves under the threshold, regardless of
    /// what the leaf is called.
    #[test]
    fn split_oversized_leaves_noop_for_short_leaf() {
        let leaf = DocNode {
            level: NodeLevel::Article,
            label: "Abstract".to_string(),
            title: None,
            text: "Short abstract content.".to_string(),
            citation: "Abstract".to_string(),
            unit_path: "Abstract".to_string(),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: Vec::new(),
        };
        let r = split_oversized_leaves(leaf.clone(), 1000);
        assert_eq!(r.text, leaf.text, "short leaf untouched");
        assert!(r.children.is_empty());
    }

    /// Internal nodes with long own-text + existing children also split.
    /// This is the APT Rule 12(3) / E&D Rule_14 case: a clause with detected
    /// sub-clauses but ALSO carrying a huge annexure/preamble block in its
    /// own text. Original splitter early-returned on `!children.is_empty()`
    /// and let 14k-char chunks through. Fix appends paragraph children
    /// AFTER the existing structural children so chunk_index order stays
    /// detected-children-first, paragraph-fallback-after.
    #[test]
    fn split_oversized_leaves_splits_internal_node_own_text() {
        let detected_child = DocNode {
            level: NodeLevel::SubClause,
            label: "(a)".into(), title: None, text: "real sub-clause".into(),
            citation: "(a)".into(), unit_path: "Rule_12/(3)/(a)".into(),
            attached: vec![], footnote_refs: vec![], children: vec![],
        };
        // Long body with paragraph breaks so split_long_text returns >=2 parts.
        let long = (0..6)
            .map(|i| format!("Para {i}. {}", "filler. ".repeat(50)))
            .collect::<Vec<_>>()
            .join("\n\n");
        let internal = DocNode {
            level: NodeLevel::Clause,
            label: "(3)".into(), title: None,
            text: long,
            citation: "(3)".into(), unit_path: "Rule_12/(3)".into(),
            attached: vec![], footnote_refs: vec![],
            children: vec![detected_child],
        };
        let r = split_oversized_leaves(internal, 200);
        assert_eq!(r.text, "", "internal node body emptied after split");
        assert!(r.children.len() > 1, "paragraph children appended");
        // First child is the originally-detected sub-clause.
        assert_eq!(r.children[0].unit_path, "Rule_12/(3)/(a)");
        // Remaining children are paragraph fallbacks named `(3) part N`.
        for (i, c) in r.children.iter().skip(1).enumerate() {
            assert_eq!(c.citation, "(3)", "para child citation = parent");
            assert_eq!(c.unit_path, format!("Rule_12/(3)/p{}", i + 1));
            assert!(c.text.chars().count() <= 600); // bounded by threshold + paragraph residual
        }
    }

    /// Real-world acceptance: the splitter applied to the actual
    /// constitution tree splits the Preamble specifically (because it's
    /// the only leaf >1000 chars in this corpus). Same code path that
    /// would handle any future long-opening leaf.
    #[test]
    fn parse_tree_splits_real_oversized_leaves_via_generic_transform() {
        use crate::services::constitution_ir;
        let ir = constitution_ir::load(&ir_path()).unwrap();
        let tree = parse_tree(&ir, &CONSTITUTION);

        // Find the Preamble node in the tree (it's a top-level child).
        let preamble = tree
            .children
            .iter()
            .find(|c| c.unit_path == "Preamble")
            .expect("Preamble node present");
        assert_eq!(preamble.text, "", "split parent has empty body");
        assert!(
            preamble.children.len() >= 5,
            "real Preamble splits into ≥5 paragraph children, got {}",
            preamble.children.len()
        );
        // The Founder sentence is in exactly one paragraph child.
        let founder = preamble
            .children
            .iter()
            .filter(|c| c.text.contains("Quaid-i-Azam"))
            .count();
        assert_eq!(founder, 1);
        // Children's labels / paths are derived from the parent — no hardcoded literal.
        for (i, c) in preamble.children.iter().enumerate() {
            assert_eq!(c.label, format!("{} part {}", preamble.label, i + 1));
            assert_eq!(c.unit_path, format!("{}/p{}", preamble.unit_path, i + 1));
            assert_eq!(c.citation, preamble.citation);
        }
    }

    #[tokio::test]
    async fn persisted_leaf_carries_canonical_citation_and_path() {
        let ir = constitution_ir::load(&ir_path()).unwrap();
        let tree = parse_tree(&ir, &CONSTITUTION);
        let pool = pool_with_source().await;
        persist_tree(&pool, "c1", &tree).await.unwrap();

        // A2 citation accuracy — sub-clause 8(3)(b).
        let (path, meta, content): (String, String, String) = sqlx::query_as(
            "SELECT unit_path, domain_metadata, content \
             FROM chunks WHERE unit_label = 'Article 8(3)(b)'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            path.ends_with("/Art-8/cl-(3)/sub-(b)"),
            "unit_path = {path}"
        );
        assert!(
            meta.contains(r#""citation":"Article 8(3)(b)""#),
            "domain_metadata = {meta}"
        );
        // T022 — the leaf's content carries its Article heading + clause
        // chapeau prepended, so the embedding is not context-blind.
        assert!(
            content.starts_with("Article 8"),
            "leaf content must lead with the Article heading: {content}"
        );
        assert!(
            content.contains('\n'),
            "leaf content must prepend ancestor context: {content}"
        );
    }

    // ── 010 re-chunk extension: coalescing (T003) + oversize-preserved (T003a) ──

    fn scrap_leaf(label: &str, text: &str) -> DocNode {
        DocNode {
            level: NodeLevel::SubClause,
            label: label.to_string(),
            title: None,
            text: text.to_string(),
            citation: format!("({label})"),
            unit_path: format!("Rule_42/{label}"),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: Vec::new(),
        }
    }

    fn rule_parent(children: Vec<DocNode>) -> DocNode {
        DocNode {
            level: NodeLevel::Article,
            label: "42".to_string(),
            title: None,
            text: String::new(),
            citation: "Rule 42".to_string(),
            unit_path: "Rule_42".to_string(),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children,
        }
    }

    #[test]
    fn coalesce_merges_run_of_scrap_leaves_into_one_unit() {
        // The Direct-Contracting fragmentation signature: a run of tiny scraps.
        let parent = rule_parent(vec![
            scrap_leaf("d", "Direct Contracting"),
            scrap_leaf("s", "means procurement from a single source without competition"),
            scrap_leaf("s", "obtainable only from one source"),
        ]);
        let out = coalesce_undersized_leaves(parent, 120, 600, 800);
        assert_eq!(out.children.len(), 1, "scraps should merge into one unit");
        let m = &out.children[0];
        assert_eq!(m.citation, "Rule 42", "merged unit inherits the parent provision label");
        assert!(m.text.contains("Direct Contracting"));
        assert!(m.text.contains("single source"));
        assert!(m.text.contains("obtainable only from one source"));
    }

    /// US2 (D5): a bare sub-marker citation is qualified by the nearest named
    /// ancestor; a deeper bare marker chains; a node with no named ancestor
    /// keeps its bare citation (→ flagged unresolved at persist).
    #[test]
    fn resolve_bare_citations_qualifies_against_hierarchy() {
        let mut tree = DocNode {
            level: NodeLevel::Article,
            label: "27".into(),
            title: None,
            text: String::new(),
            citation: "Rule 27".into(),
            unit_path: "Rule_27".into(),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: vec![DocNode {
                level: NodeLevel::Clause,
                label: "(1)".into(),
                title: None,
                text: "sub one".into(),
                citation: "(1)".into(),
                unit_path: "Rule_27/(1)".into(),
                attached: Vec::new(),
                footnote_refs: Vec::new(),
                children: vec![DocNode {
                    level: NodeLevel::SubClause,
                    label: "(a)".into(),
                    title: None,
                    text: "sub a".into(),
                    citation: "(a)".into(),
                    unit_path: "Rule_27/(1)/(a)".into(),
                    attached: Vec::new(),
                    footnote_refs: Vec::new(),
                    children: Vec::new(),
                }],
            }],
        };
        resolve_bare_citations(&mut tree, None);
        assert_eq!(tree.citation, "Rule 27", "named provision untouched");
        assert_eq!(tree.children[0].citation, "Rule 27(1)");
        assert_eq!(tree.children[0].children[0].citation, "Rule 27(1)(a)");
    }

    /// A bare marker with no named ancestor stays bare (will be flagged
    /// unresolved) — we never invent a provision name.
    #[test]
    fn resolve_bare_citations_leaves_orphan_marker_bare() {
        let mut tree = DocNode {
            level: NodeLevel::Clause,
            label: "(1)".into(),
            title: None,
            text: "orphan".into(),
            citation: "(1)".into(),
            unit_path: "(1)".into(),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: Vec::new(),
        };
        resolve_bare_citations(&mut tree, None);
        assert_eq!(tree.citation, "(1)", "no ancestor → no invention");
        assert!(is_bare_marker(&tree.citation));
    }

    /// `is_bare_marker` must match only genuine sub-marker shapes — never a
    /// descriptive label, document title, or filename (which were over-flagged
    /// as `unresolved` on the live re-index).
    #[test]
    fn is_bare_marker_only_matches_sub_marker_shapes() {
        for m in ["(1)", "(a)", "(iv)", "(e)", "i.", "a)", "3", "(2)", "(v)"] {
            assert!(is_bare_marker(m), "should flag bare marker {m:?}");
        }
        for ok in ["Preamble", "body", "Rule 27", "Article 19A", "Part II",
                   "11_Sindh_Civil_Servants_APT_Rules_1974.pdf", "Schedule"] {
            assert!(!is_bare_marker(ok), "should NOT flag {ok:?}");
        }
    }

    #[test]
    fn coalesce_respects_boundaries_between_runs() {
        // [scrap, scrap, BIG, scrap, scrap] → two merged units around the big leaf.
        let big = "x".repeat(200); // >= min — a substantial standalone leaf
        let parent = rule_parent(vec![
            scrap_leaf("a", "tiny one"),
            scrap_leaf("b", "tiny two"),
            scrap_leaf("big", &big),
            scrap_leaf("c", "tiny three"),
            scrap_leaf("d", "tiny four"),
        ]);
        let out = coalesce_undersized_leaves(parent, 120, 600, 800);
        assert_eq!(out.children.len(), 3, "two merged runs + the untouched big leaf");
        assert_eq!(out.children[1].text.chars().count(), 200, "big leaf untouched");
    }

    #[test]
    fn coalesce_does_not_merge_distinct_named_provisions() {
        // FR-002 / spec edge case: genuinely short standalone rules each keep
        // their own unit — a run of distinct NAMED provisions must not merge.
        let named = |n: &str, body: &str| DocNode {
            level: NodeLevel::Article,
            label: n.to_string(),
            title: None,
            text: body.to_string(),
            citation: format!("Rule {n}"),
            unit_path: format!("Rule_{n}"),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: Vec::new(),
        };
        let doc = DocNode {
            level: NodeLevel::Document,
            label: String::new(),
            title: Some("x".into()),
            text: String::new(),
            citation: "x".into(),
            unit_path: "document".into(),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: vec![
                named("1", "Title. body."),
                named("2", "Definitions. body."),
                named("3", "Procedure. body."),
            ],
        };
        let out = coalesce_undersized_leaves(doc, 120, 600, 800);
        assert_eq!(out.children.len(), 3, "distinct named rules must not merge");
        assert_eq!(out.children[0].citation, "Rule 1");
        assert_eq!(out.children[2].citation, "Rule 3");
    }

    #[test]
    fn coalesce_leaves_isolated_short_clause_untouched() {
        let big = "y".repeat(200);
        let parent = rule_parent(vec![
            scrap_leaf("1", &big),
            scrap_leaf("2", "short isolated clause"), // lone scrap, not a run
            scrap_leaf("3", &big),
        ]);
        let out = coalesce_undersized_leaves(parent, 120, 600, 800);
        assert_eq!(out.children.len(), 3, "lone scrap is not a run — preserved");
        assert_eq!(out.children[1].label, "2", "isolated short clause preserved as-is");
    }

    #[test]
    fn coalesce_does_not_merge_substantial_clauses_noregress() {
        // SC-005: well-formed clauses (each >= min) must NOT be merged.
        let c = |n: &str| scrap_leaf(n, &"clause body ".repeat(15)); // ~180c each
        let parent = rule_parent(vec![c("1"), c("2"), c("3")]);
        let out = coalesce_undersized_leaves(parent, 120, 600, 800);
        assert_eq!(out.children.len(), 3, "substantial clauses must NOT merge (SC-005)");
    }

    #[test]
    fn coalesce_drops_empty_and_label_only_leaves() {
        // RC-D (2026-06-30): a blank chunk and a chunk whose only content echoes
        // its own label ("2" under label "2") are uncitable noise — dropped
        // outright. The real clause beside them survives untouched.
        let label_echo = DocNode {
            level: NodeLevel::SubClause,
            label: "2".into(), title: None, text: "2".into(),
            citation: "(2)".into(), unit_path: "Rule_42/2".into(),
            attached: vec![], footnote_refs: vec![], children: vec![],
        };
        let parent = rule_parent(vec![
            scrap_leaf("a", "   "),                            // whitespace-only → drop
            label_echo,                                        // pure label echo → drop
            scrap_leaf("b", "a real short clause body here"),  // kept
        ]);
        let out = coalesce_undersized_leaves(parent, 120, 600, 800);
        assert_eq!(out.children.len(), 1, "empty + label-only dropped, real clause kept");
        assert!(out.children[0].text.contains("real short clause"));
    }

    #[test]
    fn split_oversized_container_splits_pathological_body_but_keeps_short_index() {
        // RC-D (2026-06-30): a Part/Chapter normally keeps its short enumerated
        // child-index intact (Fix-B invariant), but a pathologically large body
        // (a table absorbed onto the container) is split into context-sized
        // `… part N` pieces instead of shipping one 40k uncitable chunk.
        let make = |text: &str| DocNode {
            level: NodeLevel::Part,
            label: "Part II".into(), title: None, text: text.into(),
            citation: "Part II".into(), unit_path: "Part_II".into(),
            attached: vec![], footnote_refs: vec![], children: vec![],
        };
        // Short index → untouched.
        let small = make("1. Foo  2. Bar  3. Baz");
        let r = split_oversized_leaves(small.clone(), 1500);
        assert_eq!(r.text, small.text, "short container index preserved");
        assert!(r.children.is_empty());

        // Pathological body (> oversize ceiling) → split, each piece context-sized.
        let huge = "A paragraph of moderate length with several words.\n\n".repeat(200); // ~10k chars
        let big = split_oversized_leaves(make(&huge), 1500);
        assert_eq!(big.text, "", "oversized container body lifted into children");
        assert!(big.children.len() > 1, "split into multiple citable pieces");
        for c in &big.children {
            assert!(c.text.chars().count() <= 1500, "each piece stays context-sized (<=1500c)");
            assert_eq!(c.citation, "Part II", "child citation derived from container");
        }
    }

    #[test]
    fn coalesce_caps_merged_units_at_hard_max() {
        let scraps: Vec<DocNode> =
            (0..20).map(|_| scrap_leaf("s", &"fragment text ".repeat(6))).collect(); // ~84c each
        let out = coalesce_undersized_leaves(rule_parent(scraps), 120, 600, 800);
        assert!(out.children.len() >= 2, "run exceeding hard_max splits into multiple units");
        for ch in &out.children {
            assert!(
                ch.text.chars().count() <= 800,
                "no unit exceeds hard_max: {}",
                ch.text.chars().count()
            );
        }
    }

    #[test]
    fn coalesce_merges_oversplit_paragraph_fragments() {
        // The real Sindh pattern (live re-index): an OCR-heavy node chopped by
        // split_oversized_leaves into many tiny `/p{N}` paragraph children
        // (e.g. Direct-Contracting at Part_II/(1)/(d)/p1..p26). Coalescing must
        // reassemble them — they are scraps by SIZE despite the /p marker.
        let children: Vec<DocNode> = (1..=26)
            .map(|i| DocNode {
                level: NodeLevel::Clause,
                label: format!("d part {i}"),
                title: None,
                text: format!("fragment line {i}"),
                citation: "(d)".into(),
                unit_path: format!("Part_II/(1)/(d)/p{i}"),
                attached: Vec::new(),
                footnote_refs: Vec::new(),
                children: Vec::new(),
            })
            .collect();
        let parent = DocNode {
            level: NodeLevel::Clause,
            label: "d".into(),
            title: None,
            text: String::new(),
            citation: "(d)".into(),
            unit_path: "Part_II/(1)/(d)".into(),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children,
        };
        let out = coalesce_undersized_leaves(parent, 120, 600, 800);
        assert!(
            out.children.len() < 26,
            "tiny /p fragments must coalesce, got {} units",
            out.children.len()
        );
    }

    #[test]
    fn finalize_still_splits_oversized_leaf_after_coalesce() {
        // T003a / FR-003: split-then-coalesce must keep oversize splitting.
        let para = "This is a sentence of the provision body. ".repeat(20); // ~840c
        let long = format!("{para}\n\n{para}"); // ~1680c, two paragraphs, > 1500 threshold
        let doc = DocNode {
            level: NodeLevel::Document,
            label: String::new(),
            title: Some("doc".into()),
            text: String::new(),
            citation: "doc".into(),
            unit_path: "document".into(),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: vec![DocNode {
                level: NodeLevel::Article,
                label: "1".into(),
                title: None,
                text: long,
                citation: "Article 1".into(),
                unit_path: "Art-1".into(),
                attached: Vec::new(),
                footnote_refs: Vec::new(),
                children: Vec::new(),
            }],
        };
        let out = finalize_tree(doc);
        let art = &out.children[0];
        assert!(art.text.is_empty(), "oversized leaf body lifted to paragraph children");
        assert!(art.children.len() >= 2, "oversized leaf split into paragraph children");
        assert!(
            art.children.iter().all(|c| c.children.is_empty()),
            "paragraph children must not be re-merged"
        );
    }
}
