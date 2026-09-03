//! `ConstitutionIR` — the structured intermediate representation emitted by the
//! `constitution_import` dev tool (feature 003, stage S1). See data-model.md §6.
//!
//! Tree: Constitution → Part → (Chapter?) → Article → Clause → Sub-clause.
//! Provisos and explanations are attached to their parent clause/sub-clause,
//! never modelled as standalone nodes (design decision D3) — this prevents the
//! negation-inversion failure (F3) downstream.
//!
//! Amendment model (three layers):
//!  - body markers: `footnote_refs` on Article/Clause/Sub-clause — the footnote
//!    ids of pakistani.org's `N[ ]N` amendment brackets anchored in that node;
//!  - `footnotes`: per-footnote description (what changed, by which instrument);
//!  - `amendments`: the catalogue of enacted Amendment Acts / Orders.

use serde::{Deserialize, Serialize};

/// Root of the IR. Produced once, offline; bundled later as a static asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstitutionIR {
    pub title: String,
    pub source_url: String,
    /// ISO-8601 date the source pages were retrieved.
    pub retrieved: String,
    /// Human label for the amendment cut-off of the source.
    pub amendments_through: String,
    /// Preamble text.
    pub preamble: Option<String>,
    /// Catalogue of enacted Amendment Acts / Orders (Layer 3).
    pub amendments: Vec<Amendment>,
    /// Footnote descriptions — footnote id → what it changed (Layer 2).
    pub footnotes: Vec<Footnote>,
    pub parts: Vec<Part>,
}

/// A catalogue entry — an enacted Amendment Act or constitutional Order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Amendment {
    /// Ordinal number, e.g. `18` for the Eighteenth Amendment.
    pub number: u32,
    /// `amendment` or `order`.
    pub kind: String,
    pub year: Option<u32>,
    /// Title as printed, e.g. `"Eighteenth Amendment (2010)"`.
    pub title: String,
    /// One-line description from the source catalogue.
    pub summary: String,
}

/// A footnote description from a Part page's `references` block — the bridge
/// between a body `footnote_refs` id and the instrument that made the change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Footnote {
    /// Footnote id as printed in the body, e.g. `"667"`.
    pub id: String,
    /// `substituted` / `inserted` / `added` / `omitted` / `renumbered`
    /// (amendment actions) · `note` (editorial reference) · `other`.
    pub change: String,
    /// Instrument that made the change, e.g.
    /// `"Constitution (First Amendment) Act, 1974 (33 of 1974)"`.
    pub instrument: String,
    /// Effective date if the description states one.
    pub effective_date: Option<String>,
    /// The text as it stood *before* this change, if the description quotes it.
    pub previous_text: Option<String>,
    /// Full raw description text — always kept (the structured fields above are
    /// best-effort, as the source phrasing varies).
    pub raw: String,
}

/// A Part of the Constitution (Part I … Part XII).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    /// Roman numeral, e.g. `"II"`.
    pub number: String,
    pub title: String,
    /// pakistani.org page id of this Part's standalone page, if it has one.
    pub source_page: Option<String>,
    /// Articles directly under the Part — chapter-less Parts, plus the Part II
    /// "Definition of the State" article that precedes Chapter 1.
    pub articles: Vec<Article>,
    pub chapters: Vec<Chapter>,
}

/// A Chapter within a Part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    /// Chapter label as printed, e.g. `"1"`, `"1A"`.
    pub number: String,
    pub title: String,
    /// pakistani.org page id this chapter was parsed from.
    pub source_page: String,
    pub articles: Vec<Article>,
}

/// A single Article.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    /// Article label as printed, e.g. `"10"`, `"10A"`, `"19A"` (trailing `.` stripped).
    pub number: String,
    /// Marginal note (the bold heading). Empty only when the source omits it.
    pub marginal_note: String,
    /// Article-level chapeau text — present for flat articles, and for articles
    /// that carry intro text before their enumerated clauses. `None` otherwise.
    pub text: Option<String>,
    pub clauses: Vec<Clause>,
    /// Provisos that hang off the Article itself — flat articles whose proviso
    /// has no numbered clause to attach to (e.g. Article 18).
    pub provisos: Vec<Proviso>,
    /// Explanations that hang off the Article itself — same rationale.
    pub explanations: Vec<Explanation>,
    /// Footnote ids of amendment markers anchored in this Article — including
    /// article-level wrappers (`N[ … ]N` around the whole Article).
    pub footnote_refs: Vec<String>,
}

/// A numbered clause `(1)`, `(2)`, … of an Article.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clause {
    /// Clause id as printed, e.g. `"(1)"`, `"(2A)"`.
    pub id: String,
    /// Clause chapeau text — excludes nested sub-clauses, provisos, explanations.
    pub text: String,
    pub subclauses: Vec<SubClause>,
    pub provisos: Vec<Proviso>,
    pub explanations: Vec<Explanation>,
    /// Footnote ids of amendment markers anchored directly in this clause (not
    /// in its sub-clauses). An id whose `footnotes` entry is `change: omitted`
    /// marks content the amendment deleted at this position.
    pub footnote_refs: Vec<String>,
}

/// A lettered/roman sub-clause `(a)`, `(b)`, `(i)`, … — the v1 retrieval leaf.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubClause {
    /// Sub-clause id as printed, e.g. `"(a)"`, `"(i)"`.
    pub id: String,
    pub text: String,
    pub provisos: Vec<Proviso>,
    pub explanations: Vec<Explanation>,
    /// Footnote ids of amendment markers anchored directly in this sub-clause.
    pub footnote_refs: Vec<String>,
}

/// A "Provided that …" proviso, attached in-leaf to its parent clause/sub-clause.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proviso {
    pub text: String,
}

/// An "Explanation" block, attached in-leaf to its parent clause/sub-clause.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    /// Explanation label, e.g. `"I"`, `"II"`, or `""` when unnumbered.
    pub label: String,
    /// Full explanation text, including the `"Explanation-I:"` lead-in.
    pub text: String,
}
