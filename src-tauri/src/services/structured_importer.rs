//! Feature 005 — generic structured importer.
//!
//! Orchestrates: normalize → detect profile → detect headings → tree → persist.
//! Reuses Feature 003's `structure_parser::persist_tree`,
//! `structure_parser::split_oversized_leaves` (via `build_tree_from_headings`),
//! and `graph_builder::build_graph`.
//!
//! Caller (`indexer::do_index`) is responsible for: file extraction,
//! `chunks::delete_by_source` before persisting, embedding, and NER.

use crate::db::Db;

use crate::services::{doc_profiles, graph_builder, heading_detector, structure_parser, text_normalizer};
use crate::services::doc_profiles::DetectionSource;
use crate::services::structure_parser::DocNode;
use crate::db::repositories::graph as graph_repo;

/// Outcome of a structured-import attempt — what to persist and which UI
/// status to record.
pub struct ImportOutcome {
    pub tree: DocNode,
    pub doc_type: &'static str,
    pub doc_type_source: DetectionSource,
    /// Normalized text used to build the tree. Threaded to `persist` so chunk
    /// content matches the byte offsets recorded on the tree nodes.
    pub normalized_text: String,
    /// `true` when heading coverage cleared the threshold. `false` =
    /// caller should `structure_status='failed'` and fall back to size-based.
    pub structured: bool,
    /// US1 (D3): page-chrome blocks stripped from the body before tree-build.
    /// Persisted as separate `block_class='chrome'` chunks (kept + auditable,
    /// excluded from retrieval) — never attached under a provision.
    pub chrome_blocks: Vec<crate::services::parsers::chrome::ChromeBlock>,
    /// P1/D5: footnote/amendment-history blocks isolated from the body, persisted
    /// as separate retrievable `block_class='footnote'` chunks.
    pub footnote_blocks: Vec<crate::services::parsers::chrome::FootnoteBlock>,
    /// 013/iter6: front-matter sections (TOC / preface / committee …) lifted out
    /// of the body, persisted as separate retrievable `block_class='front_matter'`
    /// chunks (kept for summarisation / document comparison).
    pub front_matter_blocks: Vec<crate::services::parsers::front_matter::FrontMatterBlock>,
    /// 013/iter8: tail Schedule/Annex/Appendix sections lifted out of the body,
    /// persisted as separate retrievable `block_class='back_matter'` chunks.
    pub back_matter_blocks: Vec<crate::services::parsers::back_matter::BackMatterBlock>,
    /// 013/D12: `unit_path -> physical page` for each provision, mapped from the
    /// chrome-aligned page offsets. Consumed by `persist` to stamp `page_number`.
    pub page_by_path: std::collections::HashMap<String, i64>,
}

pub struct PersistStats {
    pub chunks:      usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
}

const MIN_HEADING_COVERAGE: f32 = 0.10;

/// Whether the Feature 005 pipeline is active. Default: on. Disable with
/// `ECHO_STRUCTURED_IMPORTER=0`.
pub fn enabled() -> bool {
    std::env::var("ECHO_STRUCTURED_IMPORTER").map(|v| v != "0").unwrap_or(true)
}

/// Bump this constant to force a corpus-wide re-index on the next app launch
/// (consumed by `enqueue_structure_reextract`). Starts at 1 with Feature 005.
/// v2 (2026-07-12): item-1 amendment-footnote parser fix (rules.rs / back_matter /
/// regex_pass) — re-extract every PDF so the corrected structure (E&D Rule 12, no
/// dup Rule 7, no spurious Part IV) is applied to already-indexed sources.
/// v3 (2026-07-12): Feature 023 structured-table capture — re-extract so detected
/// tables become whole-table + per-row units (RoB Schedule-I 39-department table).
/// v4 (2026-07-27): contents-listing leak (trace a72f1539). `front_matter` now
/// detects a table of contents that carries NO dot leaders, and the heading
/// sequence guard refuses to run when it would discard most of the Articles.
/// Before this, the Constitution's contents pages were minted as Articles, which
/// pushed the expected number past 280 and made the guard delete every real
/// Article below ~140 — 100 chunks carried `Article 154`, including the text of
/// Article 63. Re-extract every PDF so labels are rebuilt.
pub const STRUCTURED_IMPORTER_VERSION: i64 = 4;

/// A run of chunks this long sharing one label means headings were missed or
/// deleted — real provisions do not span this many chunks.
const LABEL_RUN_WARN: usize = 25;
/// Share of chunks whose text disagrees with their own label, above which the
/// parse is reported as degraded.
const MISMATCH_WARN_PCT: usize = 5;

/// Largest run of consecutive chunks carrying the same `unit_label`, with that
/// label. The single most diagnostic number about a parse: healthy documents
/// keep it in the tens, a leaked contents page pushes it into the hundreds.
fn largest_label_run(chunks: &[crate::db::repositories::chunks::Chunk]) -> (String, usize) {
    let mut best = (String::new(), 0usize);
    let mut cur_label: Option<&str> = None;
    let mut cur = 0usize;
    for c in chunks {
        let label = c.unit_label.as_deref().unwrap_or("");
        if Some(label) == cur_label {
            cur += 1;
        } else {
            cur_label = Some(label);
            cur = 1;
        }
        if cur > best.1 && !label.is_empty() {
            best = (label.to_string(), cur);
        }
    }
    best
}

/// Whether upstream chrome-strip (US1) is active. Default: on. Disable with
/// `ECHO_CHROME_STRIP=0` for A/B or revert without a rebuild.
fn chrome_strip_enabled() -> bool {
    std::env::var("ECHO_CHROME_STRIP").map(|v| v != "0").unwrap_or(true)
}

/// Feature 023 — remove every `⟦TBL:n⟧…⟦/TBL:n⟧` table region (marker lines and
/// the linearised rows between them) from `text`, returning the cleaned body and
/// re-derived page offsets. No-op (offsets untouched) when the text carries no
/// table markers. Form feeds never sit inside a region (reflow wraps only a
/// single page's rows), so the page COUNT is preserved and the offsets recompute
/// cleanly from the cleaned text.
fn strip_table_regions(text: &str, page_offsets: &[(u32, usize)]) -> (String, Vec<(u32, usize)>) {
    use crate::services::parsers::table::{is_table_marker_line, parse_table_marker};
    if !text.contains("\u{27E6}TBL:") {
        return (text.to_string(), page_offsets.to_vec());
    }
    // Scan PAGE BY PAGE. Two things fall out of that, and both were live defects:
    //
    // 1. A form feed can no longer sit on a line with a marker. Pages are joined
    //    with a bare `\x0c` (no newline), so a table starting at the top of a page
    //    used to arrive as `…last line of p.82\x0c⟦TBL:37⟧` — not a marker line,
    //    so the region never opened and its linearised rows stayed in the body to
    //    be minted as provisions. `reflow_with_sidecar` now newline-wraps each
    //    rewritten page; splitting here as well means neither half of that pair
    //    has to hold for the text to come out right.
    // 2. `in_region` resets at every page boundary. A region is written per page,
    //    so a state that survived one could only ever be a lost close marker — and
    //    that state DELETED every provision until the next table.
    //
    // The page count is preserved by construction (n pages in, n out), which is
    // what `pdftotext_page_offsets` counts. That matters: dropping a form feed
    // stopped the page counter advancing, and on `Manual for Development Projects
    // 2019 PC-I` (63 tables) 238 real pages collapsed to 149 stamped ones, with
    // 341 chunks piled onto p137 — each also inheriting the last-seen label.
    // Segment so a form feed ALWAYS stands alone. Pages are joined with a bare
    // `\x0c` and no newline, so a table opening at the top of a page arrived as
    // `…last line of p.82\x0c⟦TBL:37⟧` — one line, which `is_table_marker_line`
    // does not recognise. The region then never opened and its linearised rows
    // stayed in the body to be minted as provisions (Delegation: 111 duplicate
    // `clause` chunks; APT: a table row stored as `Rule 3`).
    let mut seg: Vec<&str> = Vec::new();
    for line in text.split_inclusive('\n') {
        let mut rest = line;
        while let Some(i) = rest.find('\u{0c}') {
            if i > 0 {
                seg.push(&rest[..i]);
            }
            seg.push("\u{0c}");
            rest = &rest[i + 1..];
        }
        if !rest.is_empty() {
            seg.push(rest);
        }
    }

    // Only segments between a MATCHED open/close pair are dropped. An unpaired
    // marker — one half lost to some future pass — drops itself and nothing
    // else: text kept as prose can still be read and cited, text deleted on a
    // guess is gone silently. Same bias as the extraction tiers, which fall back
    // rather than emit a scramble.
    let mut drop: Vec<bool> = vec![false; seg.len()];
    let mut open_at: Option<usize> = None;
    for (i, s) in seg.iter().enumerate() {
        let bare = s.trim_end_matches('\n');
        if !is_table_marker_line(bare) {
            continue;
        }
        drop[i] = true; // a marker is never content
        match parse_table_marker(bare) {
            Some((_, true)) => open_at = Some(i),
            Some((_, false)) => {
                if let Some(start) = open_at.take() {
                    // Linearised rows are re-emitted from `ExtractedTable`.
                    drop[start + 1..i].fill(true);
                }
            }
            None => {}
        }
    }
    if open_at.is_some() {
        log::warn!(
            "a table region was opened and never closed — its rows stay in the body (they will \
             also exist as table units). A marker was lost upstream."
        );
    }

    let mut out = String::with_capacity(text.len());
    for (i, s) in seg.iter().enumerate() {
        if !drop[i] {
            out.push_str(s);
            continue;
        }
        // A dropped segment gives up its text but never its page boundary:
        // `pdftotext_page_offsets` counts pages by form feed alone, and losing
        // one froze the page counter for the rest of the document. Measured on
        // `Manual for Development Projects 2019 PC-I`: 238 real pages collapsed
        // to 149 stamped ones, 341 chunks piled onto p137, each citing the wrong
        // page and inheriting the last-seen label.
        for _ in s.matches('\u{0c}') {
            out.push('\u{0c}');
        }
    }
    let new_offsets = if out.contains('\u{0c}') {
        crate::services::parsers::pdf::pdftotext_page_offsets(&out)
    } else {
        page_offsets.to_vec()
    };
    (out, new_offsets)
}

/// Run the pipeline up to tree construction. Caller decides whether to persist
/// (when `structured == true`) or fall through to flat hierarchical chunking.
/// The bare provision ordinal inside a node label. `DocNode::label` carries the
/// rendered citation (`"Article 54"`, `"Section 19A"`), while the catchline map
/// is keyed by the printed number alone (`"54"`, `"19A"`), so the two must be
/// bridged. Structural labels ("Preamble", "body") yield `None`.
fn provision_ordinal(label: &str) -> Option<&str> {
    static RE: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex_lite::Regex::new(r"(\d{1,3}[A-Z]?)\s*$").unwrap())
        .captures(label.trim())
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
}

/// Set each provision node's `title` from the catchline map. Returns how many
/// were applied.
///
/// Only fills a title that is absent — a title parsed from the heading line
/// itself is more specific to that document than a margin note, so it wins.
fn apply_provision_titles(
    node: &mut structure_parser::DocNode,
    titles: &std::collections::HashMap<String, String>,
) -> usize {
    let mut applied = 0;
    if node.level == structure_parser::NodeLevel::Article
        && node.title.as_deref().unwrap_or("").trim().is_empty()
    {
        if let Some(t) = provision_ordinal(&node.label).and_then(|n| titles.get(n)) {
            node.title = Some(t.clone());
            applied += 1;
        }
    }
    for child in &mut node.children {
        applied += apply_provision_titles(child, titles);
    }
    applied
}

/// Page-break guard (2026-07-28): count of form feeds — the page-identity
/// invariant every strip stage must preserve. See `page_guard`.
fn ff_count(s: &str) -> usize {
    s.matches('\u{0c}').count()
}

/// Scream when a pipeline stage LOST a page break. Both 2026-07-27 page
/// defects (header strip deleting the break on the header line; a table
/// region spanning a break) were silent for weeks because nothing measured
/// this invariant — the page counter just froze and every later provision
/// kept an earlier page. The guard turns that class of bug into a loud,
/// stage-named log line on the very first affected document.
fn page_guard(stage: &str, before: usize, after: usize, doc_title: &str) -> usize {
    if after < before {
        log::error!(
            "PAGE-BREAK LOST: stage '{stage}' reduced form feeds {before} → {after} \
             for {doc_title}; page numbers after this point will be wrong \
             (invariant: strip stages must re-emit the \\x0c of every deleted line)"
        );
    }
    after
}

