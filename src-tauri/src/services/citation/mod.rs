//! Officer-grade legal citation rendering (constitution §XI).
//!
//! Single source of truth for how a citation is rendered anywhere in the
//! app — chat answers, "see also" lists, exports, copy-as-note. Every
//! legal citation in the UI flows through one of the `format_*` methods on
//! [`LegalCitation`]. No other module may produce citation strings.
//!
//! See `specs/007-legal-rag-pakistan/data-model.md` E2/E3 and the 12 golden
//! cases in `src-tauri/tests/citation_golden_test.rs`.

pub mod formatter;
pub mod types;

pub use types::{AmendmentKind, AmendmentNote, LegalCitation, PathSegment};
