//! PDF → image (pdftoppm) → text (tesseract) via subprocess.
//! Owns all temp-file handling. Never writes outside the configured
//! data directory.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use super::{ParseError, ParsedDocument};

pub struct OcrConfig {
    pub dpi: u32,
    pub timeout_per_page_secs: u64,
    /// Tesseract page-segmentation mode. Defaults to 1 (auto-OSD) which works
    /// for single-column scans, but multi-column docs with marginal labels
    /// (Sindh RoB 1986) interleave columns under PSM 1. Try `4` (single
    /// column of variable text) or `6` (uniform block) when re-OCR'ing a
    /// problem source. See `tesseract --help-psm`.
    pub psm: u32,
    /// Parent directory under which a uniquely-named temp dir is created.
    /// Must NOT be `/tmp` (root-disk constraint) — caller passes the data dir.
    pub tmp_root: PathBuf,
}

impl OcrConfig {
    /// Read PSM mode from `ECHO_OCR_PSM` (1–13 per tesseract). Default 1.
    pub fn psm_from_env() -> u32 {
        std::env::var("ECHO_OCR_PSM")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|n| (1..=13).contains(n))
            .unwrap_or(1)
    }

    /// Config for OCR-ing a standalone image. `tmp_root` is deliberately empty:
    /// the file is already a page image, so `rasterise` never runs and nothing
    /// temporary is written. Honours the same `ECHO_OCR_*` overrides.
    pub fn for_image() -> Self {
        Self {
            dpi: Self::dpi_from_env(),
            timeout_per_page_secs: 60,
            psm: Self::psm_from_env(),
            tmp_root: PathBuf::new(),
        }
    }

    /// Read DPI from `ECHO_OCR_DPI`. Default 300.
    pub fn dpi_from_env() -> u32 {
        std::env::var("ECHO_OCR_DPI")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|n| (72..=600).contains(n))
            .unwrap_or(300)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_is_constructible() {
        let cfg = OcrConfig {
            dpi: 300,
            timeout_per_page_secs: 60,
            psm: 1,
            tmp_root: PathBuf::from("/var/tmp/echoai-test"),
        };
        assert_eq!(cfg.dpi, 300);
        assert_eq!(cfg.psm, 1);
    }
}

/// In-flight rasterisation result. Drop deletes `tmp_dir` and everything
/// inside it.
pub struct OcrWork {
    pub tmp_dir: PathBuf,
    pub png_paths: Vec<PathBuf>,
}

impl Drop for OcrWork {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.tmp_dir);
    }
}

