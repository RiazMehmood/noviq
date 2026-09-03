//! Client for the Phase 9a **ingestion sidecar** (pdfplumber table extraction).
//!
//! Indexing-time, deterministic, NO LLM. The sidecar is an optional frozen
//! Python binary (see `src-tauri/sidecars/ingestion-sidecar/`); when it is
//! absent or fails, the caller falls back to the Rust projection engine — a
//! missing sidecar never fails the index.
//!
//! Unlike the whisper sidecar this runs from the *parser* layer (no `AppHandle`),
//! so it is discovered + spawned with `std::process::Command`, mirroring how
//! `pdf.rs` already shells out to `pdftotext`. v1 protocol is one-shot CLI: a
//! single JSON request on stdin, a single JSON response on stdout, process exits.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::table::{ExtractedTable, SidecarPage, SidecarTable};

/// Indexing-time budget for the whole document. pdfplumber scans every page for
/// ruled tables; a few seconds is typical even for 200+ page docs, but bound it
/// so a pathological PDF can never hang the indexer.
const TIMEOUT: Duration = Duration::from_secs(300);

// ── wire types (sidecar JSON → these → pure `table::Sidecar*`) ───────────────
#[derive(Debug, Deserialize)]
struct WireResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    pages: std::collections::HashMap<String, WirePage>,
}

#[derive(Debug, Deserialize)]
struct WirePage {
    /// Full page text (only present on the `extract_document` op — pdfplumber as
    /// the PRIMARY text extractor). Empty/absent for the tables-only op.
    #[serde(default)]
    text: String,
    #[serde(default)]
    above: String,
    #[serde(default)]
    below: String,
    #[serde(default)]
    tables: Vec<WireTable>,
    /// `{provision_number: title}` read off the marginal catchline column, when
    /// the document has one (`extract_document` only). The sidecar does this
    /// pairing because it needs both columns' y-coordinates, which exist nowhere
    /// downstream.
    #[serde(default)]
    provision_titles: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct WireTable {
    #[serde(default)]
    header: Vec<String>,
    #[serde(default)]
    rows: Vec<Vec<String>>,
}

/// Run `extract_tables` against `pdf_path`. Returns the per-page table data, or
/// `None` when the sidecar is unavailable, errors, times out, or the document
/// has no detectable tables — every `None` path means "fall back to projection".
pub fn extract_tables(pdf_path: &std::path::Path) -> Option<Vec<SidecarPage>> {
    let bin = discover()?;
    let request = serde_json::json!({
        "op": "extract_tables",
        "pdf_path": pdf_path.to_string_lossy(),
        "pages": "all",
    })
    .to_string();

    let stdout = match run(&bin, &request) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("ingestion sidecar failed for {:?}: {e}; falling back to projection", pdf_path);
            return None;
        }
    };

    match parse_response(stdout.trim()) {
        Ok(pages) => pages,
        Err(e) => {
            log::warn!("ingestion sidecar response for {:?}: {e}; falling back", pdf_path);
            None
        }
    }
}

/// Run `extract_document` — pdfplumber as the **PRIMARY** text+table extractor.
/// Returns `(reflowed_text, reconstructed_table_rows)`: the full document text,
/// form-feed-joined per page (matching the `pdftotext -layout` page convention so
/// `pdftotext_page_offsets` works downstream) with any detected tables linearised
/// in place via the proven sidecar reflow. `None` ⇒ sidecar unavailable, failed,
/// timed out, or produced no text ⇒ the caller falls back to `pdftotext`.
/// What `extract_document` yields. A struct rather than a tuple: it already
/// carried three unlabelled members and the provision-title map makes four.
pub struct DocumentExtraction {
    pub text: String,
    pub rows: Vec<String>,
    pub tables: Vec<ExtractedTable>,
    /// `{provision_number: title}` from the marginal catchline column; empty for
    /// documents that have no such column.
    pub provision_titles: std::collections::HashMap<String, String>,
}

pub fn extract_document(pdf_path: &std::path::Path) -> Option<DocumentExtraction> {
    let bin = discover()?;
    let request = serde_json::json!({
        "op": "extract_document",
        "pdf_path": pdf_path.to_string_lossy(),
    })
    .to_string();

    let stdout = match run(&bin, &request) {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "ingestion sidecar (document) failed for {:?}: {e}; falling back to pdftotext",
                pdf_path
            );
            return None;
        }
    };

    match parse_document(stdout.trim()) {
        Ok(res) => res,
        Err(e) => {
            log::warn!(
                "ingestion sidecar document response for {:?}: {e}; falling back to pdftotext",
                pdf_path
            );
            None
        }
    }
}

