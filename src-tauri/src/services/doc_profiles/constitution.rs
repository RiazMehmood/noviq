//! The `constitution` doc-type profile (Feature 003, task T010).
//!
//! Feature 005 leaves `detection_patterns` and `heading_patterns` empty:
//! the Constitution still consumes a pre-built JSON IR via
//! `try_load_constitution`, which bypasses the regex pipeline.

use super::{DocProfile, DetectionPattern, HeadingPattern};

pub const CONSTITUTION: DocProfile = DocProfile {
    doc_type: "constitution",
    level_vocab: [
        "document", "part", "chapter", "article", "clause", "subclause",
    ],
    citation_unit: "Article",
    salient_units: &["Preamble"],
    detection_patterns: &[] as &[DetectionPattern],
    heading_patterns:   &[] as &[HeadingPattern],
    min_confidence:     0.0,
};