/// Give the nodes `finalize_tree` invents an offset of their own (2026-07-28).
///
/// `build_tree_from_headings` records `offset_by_path` for HEADING nodes only.
/// `finalize_tree` then splits oversized leaves into `/pN` paragraph children
/// and merges scrap runs into `/mN` nodes — all created after that record, so
/// they were absent from the map and `collect_chunks` fell back to
/// `.or(parent_page)`. Every piece of a section that spans a page break then
/// claimed the section's FIRST page (tester report: a scholarship-powers
/// passage printed on p.18 was cited as p.27).
///
/// Locate each such node by its own text. Walk order is document order and the
/// cursor only moves forward, so the pass is linear in the body length. A node
/// whose text cannot be located keeps the old inherit-from-ancestor behaviour —
/// this can add precision, never remove it.
fn stamp_split_child_offsets(
    node: &structure_parser::DocNode,
    text: &str,
    cursor: &mut usize,
    offsets: &mut std::collections::HashMap<String, usize>,
) {
    if let Some(&own) = offsets.get(&node.unit_path) {
        // A heading node: its recorded offset is authoritative — resync the
        // cursor so later searches start from the right place.
        *cursor = (*cursor).max(own);
    } else {
        let body = node.text.trim();
        if !body.is_empty() {
            // Match on a PREFIX: `split_long_text` trims each piece and the
            // coalescer joins merged scraps, so only the head of the text is
            // guaranteed to be a verbatim substring of the body.
            let needle: String = body.chars().take(48).collect();
            if let Some(rel) = text.get(*cursor..).and_then(|hay| hay.find(&needle)) {
                let abs = *cursor + rel;
                offsets.insert(node.unit_path.clone(), abs);
                *cursor = abs;
            }
        }
    }
    for child in &node.children {
        stamp_split_child_offsets(child, text, cursor, offsets);
    }
}

pub fn import(
    raw_text: &str,
    doc_title: &str,
    force_profile: Option<&str>,
    page_offsets: &[(u32, usize)],
    provision_titles: &std::collections::HashMap<String, String>,
) -> ImportOutcome {
    let normalized = text_normalizer::normalize(raw_text);
    let mut ff = page_guard("normalize", ff_count(raw_text), ff_count(&normalized), doc_title);

    // RC-PG fix (2026-06-29 chunking audit): the caller's `page_offsets` are byte
    // offsets into the RAW extracted text, but `normalize` above shortens the text
    // (collapses 3+ space gutters, blank-line runs, leading tabs — legal `-layout`
    // PDFs are mostly gutter padding), so those raw byte-starts now point too far
    // ahead in `normalized`. Every provision then stamps to an EARLIER page and the
    // error compounds with depth (audit: SPPRA Rule 17 real p13 → stored p5, Rule 50
    // p35 → p14). `normalize` preserves form feeds, so re-derive the page map from
    // `normalized` — the exact text the strip chain + tree build operate on. Only
    // when form feeds are present (multi-page); a single-page/`.txt` body keeps the
    // caller's offsets (already `[(1,0)]`-equivalent, no drift).
    let realigned_offsets = if normalized.contains('\u{0c}') {
        Some(crate::services::parsers::pdf::pdftotext_page_offsets(&normalized))
    } else {
        None
    };
    let page_offsets: &[(u32, usize)] = realigned_offsets.as_deref().unwrap_or(page_offsets);

    // US1 (D3): strip page chrome BEFORE profile/heading/tree so a masthead /
    // page marker / contact line never inherits a provision label. The body
    // ('text') drives detection + tree-build; offsets stay self-consistent.
    let (text, chrome_blocks, footnote_blocks, body_page_offsets) = if chrome_strip_enabled() {
        let r = crate::services::parsers::chrome::strip_chrome(&normalized, page_offsets);
        (r.cleaned, r.chrome, r.footnotes, r.cleaned_page_offsets)
    } else {
        (normalized, Vec::new(), Vec::new(), page_offsets.to_vec())
    };

    // 013/iter6: lift the initial front-matter region (Table of Contents /
    // Preface / Foreword / Abbreviations / Drafting Committee / List of Annexes)
    // OUT of the body BEFORE heading detection — its dotted TOC entries and
    // numbered rosters were being promoted to DUPLICATE provisions (constitution
    // 297 duplicated articles; doc-06 committee roster as `Article 1..46`). Each
    // section is kept as a separate retrievable `front_matter` chunk (useful for
    // summarisation + document comparison). Safe: detection starts only near the
    // document top and never crosses into the body.
    ff = page_guard("strip_chrome", ff, ff_count(&text), doc_title);
    let (text, front_matter_blocks, body_page_offsets) = {
        let fm = crate::services::parsers::front_matter::strip_front_matter(&text, &body_page_offsets);
        (fm.cleaned, fm.blocks, fm.cleaned_page_offsets)
    };
    let _ = page_guard("strip_front_matter", ff, ff_count(&text), doc_title);

    // 013/iter8: lift the tail Schedule/Annex/Appendix cluster OUT of the body so
    // its numbered lists (e.g. a First Schedule "list of laws") never become
    // false provisions ("Article 4" = the name of a 1972 law). Each section is
    // kept as a retrievable `back_matter` chunk. Only the contiguous tail cluster
    // reaching EOF (starting in the last 20%) is taken — mid-body schedules are
    // untouched.
    let (text, back_matter_blocks, body_page_offsets) = {
        let bm = crate::services::parsers::back_matter::strip_back_matter(&text, &body_page_offsets);
        (bm.cleaned, bm.blocks, bm.cleaned_page_offsets)
    };

    // Feature 023: lift detected TABLE regions (`⟦TBL:n⟧…⟦/TBL:n⟧`, written by the
    // reflow sidecar) OUT of the body BEFORE heading detection — otherwise the
    // clause parser shreds the linearised rows into hundreds of micro-chunks (the
    // RoB Schedule-I 39-department → 292-fragment defect). The prose above/below a
    // table sits OUTSIDE the markers and is preserved. The tables' actual content
    // is re-emitted as whole-table + per-row units by `persist` from the
    // structured `ExtractedTable` list. Form feeds never fall inside a region, so
    // recomputed page offsets keep their page numbers.
    // NOTE: no guard around `strip_back_matter` — the tail cluster leaves the
    // body WITH its pages (legitimate count drop); remaining pages' feeds all
    // sit before the cut and are untouched. Re-baseline the count after it.
    ff = ff_count(&text);
    let (text, body_page_offsets) = strip_table_regions(&text, &body_page_offsets);
    let _ = page_guard("strip_table_regions", ff, ff_count(&text), doc_title);

    // Title-first classification (ADR-0019 follow-up): the document's title is
    // authoritative over body-word frequency, which misclassified an Act
    // ("…Procurement Act 2009" → manual) and would misclassify a rules
    // compendium that quotes "this Act" 47× (→ act). Body scoring is the
    // fallback for type-less titles.
    let (profile, source) =
        doc_profiles::detect_profile_titled(&text, doc_title, force_profile);
    // Phase 5 / D1: resolve the citation unit word from the document's own usage.
    // A `manual`-classified Articles document (default unit "Section") whose body
    // says "Article 251" / "this Article" re-mints provision labels as "Article".
    // A genuine unit conflict resolves to `Unresolved` — labels keep the profile
    // default and are surfaced for review rather than minted wrong (refuse-or-flag).
    let unit_override = match doc_profiles::resolve_citation_unit(&text, profile) {
        doc_profiles::CitationUnit::Word(w) if !w.eq_ignore_ascii_case(profile.citation_unit) => {
            log::info!("Phase5/D1: citation unit resolved to '{w}' (profile default '{}') for {doc_title}", profile.citation_unit);
            Some(w)
        }
        doc_profiles::CitationUnit::Word(_) => None,
        doc_profiles::CitationUnit::Unresolved => {
            log::warn!("Phase5/D1: citation unit AMBIGUOUS for {doc_title}; keeping profile default '{}' (provisions left for review)", profile.citation_unit);
            None
        }
    };
    let headings = heading_detector::regex_pass::scan_with_profile_unit(&text, profile, unit_override);

    let coverage = if text.is_empty() {
        0.0
    } else if headings.is_empty() {
        0.0
    } else {
        // Sum of span lengths each heading "owns" — first heading's offset to
        // next heading or EOF.
        let mut covered: usize = 0;
        for (i, h) in headings.iter().enumerate() {
            let end = headings.get(i + 1).map(|n| n.offset).unwrap_or(text.len());
            covered = covered.saturating_add(end.saturating_sub(h.offset));
        }
        covered as f32 / text.len() as f32
    };

    let structured = !headings.is_empty() && coverage >= MIN_HEADING_COVERAGE;
    let (mut tree, mut offset_by_path) =
        structure_parser::build_tree_from_headings(&text, &headings, doc_title, profile);
    // Paragraph-split / merged-scrap nodes have no recorded offset — give them
    // one so they stamp their OWN page instead of inheriting the section's.
    {
        let mut cursor = 0usize;
        let before = offset_by_path.len();
        stamp_split_child_offsets(&tree, &text, &mut cursor, &mut offset_by_path);
        log::debug!(
            "split-child offsets: {} nodes located (map {} → {}) for {doc_title}",
            offset_by_path.len() - before,
            before,
            offset_by_path.len()
        );
    }

    // Marginal catchlines → provision titles. `DocNode::title` is documented as
    // "an Article's marginal note" and has never been populated for statutes,
    // because the note lives in a separate print column the text extractor used
    // to flatten into the body line. The sidecar now separates it and hands back
    // a number→title map, so a citation can read "Article 54 — Summoning and
    // prorogation of Majlis-e-Shoora" instead of a bare number.
    if !provision_titles.is_empty() {
        let applied = apply_provision_titles(&mut tree, provision_titles);
        log::info!(
            "provision titles: applied {applied} of {} catchline titles to {doc_title}",
            provision_titles.len()
        );
    }

    // 013/D12: map each provision's source offset to its physical page using the
    // chrome-aligned page offsets, so persisted chunks carry `page_number`.
    let page_by_path: std::collections::HashMap<String, i64> = offset_by_path
        .iter()
        .map(|(path, &off)| {
            (path.clone(), crate::services::parsers::chrome::page_for(off, &body_page_offsets) as i64)
        })
        .collect();

    ImportOutcome {
        tree,
        doc_type: profile.doc_type,
        doc_type_source: source,
        normalized_text: text,
        structured,
        chrome_blocks,
        footnote_blocks,
        front_matter_blocks,
        back_matter_blocks,
        page_by_path,
    }
}