/// Parse an `extract_document` response into `(text, rows, tables)`. Pure (no IO)
/// for unit-testability. `Ok(None)` ⇒ valid response but no usable text (e.g. a
/// scanned PDF) ⇒ fall back; `Err` ⇒ malformed / sidecar-reported error.
fn parse_document(json: &str) -> Result<Option<DocumentExtraction>, String> {
    let resp: WireResponse = serde_json::from_str(json).map_err(|e| format!("bad JSON: {e}"))?;
    if !resp.ok {
        return Err(resp.error.unwrap_or_else(|| "unknown sidecar error".into()));
    }
    if resp.pages.is_empty() {
        return Ok(None);
    }

    // Order pages by their 1-based number so the form-feed layout matches the PDF.
    let mut pages: Vec<(u32, WirePage)> = resp
        .pages
        .into_iter()
        .filter_map(|(k, v)| k.parse::<u32>().ok().map(|n| (n, v)))
        .collect();
    pages.sort_by_key(|(n, _)| *n);
    let max_page = match pages.last() {
        Some((n, _)) => *n as usize,
        None => return Ok(None),
    };

    // One slot per page (gaps → empty page, preserving page-number alignment).
    let mut layout_pages: Vec<String> = vec![String::new(); max_page];
    let mut sidecar_pages: Vec<SidecarPage> = Vec::new();
    // Provision titles are per-page in the wire format but per-DOCUMENT in use —
    // a provision number is unique within an instrument.
    let mut provision_titles: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (n, p) in pages {
        if let Some(slot) = layout_pages.get_mut((n - 1) as usize) {
            *slot = p.text;
        }
        for (num, title) in p.provision_titles {
            provision_titles.entry(num).or_insert(title);
        }
        if !p.tables.is_empty() {
            let tables = p
                .tables
                .into_iter()
                .map(|t| SidecarTable { header: t.header, rows: t.rows })
                .collect();
            sidecar_pages.push(SidecarPage { page: n, above: p.above, below: p.below, tables });
        }
    }

    let layout = layout_pages.join("\x0c");
    if layout.trim().is_empty() {
        // pdfplumber returned no text (image-only PDF) → let pdftotext/OCR handle it.
        return Ok(None);
    }
    sidecar_pages.sort_by_key(|p| p.page);
    let (text, rows, tables) = super::table::reflow_with_sidecar(&layout, &sidecar_pages);
    Ok(Some(DocumentExtraction { text, rows, tables, provision_titles }))
}

/// Parse a sidecar JSON response into sorted [`SidecarPage`]s. Pure (no IO) so
/// the golden test can drive it from a captured fixture. `Ok(None)` ⇒ valid
/// response with no usable tables; `Err` ⇒ malformed / sidecar-reported error.
fn parse_response(json: &str) -> Result<Option<Vec<SidecarPage>>, String> {
    let resp: WireResponse = serde_json::from_str(json).map_err(|e| format!("bad JSON: {e}"))?;
    if !resp.ok {
        return Err(resp.error.unwrap_or_else(|| "unknown sidecar error".into()));
    }
    let mut out: Vec<SidecarPage> = resp
        .pages
        .into_iter()
        .filter_map(|(pno, p)| {
            let page: u32 = pno.parse().ok()?;
            let tables: Vec<SidecarTable> = p
                .tables
                .into_iter()
                .map(|t| SidecarTable { header: t.header, rows: t.rows })
                .collect();
            if tables.is_empty() {
                return None;
            }
            Some(SidecarPage { page, above: p.above, below: p.below, tables })
        })
        .collect();
    if out.is_empty() {
        return Ok(None);
    }
    out.sort_by_key(|p| p.page);
    Ok(Some(out))
}

/// Locate the sidecar binary. Order: explicit env override → next to the running
/// executable (the bundled `externalBin` drops it there) → dev `binaries/` dir.
/// `None` ⇒ capability unavailable ⇒ caller uses the projection engine.
fn discover() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ECHO_INGESTION_SIDECAR") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
        log::warn!("ECHO_INGESTION_SIDECAR={p:?} is not a file; ignoring");
    }

    // Next to the app executable (Tauri copies externalBin here, suffix stripped).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["ingestion-sidecar", "ingestion-sidecar.exe"] {
                let cand = dir.join(name);
                if cand.is_file() {
                    return Some(cand);
                }
            }
            if let Some(found) = first_match(dir) {
                return Some(found);
            }
        }
    }

    // Dev: the repo's binaries dir, where build.sh drops `ingestion-sidecar-<triple>`.
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
    first_match(&dev)
}

/// First file named `ingestion-sidecar-*` in `dir`, if any (handles the
/// target-triple suffix without hardcoding the triple).
fn first_match(dir: &std::path::Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("ingestion-sidecar-") && e.path().is_file() {
            return Some(e.path());
        }
    }
    None
}

