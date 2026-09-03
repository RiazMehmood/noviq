pub mod pdf;
pub mod document;
pub mod text;
pub mod audio_video;
pub mod image;
pub mod text_quality;
pub mod table;
pub mod table_sidecar;
pub mod boilerplate;
pub mod chrome;
pub mod front_matter;
pub mod back_matter;
pub mod ocr;
pub mod ocr_sidecar;

use std::path::Path;

/// Supported file formats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileFormat {
    Pdf,
    Docx,
    Xlsx,
    Pptx,
    Txt,
    Markdown,
    Image,
    Audio,
    Video,
    Code,
}

impl FileFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        Some(match ext.as_str() {
            "pdf"  => FileFormat::Pdf,
            "docx" => FileFormat::Docx,
            "xlsx" | "xlsm" => FileFormat::Xlsx,
            "pptx" => FileFormat::Pptx,
            "txt"  => FileFormat::Txt,
            "md" | "markdown" => FileFormat::Markdown,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => FileFormat::Image,
            "mp3" | "wav" | "ogg" | "flac" | "m4a" => FileFormat::Audio,
            "mp4" | "mkv" | "avi" | "mov" | "webm" => FileFormat::Video,
            "rs" | "py" | "js" | "ts" | "go" | "java" | "c" | "cpp" | "h" |
            "cs" | "rb" | "php" | "swift" | "kt" | "scala" | "r" | "sql" |
            "sh" | "bash" | "toml" | "yaml" | "yml" | "json" | "xml" => FileFormat::Code,
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            FileFormat::Pdf      => "pdf",
            FileFormat::Docx     => "docx",
            FileFormat::Xlsx     => "xlsx",
            FileFormat::Pptx     => "pptx",
            FileFormat::Txt      => "txt",
            FileFormat::Markdown => "md",
            FileFormat::Image    => "image",
            FileFormat::Audio    => "audio",
            FileFormat::Video    => "video",
            FileFormat::Code     => "code",
        }
    }
}

/// Output from a file parser: extracted text plus optional page markers.
#[derive(Debug)]
pub struct ParsedDocument {
    /// Full extracted text.
    pub text: String,
    /// Page boundaries: `(page_number, byte_offset_start)`. Empty for non-paged formats.
    pub page_offsets: Vec<(u32, usize)>,
    /// Which tier produced the text: "lopdf" | "pdftotext" | "ocr" | "ocr_skipped" | "native".
    pub extraction_method: &'static str,
    /// Final quality score (0.0–1.0).
    pub quality_score: f32,
    /// US5 (T033): verbatim row lines emitted by the table-reflow engine, used
    /// downstream to tag the resulting chunks `block_class='table_row'`. Empty
    /// for non-paged formats and pages with no reconstructed table.
    pub reconstructed_rows: Vec<String>,
    /// Which extractor produced `reconstructed_rows`: `"pdfplumber"` (Phase 9a
    /// sidecar) or `"schema_anchored"` (the geometric projection engine).
    /// Persisted into `chunks.reconstruction_method` so the sidecar's coverage is
    /// measurable in the DB. Irrelevant when `reconstructed_rows` is empty.
    pub reconstruction_method: &'static str,
    /// Feature 023: the structured tables detected in the document, aligned to the
    /// `⟦TBL:{id}⟧…⟦/TBL:{id}⟧` region markers in `text`. Empty when no sidecar
    /// tables were found / the sidecar path was not used. The structuring stage
    /// consumes these to emit whole-table + per-row units.
    pub tables: Vec<crate::services::parsers::table::ExtractedTable>,
    /// `{provision_number: title}` read from a statute's marginal catchline
    /// column ("Summoning and prorogation of Majlis-e-Shoora" beside Article 54).
    /// The pairing is done in the pdfplumber sidecar, which is the only stage
    /// that has both columns' geometry; everything downstream just looks a number
    /// up. Empty for documents with no catchline column, and for every extractor
    /// other than pdfplumber.
    pub provision_titles: std::collections::HashMap<String, String>,
}

/// Errors the parser layer can return.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("File too large: {size_bytes} bytes (limit: {limit_bytes})")]
    FileTooLarge { size_bytes: u64, limit_bytes: u64 },

    #[error("Capability not available: {message}")]
    CapabilityNotAvailable { message: String },

    #[error("Scanned PDF — vision model required")]
    ScannedPdf,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Other(String),
}

#[cfg(test)]
mod format_contract_tests {
    use super::*;

    /// The exact `sources_format_check` allow-list, read from prod
    /// (`pg_get_constraintdef`) on 2026-08-07. Migration 001 line 35.
    const DB_ALLOWED: &[&str] = &[
        "pdf", "docx", "xlsx", "pptx", "txt", "md", "png", "jpg", "jpeg", "gif",
        "webp", "bmp", "mp3", "mp4", "code", "audio", "video", "image",
    ];

    /// B2 (2026-08-07) — every canonical format string MUST be one the database
    /// accepts. The chat-attachment path used to bind the RAW extension into
    /// `sources.format`, so `.html`, `.markdown`, `.xlsm`, `.wav`, `.mkv` and
    /// every code file crashed on the CHECK and leaked the constraint name to
    /// the user. Adding a `FileFormat` variant whose `as_str()` is not in this
    /// list would reintroduce that crash, so this test fails instead.
    #[test]
    fn every_canonical_format_is_accepted_by_the_db_constraint() {
        let all = [
            FileFormat::Pdf, FileFormat::Docx, FileFormat::Xlsx, FileFormat::Pptx,
            FileFormat::Txt, FileFormat::Markdown, FileFormat::Image,
            FileFormat::Audio, FileFormat::Video, FileFormat::Code,
        ];
        for f in all {
            assert!(
                DB_ALLOWED.contains(&f.as_str()),
                "{:?}.as_str() = {:?} is not in sources_format_check — inserting it \
                 would crash the upload with a raw DB constraint error",
                f, f.as_str(),
            );
        }
    }

    /// The specific extensions that crashed as chat attachments. Each must now
    /// resolve to a canonical value; the raw extension is what used to be stored.
    #[test]
    fn previously_crashing_extensions_map_to_canonical_formats() {
        let cases = [
            ("notes.markdown", "md"), ("book.xlsm", "xlsx"),
            ("call.wav", "audio"), ("call.ogg", "audio"), ("call.flac", "audio"),
            ("clip.mkv", "video"), ("clip.mov", "video"), ("clip.webm", "video"),
            ("main.rs", "code"), ("app.py", "code"), ("cfg.yaml", "code"),
            ("data.json", "code"),
        ];
        for (name, want) in cases {
            let got = FileFormat::from_path(std::path::Path::new(name))
                .unwrap_or_else(|| panic!("{name} no longer parses as a supported type"));
            assert_eq!(got.as_str(), want, "{name} mapped to the wrong format");
            assert!(DB_ALLOWED.contains(&got.as_str()), "{name} still crashes the CHECK");
        }
    }

    /// `.html` is genuinely unsupported — it must be REJECTED cleanly, not
    /// stored as "html" and crashed on. This is the tester's original #36.
    #[test]
    fn unsupported_types_are_rejected_not_stored() {
        for name in ["page.html", "sheet.csv", "archive.zip", "noext"] {
            assert!(
                FileFormat::from_path(std::path::Path::new(name)).is_none(),
                "{name} must be rejected at the upload boundary, not bound into sources.format",
            );
        }
    }
}
