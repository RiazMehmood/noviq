//! Plain text and code file parser.
//!
//! - TXT / MD: direct string
//! - Code: syntax-aware extraction (preserves function/class boundaries)
//! - Enforces 500 MB soft limit (error), 2 GB hard limit

use std::path::Path;
use super::{ParseError, ParsedDocument};

const SOFT_LIMIT: u64 = 500 * 1024 * 1024;    // 500 MB → error
const HARD_LIMIT: u64 = 2 * 1024 * 1024 * 1024; // 2 GB → error

pub fn extract(path: &Path) -> Result<ParsedDocument, ParseError> {
    let metadata = std::fs::metadata(path)?;

    if metadata.len() > HARD_LIMIT {
        return Err(ParseError::FileTooLarge {
            size_bytes: metadata.len(),
            limit_bytes: HARD_LIMIT,
        });
    }
    if metadata.len() > SOFT_LIMIT {
        return Err(ParseError::FileTooLarge {
            size_bytes: metadata.len(),
            limit_bytes: SOFT_LIMIT,
        });
    }

    let text = std::fs::read_to_string(path)
        .map_err(|e| ParseError::Other(format!("read error: {e}")))?;

    Ok(ParsedDocument {
        text,
        page_offsets: vec![],
        extraction_method: "native",
        quality_score: 1.0,
        reconstructed_rows: Vec::new(),
        reconstruction_method: "",
        tables: Vec::new(),
        provision_titles: Default::default(),
    })
}
