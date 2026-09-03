//! Feature 004 acceptance gate.
//! Every PDF in OCR_FIXTURE_DIR must end with verdict = Good.
//!
//! Run:
//!   OCR_FIXTURE_DIR="/home/riaz/Desktop/Sindh Govt Documents" \
//!     cargo test --test ocr_integration_acceptance -- --nocapture

use echo_ai_lib::services::parsers::pdf;
use echo_ai_lib::services::parsers::text_quality::{score, Verdict};

#[test]
fn every_sindh_pdf_extracts_with_good_verdict() {
    let dir = match std::env::var("OCR_FIXTURE_DIR") {
        Ok(d) => std::path::PathBuf::from(d),
        Err(_) => {
            eprintln!("skip: OCR_FIXTURE_DIR not set");
            return;
        }
    };

    let pdfs: Vec<_> = std::fs::read_dir(&dir).expect("read fixture dir")
        .filter_map(|r| r.ok().map(|d| d.path()))
        .filter(|p| p.extension().map(|e| e == "pdf").unwrap_or(false))
        .collect();
    assert!(!pdfs.is_empty(), "no PDFs found in {dir:?}");

    let mut failures: Vec<String> = Vec::new();
    for pdf_path in &pdfs {
        let name = pdf_path.file_name().unwrap().to_string_lossy().into_owned();
        match pdf::extract(pdf_path) {
            Ok(doc) => {
                let q = score(&doc.text, doc.page_offsets.len().max(1));
                println!(
                    "{name}: method={} score={:.3} verdict={:?}",
                    doc.extraction_method, q.score, q.verdict
                );
                if q.verdict != Verdict::Good {
                    failures.push(format!("{name}: verdict={:?}", q.verdict));
                }
            }
            Err(e) => failures.push(format!("{name}: parse error {e}")),
        }
    }

    assert!(failures.is_empty(), "verdict failures:\n  {}", failures.join("\n  "));
}

#[test]
fn constitution_pdf_does_not_trip_ocr() {
    // Born-digital control: confirms the heuristic does not over-trigger.
    let Ok(dir) = std::env::var("OCR_FIXTURE_DIR") else { return; };
    let path = std::path::PathBuf::from(dir).join("constitution.pdf");
    if !path.exists() { return; }
    let doc = pdf::extract(&path).expect("extract constitution");
    assert!(
        doc.extraction_method == "lopdf" || doc.extraction_method == "pdftotext",
        "constitution.pdf should not need OCR, got method={}", doc.extraction_method
    );
}