/// Persist the imported tree to chunks + graph_nodes + graph_edges + sources
/// columns. Caller MUST have already deleted any prior chunks for this source
/// (call `chunks::delete_by_source` first).
pub async fn persist(
    pool: &Db,
    source_id: &str,
    outcome: &ImportOutcome,
    reconstructed_rows: &[String],
    reconstruction_method: &str,
    tables: &[crate::services::parsers::table::ExtractedTable],
) -> anyhow::Result<PersistStats> {
    // 1. Chunks via existing structure_parser persist path.
    let chunks = structure_parser::persist_tree_with_pages(
        pool,
        source_id,
        &outcome.tree,
        &outcome.page_by_path,
    )
    .await?;

    // Fix-2 invariant guard (heading-mislabel defect 2026-06-16): warn — never
    // block — when a persisted chunk's content begins with a provision heading
    // whose number is absent from its label (an absorbed/mislabeled heading).
    // After Fix-1 detection this set should be empty on every rules-profile doc;
    // a non-empty warning means a new doc shape slipped past detection.
    let pairs: Vec<(&str, &str)> = chunks
        .iter()
        .filter_map(|c| c.unit_label.as_deref().map(|l| (l, c.content.as_str())))
        .collect();
    let mismatches = structure_parser::detect_label_mismatches(pairs);
    if !mismatches.is_empty() {
        log::warn!(
            "heading-mislabel invariant: {} chunk(s) in source {} start with an undetected provision heading (first: label={:?} content_no={} snippet={:?})",
            mismatches.len(),
            source_id,
            mismatches[0].unit_label,
            mismatches[0].content_number,
            mismatches[0].snippet,
        );
    }

    // Parse-quality signal (2026-07-27). The two numbers that reveal a bad parse
    // are the LARGEST run of chunks sharing one label, and the share of chunks
    // whose label disagrees with their own text. Both were already computable and
    // neither was reported, so a document could ingest "successfully" with a
    // hundred chunks stamped `Article 154`. Log them at ingest so the next bad
    // parse is visible in the journal instead of waiting to surface in a trace.
    let (worst_label, worst_run) = largest_label_run(&chunks);
    let mismatch_pct = if chunks.is_empty() { 0 } else { mismatches.len() * 100 / chunks.len() };
    if worst_run >= LABEL_RUN_WARN || mismatch_pct >= MISMATCH_WARN_PCT {
        log::warn!(
            "parse quality (source {source_id}): largest label run {worst_run} chunks \
             on {worst_label:?}; {}/{} chunks ({mismatch_pct}%) disagree with their label. \
             A long run usually means provision headings were missed or deleted.",
            mismatches.len(),
            chunks.len(),
        );
    } else {
        log::info!(
            "parse quality (source {source_id}): {} chunks, largest label run {worst_run}, \
             {mismatch_pct}% label mismatch",
            chunks.len(),
        );
    }

    // US3 (D4): promote the label≠content invariant from a warning to a FLAG —
    // a chunk whose content begins with a provision number disagreeing with its
    // label is mislabeled; mark it `label_status='unresolved'` so it is never
    // presented as a confident-but-wrong citation (refuse-or-flag). The full
    // re-segmentation split is a separate, more invasive step (deferred).
    let mislabeled = flag_label_content_mismatches(pool, &chunks).await?;
    if mislabeled > 0 {
        log::warn!("US3: flagged {mislabeled} label≠content chunk(s) unresolved in source {source_id}");
    }

    // US1 (D3): persist stripped page-chrome as separate, auditable
    // `block_class='chrome'` chunks (kept, never deleted) and mark them
    // `is_boilerplate=TRUE` so the existing retrieval filter excludes them from
    // ranking. Supersedes the post-persist boilerplate flag (which matched 0
    // live because junk swept under a provision had already inherited a label).
    let chrome_n = persist_chrome_chunks(pool, source_id, &outcome.chrome_blocks, chunks.len() as i64).await?;
    if chrome_n > 0 {
        log::info!("US1: stored {chrome_n} chrome chunk(s) (excluded from retrieval) in source {source_id}");
    }

    // P1/D5: persist isolated footnotes as separate `block_class='footnote'`
    // chunks — KEPT retrievable (not boilerplate) for audit + amendment capture
    // (US5), with `label_status='unresolved'` since the marker anchor is not yet
    // resolved (refuse-or-flag; never anchored by same-page heuristic).
    let footnote_n = persist_footnote_chunks(
        pool,
        source_id,
        &outcome.footnote_blocks,
        chunks.len() as i64 + chrome_n as i64,
    )
    .await?;
    if footnote_n > 0 {
        log::info!("P1/D5: stored {footnote_n} footnote chunk(s) (block_class='footnote', unresolved anchor) in source {source_id}");
    }

    // 013/iter6: persist front-matter sections (TOC / preface / committee …) as
    // separate retrievable `block_class='front_matter'` chunks — kept (not
    // boilerplate) for summarisation + document comparison.
    let fm_n = persist_front_matter_chunks(
        pool,
        source_id,
        &outcome.front_matter_blocks,
        chunks.len() as i64 + chrome_n as i64 + footnote_n as i64,
    )
    .await?;
    if fm_n > 0 {
        log::info!("iter6: stored {fm_n} front-matter chunk(s) (block_class='front_matter') in source {source_id}");
    }

    // 013/iter8: persist tail Schedule/Annex/Appendix sections as retrievable
    // `block_class='back_matter'` chunks (kept — schedules are operative content).
    let bm_n = persist_back_matter_chunks(
        pool,
        source_id,
        &outcome.back_matter_blocks,
        chunks.len() as i64 + chrome_n as i64 + footnote_n as i64 + fm_n as i64,
    )
    .await?;
    if bm_n > 0 {
        log::info!("iter8: stored {bm_n} back-matter chunk(s) (block_class='back_matter') in source {source_id}");
    }

    // US5 (T033) / Phase 9a: tag chunks that carry a verbatim reconstructed table
    // row as `block_class='table_row'` + `reconstruction_method=<extractor>`
    // (`pdfplumber` for the sidecar, `schema_anchored` for the projection engine)
    // so reflow coverage — and the sidecar's contribution — is measurable in the
    // DB. Kept retrievable (NOT boilerplate) — table rows are real, citable content.
    let method = if reconstruction_method.is_empty() { "schema_anchored" } else { reconstruction_method };
    let table_n = tag_table_row_chunks(pool, &chunks, reconstructed_rows, method).await?;
    if table_n > 0 {
        log::info!("US5: tagged {table_n} reconstructed table-row chunk(s) ({method}) in source {source_id}");
    }

    // US2 (D5): flag chunks whose resolved citation is still a bare sub-marker
    // (no named ancestor) as `label_status='unresolved'` — never a confident
    // -but-wrong label. Surfaced for review rather than silently mislabeled.
    let unresolved = flag_unresolved_labels(pool, &chunks).await?;
    if unresolved > 0 {
        log::warn!("US2: {unresolved} chunk(s) in source {source_id} have unresolved (bare) citations");
    }

    // 2. Graph from tree + persisted chunks.
    let (mut nodes, mut edges) = graph_builder::build_graph(&outcome.tree, &chunks);

    // Feature 023: emit the detected tables as first-class units — one whole-table
    // chunk (unit_type='table', markdown) + one chunk per row (unit_type=
    // 'table_row'), mirrored into the graph as a 'table' node CONTAINS its
    // 'table_row' children (attached under the document root). Counting/list/
    // summary then see the complete table instead of shredded fragments.
    let table_chunk_n = persist_table_units(
        pool,
        source_id,
        tables,
        (chunks.len()
            + chrome_n
            + footnote_n
            + fm_n
            + bm_n) as i64,
        &mut nodes,
        &mut edges,
    )
    .await?;
    if table_chunk_n > 0 {
        log::info!(
            "Feature 023: stored {} structured-table unit(s) from {} table(s) in source {source_id}",
            table_chunk_n,
            tables.len()
        );
    }

    graph_repo::insert_nodes(pool, &nodes).await?;
    graph_repo::insert_edges(pool, &edges).await?;

    // US5 (T037/T038): capture amendment events from the isolated footnotes
    // (Phase 3) into `amendment_events`, anchored to the provision on the
    // footnote's page. Deterministic, verbatim instrument; an ambiguous or
    // missing anchor is SKIPPED (the footnote chunk already carries
    // `label_status='unresolved'`) — never an invented link.
    let amend_n = persist_amendment_events(pool, &outcome.footnote_blocks, &chunks, &nodes).await?;
    if amend_n > 0 {
        log::info!("US5: captured {amend_n} amendment event(s) from footnotes in source {source_id}");
    }

    // 3. Update source columns.
    sqlx::query(
        "UPDATE sources SET doc_type = $1, doc_type_source = $2, structure_status = $3, structure_importer_version = $5 WHERE id = $4",
    )
    .bind(outcome.doc_type)
    .bind(outcome.doc_type_source.as_str())
    .bind(if outcome.structured { "structured" } else { "failed" })
    .bind(source_id)
    .bind(STRUCTURED_IMPORTER_VERSION)
    .execute(pool)
    .await?;

    Ok(PersistStats { chunks: chunks.len(), graph_nodes: nodes.len(), graph_edges: edges.len() })
}

