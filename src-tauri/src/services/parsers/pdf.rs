//! PDF parser — three-tier extraction:
//!   1. pdftotext (poppler-utils — handles CID fonts, ToUnicode maps; keeps
//!      full lines, unlike lopdf which shatters column layouts one-per-line).
//!   2. lopdf (fast, no system deps — no-poppler fallback only).
//!   3. tesseract OCR (image-only or font-corrupted PDFs).
//!
//! Tier promotion is gated by `text_quality::score`. Each tier produces a
//! candidate; the first tier whose verdict is `Good` wins. If no tier
//! reaches `Good`, the best-scoring candidate is returned and marked
//! `ocr_skipped` (or `ScannedPdf` if no tier produced anything usable).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use super::{ParseError, ParsedDocument};
use super::ocr::{self, ocr_pdf, OcrConfig};
use super::table::{self, WordBox};
use super::table_sidecar;
use super::text_quality::{score, Verdict};
use regex_lite::Regex;

const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GB absolute hard limit
/// Conservative limit applied on all systems — lopdf reads the entire PDF into
/// RAM for random page access, so a 200 MB PDF = 200 MB+ RAM spike.
const SAFE_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB soft limit

pub fn extract(path: &Path) -> Result<ParsedDocument, ParseError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(ParseError::FileTooLarge {
            size_bytes: metadata.len(),
            limit_bytes: MAX_FILE_SIZE,
        });
    }
    if metadata.len() > SAFE_FILE_SIZE {
        log::warn!(
            "PDF {:?} is {:.1} MB — lopdf will load it entirely into RAM. \
             Consider splitting large PDFs to avoid memory pressure.",
            path,
            metadata.len() as f64 / 1_048_576.0
        );
    }

    // Best-so-far across all tiers: (text, page_offsets, method, score).
    let mut best: Option<(String, Vec<(u32, usize)>, &'static str, f32)> = None;

    // ── Tier 0: pdfplumber sidecar (PRIMARY — text + tables) ──────────────────
    // pdfplumber returns layout-preserving page text AND structured table cells,
    // so it both reads prose more faithfully and linearises ruled tables that
    // `pdftotext -layout` scrambles. Quality-gated: a `Good` result wins; a weaker
    // result (or a missing/failed sidecar) falls through to pdftotext below — the
    // proven fallback. Toggle: ECHO_PDFPLUMBER_PRIMARY=0 reverts to pdftotext-first.
    if pdfplumber_primary_enabled() {
        if let Some(ex) = table_sidecar::extract_document(path) {
            let table_sidecar::DocumentExtraction { text, rows, tables, provision_titles } = ex;
            if !text.trim().is_empty() {
                let page_offsets = pdftotext_page_offsets(&text);
                let q = score(&text, page_offsets.len().max(1));
                // Completeness cross-check. `score` grades each tier ALONE and
                // measures only shape (chars/page, tiny lines, punct, dict hits),
                // so text that has been silently truncated but still reads as
                // English scores 1.0/Good. 2026-07-30: the SLGA Compendium scored
                // exactly that while losing 13 provisions to a bad catchline crop,
                // so pdftotext — which found all of them — never ran. Nothing in a
                // single-tier score can catch that; only a comparison can.
                let complete = q.verdict != Verdict::Good || tier0_is_complete(path, &text);
                if q.verdict == Verdict::Good && complete {
                    log::info!(
                        "pdfplumber extracted {} chars / {} pages from {:?} ({} table rows linearised, {} tables, {} provision titles)",
                        text.len(), page_offsets.len(), path, rows.len(), tables.len(),
                        provision_titles.len()
                    );
                    return Ok(ParsedDocument {
                        text,
                        page_offsets,
                        extraction_method: "pdfplumber",
                        quality_score: q.score,
                        reconstructed_rows: rows,
                        reconstruction_method: "pdfplumber",
                        tables,
                        provision_titles,
                    });
                }
                if complete {
                    log::debug!(
                        "pdfplumber verdict {:?} (score {:.2}) for {:?} — falling back to pdftotext",
                        q.verdict, q.score, path
                    );
                } else {
                    log::warn!(
                        "pdfplumber scored {:?} ({:.2}) for {:?} but finds materially \
                         fewer provision anchors than pdftotext — refusing tier 0 as \
                         INCOMPLETE and falling back",
                        q.verdict, q.score, path
                    );
                }
                // Structured tables are only carried on the winning pdfplumber path;
                // if it loses the quality gate we fall through to pdftotext (no
                // structured tables there today).
                best = Some((text, page_offsets, "pdfplumber", q.score));
            }
        }
    }

    // ── Tier 1: pdftotext (poppler) ──────────────────────────────────────────
    // Primary text extractor: pdftotext -layout keeps full lines (~2% tiny lines
    // on a real SPPRA page) whereas lopdf shatters words one-per-line (42% tiny).
    // lopdf was the historical Tier 1 for being dependency-free, but it loses on
    // every poppler-equipped machine; it is now the no-poppler fallback below.
    if let Ok(text) = extract_with_pdftotext(path) {
        if !text.trim().is_empty() {
            // US8/R1 (FR-013): reflow multi-column tables from `-bbox` geometry
            // so scrambled column rows become coherent, citable units. Prose
            // pages are untouched. Toggle: ECHO_TABLE_REFLOW=0 disables.
            let (text, reconstructed_rows, tables, reconstruction_method) =
                maybe_reflow_tables(path, text);
            let page_offsets = pdftotext_page_offsets(&text);
            let q = score(&text, page_offsets.len().max(1));
            if q.verdict == Verdict::Good {
                log::info!(
                    "pdftotext extracted {} chars / {} pages from {:?} ({} table rows reconstructed via {}, {} structured tables)",
                    text.len(), page_offsets.len(), path, reconstructed_rows.len(),
                    reconstruction_method, tables.len()
                );
                return Ok(ParsedDocument {
                    text,
                    page_offsets,
                    extraction_method: "pdftotext",
                    quality_score: q.score,
                    reconstructed_rows,
                    reconstruction_method,
                    tables,
                    provision_titles: Default::default(),
                });
            }
            log::debug!("pdftotext verdict {:?} (score {:.2}) for {:?}", q.verdict, q.score, path);
            best = Some((text, page_offsets, "pdftotext", q.score));
        }
    }

    // ── Tier 2: lopdf (no-dependency fallback) ───────────────────────────────
    // Only wins when pdftotext is unavailable/empty AND its output clears the
    // quality gate (the per-line fragmentation signal keeps column-exploded
    // output from scoring Good here).
    if let Ok(doc) = lopdf::Document::load(path) {
        let page_ids: Vec<u32> = doc.get_pages().keys().copied().collect();
        let total_pages = page_ids.len().max(1);

        let mut full_text = String::new();
        let mut page_offsets: Vec<(u32, usize)> = Vec::new();
        for (page_idx, &page_num) in page_ids.iter().enumerate() {
            page_offsets.push((page_idx as u32 + 1, full_text.len()));
            if let Ok(text) = doc.extract_text(&[page_num]) {
                full_text.push_str(&text);
            }
            full_text.push('\n');
        }

        let q = score(&full_text, total_pages);
        if q.verdict == Verdict::Good {
            return Ok(ParsedDocument {
                text: full_text,
                page_offsets,
                extraction_method: "lopdf",
                quality_score: q.score,
                reconstructed_rows: Vec::new(),
                reconstruction_method: "",
                tables: Vec::new(),
                provision_titles: Default::default(),
            });
        }
        log::debug!("lopdf verdict {:?} (score {:.2}) for {:?}", q.verdict, q.score, path);
        if best.as_ref().map(|b| b.3).unwrap_or(0.0) < q.score {
            best = Some((full_text, page_offsets, "lopdf", q.score));
        }
    }

    // ── Tier 3: OCR ──────────────────────────────────────────────────────────
    let ocr_cfg = OcrConfig {
        dpi: OcrConfig::dpi_from_env(),
        timeout_per_page_secs: 60,
        psm: OcrConfig::psm_from_env(),
        tmp_root: ocr_tmp_root(),
    };
    // Phase 9b: PaddleOCR is the DEFAULT scanned/image extractor when its sidecar
    // venv is present (`ECHO_PADDLE_OCR=0` opts out) — it doesn't fabricate numbers
    // the way tesseract does on scans. Tesseract is the fallback on any failure, and
    // the default only when the Paddle venv is absent. (See setup-ocr-venv.sh.)
    let ocr_result = if ocr::paddle_ocr_enabled() {
        ocr::ocr_pdf_paddle(path, &ocr_cfg).or_else(|e| {
            log::warn!("PaddleOCR failed for {:?}: {e}; falling back to tesseract", path);
            ocr_pdf(path, &ocr_cfg)
        })
    } else {
        ocr_pdf(path, &ocr_cfg)
    };
    match ocr_result {
        Ok(doc) => {
            let q = score(&doc.text, doc.page_offsets.len().max(1));
            let prior_score = best.as_ref().map(|b| b.3).unwrap_or(0.0);
            if q.score >= prior_score {
                return Ok(ParsedDocument {
                    text: doc.text,
                    page_offsets: doc.page_offsets,
                    extraction_method: doc.extraction_method,
                    quality_score: q.score,
                    reconstructed_rows: Vec::new(),
                    reconstruction_method: "",
                    tables: Vec::new(),
                    provision_titles: Default::default(),
                });
            }
            log::warn!(
                "OCR scored {:.2} below prior best {:.2} for {:?}; keeping prior",
                q.score, prior_score, path
            );
        }
        Err(e) => {
            log::warn!("OCR failed for {:?}: {e}", path);
        }
    }

    // ── Fallback: best-so-far, marked ocr_skipped ────────────────────────────
    if let Some((text, page_offsets, _, qscore)) = best {
        // The runner-up text was captured before its table list was, and the
        // importer DELETES every `⟦TBL:n⟧` region on the promise that the tables
        // come back as units. With no tables to come back, strip the markers
        // instead and let the linearised rows stay in the body as prose — a
        // degraded tier must not be the one that loses content outright.
        let text = table::strip_table_markers(&text);
        return Ok(ParsedDocument {
            text,
            page_offsets,
            extraction_method: "ocr_skipped",
            quality_score: qscore,
            reconstructed_rows: Vec::new(),
            reconstruction_method: "",
            tables: Vec::new(),
            provision_titles: Default::default(),
        });
    }
    Err(ParseError::ScannedPdf)
}

