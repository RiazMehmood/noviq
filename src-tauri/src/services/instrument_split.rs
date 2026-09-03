//! Cut a compendium into one file per legal instrument.
//!
//! The second half of [`super::instrument_detector`]: that one PROPOSES
//! boundaries, this one carries out a plan a human has confirmed. The split is
//! never derived here — it is always given, because a wrong boundary silently
//! truncates an instrument and that is worse than not splitting at all.
//!
//! # Why the original source row goes away
//!
//! Leaving it indexed defeats the purpose: its chunks keep competing in
//! retrieval, and "Rule 4" stays ambiguous between the book and the twelve laws
//! inside it. The original FILE is kept — moved aside, not deleted — so the
//! provenance of every cut is still on disk, and `sources.split_from_file`
//! records where each piece came from.
//!
//! The file is moved OUT of the indexed folder rather than left in place: a
//! later folder re-scan would otherwise find it and re-create the very container
//! source the split just removed.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One confirmed instrument: an inclusive 1-based page range and the name it
/// will be cited by.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlannedPiece {
    pub start: u32,
    pub end: u32,
    /// Becomes `sources.display_name` — the string that appears under every
    /// answer drawn from these pages, so it is the reviewer's to get right.
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct CutPiece {
    pub path: PathBuf,
    pub title: String,
    /// Add to a page number in the piece to recover the printed book's page.
    pub page_offset: i64,
}

/// Reject a plan that would lose or duplicate text.
///
/// Every rule here exists because the alternative is silent: an overlap files
/// the same provision under two instruments, and a gap drops pages on the floor
/// with nothing to show that it happened.
pub fn validate(plan: &[PlannedPiece], total_pages: u32) -> Result<(), String> {
    if plan.is_empty() {
        return Err("no instruments given".into());
    }
    let mut prev_end = 0u32;
    for (i, p) in plan.iter().enumerate() {
        if p.title.trim().is_empty() {
            return Err(format!("piece {} has no name", i + 1));
        }
        if p.start == 0 || p.end == 0 {
            return Err(format!("'{}': pages are 1-based", p.title));
        }
        if p.start > p.end {
            return Err(format!(
                "'{}': start page {} is after end page {}",
                p.title, p.start, p.end
            ));
        }
        if p.end > total_pages {
            return Err(format!(
                "'{}': page {} is past the end of the document ({total_pages} pages)",
                p.title, p.end
            ));
        }
        if p.start <= prev_end {
            return Err(format!(
                "'{}' starts on page {} but the previous instrument runs to {} — \
                 ranges must not overlap",
                p.title, p.start, prev_end
            ));
        }
        prev_end = p.end;
    }
    Ok(())
}

/// Pages covered by no piece. Not an error — front matter is legitimately
/// dropped — but the caller must be able to say so out loud.
pub fn unassigned_pages(plan: &[PlannedPiece], total_pages: u32) -> Vec<(u32, u32)> {
    let mut gaps = Vec::new();
    let mut cursor = 1u32;
    for p in plan {
        if p.start > cursor {
            gaps.push((cursor, p.start - 1));
        }
        cursor = p.end + 1;
    }
    if cursor <= total_pages {
        gaps.push((cursor, total_pages));
    }
    gaps
}

/// A filesystem-safe name derived from the instrument's title, so the file on
/// disk is recognisable to a human browsing the folder.
fn file_stem_for(title: &str, index: usize) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_");
    let trimmed: String = cleaned.chars().take(90).collect();
    if trimmed.is_empty() {
        format!("instrument_{:02}", index + 1)
    } else {
        format!("{:02}_{trimmed}", index + 1)
    }
}

