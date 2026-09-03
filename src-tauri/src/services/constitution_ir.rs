//! Deserialization types for `constitution_ir.json` — the output of the S1
//! importer (feature 003). A read-only mirror of
//! `tools/constitution_import/src/ir.rs`: that importer is a standalone crate
//! (deliberately not a workspace member — Constitution Principle I), so the
//! shapes are duplicated here rather than shared.
//!
//! Only the fields the app needs are declared; serde ignores the rest
//! (`source_url`, `retrieved`, `source_page`, …).

use serde::Deserialize;
use std::path::Path;

/// Root of the intermediate representation.
#[derive(Debug, Clone, Deserialize)]
pub struct ConstitutionIr {
    pub title: String,
    #[serde(default)]
    pub preamble: Option<String>,
    #[serde(default)]
    pub amendments: Vec<Amendment>,
    #[serde(default)]
    pub footnotes: Vec<Footnote>,
    pub parts: Vec<Part>,
}

/// A catalogue entry — an enacted Amendment Act or constitutional Order.
#[derive(Debug, Clone, Deserialize)]
pub struct Amendment {
    pub number: u32,
    pub kind: String,
    pub year: Option<u32>,
    pub title: String,
    pub summary: String,
}

/// A footnote description — footnote id → what changed, by which instrument.
#[derive(Debug, Clone, Deserialize)]
pub struct Footnote {
    pub id: String,
    pub change: String,
    pub instrument: String,
    pub effective_date: Option<String>,
    pub previous_text: Option<String>,
    pub raw: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Part {
    pub number: String,
    pub title: String,
    #[serde(default)]
    pub articles: Vec<Article>,
    #[serde(default)]
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Chapter {
    pub number: String,
    pub title: String,
    #[serde(default)]
    pub articles: Vec<Article>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Article {
    pub number: String,
    pub marginal_note: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub clauses: Vec<Clause>,
    #[serde(default)]
    pub provisos: Vec<Proviso>,
    #[serde(default)]
    pub explanations: Vec<Explanation>,
    #[serde(default)]
    pub footnote_refs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Clause {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub subclauses: Vec<SubClause>,
    #[serde(default)]
    pub provisos: Vec<Proviso>,
    #[serde(default)]
    pub explanations: Vec<Explanation>,
    #[serde(default)]
    pub footnote_refs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubClause {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub provisos: Vec<Proviso>,
    #[serde(default)]
    pub explanations: Vec<Explanation>,
    #[serde(default)]
    pub footnote_refs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Proviso {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Explanation {
    /// Label such as `"I"`, `"II"`; empty when unnumbered.
    pub label: String,
    pub text: String,
}

/// Load and deserialize a `constitution_ir.json` file.
pub fn load(path: &Path) -> std::io::Result<ConstitutionIr> {
    let raw = std::fs::read_to_string(path)?;
    serde_json::from_str(&raw)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
