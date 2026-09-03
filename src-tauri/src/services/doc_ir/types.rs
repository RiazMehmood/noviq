//! T007 — Intermediate Representation types for the LLM PDF importer.
//!
//! See specs/006-llm-pdf-importer/data-model.md §1.

use serde::{Deserialize, Serialize};

/// Embedded canonical JSON schema (used by both llguidance and the validator).
pub const SCHEMA_JSON: &str = include_str!("schema.json");

/// Root IR — output of `MistralIrBuilder`, input to the indexer's LLM IR path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocIr {
    /// SHA-256 of source PDF bytes (lowercase hex).
    pub doc_id: String,
    pub doc_type: DocType,
    pub title: String,
    /// Absolute path at extraction time. Filename may change later — the IR is
    /// keyed by `doc_id`.
    pub source_pdf: String,
    pub extracted_at: chrono::DateTime<chrono::Utc>,
    /// `"mistral-7b-instruct-v0.3-q4_k_m"` etc.
    pub llm_model: String,
    pub schema_version: u32,
    pub nodes: Vec<StructureNode>,
    #[serde(default)]
    pub doc_metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DocType {
    Constitution,
    Act,
    Rules,
    Sro,
    Contract,
    Generic,
}

impl DocType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Constitution => "constitution",
            Self::Act => "act",
            Self::Rules => "rules",
            Self::Sro => "sro",
            Self::Contract => "contract",
            Self::Generic => "generic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructureNode {
    /// `n0001`-style, doc-unique.
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub kind: NodeKind,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// Empty for omissions / pure containers.
    #[serde(default)]
    pub text: String,
    /// 1-based page index in the source PDF.
    #[serde(default = "default_page_one")]
    pub page: u32,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub footnote_refs: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Defaults to `Ok` when the model omits the field (free-text mode).
    /// Builder retries + post-validation may downgrade to `LowConfidence`.
    #[serde(default = "default_quality_flag_ok")]
    pub quality_flag: QualityFlag,
    #[serde(default)]
    pub quality_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Part,
    Chapter,
    Section,
    Subsection,
    Clause,
    SubClause,
    Proviso,
    Explanation,
    Schedule,
    Preamble,
    Paragraph,
    Heading,
    Footnote,
}

/// Custom deserializer: any string the LLM emits that isn't one of the known
/// snake_case variants falls back to `Paragraph` so we don't lose the whole
/// chunk on one unrecognised `kind` (e.g. "definition", "annex").
impl<'de> Deserialize<'de> for NodeKind {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(de)?;
        Ok(match s.as_str() {
            "part" => Self::Part,
            "chapter" => Self::Chapter,
            "section" => Self::Section,
            "subsection" => Self::Subsection,
            "clause" => Self::Clause,
            "sub_clause" => Self::SubClause,
            "proviso" => Self::Proviso,
            "explanation" => Self::Explanation,
            "schedule" => Self::Schedule,
            "preamble" => Self::Preamble,
            "paragraph" => Self::Paragraph,
            "heading" => Self::Heading,
            "footnote" => Self::Footnote,
            _ => Self::Paragraph,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QualityFlag {
    Ok,
    LowConfidence,
    UserRepaired,
}

fn default_quality_flag_ok() -> QualityFlag {
    QualityFlag::Ok
}

fn default_page_one() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Annotation {
    #[serde(rename = "type")]
    pub kind: AnnotationKind,
    /// Originally required. Made `#[serde(default)]` (empty when missing) so a
    /// single annotation without `by` doesn't fail the whole chunk's
    /// deserialization — Mistral often uses adjacent fields like
    /// `notification`, `source`, `content` instead. Empty `by` is still flagged
    /// by the validator (`AnnotationMissingBy`, Severity::Error), surfacing as
    /// a LowConfidence repair candidate in US2 rather than a parse failure.
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub by_date: Option<String>,
    #[serde(default)]
    pub footnote_id: Option<String>,
    #[serde(default)]
    pub original: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    Insertion,
    Substitution,
    Omission,
    Renumbering,
    ProvisoAdded,
}

/// Custom deserializer: any string the LLM emits that isn't one of the known
/// snake_case variants falls back to `Insertion` so we don't lose the whole
/// chunk on one unrecognised `type` (e.g. "amendment", "clauseInserted",
/// "annotation"). Mirrors the `NodeKind` strategy. The downstream validator
/// will still see the annotation; semantics-wise "this was modified" is the
/// safest fallback bucket.
impl<'de> Deserialize<'de> for AnnotationKind {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(de)?;
        Ok(match s.to_ascii_lowercase().as_str() {
            "insertion" | "inserted" | "clauseinserted" | "clause_inserted" => Self::Insertion,
            "substitution" | "substituted" | "subs" => Self::Substitution,
            "omission" | "omitted" => Self::Omission,
            "renumbering" | "renumbered" => Self::Renumbering,
            "proviso_added" | "provisoadded" => Self::ProvisoAdded,
            _ => Self::Insertion,
        })
    }
}

/// Envelope wrapper for the on-disk JSON file (separate from schema_version so
/// the file format can evolve independently of the IR shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocIrEnvelope {
    pub envelope_version: u32,
    pub doc_ir: DocIr,
}

impl DocIrEnvelope {
    pub fn wrap(doc_ir: DocIr) -> Self {
        Self {
            envelope_version: 1,
            doc_ir,
        }
    }
}
