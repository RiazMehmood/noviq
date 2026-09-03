//! Client for the Phase 9b **PaddleOCR** capability of the ingestion sidecar.
//!
//! Indexing-time, deterministic, NO LLM. Unlike the 9a table path (a frozen
//! pdfplumber binary), OCR runs PaddlePaddle — heavy and best served from a
//! Python **venv** (the IBA GPU server, where OCR belongs, runs Python directly;
//! freezing Paddle buys nothing there). So this client spawns
//! `<venv-python> ingestion_sidecar.py` with `op=ocr`.
//!
//! Rust owns rasterisation (reuses `ocr::rasterise` → `pdftoppm`); the sidecar
//! receives the page PNG paths and returns recognised text + confidence per page.
//! Any failure (venv missing, import error, timeout) returns `None` so the caller
//! falls back to tesseract — OCR never fails the index.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

/// One OCR'd page: recognised text plus a mean confidence (0–1) used downstream
/// to flag low-confidence pages (refuse-or-flag).
#[derive(Debug, Clone)]
pub struct OcrPage {
    pub text: String,
    pub mean_conf: f32,
}

#[derive(Debug, Deserialize)]
struct WireResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    pages: Vec<WirePage>,
}

#[derive(Debug, Deserialize)]
struct WirePage {
    #[serde(default)]
    text: String,
    #[serde(default)]
    mean_conf: f32,
}

/// Generous per-document budget: PaddleOCR is ~1.7 min/page on CPU, so allow a
/// long window (the model loads once for the whole batch). Bounded so a wedged
/// process can never hang the indexer.
fn timeout_for(n_pages: usize) -> Duration {
    // ~3 min/page ceiling + a 2 min floor for model load.
    Duration::from_secs((n_pages as u64 * 180).max(120) + 120)
}

/// OCR the given page PNGs (in order) via PaddleOCR. `None` ⇒ capability
/// unavailable / failed ⇒ caller falls back to tesseract.
pub fn ocr_images(png_paths: &[PathBuf], lang: &str) -> Option<Vec<OcrPage>> {
    if png_paths.is_empty() {
        return None;
    }
    let python = discover_python()?;
    let script = script_path();
    if !script.is_file() {
        log::warn!("OCR sidecar script not found at {script:?}; falling back to tesseract");
        return None;
    }

    let images: Vec<String> = png_paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let request = serde_json::json!({ "op": "ocr", "images": images, "lang": lang }).to_string();

    let stdout = match run(&python, &script, &request, timeout_for(png_paths.len())) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("PaddleOCR sidecar failed: {e}; falling back to tesseract");
            return None;
        }
    };
    let resp: WireResponse = match serde_json::from_str(stdout.trim()) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("PaddleOCR sidecar bad JSON: {e}; falling back to tesseract");
            return None;
        }
    };
    if !resp.ok {
        log::warn!(
            "PaddleOCR sidecar error: {}; falling back to tesseract",
            resp.error.unwrap_or_else(|| "unknown".into())
        );
        return None;
    }
    Some(
        resp.pages
            .into_iter()
            .map(|p| OcrPage { text: p.text, mean_conf: p.mean_conf })
            .collect(),
    )
}

/// Is the PaddleOCR capability available (venv + script present)? Lets callers
/// decide before paying rasterisation cost.
pub fn available() -> bool {
    discover_python().is_some() && script_path().is_file()
}

/// Locate a Python interpreter that has PaddleOCR. `ECHO_OCR_PYTHON` overrides
/// (set this in the server `.env` on the VM); otherwise the conventional venv at
/// `~/.cache/echo-ocr-venv-build/.ocr-venv` (built by setup-ocr-venv.sh). `None`
/// ⇒ capability unavailable ⇒ caller uses tesseract.
fn discover_python() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ECHO_OCR_PYTHON") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
        log::warn!("ECHO_OCR_PYTHON={p:?} is not a file; ignoring");
    }
    let home = std::env::var("HOME").ok()?;
    let cand = PathBuf::from(home).join(".cache/echo-ocr-venv-build/.ocr-venv/bin/python");
    cand.is_file().then_some(cand)
}

/// The sidecar script (same file the 9a table op lives in). `ECHO_OCR_SIDECAR_SCRIPT`
/// overrides; otherwise the in-repo path.
fn script_path() -> PathBuf {
    if let Ok(p) = std::env::var("ECHO_OCR_SIDECAR_SCRIPT") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("sidecars/ingestion-sidecar/ingestion_sidecar.py")
}

/// Spawn `python script`, write `request` to stdin, return stdout. Bounded by
/// `timeout`; on overrun the child is killed.
fn run(python: &Path, script: &Path, request: &str, timeout: Duration) -> std::io::Result<String> {
    let mut child = Command::new(python)
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let mut stdin = child.stdin.take().expect("piped stdin");
    let req = request.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(req.as_bytes());
    });
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "PaddleOCR sidecar timed out",
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
    let _ = writer.join();
    Ok(reader.join().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual end-to-end: drive the real venv sidecar over already-rasterised
    /// page PNGs. Ignored by default (needs the PaddleOCR venv + page images).
    ///   ECHO_OCR_TEST_IMAGES="/path/p3.png,/path/p7.png" \
    ///   cargo test --lib ocr_sidecar::tests::smoke -- --ignored --nocapture
    #[test]
    #[ignore]
    fn smoke_ocr_images() {
        let imgs = std::env::var("ECHO_OCR_TEST_IMAGES").expect("set ECHO_OCR_TEST_IMAGES");
        let paths: Vec<PathBuf> = imgs.split(',').map(PathBuf::from).collect();
        let pages = ocr_images(&paths, "en").expect("ocr_images returned None");
        for p in &pages {
            eprintln!("mean_conf={:.2} chars={}", p.mean_conf, p.text.len());
        }
        assert!(
            pages.iter().any(|p| p.text.contains("2013")),
            "PaddleOCR should read the year 2013 verbatim"
        );
    }
}