/// Spawn the sidecar, write `request` to stdin, return stdout. Bounded by
/// `TIMEOUT`; on overrun the child is killed and an error returned.
fn run(bin: &std::path::Path, request: &str) -> std::io::Result<String> {
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    // Write the request from a thread; dropping the handle closes stdin so the
    // one-shot sidecar knows the request is complete. Reading stdout in another
    // thread avoids a pipe-buffer deadlock on large responses.
    let mut stdin = child.stdin.take().expect("piped stdin");
    let req = request.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(req.as_bytes());
        // stdin dropped here → EOF to the child.
    });
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });

    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "ingestion sidecar timed out",
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    let _ = writer.join();
    Ok(reader.join().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual end-to-end harness against the real frozen binary + a real PDF.
    /// Ignored by default (needs the binary in `binaries/` and a PDF path).
    /// Run e.g.:
    ///   ECHO_SPIKE_PDF=~/Desktop/table_ingestion_test/09_Sindh_Delegation_of_Financial_Powers_2019.pdf \
    ///   cargo test --lib table_sidecar::tests::spike -- --ignored --nocapture
    #[test]
    #[ignore]
    fn spike_extract_and_reflow() {
        let pdf = std::env::var("ECHO_SPIKE_PDF").expect("set ECHO_SPIKE_PDF");
        let pdf = std::path::PathBuf::from(shellexpand_home(&pdf));
        let pages = match extract_tables(&pdf) {
            Some(p) => p,
            None => {
                eprintln!("no machine-detectable tables (→ projection-engine fallback)");
                return;
            }
        };
        let n_tables: usize = pages.iter().map(|p| p.tables.len()).sum();
        eprintln!("pages-with-tables={} tables={}", pages.len(), n_tables);
        // A layout with enough form-feed pages that every reported page is in range.
        let max_page = pages.iter().map(|p| p.page).max().unwrap_or(1) as usize;
        let layout = vec!["·"; max_page].join("\x0c");
        let (_text, rows, _tables) = super::super::table::reflow_with_sidecar(&layout, &pages);
        eprintln!("emitted-rows={}", rows.len());
        for r in rows.iter().take(8) {
            eprintln!("ROW: {r}");
        }
        assert!(!pages.is_empty());
    }

    /// What the structured table list looks like for a REAL document — the
    /// stitching + header classification this module feeds. Reports one line per
    /// logical table so a page-per-table regression is visible at a glance.
    /// Ignored by default (needs the frozen binary and a PDF). Run:
    ///   ECHO_INGESTION_SIDECAR=…/ingestion-sidecar \
    ///   ECHO_SPIKE_PDF="…/Sindh Rules of Business 1986.pdf" \
    ///   cargo test --lib table_sidecar::tests::spike_structured -- --ignored --nocapture
    #[test]
    #[ignore]
    fn spike_structured_tables() {
        let pdf = std::env::var("ECHO_SPIKE_PDF").expect("set ECHO_SPIKE_PDF");
        let pdf = std::path::PathBuf::from(shellexpand_home(&pdf));
        let ex = extract_document(&pdf).expect("sidecar produced a document");
        eprintln!("logical tables = {}", ex.tables.len());
        for t in &ex.tables {
            let last = t.rows.iter().map(|r| r.page).max().unwrap_or(t.page);
            eprintln!(
                "table {:>3} pp.{:>3}-{:<3} rows={:<4} caption={:?} header={:?}",
                t.id, t.page, last, t.rows.len(), t.caption, t.header
            );
        }
        for t in &ex.tables {
            let serials: Vec<String> = t
                .rows
                .iter()
                .filter_map(|r| r.cells.first())
                .map(|c| c.trim())
                .filter(|c| !c.is_empty() && c.chars().next().is_some_and(|d| d.is_ascii_digit()))
                .map(|c| c.to_string())
                .collect();
            eprintln!("table {} serials ({}): {}", t.id, serials.len(), serials.join(", "));
        }
        // What the importer will keep. A row that ends up OUTSIDE a region is the
        // defect this whole pass exists to stop: it stays in the body and the
        // clause parser mints it as a provision. Mirrors
        // `structured_importer::strip_table_regions` — page by page, so a lost
        // marker cannot outlive its page.
        let mut outside: Vec<&str> = Vec::new();
        for page in ex.text.split('\u{0c}') {
            let mut in_region = false;
            for line in page.lines() {
                if super::super::table::is_table_marker_line(line) {
                    in_region = matches!(super::super::table::parse_table_marker(line), Some((_, true)));
                    continue;
                }
                if !in_region && line.contains(" — ") && line.contains(": ") {
                    outside.push(line);
                }
            }
        }
        eprintln!("linearised rows left in the body = {}", outside.len());
        for l in outside.iter().take(6) {
            eprintln!("  BODY: {}", &l[..l.len().min(140)]);
        }

        // ...and then through the REAL importer. Measuring the reflow alone is
        // what let the chrome-strip defect ship: the markers were intact here and
        // deleted three passes later, so this said 0 while production had 105
        // table rows stored as provisions. The number that matters is the one
        // after the strip chain.
        let offsets = super::super::pdf::pdftotext_page_offsets(&ex.text);
        let outcome = crate::services::structured_importer::import(
            &ex.text,
            &pdf.file_name().unwrap_or_default().to_string_lossy(),
            None,
            &offsets,
            &ex.provision_titles,
        );
        fn walk(n: &crate::services::structure_parser::DocNode, hits: &mut Vec<String>) {
            let t = n.text.trim();
            if t.contains(" — ") && t.contains(": ") {
                hits.push(format!("{} :: {}", n.citation, &t[..t.len().min(90)]));
            }
            for c in &n.children {
                walk(c, hits);
            }
        }
        let mut hits = Vec::new();
        walk(&outcome.tree, &mut hits);
        eprintln!("PROVISIONS that are really table rows = {}", hits.len());
        for h in hits.iter().take(6) {
            eprintln!("  PROVISION: {h}");
        }

        // Completeness. Numbered provisions run consecutively, so the document
        // audits itself — which is how the 13 Articles a one-line circulation
        // test deleted were found. Any table change can move this number, so it
        // is reported next to the table counts, not in a separate tool.
        fn numbers(n: &crate::services::structure_parser::DocNode, out: &mut Vec<u32>) {
            let c = n.citation.trim();
            if let Some(rest) = ["Article ", "Rule ", "Section ", "Regulation "]
                .iter()
                .find_map(|p| c.strip_prefix(*p))
            {
                if let Ok(v) = rest.parse::<u32>() {
                    out.push(v);
                }
            }
            for ch in &n.children {
                numbers(ch, out);
            }
        }
        let mut ns = Vec::new();
        numbers(&outcome.tree, &mut ns);
        ns.sort_unstable();
        ns.dedup();
        if let (Some(&lo), Some(&hi)) = (ns.first(), ns.last()) {
            let missing: Vec<u32> = (lo..=hi).filter(|n| !ns.contains(n)).collect();
            eprintln!(
                "provisions: {} distinct, {lo}..{hi}, {} absent{}",
                ns.len(),
                missing.len(),
                if missing.is_empty() {
                    String::new()
                } else {
                    format!(
                        ": {}",
                        missing.iter().take(20).map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
                    )
                }
            );
        }
    }

    /// Frozen golden: the captured real p8 sidecar JSON, parsed + reflowed, must
    /// produce the exact schema-anchored rows. Portable (no binary/PDF needed) —
    /// protects against linearize/format drift and parse regressions.
    #[test]
    fn golden_delegation_p8_reflow() {
        let json = include_str!("../../../tests/fixtures/table_sidecar/delegation_p8.json");
        let pages = parse_response(json).expect("valid response").expect("has tables");
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].page, 8);

        let layout = vec!["·"; 8].join("\x0c");
        let (text, rows, _tables) = super::super::table::reflow_with_sidecar(&layout, &pages);

        let expected_first = "1. Creation of Post — To whom delegated: (i) Administrative Departments shall have full powers to create new post (s) on temporary basis in the development budget (Public Sector Development Programme), included in PC-I or PC-II, subject to the approval of the project or scheme by the relevant forum and subject to availability of development budget against Employees Related Expenses. These posts shall be continued on year to year basis till the completion of the project or scheme. Such post(s) shall cease to exist on the closure, or completion of the project or scheme.; Extent: Full Powers.";
        assert_eq!(rows[0], expected_first, "golden first row");
        assert!(rows.iter().any(|r| r.starts_with("(ii) After completion")), "continuation row");
        assert!(rows.iter().any(|r| r.starts_with("2. Reciprocal")), "serial 2 row");
        assert!(rows.iter().any(|r| r.starts_with("Note: All orders")), "spanning note");
        // No enumerator leaked; prose band preserved.
        assert!(!rows.iter().any(|r| r.contains("(2)") && r.contains("(3)")), "enumerator dropped");
        assert!(text.contains("SECOND SCHEDULE"), "above-prose preserved");
    }

    fn shellexpand_home(p: &str) -> String {
        match p.strip_prefix("~/") {
            Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()),
            None => p.to_string(),
        }
    }
}
