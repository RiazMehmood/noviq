//! Citation data types. Pure data; no formatting logic (that lives in
//! `formatter.rs`).

use serde::{Deserialize, Serialize};

/// One legal citation, fully resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalCitation {
    /// Full title of the instrument, e.g. "The Right of Access to
    /// Information Act, 2017".
    pub instrument_title: String,

    /// Official short identifier if present, e.g. "Act No. XXXIV of 2017".
    /// `None` for the Constitution and for instruments without numbering.
    pub instrument_id: Option<String>,

    /// Year of enactment, if known.
    pub year: Option<u32>,

    /// Hierarchical path from root toward the cited node, in order.
    /// Empty = citation of the instrument as a whole.
    pub path: Vec<PathSegment>,

    /// If the cited node has been amended, the latest amendment note.
    pub amendment: Option<AmendmentNote>,
}

/// One segment in the path to a cited node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathSegment {
    Preamble,
    Chapter(String),
    Part(String),
    Section(String),
    Article(String),
    SubSection(String),
    Clause(String),
    SubClause(String),
    Proviso(String),
    Explanation(String),
    Illustration(String),
    Schedule(String),
    ScheduleItem(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmendmentNote {
    pub kind: AmendmentKind,
    pub by_instrument: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentKind {
    Inserted,
    Substituted,
    Repealed,
    Omitted,
}

impl AmendmentKind {
    pub fn as_verb_past(self) -> &'static str {
        match self {
            AmendmentKind::Inserted => "inserted",
            AmendmentKind::Substituted => "substituted",
            AmendmentKind::Repealed => "repealed",
            AmendmentKind::Omitted => "omitted",
        }
    }
}

impl LegalCitation {
    /// Officer-grade citation — the long, fully-qualified form used in chat
    /// bubbles, "see also" lists, exports, and copy-as-note.
    pub fn format_officer(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.instrument_title);
        if let Some(id) = &self.instrument_id {
            s.push_str(" (");
            s.push_str(id);
            s.push(')');
        }
        s.push_str(&super::formatter::format_path_segment_list(
            &self.path,
            &self.instrument_title,
        ));
        if let Some(a) = &self.amendment {
            s.push_str(" — ");
            s.push_str(a.kind.as_verb_past());
            s.push_str(" by ");
            s.push_str(&a.by_instrument);
        }
        s
    }

    /// Short / inline form. Used for citation pills in dense UI.
    pub fn format_short(&self) -> String {
        let mut s = super::formatter::abbreviate_title(&self.instrument_title, self.year);
        let path = super::formatter::format_short_path(&self.path);
        if !path.is_empty() {
            s.push(' ');
            s.push_str(&path);
        }
        s
    }
}