/// Per-call OCR working directory. Lives under the data partition, never `/tmp`
/// (root-disk constraint). Wired to config in Task 12.
fn ocr_tmp_root() -> PathBuf {
    std::env::var("ECHOAI_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("ocr")
}

/// Build page_offsets by scanning for form feed characters that pdftotext
/// inserts between pages. Returns at minimum `[(1, 0)]`.
///
/// `pub(crate)` so `structured_importer` can RE-derive the page map against the
/// normalized body (RC-PG): the offsets first computed here are aligned to the
/// raw extracted text, but `text_normalizer::normalize` later shortens that text
/// (gutter/blank-line/tab collapse) while preserving form feeds — so the raw
/// offsets must be recomputed on the normalized text or every page stamp drifts.
pub(crate) fn pdftotext_page_offsets(text: &str) -> Vec<(u32, usize)> {
    let mut offsets: Vec<(u32, usize)> = vec![(1, 0)];
    let mut page: u32 = 1;
    for (byte_idx, ch) in text.char_indices() {
        if ch == '\x0c' {
            page += 1;
            offsets.push((page, byte_idx + ch.len_utf8()));
        }
    }
    offsets
}

/// Run `pdftotext` (poppler-utils) and return stdout as a String.
fn extract_with_pdftotext(path: &Path) -> Result<String, ParseError> {
    let output = Command::new("pdftotext")
        .arg("-layout")
        .arg(path)
        .arg("-")
        .output()
        .map_err(|e| ParseError::Other(format!("pdftotext not found: {e}")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(ParseError::Other(format!(
            "pdftotext failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

/// Phase 9c: is pdfplumber the PRIMARY (text+table) extractor? ON by default;
/// `ECHO_PDFPLUMBER_PRIMARY=0` reverts to the historical pdftotext-first order
/// (used as a fast escape hatch if a pdfplumber regression is ever suspected).
fn pdfplumber_primary_enabled() -> bool {
    std::env::var("ECHO_PDFPLUMBER_PRIMARY").map(|v| v != "0").unwrap_or(true)
}

/// A line-initial provision ordinal — `1.`, `[12.`, `19A.`, `142.`.
///
/// Deliberately loose, and deliberately indifferent to what FOLLOWS the dot: it
/// counts the ordinal's presence, not its detectability, so a tier that merely
/// glues the number to its text (`19.(1)`, repaired later by
/// `text_normalizer::normalize`) is not accused of losing a provision.
fn provision_anchor_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^[ \t]*\[?[ \t]*[0-9]{1,3}[A-Z]?\.").unwrap())
}

/// Fraction of pdftotext's anchors that tier 0 may lack before it is refused.
/// Not zero: the two extractors reflow differently, so ±1 on a long document is
/// noise. 2% (floor 2) is far below any real defect — the Compendium's bad crop
/// lost 30% — and far above reflow jitter.
const ANCHOR_LOSS_TOLERANCE: f32 = 0.02;

/// Does tier 0 find as many provision ordinals as plain `pdftotext` does?
///
/// The completeness question `text_quality::score` cannot answer, because it
/// grades one extraction in isolation. Truncated-but-fluent text passes every
/// shape test it applies, so the only way to notice missing content is to ask a
/// second extractor about the SAME file and compare.
///
/// Fails OPEN: if pdftotext is unavailable or errors, tier 0 is accepted as
/// before. This gate exists to catch silent loss, never to block indexing.
fn tier0_is_complete(path: &Path, tier0_text: &str) -> bool {
    let reference = match extract_with_pdftotext(path) {
        Ok(t) => t,
        Err(e) => {
            log::debug!("completeness cross-check skipped for {:?}: {e}", path);
            return true;
        }
    };
    let theirs = provision_anchor_count(&reference);
    let ours = provision_anchor_count(tier0_text);
    let complete = anchors_within_tolerance(ours, theirs);
    if !complete {
        log::warn!(
            "completeness cross-check for {:?}: pdfplumber {} provision anchors vs \
             pdftotext {}",
            path, ours, theirs
        );
    }
    complete
}

fn provision_anchor_count(text: &str) -> usize {
    provision_anchor_re().find_iter(text).count()
}

/// Pure half of [`tier0_is_complete`], so the arithmetic is testable without a PDF.
fn anchors_within_tolerance(ours: usize, theirs: usize) -> bool {
    let allowed = ((theirs as f32) * ANCHOR_LOSS_TOLERANCE).max(2.0);
    (ours as f32) + allowed >= theirs as f32
}

#[cfg(test)]
mod completeness_tests {
    use super::{anchors_within_tolerance, provision_anchor_count};

    /// The shapes a provision ordinal takes across the corpus, including the
    /// glued form — the counter measures presence, not detectability.
    #[test]
    fn counts_every_provision_ordinal_shape() {
        let text = "\
1. Short title and commencement.
19.(1) The Returning Officer shall
  [113. The qualifications
19A. Right to information
142.Subject to the Constitution
";
        assert_eq!(provision_anchor_count(text), 5);
    }

    /// Things that are not line-initial ordinals must not inflate the count, or
    /// the comparison drifts and the gate stops meaning anything.
    #[test]
    fn ignores_non_anchors() {
        assert_eq!(provision_anchor_count("see rule 19.(1) above\n"), 0);
        assert_eq!(provision_anchor_count("the year 1996. Then\n"), 0);
        assert_eq!(provision_anchor_count("plain prose with no ordinal\n"), 0);
    }

    /// Reflow jitter is tolerated; real loss is not. The Compendium's bad crop
    /// found 33 where pdftotext found 47 — that must be refused.
    #[test]
    fn tolerates_jitter_but_refuses_real_loss() {
        assert!(anchors_within_tolerance(47, 47));
        assert!(anchors_within_tolerance(46, 47), "one anchor is reflow noise");
        assert!(anchors_within_tolerance(45, 47), "two is still within the floor");
        assert!(!anchors_within_tolerance(33, 47), "the Compendium defect");
        assert!(!anchors_within_tolerance(0, 280), "total loss");
        // A tier finding MORE than the reference is fine — never a reason to refuse.
        assert!(anchors_within_tolerance(300, 280));
        // Documents with no provisions at all must not be refused.
        assert!(anchors_within_tolerance(0, 0));
    }
}

/// US8/R1: reflow detected table regions in the `-layout` text using `-bbox`
/// word geometry. Returns `layout_text` unchanged when the toggle is off, when
/// the `-bbox` pass is unavailable, or when no table is detected (the common
/// prose case). `ECHO_TABLE_REFLOW=0` disables; default is ON.
fn maybe_reflow_tables(
    path: &Path,
    layout_text: String,
) -> (String, Vec<String>, Vec<table::ExtractedTable>, &'static str) {
    if std::env::var("ECHO_TABLE_REFLOW").map(|v| v == "0").unwrap_or(false) {
        return (layout_text, Vec::new(), Vec::new(), "");
    }
    // Phase 9a: prefer the pdfplumber sidecar — it returns correct cells where
    // `-layout` interleaves columns. ON by default; `ECHO_TABLE_SIDECAR=0` opts
    // out. Unavailable / no tables / any failure falls through to the geometric
    // projection engine below, so default-on is safe even with no sidecar binary.
    if std::env::var("ECHO_TABLE_SIDECAR").map(|v| v != "0").unwrap_or(true) {
        if let Some(pages) = table_sidecar::extract_tables(path) {
            let n_tables: usize = pages.iter().map(|p| p.tables.len()).sum();
            // The structured tables have to be carried out of here, not dropped:
            // this reflow writes `⟦TBL:n⟧` regions into the text and the importer
            // lifts those regions out of the body, so a discarded table list means
            // the table is deleted from the index outright. Measured on `Manual
            // for Development Projects 2019 PC-I` — the one corpus document that
            // takes this tier — 63 detected tables produced 0 table units.
            let (text, rows, tables) = table::reflow_with_sidecar(&layout_text, &pages);
            log::info!(
                "pdfplumber sidecar reflowed {} tables across {} pages ({} rows, {} structured) for {:?}",
                n_tables, pages.len(), rows.len(), tables.len(), path
            );
            return (text, rows, tables, "pdfplumber");
        }
    }
    match extract_with_pdftotext_bbox(path) {
        Ok(xml) => {
            let words = parse_bbox(&xml);
            if words.is_empty() {
                return (layout_text, Vec::new(), Vec::new(), "");
            }
            // The projection engine writes its rows into the body WITHOUT region
            // markers, so they stay part of the prose and no table unit is owed.
            let (text, rows) = table::reflow_tables_with_rows(&words, &layout_text);
            (text, rows, Vec::new(), "schema_anchored")
        }
        Err(e) => {
            log::debug!("pdftotext -bbox unavailable for {:?}: {e}; skipping table reflow", path);
            (layout_text, Vec::new(), Vec::new(), "")
        }
    }
}

/// Run `pdftotext -bbox` (poppler) and return the XML word-box stream.
fn extract_with_pdftotext_bbox(path: &Path) -> Result<String, ParseError> {
    let output = Command::new("pdftotext")
        .arg("-bbox")
        .arg(path)
        .arg("-")
        .output()
        .map_err(|e| ParseError::Other(format!("pdftotext not found: {e}")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(ParseError::Other(format!(
            "pdftotext -bbox failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

/// Parse the `pdftotext -bbox` XML into per-word boxes. Pages are delimited by
/// `<page …>` elements (1-based); words carry `xMin/yMin/xMax/yMax`.
fn parse_bbox(xml: &str) -> Vec<WordBox> {
    let word_re = Regex::new(
        r#"<word xMin="([0-9.]+)" yMin="([0-9.]+)" xMax="([0-9.]+)" yMax="([0-9.]+)"[^>]*>([^<]*)</word>"#,
    )
    .expect("bbox word regex compiles");
    let mut out = Vec::new();
    // split on `<page`: index 0 is the preamble, real pages start at 1.
    for (pi, chunk) in xml.split("<page").enumerate() {
        if pi == 0 {
            continue;
        }
        let pno = pi as u32;
        for c in word_re.captures_iter(chunk) {
            let xmin: f32 = c[1].parse().unwrap_or(0.0);
            let ymin: f32 = c[2].parse().unwrap_or(0.0);
            let xmax: f32 = c[3].parse().unwrap_or(0.0);
            let ymax: f32 = c[4].parse().unwrap_or(0.0);
            let text = decode_xml_entities(&c[5]);
            if text.trim().is_empty() {
                continue;
            }
            out.push(WordBox {
                text,
                x: xmin,
                y: ymin,
                w: (xmax - xmin).max(0.0),
                h: (ymax - ymin).max(0.0),
                page: pno,
            });
        }
    }
    out
}

/// Minimal XML entity decode for pdftotext `-bbox` word text.
fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}