/// US1 (D3): persist stripped chrome blocks as separate chunks classified
/// `block_class='chrome'` + `is_boilerplate=TRUE` (kept, auditable, excluded
/// from retrieval). `start_index` continues the source's chunk_index sequence
/// after the provision chunks. Never attached under a provision node.
async fn persist_chrome_chunks(
    pool: &Db,
    source_id: &str,
    chrome: &[crate::services::parsers::chrome::ChromeBlock],
    start_index: i64,
) -> anyhow::Result<usize> {
    if chrome.is_empty() {
        return Ok(0);
    }
    use crate::db::repositories::chunks::NewChunk;
    let rows: Vec<NewChunk> = chrome
        .iter()
        .enumerate()
        .map(|(i, b)| NewChunk {
            source_id: source_id.to_string(),
            chunk_index: start_index + i as i64,
            content: b.content.clone(),
            char_start: 0,
            char_end: b.content.chars().count() as i64,
            page_number: Some(b.page as i64),
            token_count: b.content.split_whitespace().count() as i64,
            parent_chunk_id: None,
            unit_type: "leaf".to_string(),
            unit_label: None,
            unit_path: None,
            domain_metadata: None,
        })
        .collect();
    let inserted = crate::db::repositories::chunks::bulk_insert(pool, rows).await?;
    let ids: Vec<String> = inserted.iter().map(|c| c.id.clone()).collect();
    sqlx::query("UPDATE chunks SET block_class = 'chrome', is_boilerplate = TRUE WHERE id = ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await?;
    Ok(ids.len())
}

/// 013/iter6: persist front-matter sections as separate `block_class='front_matter'`
/// chunks. KEPT retrievable (NOT boilerplate) — a TOC / preface / committee list is
/// real, citable content useful for "what is this document" and document comparison.
/// The `section_kind` is recorded in `domain_metadata`. `start_index` continues the
/// source's chunk_index sequence.
async fn persist_front_matter_chunks(
    pool: &Db,
    source_id: &str,
    blocks: &[crate::services::parsers::front_matter::FrontMatterBlock],
    start_index: i64,
) -> anyhow::Result<usize> {
    if blocks.is_empty() {
        return Ok(0);
    }
    use crate::db::repositories::chunks::NewChunk;
    // A pathologically large front-matter body (e.g. the constitution TOC at
    // 42,658c) is split into context-sized pieces so it never ships as one
    // uncitable 40k blob; normal-sized sections stay a single chunk. `unit_type`
    // has a CHECK constraint (no 'front_matter'); use 'leaf' like chrome/footnote
    // chunks. The section classification lives in `block_class='front_matter'` +
    // `section_kind` (domain_metadata) + the `unit_label` (kind).
    let mut rows: Vec<NewChunk> = Vec::with_capacity(blocks.len());
    let mut idx = start_index;
    for b in blocks {
        let pieces = crate::services::structure_parser::split_block_for_persist(&b.content);
        for piece in pieces {
            rows.push(NewChunk {
                source_id: source_id.to_string(),
                chunk_index: idx,
                char_start: 0,
                char_end: piece.chars().count() as i64,
                page_number: Some(b.page as i64),
                token_count: piece.split_whitespace().count() as i64,
                parent_chunk_id: None,
                unit_type: "leaf".to_string(),
                unit_label: Some(b.kind.clone()),
                unit_path: None,
                domain_metadata: Some(format!("{{\"section_kind\":\"{}\"}}", b.kind)),
                content: piece,
            });
            idx += 1;
        }
    }
    let inserted = crate::db::repositories::chunks::bulk_insert(pool, rows).await?;
    let ids: Vec<String> = inserted.iter().map(|c| c.id.clone()).collect();
    // Kept retrievable (NOT is_boilerplate); just tag the block_class.
    sqlx::query("UPDATE chunks SET block_class = 'front_matter' WHERE id = ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await?;
    Ok(ids.len())
}

/// 013/iter8: persist tail Schedule/Annex/Appendix sections as
/// `block_class='back_matter'` chunks. KEPT retrievable (schedules are operative
/// legal content) with `section_kind` in `domain_metadata`. Same shape as
/// `persist_front_matter_chunks`; `unit_type='leaf'` (CHECK constraint).
async fn persist_back_matter_chunks(
    pool: &Db,
    source_id: &str,
    blocks: &[crate::services::parsers::back_matter::BackMatterBlock],
    start_index: i64,
) -> anyhow::Result<usize> {
    if blocks.is_empty() {
        return Ok(0);
    }
    use crate::db::repositories::chunks::NewChunk;
    // Schedules/annexes are operative legal content and can be very large (live:
    // 17,708c / 10,504c). Split a body above the oversize ceiling into citable
    // pieces rather than shipping one 17k blob; smaller sections stay whole.
    let mut rows: Vec<NewChunk> = Vec::with_capacity(blocks.len());
    let mut idx = start_index;
    for b in blocks {
        let pieces = crate::services::structure_parser::split_block_for_persist(&b.content);
        for piece in pieces {
            rows.push(NewChunk {
                source_id: source_id.to_string(),
                chunk_index: idx,
                char_start: 0,
                char_end: piece.chars().count() as i64,
                page_number: Some(b.page as i64),
                token_count: piece.split_whitespace().count() as i64,
                parent_chunk_id: None,
                unit_type: "leaf".to_string(),
                unit_label: Some(b.kind.clone()),
                unit_path: None,
                domain_metadata: Some(format!("{{\"section_kind\":\"{}\"}}", b.kind)),
                content: piece,
            });
            idx += 1;
        }
    }
    let inserted = crate::db::repositories::chunks::bulk_insert(pool, rows).await?;
    let ids: Vec<String> = inserted.iter().map(|c| c.id.clone()).collect();
    sqlx::query("UPDATE chunks SET block_class = 'back_matter' WHERE id = ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await?;
    Ok(ids.len())
}

/// Feature 023 — persist each detected table as a whole-table chunk
/// (`unit_type='table'`, markdown) plus one chunk per row (`unit_type='table_row'`,
/// header-labelled cells), and mirror them into the graph: a `table` node that
/// `contains` its `table_row` children, attached under the document root node.
/// Appends the new graph nodes/edges to `nodes`/`edges` (inserted by the caller).
/// Returns the number of chunks emitted (`Σ 1 + rows` per non-empty table).
async fn persist_table_units(
    pool: &Db,
    source_id: &str,
    tables: &[crate::services::parsers::table::ExtractedTable],
    start_index: i64,
    nodes: &mut Vec<crate::db::repositories::graph::GraphNode>,
    edges: &mut Vec<crate::db::repositories::graph::GraphEdge>,
) -> anyhow::Result<usize> {
    if tables.is_empty() {
        return Ok(0);
    }
    use crate::db::repositories::chunks::NewChunk;
    use crate::db::repositories::graph::GraphNode;

    // Attach tables under the document root graph node (if the tree produced one).
    let doc_node_id: Option<String> = nodes
        .iter()
        .find(|n| n.node_type == "document")
        .map(|n| n.id.clone());

    let mut idx = start_index;
    let mut emitted = 0usize;
    for t in tables {
        if t.rows.is_empty() {
            continue; // no usable rows ⇒ no table unit (contract C-6)
        }
        // Cite the table by the name it prints, falling back to its page. A
        // stitched table spans pages, so the span is read off the rows rather
        // than restated — `t.page` is only where it starts.
        let last_page = t.rows.iter().map(|r| r.page).max().unwrap_or(t.page);
        let where_ = if last_page > t.page {
            format!("pp.{}-{}", t.page, last_page)
        } else {
            format!("p.{}", t.page)
        };
        let label = match &t.caption {
            Some(c) => format!("{c} ({where_})"),
            None => format!("Table ({where_})"),
        };
        let path = format!("Table-{}", t.id);

        // Whole-table chunk: markdown header + all rows.
        let mut whole_rows: Vec<NewChunk> = Vec::with_capacity(1 + t.rows.len());
        whole_rows.push(NewChunk {
            source_id: source_id.to_string(),
            chunk_index: idx,
            char_start: 0,
            char_end: 0,
            page_number: Some(t.page as i64),
            token_count: 0,
            parent_chunk_id: None,
            unit_type: "table".to_string(),
            unit_label: Some(label.clone()),
            unit_path: Some(path.clone()),
            domain_metadata: Some(format!("{{\"table_id\":{},\"rows\":{}}}", t.id, t.rows.len())),
            content: render_table_markdown(t.caption.as_deref(), &t.header, &t.rows),
        });
        idx += 1;

        // Per-row chunks: header-labelled cells, each stamped with the page it
        // was printed on (not the table's first page).
        for (i, row) in t.rows.iter().enumerate() {
            whole_rows.push(NewChunk {
                source_id: source_id.to_string(),
                chunk_index: idx,
                char_start: 0,
                char_end: 0,
                page_number: Some(row.page as i64),
                token_count: 0,
                parent_chunk_id: None,
                unit_type: "table_row".to_string(),
                unit_label: Some(format!("{label} row {}", i + 1)),
                unit_path: Some(format!("{path}/row-{}", i + 1)),
                domain_metadata: Some(format!("{{\"table_id\":{},\"row\":{}}}", t.id, i + 1)),
                content: render_row_with_header(&t.header, &row.cells),
            });
            idx += 1;
        }

        let inserted = crate::db::repositories::chunks::bulk_insert(pool, whole_rows).await?;
        emitted += inserted.len();

        // Graph: table node (chunk[0]) contains each row node (chunk[1..]).
        if let Some((table_chunk, row_chunks)) = inserted.split_first() {
            let table_node_id = uuid::Uuid::new_v4().to_string();
            nodes.push(GraphNode {
                id: table_node_id.clone(),
                chunk_id: table_chunk.id.clone(),
                node_type: "table".to_string(),
                citation: Some(label.clone()),
                metrics: None,
            });
            if let Some(doc) = &doc_node_id {
                edges.push(new_contains_edge(doc, &table_node_id));
            }
            for rc in row_chunks {
                let row_node_id = uuid::Uuid::new_v4().to_string();
                nodes.push(GraphNode {
                    id: row_node_id.clone(),
                    chunk_id: rc.id.clone(),
                    node_type: "table_row".to_string(),
                    citation: None,
                    metrics: None,
                });
                edges.push(new_contains_edge(&table_node_id, &row_node_id));
            }
        }

        // Tag the whole-table + row chunks so they are recognisably table-derived.
        let ids: Vec<String> = inserted.iter().map(|c| c.id.clone()).collect();
        sqlx::query("UPDATE chunks SET block_class = 'table' WHERE id = ANY($1)")
            .bind(&ids)
            .execute(pool)
            .await?;
    }
    Ok(emitted)
}

fn new_contains_edge(from: &str, to: &str) -> crate::db::repositories::graph::GraphEdge {
    crate::db::repositories::graph::GraphEdge {
        id: uuid::Uuid::new_v4().to_string(),
        from_node: from.to_string(),
        to_node: to.to_string(),
        edge_type: "contains".to_string(),
        weight: 1.0,
    }
}

/// Render a detected table as a compact GitHub-flavoured markdown table (header
/// row + separator + one line per body row), under the name the table prints.
/// Countable by body-row lines.
fn render_table_markdown(
    caption: Option<&str>,
    header: &[String],
    rows: &[crate::services::parsers::table::TableRow],
) -> String {
    let ncols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(header.len());
    if ncols == 0 {
        return String::new();
    }
    let cell = |s: &str| s.trim().replace('|', "\\|").replace('\n', " ");
    let mut out = String::new();
    if let Some(c) = caption.map(|c| cell(c)).filter(|c| !c.is_empty()) {
        out.push_str(&format!("{c}\n\n"));
    }
    // Header (synthesize generic labels if the detected header is blank).
    let hdr: Vec<String> = (0..ncols)
        .map(|i| {
            header
                .get(i)
                .map(|h| cell(h))
                .filter(|h| !h.is_empty())
                .unwrap_or_else(|| format!("col{}", i + 1))
        })
        .collect();
    out.push_str(&format!("| {} |\n", hdr.join(" | ")));
    out.push_str(&format!("| {} |\n", vec!["---"; ncols].join(" | ")));
    for r in rows {
        let cells: Vec<String> =
            (0..ncols).map(|i| cell(r.cells.get(i).map(String::as_str).unwrap_or(""))).collect();
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    out.trim_end().to_string()
}

/// Render one table row as header-labelled cells (`"Header: cell | Header2: cell2"`);
/// keeps the amendment/editorial note that travels inside a cell.
fn render_row_with_header(header: &[String], row: &[String]) -> String {
    row.iter()
        .enumerate()
        .filter(|(_, c)| !c.trim().is_empty())
        .map(|(i, c)| match header.get(i).map(|h| h.trim()).filter(|h| !h.is_empty()) {
            Some(h) => format!("{h}: {}", c.trim()),
            None => c.trim().to_string(),
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// US5 (T038): the provision an isolated footnote amends — the SINGLE distinct
/// Article citation present on the footnote's page. Ambiguous (more than one
/// Article on the page) or absent → `None`: the footnote stays flagged
/// `unresolved` and no amendment row is written. Conservative by design — a
/// missing event is preferable to a wrong anchor (refuse-or-flag).
fn anchor_citation_for_page(
    page: i64,
    provisions: &[crate::db::repositories::chunks::Chunk],
) -> Option<String> {
    let mut found: Option<String> = None;
    for c in provisions {
        if c.unit_type != "article" || c.page_number != Some(page) {
            continue;
        }
        if let Some(lbl) = &c.unit_label {
            match &found {
                None => found = Some(lbl.clone()),
                Some(f) if f != lbl => return None, // >1 distinct Article ⇒ ambiguous
                _ => {}
            }
        }
    }
    found
}

/// The leading ordinal marker a footnote carries (`14. Substituted by …`,
/// `[18. Response Time …`). Legal drafting prints the same digit as an in-body
/// superscript beside the amendment bracket (`14[inserted text]` / `[…]14`), so
/// this marker uniquely identifies WHICH provision the footnote amends when a
/// page holds several. Tolerates a leading amendment bracket and whitespace.
fn footnote_marker(content: &str) -> Option<String> {
    let s = content.trim_start();
    let s = s.strip_prefix('[').map(str::trim_start).unwrap_or(s);
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    (!digits.is_empty()).then_some(digits)
}

/// RC-F (2026-07-01): resolve the provision an isolated footnote amends. Fast
/// path is the conservative single-Article-on-page rule
/// ([`anchor_citation_for_page`]). When a page holds SEVERAL provisions — the
/// common case that previously dropped to `None` — rescue it by matching the
/// footnote's marker digit to the in-body amendment marker (`N[` / `]N`) and
/// anchoring only when exactly one same-page provision carries it. Still
/// refuse-or-flag: an ambiguous marker (0 or >1 matches) yields `None`.
fn anchor_citation_for_footnote(
    footnote: &crate::services::parsers::chrome::FootnoteBlock,
    provisions: &[crate::db::repositories::chunks::Chunk],
) -> Option<String> {
    let page = footnote.page as i64;
    if let Some(cite) = anchor_citation_for_page(page, provisions) {
        return Some(cite);
    }
    let marker = footnote_marker(&footnote.content)?;
    let needle_pre = format!("{marker}[");
    let needle_post = format!("]{marker}");
    let mut hits: Vec<String> = provisions
        .iter()
        .filter(|c| c.unit_type == "article" && c.page_number == Some(page))
        .filter(|c| c.content.contains(&needle_pre) || c.content.contains(&needle_post))
        .filter_map(|c| c.unit_label.clone())
        .collect();
    hits.sort();
    hits.dedup();
    (hits.len() == 1).then(|| hits.remove(0))
}

/// US5 (T037/T038): parse each isolated footnote into an amendment event and
/// persist it against the anchored provision's graph node. Capture only — no
/// `amended_by` edge is created (the instrument is not a graph node; see
/// `graph_builder` note), and an unanchored/unparseable footnote is skipped.
async fn persist_amendment_events(
    pool: &Db,
    footnotes: &[crate::services::parsers::chrome::FootnoteBlock],
    provisions: &[crate::db::repositories::chunks::Chunk],
    nodes: &[crate::db::repositories::graph::GraphNode],
) -> anyhow::Result<usize> {
    if footnotes.is_empty() {
        return Ok(0);
    }
    use crate::services::legal_ir::amendment_parser::parse_amendment;
    let cite_to_node: std::collections::HashMap<&str, &str> = nodes
        .iter()
        .filter_map(|n| n.citation.as_deref().map(|c| (c, n.id.as_str())))
        .collect();

    let mut n = 0usize;
    for fb in footnotes {
        let Some(ev) = parse_amendment(&fb.content) else { continue };
        let Some(cite) = anchor_citation_for_footnote(fb, provisions) else { continue };
        let Some(node_id) = cite_to_node.get(cite.as_str()) else { continue };
        sqlx::query(
            "INSERT INTO amendment_events(node_id, kind, by_instrument, effective_date, source_page) \
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(*node_id)
        .bind(ev.kind.as_str())
        .bind(&ev.by_instrument)
        .bind(ev.effective_date.as_deref())
        .bind(fb.page as i64)
        .execute(pool)
        .await?;
        n += 1;
    }
    Ok(n)
}

/// P1/D5: persist isolated footnote blocks as separate `block_class='footnote'`
/// chunks. Unlike chrome, footnotes are KEPT retrievable (`is_boilerplate`
/// stays FALSE) — amendment history has citation value (US5). The marker anchor
/// is not yet resolved, so each is flagged `label_status='unresolved'` rather
/// than anchored by a same-page guess. `start_index` continues the chunk_index
/// sequence after the provision + chrome chunks.
async fn persist_footnote_chunks(
    pool: &Db,
    source_id: &str,
    footnotes: &[crate::services::parsers::chrome::FootnoteBlock],
    start_index: i64,
) -> anyhow::Result<usize> {
    if footnotes.is_empty() {
        return Ok(0);
    }
    use crate::db::repositories::chunks::NewChunk;
    let rows: Vec<NewChunk> = footnotes
        .iter()
        .enumerate()
        .map(|(i, b)| NewChunk {
            source_id: source_id.to_string(),
            chunk_index: start_index + i as i64,
            content: b.content.clone(),
            char_start: 0,
            char_end: b.content.chars().count() as i64,
            page_number: Some(b.page as i64),
            token_count: b.content.split_whitespace().count() as i64,
            parent_chunk_id: None,
            unit_type: "leaf".to_string(),
            unit_label: None,
            unit_path: None,
            domain_metadata: None,
        })
        .collect();
    let inserted = crate::db::repositories::chunks::bulk_insert(pool, rows).await?;
    let ids: Vec<String> = inserted.iter().map(|c| c.id.clone()).collect();
    sqlx::query("UPDATE chunks SET block_class = 'footnote', label_status = 'unresolved' WHERE id = ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await?;
    Ok(ids.len())
}

/// US5 (T033): mark chunks whose content contains a verbatim reconstructed table
/// row (from the reflow engine) as `block_class='table_row'` +
/// `reconstruction_method='schema_anchored'`. The reflow runs pre-normalize, so
/// each row is normalized here to match the persisted (normalized) chunk content.
async fn tag_table_row_chunks(
    pool: &Db,
    chunks: &[crate::db::repositories::chunks::Chunk],
    reconstructed_rows: &[String],
    method: &str,
) -> anyhow::Result<usize> {
    if reconstructed_rows.is_empty() {
        return Ok(0);
    }
    // Build verbatim match-keys from every emitted row: the full row PLUS its
    // head/body segments (see `row_match_keys`). The tree-builder fragments rows
    // across chunks, so a whole-row test alone under-tags; segment keys recover
    // the row-bearing chunks that hold only a fragment. ≥12-char floor (applied
    // per key) keeps short cells from matching prose. Dedup to bound the scan.
    let mut keys: Vec<String> = reconstructed_rows
        .iter()
        .flat_map(|r| row_match_keys(&text_normalizer::normalize(r)))
        .collect();
    keys.sort();
    keys.dedup();
    if keys.is_empty() {
        return Ok(0);
    }
    let ids: Vec<String> = chunks
        .iter()
        .filter(|c| chunk_is_table_row(&c.content, &keys))
        .map(|c| c.id.clone())
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    sqlx::query(
        "UPDATE chunks SET block_class = 'table_row', reconstruction_method = $2 WHERE id = ANY($1)",
    )
    .bind(&ids)
    .bind(method)
    .execute(pool)
    .await?;

    // RC-E (2026-07-01): make the tagged rows citable. A table row whose own
    // `unit_label` is a bare serial/marker (`5`, `(i)`) — or absent — is not
    // citable on its own and otherwise lands in `label_status='unresolved'`
    // (Delegation 57% / APT 58% of rows). Resolve it to its nearest NAMED
    // ancestor provision (walking `parent_chunk_id`) so it cites e.g.
    // "Schedule I — 5". Never invents a label — a row with no named ancestor is
    // left untouched (still surfaced as unresolved). Deterministic.
    let id_set: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
    let by_id: std::collections::HashMap<&str, &crate::db::repositories::chunks::Chunk> =
        chunks.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut resolved = 0usize;
    for c in chunks.iter().filter(|c| id_set.contains(c.id.as_str())) {
        let own_bare = c
            .unit_label
            .as_deref()
            .map(structure_parser::is_bare_marker)
            .unwrap_or(true);
        if !own_bare {
            continue;
        }
        let Some(anc) = nearest_named_ancestor(c, &by_id) else { continue };
        let new_label = match c.unit_label.as_deref() {
            Some(serial) => format!("{anc} — {serial}"),
            None => anc.to_string(),
        };
        sqlx::query("UPDATE chunks SET unit_label = $2, label_status = 'resolved' WHERE id = $1")
            .bind(&c.id)
            .bind(&new_label)
            .execute(pool)
            .await?;
        resolved += 1;
    }
    if resolved > 0 {
        log::info!("RC-E: resolved {resolved} table-row chunk(s) to a named ancestor");
    }
    Ok(ids.len())
}

/// RC-E: nearest ancestor of `chunk` (via `parent_chunk_id`) that names a real
/// citation. This deliberately catches heading anchors the tree-level
/// `resolve_bare_citations` misses — its `named_provision` requires
/// `keyword<space>ordinal`, so a delegation/APT table hanging off `SCHEDULE-I`,
/// `ANNEXURE-A`, or a descriptive section title stays unresolved. A non-bare
/// `unit_label` is accepted, EXCEPT the filename-titled document root
/// (`unit_type='document'`), which is skipped so a row with no meaningful
/// ancestor stays unresolved rather than citing the PDF filename. Pure; `by_id`
/// maps chunk id → chunk within the same source; bounded walk guards a
/// cyclic/corrupt parent chain.
fn nearest_named_ancestor<'a>(
    chunk: &crate::db::repositories::chunks::Chunk,
    by_id: &std::collections::HashMap<&'a str, &'a crate::db::repositories::chunks::Chunk>,
) -> Option<String> {
    let mut cur = chunk.parent_chunk_id.as_deref();
    for _ in 0..64 {
        let pid = cur?;
        let parent = by_id.get(pid)?;
        if parent.unit_type != "document" {
            if let Some(lbl) = parent.unit_label.as_deref() {
                if !structure_parser::is_bare_marker(lbl) {
                    return Some(lbl.to_string());
                }
            }
        }
        cur = parent.parent_chunk_id.as_deref();
    }
    None
}

/// Verbatim match-keys for one normalized emitted row: the full row PLUS its
/// HEAD (serial + nature-of-power text before the ` — ` separator) and BODY (the
/// label-folded cells after it). The structured tree-builder can split a row
/// across chunks — the serial/description becomes a heading while the
/// `— To whom delegated: …` body lands in a child (or several rows merge into one
/// provision chunk) — so a whole-row substring test misses ~⅓ of row-bearing
/// chunks. Head and body are still verbatim table-cell text, distinctive enough
/// that a hit means real table content. Each key must clear a ≥12-char floor so a
/// short cell can't match prose. Single-cell rows (Note/continuation, no `—`)
/// contribute only the full row.
fn row_match_keys(norm_row: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        // Whitespace-insensitive: a reflowed cell joins wrapped lines with spaces
        // while the persisted chunk keeps the source's line breaks (`Rs.\n0.015`,
        // `in a case.\nTotal …`), so we collapse all whitespace to single spaces on
        // both sides before comparing. ≥12 (collapsed) chars keeps short cells from
        // matching prose.
        let s = collapse_ws(s);
        if s.chars().count() >= 12 {
            keys.push(s);
        }
    };
    push(norm_row);
    // Split at the first em-dash (the linearize lead↔tail separator); robust to
    // normalization of the surrounding spaces.
    if let Some(idx) = norm_row.find('—') {
        push(&norm_row[..idx]);
        let body = &norm_row[idx + '—'.len_utf8()..];
        push(body);
        // Also emit each folded body CELL (`label: value`, split at the `; `
        // tail-cell separator). A long row can be truncated at a chunk boundary
        // (`… Finance Department Govt.` while the emitted row runs on) or carry an
        // inherited description, so neither the full row nor full body is a
        // substring — but an individual cell like `To whom delegated: Ombudsman`
        // still is, and is distinctive table text.
        for cell in body.split(';') {
            push(cell);
        }
    }
    keys
}

/// Collapse every whitespace run (spaces, tabs, newlines) to a single space and
/// trim. Makes substring matching insensitive to line-wrapping differences between
/// a reflowed row and the persisted chunk text.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Pure: a chunk carries reconstructed table content when its whitespace-collapsed
/// text contains, verbatim, any match-key (full row, head, or body — see
/// [`row_match_keys`], which collapses keys the same way). Verbatim on the
/// non-whitespace characters — no fuzzy heuristics — so it stays correct after
/// reflow rewrites.
fn chunk_is_table_row(content: &str, keys: &[String]) -> bool {
    let content = collapse_ws(content);
    keys.iter().any(|k| content.contains(k.as_str()))
}

/// US3 (D4): flag chunks whose content begins with a provision number that
/// disagrees with the chunk's own label (an absorbed/mislabeled heading) as
/// `label_status='unresolved'`. Reuses the same detector as the persist-time
/// warning, scoped per chunk so the row id is known. Returns the count flagged.
async fn flag_label_content_mismatches(
    pool: &Db,
    chunks: &[crate::db::repositories::chunks::Chunk],
) -> anyhow::Result<usize> {
    let ids: Vec<String> = chunks
        .iter()
        .filter(|c| {
            c.unit_label.as_deref().is_some_and(|l| {
                !structure_parser::detect_label_mismatches(vec![(l, c.content.as_str())]).is_empty()
            })
        })
        .map(|c| c.id.clone())
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    sqlx::query("UPDATE chunks SET label_status = 'unresolved' WHERE id = ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await?;
    Ok(ids.len())
}

/// US2 (D5): set `label_status='unresolved'` on persisted chunks whose
/// `unit_label` is still a bare sub-marker (e.g. `(i)`) after hierarchical
/// resolution — i.e. a parse that could not assign a real citation. Returns the
/// count flagged. Never invents a label; surfaces uncertainty as a first-class
/// state.
async fn flag_unresolved_labels(
    pool: &Db,
    chunks: &[crate::db::repositories::chunks::Chunk],
) -> anyhow::Result<usize> {
    let ids: Vec<String> = chunks
        .iter()
        .filter(|c| {
            c.unit_label
                .as_deref()
                .map(structure_parser::is_bare_marker)
                .unwrap_or(false)
        })
        .map(|c| c.id.clone())
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    sqlx::query("UPDATE chunks SET label_status = 'unresolved' WHERE id = ANY($1)")
        .bind(&ids)
        .execute(pool)
        .await?;
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DIAGNOSTIC: where do page breaks disappear in the strip chain?
    ///
    /// Two fixes aimed at this by reasoning about the mechanism, and both changed
    /// nothing measurable. This counts form feeds after every stage instead, so
    /// the losing stage is observed rather than guessed. Ignored by default; run
    /// with a real PDF:
    ///
    ///   DIAG_PDF=/path/to.pdf cargo test --lib --no-default-features \
    ///     --features server diag_page_break_loss -- --ignored --nocapture
    #[test]
    #[ignore]
    fn diag_page_break_loss() {
        let path = match std::env::var("DIAG_PDF") {
            Ok(p) => p,
            Err(_) => {
                println!("set DIAG_PDF to a PDF path");
                return;
            }
        };
        let ff = |s: &str| s.matches('\u{0c}').count();
        let parsed = crate::services::parsers::pdf::extract(std::path::Path::new(&path))
            .expect("extract failed");
        println!("extract       : ff={:>4}  via {}", ff(&parsed.text), parsed.extraction_method);

        let normalized = text_normalizer::normalize(&parsed.text);
        println!("normalize     : ff={:>4}", ff(&normalized));

        let po = crate::services::parsers::pdf::pdftotext_page_offsets(&normalized);
        println!("               offsets={}", po.len());

        let c = crate::services::parsers::chrome::strip_chrome(&normalized, &po);
        println!("chrome        : ff={:>4}  offsets={}", ff(&c.cleaned), c.cleaned_page_offsets.len());

        let f = crate::services::parsers::front_matter::strip_front_matter(
            &c.cleaned, &c.cleaned_page_offsets);
        println!("front_matter  : ff={:>4}  offsets={}", ff(&f.cleaned), f.cleaned_page_offsets.len());

        let b = crate::services::parsers::back_matter::strip_back_matter(
            &f.cleaned, &f.cleaned_page_offsets);
        println!("back_matter   : ff={:>4}  offsets={}", ff(&b.cleaned), b.cleaned_page_offsets.len());

        let (t, o) = strip_table_regions(&b.cleaned, &b.cleaned_page_offsets);
        println!("table_regions : ff={:>4}  offsets={}   <- offsets used for page stamping", ff(&t), o.len());

        // ── the half that actually assigns pages ─────────────────────────────
        // `page_by_path` is built from `offset_by_path`, then every node looks
        // itself up by `unit_path`; a miss inherits the parent's page. Measure
        // the HIT RATE — that is what decides how many distinct pages exist.
        let (profile, _) = doc_profiles::detect_profile_titled(&t, "diag", None);
        let unit_override = match doc_profiles::resolve_citation_unit(&t, profile) {
            doc_profiles::CitationUnit::Word(w)
                if !w.eq_ignore_ascii_case(profile.citation_unit) => Some(w),
            _ => None,
        };
        let headings =
            heading_detector::regex_pass::scan_with_profile_unit(&t, profile, unit_override);
        println!("\nprofile={} headings={}", profile.doc_type, headings.len());

        let (tree, offset_by_path) =
            structure_parser::build_tree_from_headings(&t, &headings, "diag", profile);
        println!("offset_by_path entries = {}", offset_by_path.len());

        // Walk the finished tree and count how many nodes can find themselves.
        fn walk(
            n: &structure_parser::DocNode,
            map: &std::collections::HashMap<String, usize>,
            total: &mut usize,
            hits: &mut usize,
            misses: &mut Vec<String>,
        ) {
            *total += 1;
            if map.contains_key(&n.unit_path) {
                *hits += 1;
            } else if misses.len() < 8 {
                misses.push(format!("{} [{}] {:?}", n.unit_path, n.label, n.level));
            }
            for c in &n.children {
                walk(c, map, total, hits, misses);
            }
        }
        let (mut total, mut hits, mut misses) = (0usize, 0usize, Vec::new());
        walk(&tree, &offset_by_path, &mut total, &mut hits, &mut misses);
        println!(
            "tree nodes = {total}, nodes WITH their own offset = {hits}, without = {}",
            total - hits
        );
        println!("distinct pages reachable = {}", {
            use std::collections::HashSet;
            let mut pages: HashSet<u32> = HashSet::new();
            for off in offset_by_path.values() {
                pages.insert(crate::services::parsers::chrome::page_for(*off, &o));
            }
            pages.len()
        });
        println!("sample nodes with NO offset entry:");
        for m in &misses {
            println!("   {m}");
        }

        // 267 headings -> 31 keys means the KEY is not unique per heading. Show
        // the collision directly rather than infer it.
        let mut label_counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for h in &headings {
            *label_counts.entry(h.label.clone()).or_insert(0) += 1;
        }
        let repeated: Vec<_> =
            label_counts.iter().filter(|(_, &n)| n > 1).collect();
        println!(
            "\nheading labels: {} distinct, {} appear MORE THAN ONCE",
            label_counts.len(),
            repeated.len()
        );
        for (lbl, n) in repeated.iter().take(10) {
            println!("   {lbl:<22} x{n}");
        }
        println!("\noffset_by_path keys (all {}):", offset_by_path.len());
        let mut keys: Vec<_> = offset_by_path.keys().cloned().collect();
        keys.sort();
        for k in keys.iter().take(35) {
            println!("   {k}");
        }
    }

    /// A table spanning a page break must not destroy that break.
    ///
    /// `strip_table_regions` drops every line of a table region, and page numbers
    /// are counted from form feeds alone — so a form feed living inside a table
    /// region used to vanish, freezing the page counter for the rest of the
    /// document. On the Manual for Development Projects that collapsed 238 real
    /// pages into 149 stamped ones and piled 341 chunks onto a single page, every
    /// one of them citing the wrong page.
    #[test]
    fn table_spanning_a_page_break_keeps_the_page_boundary() {
        let text = "1. First provision.\n\
                    \u{27E6}TBL:1\u{27E7}\n\
                    row one | a | b\n\
                    row two | c | d\u{0c}\n\
                    row three | e | f\n\
                    \u{27E6}/TBL:1\u{27E7}\n\
                    2. Second provision.\n";
        let (out, offsets) = strip_table_regions(text, &[(1, 0)]);

        // The table TEXT is still removed...
        assert!(!out.contains("row one"), "table rows must still be stripped: {out:?}");
        assert!(out.contains("1. First provision."));
        assert!(out.contains("2. Second provision."));
        // ...but the page break inside it survives, so page 2 exists.
        assert_eq!(
            offsets.len(),
            2,
            "page break inside the table region was lost: {offsets:?}"
        );
        // And the provision after the table is attributed to page 2, not page 1.
        let second = out.find("2. Second provision.").expect("provision present");
        let page = crate::services::parsers::chrome::page_for(second, &offsets);
        assert_eq!(page, 2, "provision after a page-spanning table kept the old page");
    }

    /// A table with no page break inside it must not invent one.
    #[test]
    fn table_without_a_page_break_adds_no_page() {
        let text = "1. First provision.\n\
                    \u{27E6}TBL:1\u{27E7}\n\
                    row one | a | b\n\
                    \u{27E6}/TBL:1\u{27E7}\n\
                    2. Second provision.\n";
        let (out, offsets) = strip_table_regions(text, &[(1, 0)]);
        assert!(!out.contains("row one"));
        assert_eq!(offsets.len(), 1, "no page break existed, none should appear: {offsets:?}");
    }

    /// The catchline map is keyed by the printed ordinal (`54`), but
    /// `DocNode::label` carries the rendered citation (`Article 54`). The first
    /// version compared those directly and applied **0 of 313** titles on the
    /// live Constitution — silently, because "0 applied" is not an error.
    #[test]
    fn provision_ordinal_extracts_the_number_from_a_rendered_label() {
        assert_eq!(provision_ordinal("Article 54"), Some("54"));
        assert_eq!(provision_ordinal("Section 19A"), Some("19A"));
        assert_eq!(provision_ordinal("Rule 4"), Some("4"));
        assert_eq!(provision_ordinal("  Article 175A  "), Some("175A"));
        // Structural nodes carry no ordinal and must not match one.
        assert_eq!(provision_ordinal("Preamble"), None);
        assert_eq!(provision_ordinal("body"), None);
    }

    #[test]
    fn catchline_titles_are_applied_to_provision_nodes() {
        use crate::services::structure_parser::{DocNode, NodeLevel};
        let node = |label: &str, level| DocNode {
            level,
            label: label.to_string(),
            title: None,
            text: String::new(),
            citation: label.to_string(),
            unit_path: label.to_string(),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: Vec::new(),
        };
        let mut root = node("", NodeLevel::Document);
        root.children.push(node("Article 54", NodeLevel::Article));
        root.children.push(node("Article 63", NodeLevel::Article));
        root.children.push(node("Preamble", NodeLevel::Article));

        let titles: std::collections::HashMap<String, String> = [
            ("54".to_string(), "Summoning and prorogation".to_string()),
            ("63".to_string(), "Disqualifications for membership".to_string()),
        ]
        .into_iter()
        .collect();

        assert_eq!(apply_provision_titles(&mut root, &titles), 2);
        assert_eq!(root.children[0].title.as_deref(), Some("Summoning and prorogation"));
        assert_eq!(root.children[1].title.as_deref(), Some("Disqualifications for membership"));
        assert_eq!(root.children[2].title, None, "structural node must not gain a title");
    }

    /// RC-B diagnostic (2026-06-29 audit). Runs the REAL import pipeline on the
    /// real constitution text and reports which Articles 50..=80 survive, to pin
    /// down why 55..76 vanish. Gated on `CONST_TXT` (a pdftotext dump path) so it
    /// is a no-op in CI. Run:
    ///   CONST_TXT=/tmp/const.txt cargo test --features llm -p echo-ai \
    ///       rcb_diagnose -- --ignored --nocapture
    #[test]
    #[ignore]
    fn rcb_diagnose_missing_articles() {
        // CONST_PDF → drive the REAL extractor (pdfplumber primary, set
        // ECHO_INGESTION_SIDECAR to the frozen binary); CONST_TXT → a raw dump.
        let (text, page_offsets): (String, Vec<(u32, usize)>) =
            if let Ok(pdf) = std::env::var("CONST_PDF") {
                let parsed = crate::services::parsers::pdf::extract(std::path::Path::new(&pdf))
                    .expect("pdf extract");
                eprintln!("RC-B: extractor={} text_len={}", parsed.text.len(), parsed.text.len());
                (parsed.text, parsed.page_offsets)
            } else if let Ok(path) = std::env::var("CONST_TXT") {
                (std::fs::read_to_string(&path).expect("CONST_TXT readable"), vec![(1, 0)])
            } else {
                return;
            };

        // What survives the FULL pipeline (chrome+front/back-matter+filters+tree)?
        let out = import(&text, "constitution.pdf", None, &page_offsets, &Default::default());
        let mut survived: Vec<u32> = Vec::new();
        fn walk(n: &DocNode, acc: &mut Vec<u32>) {
            if let Some(num) = n
                .citation
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
            {
                if n.citation.starts_with("Article") || n.citation.starts_with("Section") {
                    acc.push(num);
                }
            }
            for c in &n.children { walk(c, acc); }
        }
        walk(&out.tree, &mut survived);
        survived.sort_unstable();
        survived.dedup();
        let present: Vec<u32> = (50..=80).filter(|n| survived.contains(n)).collect();
        let missing: Vec<u32> = (50..=80).filter(|n| !survived.contains(n)).collect();
        eprintln!("RC-B: doc_type={} structured={}", out.doc_type, out.structured);
        eprintln!("RC-B: Articles 50..=80 PRESENT: {present:?}");
        eprintln!("RC-B: Articles 50..=80 MISSING: {missing:?}");

        // Isolate the suspect filter: what does the raw heading scan emit around
        // 50..80 BEFORE drop_out_of_sequence / catchline-collapse run? Re-run the
        // strip chain exactly as import() does, then the raw profile scan.
        let normalized = text_normalizer::normalize(&text);
        let realigned = if normalized.contains('\u{0c}') {
            crate::services::parsers::pdf::pdftotext_page_offsets(&normalized)
        } else { vec![(1, 0)] };
        let r = crate::services::parsers::chrome::strip_chrome(&normalized, &realigned);
        let fm = crate::services::parsers::front_matter::strip_front_matter(&r.cleaned, &r.cleaned_page_offsets);
        let bm = crate::services::parsers::back_matter::strip_back_matter(&fm.cleaned, &fm.cleaned_page_offsets);
        let body = bm.cleaned;
        let (profile, _) = doc_profiles::detect_profile(&body, None);
        let raw = heading_detector::regex_pass::scan_with_profile_unit(&body, profile, Some("Article"));
        let near: Vec<(usize, String)> = raw
            .iter()
            .filter_map(|h| {
                let n = h.label.split_whitespace().nth(1)?.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok()?;
                (45..=85).contains(&n).then(|| (h.offset, h.label.clone()))
            })
            .collect();
        eprintln!("RC-B: post-strip body len={}, profile={}", body.len(), profile.doc_type);
        eprintln!("RC-B: heading labels (n 45..85) AFTER all filters: {near:?}");
    }

    /// RC-B regression (2026-06-29 audit). The live DB (built BEFORE today's RC-A
    /// fixes) dropped a contiguous block of constitution Articles 55–76 — the
    /// Parliament provisions whose bodies open with a bracketed clause marker
    /// (`56. [(1)] The President …`) or a catchline+body repeat. The committed
    /// pipeline must keep every one of a run of such articles as its own node
    /// (verified against the real PDF via `rcb_diagnose_missing_articles`; this is
    /// the no-PDF guard). A drop here means a heading-detector regression.
    #[test]
    /// Diagnostic for the long-standing "Article 59 dropped" failure below
    /// (2026-08-07). Same fixture, one MORE article appended. If the defect is
    /// "the last block is dropped" then 59 now survives and 60 does not — which
    /// would mean the last provision of EVERY ingested document is being lost,
    /// not something peculiar to Article 59.
    ///
    /// ⚠️ Whichever way this lands, do not "fix" it by padding the fixture: the
    /// real documents end without a trailing blank line too.
    ///
    /// **RESULT (2026-08-07): hypothesis DISPROVEN.** `survived = [54,55,56,57,
    /// 58,60]` — 59 drops even when it is not last, so this is not a missing
    /// final flush. `catchline_word_count_diagnostic` below narrows it further:
    /// the discriminator is a catchline **beginning with "The"**, not length.
    ///
    /// `#[ignore]`d, not deleted: it fails for the same unfound cause as
    /// `rcb_parliament_articles_with_bracket_openers_all_survive`, and carrying
    /// two red tests for one bug hides the next real regression. Un-ignore when
    /// the catchline handling is fixed — both should then pass together.
    ///
    /// ⚠️ **Not a corpus-wide gap**: the live Constitution has Article 59 with
    /// 17 chunks, so the full pipeline (sidecar catchline titles, chrome strip)
    /// does not reproduce it. This is a `manual`-profile weakness on synthetic
    /// input. It is NOT a re-index blocker.
    #[test]
    #[ignore = "known: catchline starting with 'The' is dropped; same root cause as the RC-B test"]
    fn last_article_in_the_document_is_not_dropped() {
        let text = "\
54. Summoning and prorogation\n54. (1) The President may summon either House.\n\n\
55. Voting in Assembly and quorum\n55. (1) Subject to the Constitution, all decisions shall be taken by majority.\n\n\
56. Address by President\n56. [(1)] The President may address either House or both Houses assembled together.\n\n\
57. Right to speak in Parliament\n57. The Prime Minister and a Federal Minister shall have the right to speak in either House.\n\n\
58. Dissolution of National Assembly\n58. [(1)] The President shall dissolve the National Assembly if so advised by the Prime Minister.\n\n\
59. The Senate\n59. (1) The Senate shall consist of the following members.\n\n\
60. Composition of the Assembly\n60. (1) The Assembly shall consist of three hundred and thirty-six members.\n";
        let out = import(text, "constitution.pdf", Some("manual"), &[(1, 0)], &Default::default());
        let mut nums: Vec<u32> = Vec::new();
        fn walk(n: &DocNode, acc: &mut Vec<u32>) {
            if n.citation.starts_with("Article") || n.citation.starts_with("Section") {
                if let Some(v) = n.citation.split_whitespace().nth(1)
                    .and_then(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
                { acc.push(v); }
            }
            for c in &n.children { walk(c, acc); }
        }
        walk(&out.tree, &mut nums);
        nums.sort_unstable();
        nums.dedup();
        for a in 54..=60 {
            assert!(nums.contains(&a), "Article {a} dropped; survived = {nums:?}");
        }
    }

    /// Narrowing diagnostic (2026-08-07). Article 59 drops even when it is NOT
    /// the last block, so it is not a missing final flush. The only thing that
    /// distinguishes it from its neighbours is a TWO-WORD catchline ("The
    /// Senate"); every surviving article has three or more. This varies ONLY the
    /// catchline and reports which lengths survive.
    #[test]
    fn catchline_word_count_diagnostic() {
        let mut report: Vec<(usize, bool)> = Vec::new();
        for catchline in [
            "Senate",
            "The Senate",
            "The Senate composition",
            "Composition of the Senate",
        ] {
            let text = format!(
                "58. Dissolution of National Assembly\n58. [(1)] The President shall dissolve the National Assembly.\n\n\
                 59. {catchline}\n59. (1) The Senate shall consist of the following members.\n\n\
                 60. Composition of the Assembly\n60. (1) The Assembly shall consist of members.\n"
            );
            let out = import(&text, "constitution.pdf", Some("manual"), &[(1, 0)], &Default::default());
            let mut nums: Vec<u32> = Vec::new();
            fn walk(n: &DocNode, acc: &mut Vec<u32>) {
                if n.citation.starts_with("Article") || n.citation.starts_with("Section") {
                    if let Some(v) = n.citation.split_whitespace().nth(1)
                        .and_then(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
                    { acc.push(v); }
                }
                for c in &n.children { walk(c, acc); }
            }
            walk(&out.tree, &mut nums);
            let survived = nums.contains(&59);
            println!(
                "CATCHLINE DIAG: words={} {:?} -> 59 survived={} (all={:?})",
                catchline.split_whitespace().count(), catchline, survived, nums,
            );
            report.push((catchline.split_whitespace().count(), survived));
        }
        // Not an assertion about the fix — this test exists to PRINT the shape.
        // It fails only if every variant drops, which would disprove the
        // catchline theory too and send the search elsewhere.
        assert!(
            report.iter().any(|(_, ok)| *ok),
            "Article 59 dropped for EVERY catchline length — the catchline is not \
             the discriminator; look elsewhere. {report:?}",
        );
    }

    // ⚠️ Restored 2026-08-07: inserting a new test immediately above this one
    // consumed its `#[test]` attribute, silently turning it into a plain fn —
    // the suite went "green" by losing a test, not by fixing anything. Left
    // FAILING, exactly as it was, rather than quietly changing the baseline.
    #[test]
    fn rcb_parliament_articles_with_bracket_openers_all_survive() {
        // Catchline then body for each article; #56/#58 open with `[(1)]` (the
        // amendment-bracket clause marker RC-A added) and #57 with a bare prose
        // body — the exact shapes around constitution Arts 55–58.
        let text = "\
54. Summoning and prorogation\n54. (1) The President may summon either House.\n\n\
55. Voting in Assembly and quorum\n55. (1) Subject to the Constitution, all decisions shall be taken by majority.\n\n\
56. Address by President\n56. [(1)] The President may address either House or both Houses assembled together.\n\n\
57. Right to speak in Parliament\n57. The Prime Minister and a Federal Minister shall have the right to speak in either House.\n\n\
58. Dissolution of National Assembly\n58. [(1)] The President shall dissolve the National Assembly if so advised by the Prime Minister.\n\n\
59. The Senate\n59. (1) The Senate shall consist of the following members.\n";
        // Force the `manual` profile the real constitution resolves to (a 6-line
        // fixture is too small to trip auto-detection); detection itself is not
        // under test here — heading survival is.
        let out = import(text, "constitution.pdf", Some("manual"), &[(1, 0)], &Default::default());
        let mut nums: Vec<u32> = Vec::new();
        fn walk(n: &DocNode, acc: &mut Vec<u32>) {
            if (n.citation.starts_with("Article") || n.citation.starts_with("Section")) {
                if let Some(v) = n.citation.split_whitespace().nth(1)
                    .and_then(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
                { acc.push(v); }
            }
            for c in &n.children { walk(c, acc); }
        }
        walk(&out.tree, &mut nums);
        nums.sort_unstable();
        nums.dedup();
        for a in 54..=59 {
            assert!(nums.contains(&a), "Article {a} dropped (RC-B regression); survived = {nums:?}");
        }
    }

    /// RC-C diagnostic (2026-06-29 audit). Runs the REAL pipeline on the real
    /// Sindh Rules of Business and reports every Rule/Section label that survives
    /// MORE THAN ONCE — the schedule serial-renumber duplicates (live: Section
    /// 30×6, 31×5, 32×4). Gated on `ROB_PDF` (+ `ECHO_INGESTION_SIDECAR`). Run:
    ///   ECHO_INGESTION_SIDECAR=…/binaries/ingestion-sidecar-… \
    ///   ROB_PDF="…/01_Sindh_Rules_of_Business_1986.pdf" \
    ///   cargo test --features llm -p echo-ai --lib rcc_diagnose -- --ignored --nocapture
    #[test]
    #[ignore]
    fn rcc_diagnose_duplicate_labels() {
        let Ok(pdf) = std::env::var("ROB_PDF") else { return };
        let parsed = crate::services::parsers::pdf::extract(std::path::Path::new(&pdf))
            .expect("pdf extract");
        let out = import(&parsed.text, "01_Sindh_Rules_of_Business_1986.pdf", None, &parsed.page_offsets, &Default::default());

        // Re-run the strip chain to get the body the scan sees, then count
        // duplicate HEADINGS per label (one per detected heading — NOT tree nodes,
        // which include /pN /mN split children that inherit a parent citation and
        // inflate the count). This is the provision-duplication the audit flagged
        // (Section 30×6, 31×5, 32×4 in the pre-fix live DB).
        let normalized = text_normalizer::normalize(&parsed.text);
        let realigned = if normalized.contains('\u{0c}') {
            crate::services::parsers::pdf::pdftotext_page_offsets(&normalized)
        } else { vec![(1, 0)] };
        let r = crate::services::parsers::chrome::strip_chrome(&normalized, &realigned);
        let fm = crate::services::parsers::front_matter::strip_front_matter(&r.cleaned, &r.cleaned_page_offsets);
        let bm = crate::services::parsers::back_matter::strip_back_matter(&fm.cleaned, &fm.cleaned_page_offsets);
        let final_hits = heading_detector::regex_pass::scan_with_profile_unit(&bm.cleaned, doc_profiles::detect_profile(&bm.cleaned, None).0, None);
        let boundaries: Vec<&str> = final_hits.iter().filter(|h| (h.level as usize) < 3).map(|h| h.label.as_str()).collect();
        let mut hcount: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for h in &final_hits {
            if h.label.starts_with("Section") || h.label.starts_with("Rule") {
                *hcount.entry(h.label.clone()).or_default() += 1;
            }
        }
        let dups: Vec<(&String, &usize)> = hcount.iter().filter(|(_, &c)| c > 1).collect();
        eprintln!("RC-C: doc_type={} structured={}", out.doc_type, out.structured);
        eprintln!("RC-C: structural boundaries = {boundaries:?}");
        eprintln!("RC-C: DUPLICATE provision headings (count>1) = {dups:?}");
    }

    /// RC-PG (2026-06-29 audit): page offsets must be re-derived against the
    /// normalized body. With heavy gutter padding (collapsed by `normalize`),
    /// the raw offsets drift forward and a late provision stamps to a too-early
    /// page; the realignment must keep Rule 3 on page 3. Fails pre-fix.
    #[test]
    fn page_offsets_realigned_after_normalize_no_compression() {
        let pad = " ".repeat(150); // gutter padding normalize() collapses to 1 space
        // Realistic page layout: each page ends with a newline before the form
        // feed, and a (distinct, per-page) running header sits at the top of the
        // next page so the provision heading is NOT on the form-feed boundary
        // itself. The `{pad}` runs are what `normalize` collapses — pre-fix that
        // shortening makes the raw page_offsets over-shoot, compressing Rule 3 to
        // an earlier page; post-fix the offsets are re-derived on the normalized body.
        let raw = format!(
            "Rule 1. Short title.{pad}\n\nbody one of the first rule here.\n\x0c\
             \nGazette of Sindh Page Two\nRule 2. Definitions.{pad}\n\nbody two of the second rule.\n\x0c\
             \nGazette of Sindh Page Three\nRule 3. Scope and applicability.{pad}\n\nbody three of the third rule."
        );
        // page_offsets as the parser computes them — on the RAW (un-normalized) text.
        let raw_offsets: Vec<(u32, usize)> = {
            let mut v = vec![(1u32, 0usize)];
            let mut pg = 1u32;
            for (i, c) in raw.char_indices() {
                if c == '\u{0c}' {
                    pg += 1;
                    v.push((pg, i + 1));
                }
            }
            v
        };
        let out = import(&raw, "rules.pdf", None, &raw_offsets, &Default::default());
        assert!(out.structured, "fixture should structure as rules");
        let p3 = out.page_by_path.iter()
            .find(|(k, _)| k.contains("Rule_3"))
            .map(|(_, v)| *v);
        assert_eq!(p3, Some(3), "Rule 3 must stamp to page 3 (got {p3:?}); page map = {:?}", out.page_by_path);
    }

    /// A long provision that SPANS a page break: `finalize_tree` splits it into
    /// `/pN` paragraph children, and the piece printed on the later page must
    /// stamp to THAT page — not inherit the provision's first page.
    ///
    /// Pre-fix (tester report 2026-07-28) every piece carried the section's
    /// start page: content on p.2 was cited as p.1 (the live example was p.18
    /// cited as p.27).
    #[test]
    fn split_children_stamp_their_own_page_not_the_parents() {
        // Rule 2's body is long enough to be split, and the split point falls
        // across the form feed, so its tail genuinely sits on page 2.
        let head: String = "alpha beta gamma delta epsilon zeta eta theta. ".repeat(30);
        let tail: String = "PAGETWO omega psi chi phi upsilon tau sigma rho. ".repeat(30);
        let raw = format!(
            "Rule 1. Short title.\n\nbody one of the first rule here for length.\n\
             \nRule 2. Long provision.\n\n{head}\n\x0c\n{tail}\n"
        );
        let raw_offsets: Vec<(u32, usize)> = {
            let mut v = vec![(1u32, 0usize)];
            let mut pg = 1u32;
            for (i, c) in raw.char_indices() {
                if c == '\u{0c}' {
                    pg += 1;
                    v.push((pg, i + 1));
                }
            }
            v
        };
        let out = import(&raw, "rules.pdf", None, &raw_offsets, &Default::default());

        // Rule 2 itself starts on page 1…
        assert_eq!(
            out.page_by_path.get("Rule_2").copied(),
            Some(1),
            "Rule 2's own heading is on page 1; map = {:?}",
            out.page_by_path
        );
        // …and its generated children must be mapped in their OWN right, with
        // the tail piece landing on page 2. `finalize_tree` splits (`/pN`) and
        // then coalesces (`/mN`), so match on the parent prefix rather than on
        // which of the two forms survived for this fixture.
        let child_pages: Vec<i64> = out
            .page_by_path
            .iter()
            .filter(|(k, _)| k.starts_with("Rule_2/"))
            .map(|(_, v)| *v)
            .collect();
        assert!(
            !child_pages.is_empty(),
            "Rule 2's split children must be mapped, not left to inherit; map = {:?}",
            out.page_by_path
        );
        assert_eq!(
            child_pages.iter().max().copied(),
            Some(2),
            "the piece printed after the page break must stamp p2, not inherit Rule 2's p1; map = {:?}",
            out.page_by_path
        );
    }

    #[test]
    fn rules_text_routes_to_rules_profile_and_is_structured() {
        let text = "Rule 1. Short title.\n\nbody body body\n\nRule 2. Defs.\n\nbody body body\n\nRule 3. Procedure.\n\nbody body";
        let out = import(text, "test.pdf", None, &[], &Default::default());
        assert_eq!(out.doc_type, "rules");
        assert!(out.structured);
    }

    fn art_chunk(label: &str, page: i64) -> crate::db::repositories::chunks::Chunk {
        crate::db::repositories::chunks::Chunk {
            id: String::new(),
            source_id: String::new(),
            chunk_index: 0,
            content: String::new(),
            char_start: 0,
            char_end: 0,
            page_number: Some(page),
            token_count: 0,
            created_at: 0,
            parent_chunk_id: None,
            unit_type: "article".to_string(),
            unit_label: Some(label.to_string()),
            unit_path: None,
            domain_metadata: None,
        }
    }

    /// US5 (T038): a footnote anchors only when its page holds a SINGLE distinct
    /// Article; multiple Articles on the page is ambiguous → None (skip, flag).
    #[test]
    fn amendment_anchor_only_when_page_has_one_article() {
        let provisions = vec![
            art_chunk("Article 8", 1),
            art_chunk("Article 8", 1), // same article, multiple chunks → still one
            art_chunk("Article 10", 2),
            art_chunk("Article 11", 2), // two distinct articles on page 2
        ];
        assert_eq!(anchor_citation_for_page(1, &provisions), Some("Article 8".to_string()));
        assert_eq!(anchor_citation_for_page(2, &provisions), None, "ambiguous page must not anchor");
        assert_eq!(anchor_citation_for_page(9, &provisions), None, "absent page must not anchor");
    }

    fn art_chunk_with_content(label: &str, page: i64, content: &str) -> crate::db::repositories::chunks::Chunk {
        let mut c = art_chunk(label, page);
        c.content = content.to_string();
        c
    }

    fn footnote(content: &str, page: u32) -> crate::services::parsers::chrome::FootnoteBlock {
        crate::services::parsers::chrome::FootnoteBlock { content: content.to_string(), page }
    }

    fn chunk_node(id: &str, parent: Option<&str>, label: Option<&str>) -> crate::db::repositories::chunks::Chunk {
        let mut c = art_chunk(label.unwrap_or(""), 1);
        c.id = id.to_string();
        c.parent_chunk_id = parent.map(str::to_string);
        c.unit_label = label.map(str::to_string);
        c
    }

    /// RC-E: a bare-marker row resolves to its nearest NAMED ancestor (including
    /// a dash/letter heading `named_provision` misses, e.g. `SCHEDULE-I`); the
    /// filename-titled document root is skipped, and a row whose only ancestor is
    /// that root stays unresolved (`None`) rather than citing the PDF filename.
    #[test]
    fn nearest_named_ancestor_walks_to_first_real_citation() {
        let root = chunk_node("root", None, Some("SCHEDULE-I")); // dash form: named_provision misses it
        let mid = chunk_node("mid", Some("root"), Some("(a)")); // bare marker
        let row = chunk_node("row", Some("mid"), Some("5")); // bare serial
        let by_id: std::collections::HashMap<&str, &crate::db::repositories::chunks::Chunk> =
            [&root, &mid, &row].iter().map(|c| (c.id.as_str(), *c)).collect();
        assert_eq!(nearest_named_ancestor(&row, &by_id), Some("SCHEDULE-I".to_string()));

        // A row whose only ancestor is the filename-titled document root → None.
        let mut docroot = chunk_node("doc", None, Some("09_Sindh_Delegation.pdf"));
        docroot.unit_type = "document".to_string();
        let row2 = chunk_node("row2", Some("doc"), Some("(i)"));
        let by_id2: std::collections::HashMap<&str, &crate::db::repositories::chunks::Chunk> =
            [&docroot, &row2].iter().map(|c| (c.id.as_str(), *c)).collect();
        assert_eq!(nearest_named_ancestor(&row2, &by_id2), None);
    }

    /// RC-F: `footnote_marker` extracts the leading ordinal, tolerating a
    /// leading amendment bracket + whitespace.
    #[test]
    fn footnote_marker_extracts_leading_ordinal() {
        assert_eq!(footnote_marker("14. Substituted by Notification …"), Some("14".to_string()));
        assert_eq!(footnote_marker("  [18. Response Time inserted …"), Some("18".to_string()));
        assert_eq!(footnote_marker("Added by Act X"), None);
    }

    /// RC-F: on a page with several provisions (previously an unconditional
    /// `None`), the footnote's marker digit matched to the in-body amendment
    /// marker rescues a precise anchor — but only when exactly one matches.
    #[test]
    fn footnote_anchors_ambiguous_page_via_marker() {
        let provisions = vec![
            art_chunk_with_content("Rule 17", 5, "17. Bidding. The Authority shall …"),
            art_chunk_with_content("Rule 18", 5, "18[ Response Time. The response …]14 shall apply"),
            art_chunk_with_content("Rule 19", 5, "19. Evaluation. Bids are opened …"),
        ];
        // Page 5 holds 3 distinct rules → the single-article rule yields None,
        // but marker 14 appears only in Rule 18's body → anchor there.
        assert_eq!(
            anchor_citation_for_footnote(&footnote("14. Rule 18 substituted by SRO …", 5), &provisions),
            Some("Rule 18".to_string()),
        );
        // A marker present in NO provision body → still None (refuse-or-flag).
        assert_eq!(
            anchor_citation_for_footnote(&footnote("99. Some amendment …", 5), &provisions),
            None,
        );
        // Single distinct provision on the page still takes the fast path.
        let single = vec![art_chunk_with_content("Rule 20", 6, "20. Body …")];
        assert_eq!(
            anchor_citation_for_footnote(&footnote("no marker here", 6), &single),
            Some("Rule 20".to_string()),
        );
    }

    #[test]
    fn unstructured_text_falls_to_generic_and_marks_unstructured() {
        let text = "just one paragraph of plain text that doesn't match any patterns whatsoever in any of the profiles we ship";
        let out = import(text, "memo.txt", None, &[], &Default::default());
        assert_eq!(out.doc_type, "generic");
        assert!(!out.structured);
    }

    #[test]
    fn chunk_is_table_row_matches_emitted_row_verbatim() {
        let rows = [
            "11. Power to declare surplus motor vehicles — Authority: Administrative Department; Extent: Full powers.",
            "12. Power to sanction — Authority: Director General; Extent: Within limits.",
        ];
        let keys: Vec<String> = rows.iter().flat_map(|r| row_match_keys(r)).collect();

        // Intact row in its chunk.
        let content = "Rule 5\n11. Power to declare surplus motor vehicles — Authority: Administrative Department; Extent: Full powers.";
        assert!(chunk_is_table_row(content, &keys), "verbatim row must match its chunk");
        // Prose with no reconstructed content stays untagged.
        assert!(
            !chunk_is_table_row("Some unrelated provision about general powers.", &keys),
            "prose with no reconstructed row must not be tagged"
        );
    }

    #[test]
    fn chunk_is_table_row_catches_fragmented_rows() {
        // The tree-builder splits a row: serial+description becomes a heading,
        // the `— Authority: …; Extent: …` body lands in a child chunk. Both
        // fragments must still be tagged (this is the 57-missing-chunks fix).
        let row = "11. Power to declare surplus motor vehicles — Authority: Administrative Department; Extent: Full powers.";
        let keys = row_match_keys(row);

        let head_only = "11. Power to declare surplus motor vehicles";
        let body_only = "Authority: Administrative Department; Extent: Full powers.";
        assert!(chunk_is_table_row(head_only, &keys), "head fragment (serial+nature) must match");
        assert!(chunk_is_table_row(body_only, &keys), "body fragment (folded labels) must match");
        // A short, generic cell must NOT leak the tag into prose.
        assert!(
            !chunk_is_table_row("The department exercised full powers.", &keys),
            "generic prose must not match a row segment"
        );
    }

    #[test]
    fn chunk_is_table_row_is_whitespace_insensitive() {
        // The reflowed row joins a wrapped Extent cell with spaces, but the
        // persisted chunk keeps the source line breaks (`in a case.\nTotal …`,
        // `Rs.\n0.020`). Matching must survive that (the 5-missing-chunks fix).
        let row = "1. To grant rewards — To whom delegated: Inspector General of Prisons.; Extent: Up to Rs.0.020 million in a case. Total rewards not to exceed Rs. 0.015 million.";
        let keys = row_match_keys(row);
        let wrapped_chunk = "46B-PRISONS.\n1.\nTo grant rewards\n— To whom delegated: Inspector General of Prisons.; Extent: Up to Rs.0.020 million in a case.\nTotal rewards not to exceed Rs.\n0.015 million.";
        assert!(
            chunk_is_table_row(wrapped_chunk, &keys),
            "line-wrapped body must still match after whitespace collapse"
        );
    }

    #[test]
    fn chunk_is_table_row_matches_truncated_long_row_by_cell() {
        // A long row whose chunk is cut at a boundary: neither the full row nor the
        // full body is a substring, but a body cell still is.
        let row = "Re-appropriation of funds — To whom delegated: Ombudsman; Extent: Full Powers for re-appropriation of funds from one object to another in consultation with Finance Secretary, Finance Department Govt. of Sindh.";
        let keys = row_match_keys(row);
        let truncated = "Re-appropriation of funds — To whom delegated: Ombudsman; Extent: Full Powers for re-appropriation of funds from one object to another object in consultation with Finance Secretary, Finance Department Govt.";
        assert!(
            chunk_is_table_row(truncated, &keys),
            "truncated long row must match on a body cell (e.g. 'To whom delegated: Ombudsman')"
        );
    }

    #[test]
    fn row_match_keys_drops_short_segments() {
        // Single-cell row (no em-dash) → only the full row, no head/body split.
        let keys = row_match_keys("Note: orders shall be passed by the Secretary.");
        assert_eq!(keys.len(), 1, "no em-dash → one key");
        // A row whose head is a bare serial must not contribute a <12-char head.
        let keys = row_match_keys("5. ok — Authority: Administrative Department.");
        assert!(!keys.iter().any(|k| k == "5. ok"), "short head dropped: {keys:?}");
        assert!(keys.iter().any(|k| k.starts_with("Authority:")), "body kept: {keys:?}");
    }

    async fn fresh_pool() -> Db {
        let pool = crate::db::test_pool().await;
        pool
    }

    #[tokio::test]
    async fn persist_rules_doc_creates_chunks_graph_nodes_and_marks_source() {
        let pool = fresh_pool().await;
        sqlx::query("INSERT INTO sources (id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,indexing_status,created_at) VALUES ('s1','/x','x.pdf','pdf',1,'h',0,'processing',0)")
            .execute(&pool).await.unwrap();

        let text = "Rule 1. Title.\nbody.\n\nRule 2. Definitions.\nbody.\n\nRule 3. Procedure.\nbody.";
        let out  = import(text, "x.pdf", None, &[], &Default::default());
        assert!(out.structured);
        let stats = persist(&pool, "s1", &out, &[], "", &[]).await.unwrap();
        assert!(stats.chunks    >= 3, "got {} chunks", stats.chunks);

        let dt: String = sqlx::query_scalar("SELECT doc_type FROM sources WHERE id='s1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(dt, "rules");
        let status: String = sqlx::query_scalar("SELECT structure_status FROM sources WHERE id='s1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(status, "structured");
    }

    // Feature 023 — table-unit rendering (pure; no DB).
    fn rows_on(page: u32, rows: Vec<Vec<String>>) -> Vec<crate::services::parsers::table::TableRow> {
        rows.into_iter()
            .map(|cells| crate::services::parsers::table::TableRow { cells, page })
            .collect()
    }

    #[test]
    fn render_table_markdown_counts_all_rows() {
        use crate::services::parsers::table::ExtractedTable;
        let t = ExtractedTable {
            id: 0,
            page: 26,
            caption: Some("SCHEDULE-I (See Rule – 3 (i)".into()),
            header: vec!["S.No".into(), "Department".into(), "Attached".into()],
            rows: rows_on(
                26,
                (1..=39)
                    .map(|i| vec![i.to_string(), format!("Dept {i}"), format!("Attached {i}")])
                    .collect(),
            ),
        };
        let md = render_table_markdown(t.caption.as_deref(), &t.header, &t.rows);
        // Caption, then header + separator + 39 body rows; body-row count == N (C-1).
        let body_rows = md.lines().filter(|l| l.starts_with("| ") && !l.contains("---")).count() - 1;
        assert_eq!(body_rows, 39, "all 39 rows present in whole-table markdown");
        assert!(md.starts_with("SCHEDULE-I"), "table cited by the name it prints: {md}");
        assert!(md.contains("Department"), "header rendered");
        assert!(md.contains("Dept 39"), "last row present");
    }

    #[test]
    fn render_row_with_header_labels_cells_and_keeps_notes() {
        let header = vec!["S.No".into(), "Department".into(), "Attached".into()];
        let row = vec![
            "7".into(),
            "Works Department".into(),
            "Buildings (inserted vide Notification 2019)".into(),
        ];
        let s = render_row_with_header(&header, &row);
        assert!(s.contains("Department: Works Department"), "header-labelled: {s}");
        assert!(s.contains("inserted vide Notification 2019"), "amendment note retained: {s}");
    }

    #[test]
    fn render_table_markdown_synthesizes_header_when_blank() {
        let rows = rows_on(1, vec![vec!["a".into(), "b".into()], vec!["c".into(), "d".into()]]);
        let md = render_table_markdown(None, &[], &rows);
        assert!(md.contains("col1") && md.contains("col2"), "generic header synthesized: {md}");
        let body_rows = md.lines().skip(2).filter(|l| l.starts_with('|')).count();
        assert_eq!(body_rows, 2);
    }

    #[test]
    fn strip_table_regions_is_noop_without_markers() {
        // SC-004: a prose-only document (no table markers) is returned byte-identical
        // with its page offsets untouched → zero regression on prose-only docs.
        let text = "Rule 1. Title.\nbody one.\n\nRule 2. Next.\nbody two.";
        let offsets = vec![(1, 0)];
        let (cleaned, off) = strip_table_regions(text, &offsets);
        assert_eq!(cleaned, text, "prose-only text unchanged");
        assert_eq!(off, offsets, "offsets unchanged");
    }

    #[test]
    fn strip_table_regions_removes_marker_regions_only() {
        use crate::services::parsers::table::{table_close_marker, table_open_marker};
        let text = format!(
            "Rule 1. Title.\nbody.\n{}\n| a | b |\nrow one\n{}\nRule 2. After.\nbody2.",
            table_open_marker(0),
            table_close_marker(0)
        );
        let (cleaned, _off) = strip_table_regions(&text, &[(1, 0)]);
        assert!(cleaned.contains("Rule 1. Title."), "prose before kept");
        assert!(cleaned.contains("Rule 2. After."), "prose after kept");
        assert!(!cleaned.contains("row one"), "table rows removed");
        assert!(!cleaned.contains("\u{27E6}TBL"), "markers removed");
    }

    /// The live defect (2026-08-05): pages are joined with a bare form feed, so a
    /// table opening at the top of a page arrived as `…prev line\x0c⟦TBL:0⟧` — one
    /// line, not a marker line. The region never opened and every linearised row
    /// stayed in the body, where the clause parser minted them as provisions
    /// (Delegation-of-Financial-Powers: 111 duplicate `clause` chunks; APT: a
    /// table row stored as `Rule 3`).
    #[test]
    fn marker_glued_to_a_page_break_still_opens_the_region() {
        use crate::services::parsers::table::{table_close_marker, table_open_marker};
        let text = format!(
            "Rule 1. Title.\nlast line of p.1\u{0c}{}\n2. Reciprocal up-gradation — Extent: Full Powers.\n{}\nRule 2. After.",
            table_open_marker(0),
            table_close_marker(0)
        );
        let (cleaned, _off) = strip_table_regions(&text, &[(1, 0), (2, 30)]);
        assert!(
            !cleaned.contains("Reciprocal up-gradation"),
            "the row must be lifted out of the body, not left to become a provision: {cleaned}"
        );
        assert!(cleaned.contains("last line of p.1"), "page-1 prose kept");
        assert!(cleaned.contains("Rule 2. After."), "page-2 prose kept");
        assert_eq!(cleaned.matches('\u{0c}').count(), 1, "the page break survives");
    }

    /// A lost close marker used to leave the region open for the rest of the
    /// document, deleting every provision after it. Now only a MATCHED pair
    /// drops anything: the stray marker removes itself, the text it failed to
    /// bound stays readable, and the next page is never at risk.
    #[test]
    fn an_unpaired_marker_deletes_nothing_but_itself() {
        use crate::services::parsers::table::table_open_marker;
        let text = format!(
            "{}\n1. A table row.\u{0c}Rule 9. Appeals.\nThe appellate authority may.",
            table_open_marker(0)
        );
        let (cleaned, _off) = strip_table_regions(&text, &[(1, 0), (2, 20)]);
        assert!(!cleaned.contains('\u{27E6}'), "the stray marker itself is dropped");
        assert!(
            cleaned.contains("A table row"),
            "content an unpaired marker failed to bound is kept, never guessed away: {cleaned}"
        );
        assert!(
            cleaned.contains("Rule 9. Appeals.") && cleaned.contains("appellate authority"),
            "the next page's provisions must survive an unclosed region: {cleaned}"
        );
    }

    /// The live defect (2026-08-05, found by re-indexing): a `⟦TBL:n⟧` marker is
    /// indistinguishable from a masthead to chrome-strip, which normalises digits
    /// away before testing cross-page repetition — so every marker in a
    /// table-heavy document collapsed to one recurring key and was deleted as
    /// chrome. `strip_table_regions` then had nothing to lift and 105 linearised
    /// rows stayed in Delegation's body as provisions.
    #[test]
    fn chrome_strip_never_eats_a_table_marker() {
        use crate::services::parsers::chrome::strip_chrome;
        use crate::services::parsers::table::{table_close_marker, table_open_marker};
        // Six pages, each opening with the same masthead and carrying a table —
        // the shape that made the markers look repeated.
        let mut text = String::new();
        let mut offsets = Vec::new();
        for p in 0..6usize {
            offsets.push((p as u32 + 1, text.len()));
            text.push_str("Government of Sindh — Services General Administration\n");
            text.push_str(&format!("{}\n", table_open_marker(p)));
            text.push_str(&format!("{}. Power to sanction — Extent: Full powers.\n", p + 1));
            text.push_str(&format!("{}\n", table_close_marker(p)));
        }
        let r = strip_chrome(&text, &offsets);
        assert_eq!(
            r.cleaned.matches("TBL:").count(),
            12,
            "every open and close marker must survive chrome-strip: {}",
            r.cleaned
        );
        assert!(
            !r.cleaned.contains("Government of Sindh"),
            "the real masthead is still stripped — the guard is narrow"
        );
        // End to end: the rows are then lifted, leaving no provision behind.
        let (body, _) = strip_table_regions(&r.cleaned, &r.cleaned_page_offsets);
        assert!(
            !body.contains("Power to sanction"),
            "with its markers intact the region is lifted out of the body: {body}"
        );
    }
}