pub fn rasterise(pdf: &Path, cfg: &OcrConfig) -> Result<OcrWork, ParseError> {
    std::fs::create_dir_all(&cfg.tmp_root)
        .map_err(|e| ParseError::Other(format!("create tmp_root: {e}")))?;
    let tmp_dir = cfg.tmp_root.join(format!("echoai-ocr-{}", uuid_like()));
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| ParseError::Other(format!("create tmp_dir: {e}")))?;

    let stem = tmp_dir.join("page");
    let output = Command::new("pdftoppm")
        .arg("-png")
        .arg("-r").arg(cfg.dpi.to_string())
        .arg(pdf)
        .arg(&stem)
        .output()
        .map_err(|e| ParseError::Other(format!("pdftoppm spawn: {e}")))?;
    if !output.status.success() {
        return Err(ParseError::Other(format!(
            "pdftoppm failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let mut png_paths: Vec<PathBuf> = std::fs::read_dir(&tmp_dir)
        .map_err(|e| ParseError::Other(format!("read_dir: {e}")))?
        .filter_map(|r| r.ok().map(|d| d.path()))
        .filter(|p| p.extension().map(|e| e == "png").unwrap_or(false))
        .collect();
    png_paths.sort();

    Ok(OcrWork { tmp_dir, png_paths })
}

/// Tiny non-crypto unique-id for temp dir names. Avoids adding a `uuid` dep
/// if the project does not already have one.
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{:x}-{}", now.as_nanos(), std::process::id())
}

/// Phase 9b: is PaddleOCR active? **ON by default**; `ECHO_PADDLE_OCR=0` opts out.
/// Safe as default-on because it ALSO requires the sidecar venv to be present
/// (`available()`) — machines without it transparently use tesseract — and the
/// OCR tier only fires for genuinely scanned docs. Where the venv exists (dev /
/// the GPU server), scans get PaddleOCR (no fabricated numbers); tesseract remains
/// the fallback on any failure. (CPU is ~1.7 min/page; the GPU server is fast.)
pub fn paddle_ocr_enabled() -> bool {
    std::env::var("ECHO_PADDLE_OCR").map(|v| v != "0").unwrap_or(true)
        && super::ocr_sidecar::available()
}

/// Phase 9b: OCR via PaddleOCR instead of tesseract. Rasterises with the same
/// `pdftoppm` path, then sends the page PNGs to the sidecar. PaddleOCR does not
/// invent numbers the way tesseract does on scans (the "Section 20413" / "Nov 02
/// 2013 → 2613" class of false citation — proven by the spike). `extraction_method
/// = "paddleocr"`. On any failure the caller falls back to [`ocr_pdf`] (tesseract).
pub fn ocr_pdf_paddle(pdf: &Path, cfg: &OcrConfig) -> Result<ParsedDocument, ParseError> {
    let work = rasterise(pdf, cfg)?;
    if work.png_paths.is_empty() {
        return Err(ParseError::Other("pdftoppm produced no pages".into()));
    }
    let pages = super::ocr_sidecar::ocr_images(&work.png_paths, "en")
        .ok_or_else(|| ParseError::Other("PaddleOCR sidecar unavailable/failed".into()))?;

    let mut full_text = String::new();
    let mut page_offsets: Vec<(u32, usize)> = Vec::with_capacity(pages.len());
    let mut low_conf = 0usize;
    for (idx, page) in pages.iter().enumerate() {
        page_offsets.push((idx as u32 + 1, full_text.len()));
        full_text.push_str(&page.text);
        if page.mean_conf > 0.0 && page.mean_conf < 0.80 {
            low_conf += 1;
        }
        if idx + 1 < pages.len() {
            full_text.push('\x0c');
        }
    }
    log::info!(
        "PaddleOCR extracted {} chars / {} pages from {:?} ({} low-confidence)",
        full_text.len(), pages.len(), pdf, low_conf
    );

    Ok(ParsedDocument {
        text: full_text,
        page_offsets,
        extraction_method: "paddleocr",
        quality_score: 1.0,
        reconstructed_rows: Vec::new(),
        reconstruction_method: "",
        tables: Vec::new(),
        provision_titles: Default::default(),
    })
}

/// OCR a standalone image file (png/jpg/…) — a photographed or scanned page.
///
/// ⚠️ Added 2026-08-11. Images previously went to `image::describe`, which
/// requires a **vision model** the deployment does not load, so every image
/// upload failed with "Vision model not loaded" and never touched OCR at all.
/// A photographed notice is the single most common thing an officer uploads, so
/// the text ladder — not a captioning model — is the right treatment.
///
/// No rasterisation step: the file already IS the page image. Same ladder as
/// scanned PDFs — PP-OCR (via the sidecar, on ONNX) first, tesseract on failure.
pub fn ocr_image(image: &Path, cfg: &OcrConfig) -> Result<ParsedDocument, ParseError> {
    let one = [image.to_path_buf()];

    if paddle_ocr_enabled() {
        if let Some(pages) = super::ocr_sidecar::ocr_images(&one, "en") {
            let text: String = pages.iter().map(|p| p.text.as_str()).collect();
            if !text.trim().is_empty() {
                log::info!("PP-OCR extracted {} chars from image {image:?}", text.len());
                return Ok(ParsedDocument {
                    text,
                    page_offsets: vec![(1, 0)],
                    extraction_method: "paddleocr",
                    quality_score: 1.0,
                    reconstructed_rows: Vec::new(),
                    reconstruction_method: "",
                    tables: Vec::new(),
                    provision_titles: Default::default(),
                });
            }
        }
    }

    let text = run_tesseract(image, Duration::from_secs(cfg.timeout_per_page_secs), cfg.psm)?;
    log::info!("tesseract extracted {} chars from image {image:?}", text.len());
    Ok(ParsedDocument {
        text,
        page_offsets: vec![(1, 0)],
        extraction_method: "ocr",
        quality_score: 1.0,
        reconstructed_rows: Vec::new(),
        reconstruction_method: "",
        tables: Vec::new(),
        provision_titles: Default::default(),
    })
}

pub fn ocr_pdf(pdf: &Path, cfg: &OcrConfig) -> Result<ParsedDocument, ParseError> {
    let work = rasterise(pdf, cfg)?;
    if work.png_paths.is_empty() {
        return Err(ParseError::Other("pdftoppm produced no pages".into()));
    }

    let mut full_text = String::new();
    let mut page_offsets: Vec<(u32, usize)> = Vec::with_capacity(work.png_paths.len());

    for (idx, png) in work.png_paths.iter().enumerate() {
        let page_num = idx as u32 + 1;
        let offset = full_text.len();
        page_offsets.push((page_num, offset));

        match run_tesseract(png, Duration::from_secs(cfg.timeout_per_page_secs), cfg.psm) {
            Ok(page_text) => {
                full_text.push_str(&page_text);
            }
            Err(e) => {
                log::warn!("tesseract failed on {png:?}: {e}; skipping page");
            }
        }
        // Form-feed separator matches pdftotext convention.
        if idx + 1 < work.png_paths.len() {
            full_text.push('\x0c');
        }
    }

    Ok(ParsedDocument {
        text: full_text,
        page_offsets,
        extraction_method: "ocr",
        quality_score: 1.0,
        reconstructed_rows: Vec::new(),
        reconstruction_method: "",
        tables: Vec::new(),
        provision_titles: Default::default(),
    })
}

fn run_tesseract(png: &Path, _timeout: Duration, psm: u32) -> Result<String, ParseError> {
    // NB: std::process does not natively support timeouts; for v1 we rely
    // on tesseract's own behaviour. Per-page wallclock cap belongs to a
    // future revision (would need a thread + kill-on-timeout).
    let output = Command::new("tesseract")
        .arg(png)
        .arg("-")             // stdout
        .arg("-l").arg("eng")
        .arg("--psm").arg(psm.to_string())
        .output()
        .map_err(|e| ParseError::Other(format!("tesseract spawn: {e}")))?;
    if !output.status.success() {
        return Err(ParseError::Other(format!(
            "tesseract: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod rasterise_tests {
    use super::*;

    pub(super) fn tesseract_and_pdftoppm_available() -> bool {
        Command::new("pdftoppm").arg("-v").output().is_ok()
            && Command::new("tesseract").arg("--version").output().is_ok()
    }

    pub(super) fn fixture_pdf() -> Option<PathBuf> {
        let p = std::env::var("OCR_FIXTURE_DIR").ok()?;
        let dir = PathBuf::from(p);
        let candidate = dir.join("REMOVAL FROM SERVICE SPECIAL POWERS SINDH.pdf");
        candidate.exists().then_some(candidate)
    }

    #[test]
    fn rasterise_produces_pngs_and_cleans_up() {
        if !tesseract_and_pdftoppm_available() {
            eprintln!("skip: tesseract/pdftoppm not available");
            return;
        }
        let Some(pdf) = fixture_pdf() else {
            eprintln!("skip: OCR_FIXTURE_DIR not set or fixture missing");
            return;
        };

        let cfg = OcrConfig {
            dpi: 150,
            timeout_per_page_secs: 60,
            psm: 1,
            tmp_root: std::env::temp_dir(),
        };

        let work = rasterise(&pdf, &cfg).expect("rasterise");
        assert!(work.png_paths.len() >= 1, "at least one page rendered");
        for p in &work.png_paths {
            assert!(p.exists(), "{p:?} should exist before drop");
        }
        let dir = work.tmp_dir.clone();
        drop(work);
        assert!(!dir.exists(), "tmp dir should be cleaned on drop: {dir:?}");
    }
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use super::rasterise_tests::{fixture_pdf, tesseract_and_pdftoppm_available};

    #[test]
    fn ocr_pdf_extracts_text_from_fixture() {
        let Some(pdf) = fixture_pdf() else {
            eprintln!("skip: OCR_FIXTURE_DIR not set");
            return;
        };
        if !tesseract_and_pdftoppm_available() {
            eprintln!("skip: tesseract/pdftoppm not available");
            return;
        }

        let cfg = OcrConfig {
            dpi: 150,
            timeout_per_page_secs: 60,
            psm: 1,
            tmp_root: std::env::temp_dir(),
        };
        let doc = ocr_pdf(&pdf, &cfg).expect("ocr_pdf");
        assert!(doc.text.len() > 200, "expected non-trivial OCR text");
        assert!(!doc.page_offsets.is_empty());
        // Heuristic: ASCII content dominates.
        let ascii = doc.text.chars().filter(|c| c.is_ascii()).count();
        assert!(ascii as f32 / doc.text.chars().count() as f32 > 0.90);
    }
}