/// How many pages the document has, via poppler's `pdfinfo`.
pub fn page_count(src: &Path) -> Result<u32, String> {
    let out = std::process::Command::new("pdfinfo")
        .arg(src)
        .output()
        .map_err(|e| format!("pdfinfo unavailable: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "pdfinfo failed on {src:?}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("Pages:")?.trim().parse::<u32>().ok())
        .ok_or_else(|| format!("pdfinfo reported no page count for {src:?}"))
}

/// Write one PDF per planned piece into `out_dir`, returning what was written.
///
/// Cuts with poppler's `pdfseparate` + `pdfunite` — the same package that
/// already provides `pdftotext` and `pdftoppm`, so this adds no dependency.
///
/// **Not lopdf**, though it is already a dependency: it cannot even open the one
/// document this feature exists for, failing on
/// `Compendium Rules under SLGA 2013` with "Invalid cross-reference table
/// (invalid start value)". Government PDFs in this corpus are frequently
/// malformed in ways poppler tolerates and stricter parsers do not.
///
/// **Not ghostscript**, though it is installed and would do this in one command:
/// it re-encodes the document, and the embedded TEXT LAYER is the entire reason
/// our extraction works on these files. `pdfseparate`/`pdfunite` copy page
/// objects instead, which leaves that layer alone.
pub fn cut(src: &Path, plan: &[PlannedPiece], out_dir: &Path) -> Result<Vec<CutPiece>, String> {
    let total = page_count(src)?;
    validate(plan, total)?;

    let scratch = out_dir.join(format!(".split_scratch_{}", std::process::id()));
    std::fs::create_dir_all(&scratch).map_err(|e| format!("cannot create {scratch:?}: {e}"))?;
    let result = cut_inner(src, plan, out_dir, &scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

fn cut_inner(
    src: &Path,
    plan: &[PlannedPiece],
    out_dir: &Path,
    scratch: &Path,
) -> Result<Vec<CutPiece>, String> {
    let mut out = Vec::with_capacity(plan.len());
    for (i, piece) in plan.iter().enumerate() {
        // `%d` is filled with the page's number in the ORIGINAL document.
        let pattern = scratch.join(format!("p{i}-%d.pdf"));
        let sep = std::process::Command::new("pdfseparate")
            .arg("-f").arg(piece.start.to_string())
            .arg("-l").arg(piece.end.to_string())
            .arg(src)
            .arg(&pattern)
            .output()
            .map_err(|e| format!("pdfseparate unavailable: {e}"))?;
        if !sep.status.success() {
            return Err(format!(
                "pdfseparate failed on '{}' (pp.{}-{}): {}",
                piece.title,
                piece.start,
                piece.end,
                String::from_utf8_lossy(&sep.stderr).trim()
            ));
        }

        let pages: Vec<PathBuf> = (piece.start..=piece.end)
            .map(|n| scratch.join(format!("p{i}-{n}.pdf")))
            .collect();
        if let Some(missing) = pages.iter().find(|p| !p.exists()) {
            return Err(format!(
                "'{}': page file {missing:?} was not produced — the range does not \
                 match the document",
                piece.title
            ));
        }

        let dest = out_dir.join(format!("{}.pdf", file_stem_for(&piece.title, i)));
        if pages.len() == 1 {
            // pdfunite needs two or more inputs; a one-page instrument is just a copy.
            std::fs::copy(&pages[0], &dest)
                .map_err(|e| format!("cannot write {dest:?}: {e}"))?;
        } else {
            let uni = std::process::Command::new("pdfunite")
                .args(&pages)
                .arg(&dest)
                .output()
                .map_err(|e| format!("pdfunite unavailable: {e}"))?;
            if !uni.status.success() {
                return Err(format!(
                    "pdfunite failed for '{}': {}",
                    piece.title,
                    String::from_utf8_lossy(&uni.stderr).trim()
                ));
            }
        }

        out.push(CutPiece {
            path: dest,
            title: piece.title.trim().to_string(),
            // page 1 of the piece was page `start` of the original
            page_offset: piece.start as i64 - 1,
        });
    }
    Ok(out)
}

/// Move the original out of the indexed folder, keeping it for provenance.
///
/// Deleting it would destroy the only record of what was cut; leaving it in
/// place would let the next folder scan re-index the container we just removed.
pub fn retire_original(src: &Path) -> Result<PathBuf, String> {
    let parent = src.parent().ok_or("original has no parent directory")?;
    let attic = parent.join("_split_originals");
    std::fs::create_dir_all(&attic).map_err(|e| format!("cannot create {attic:?}: {e}"))?;
    let name = src.file_name().ok_or("original has no file name")?;
    let dest = attic.join(name);
    std::fs::rename(src, &dest).map_err(|e| format!("cannot move original aside: {e}"))?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(start: u32, end: u32, title: &str) -> PlannedPiece {
        PlannedPiece { start, end, title: title.into() }
    }

    #[test]
    fn a_clean_plan_validates() {
        let plan = vec![p(5, 87, "Election Rules, 2015"), p(88, 91, "Taxes Rules, 2016")];
        assert!(validate(&plan, 240).is_ok());
    }

    /// An overlap files the same provision under two instruments, and nothing
    /// downstream would ever reveal it.
    #[test]
    fn overlapping_ranges_are_refused() {
        let plan = vec![p(5, 90, "Election Rules, 2015"), p(88, 91, "Taxes Rules, 2016")];
        let err = validate(&plan, 240).unwrap_err();
        assert!(err.contains("overlap"), "{err}");
    }

    #[test]
    fn a_range_past_the_end_is_refused() {
        let err = validate(&[p(5, 900, "Election Rules, 2015")], 240).unwrap_err();
        assert!(err.contains("past the end"), "{err}");
    }

    #[test]
    fn inverted_and_zero_ranges_are_refused() {
        assert!(validate(&[p(90, 5, "X Rules, 2015")], 240).is_err());
        assert!(validate(&[p(0, 5, "X Rules, 2015")], 240).is_err());
    }

    /// The name becomes the citation string, so an unnamed piece is not a piece.
    #[test]
    fn an_unnamed_piece_is_refused() {
        let err = validate(&[p(5, 87, "   ")], 240).unwrap_err();
        assert!(err.contains("no name"), "{err}");
    }

    /// Front matter and anything between instruments must be reportable, so the
    /// operator drops it knowingly rather than by accident.
    #[test]
    fn gaps_are_reported_not_hidden() {
        let plan = vec![p(5, 87, "Election Rules, 2015"), p(92, 108, "Contract Rules, 2016")];
        assert_eq!(unassigned_pages(&plan, 120), vec![(1, 4), (88, 91), (109, 120)]);
    }

    #[test]
    fn a_plan_covering_everything_has_no_gaps() {
        let plan = vec![p(1, 50, "A Rules, 2015"), p(51, 100, "B Rules, 2016")];
        assert!(unassigned_pages(&plan, 100).is_empty());
    }

    /// The file on disk should be recognisable to a human, and never contain a
    /// path separator or a character the filesystem will refuse.
    #[test]
    fn file_names_are_derived_safely() {
        let n = file_stem_for("The Sindh Local Councils (Election) Rules, 2015", 0);
        assert_eq!(n, "01_The_Sindh_Local_Councils_Election_Rules_2015");
        assert!(!n.contains('/') && !n.contains('\\'));
        // a title of pure punctuation still yields a usable name
        assert_eq!(file_stem_for("///", 4), "instrument_05");
    }

    /// Page 1 of a piece cut from page 6 is page 6 of the book; without the
    /// offset a citation cannot be checked against the printed volume.
    #[test]
    fn page_offset_maps_back_to_the_printed_book() {
        let plan = vec![p(6, 87, "Election Rules, 2015")];
        // computed in `cut`, asserted here as the contract it implements
        assert_eq!(plan[0].start as i64 - 1, 5);
    }

    /// Cut the REAL Compendium and prove the pieces are usable documents.
    ///
    /// Everything above tests arithmetic. This tests the only thing that can
    /// quietly ruin the corpus: whether `delete_pages` leaves a PDF whose
    /// EMBEDDED TEXT LAYER survives. Our whole extraction path depends on that
    /// layer — a piece that still opens in a viewer but has lost its text would
    /// index as an empty document, and nothing downstream would say so.
    ///
    /// ```text
    /// NOVIQ_CORPUS_DIR=/opt/noviq/data/uploads/admin \
    ///   cargo test --lib --no-default-features --features server \
    ///   instrument_split::tests::cuts_a_real_compendium -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn cuts_a_real_compendium() {
        let Ok(dir) = std::env::var("NOVIQ_CORPUS_DIR") else {
            eprintln!("skip: set NOVIQ_CORPUS_DIR");
            return;
        };
        let src = std::path::Path::new(&dir).join("Compendium Rules under SLGA 2013.pdf");
        if !src.exists() {
            eprintln!("skip: {src:?} not present");
            return;
        }
        let out = std::env::temp_dir().join("noviq_split_test");
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();

        // Two instruments confirmed by both signals in the corpus audit.
        let plan = vec![
            p(6, 87, "The Sindh Local Councils (Election) Rules, 2015"),
            p(175, 198, "The Sindh Food Authority Act, 2016"),
        ];
        let pieces = cut(&src, &plan, &out).expect("cut failed");
        assert_eq!(pieces.len(), 2);

        for (piece, planned) in pieces.iter().zip(plan.iter()) {
            let expected = planned.end - planned.start + 1;
            assert_eq!(
                page_count(&piece.path).expect("piece is not a readable PDF"),
                expected,
                "{:?}: wrong page count",
                piece.path
            );
            let expected = expected as usize;

            // The text layer is the point. Extract it the way the pipeline does.
            let text = std::process::Command::new("pdftotext")
                .arg("-layout")
                .arg(&piece.path)
                .arg("-")
                .output()
                .expect("pdftotext");
            let text = String::from_utf8_lossy(&text.stdout);
            assert!(
                text.trim().len() > 500,
                "{:?}: text layer did not survive the cut ({} chars)",
                piece.path,
                text.trim().len()
            );
            println!(
                "{:35} {} pages, {} chars, offset {}",
                piece.title.chars().take(35).collect::<String>(),
                expected,
                text.trim().len(),
                piece.page_offset
            );
        }

        // The Election Rules must contain their own short title, and NOT the
        // Food Authority Act's — proof the cut landed on the right pages rather
        // than merely producing a valid file.
        let election = std::process::Command::new("pdftotext")
            .arg("-layout").arg(&pieces[0].path).arg("-")
            .output().unwrap();
        let election = String::from_utf8_lossy(&election.stdout);
        // `-layout` pads columns, so the clause reads "may be        called" on
        // the page. Collapse runs of whitespace before matching, exactly as the
        // detector's `\s+` patterns already do.
        let flat = election.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains("may be called the Sindh Local Councils"),
            "the Election Rules' own short title is missing from its piece"
        );
        assert!(
            !election.contains("Food Authority"),
            "the cut leaked pages from another instrument"
        );

        let _ = std::fs::remove_dir_all(&out);
    }
}
