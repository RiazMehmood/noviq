//! Corpus cross-document linker (GraphRAG Phase 3, 2026-07-12).
//!
//! `graph_builder` builds the WITHIN-document overlay (it runs per-doc with a
//! per-doc citation map, so it cannot see other instruments). This pass runs
//! AFTER all documents are ingested and adds the CROSS-document `cross_reference`
//! edges — a subordinate Rules document that names its enabling Act ("under the
//! Sindh Local Government Act, 2013, section 74") gets an edge to that Act's node
//! in the OTHER source. Phase 0's `graph::expand` already follows `cross_reference`,
//! so these cross-doc edges activate for retrieval with no extra wiring.
//!
//! Resolution (architect-approved, NO fuzzy matching):
//!  - (b) title + provision → the specific provision node in the target source;
//!  - (a) title only → the target source's `document` node (fallback).
//! A reference must match a target instrument's distinctive multi-word title
//! (name + type word, e.g. "local government act") — a single generic token
//! never matches. Bare "the Act" references are out of scope for v1 (context, not
//! a title match). Idempotent: cross-doc `cross_reference` edges are deleted and
//! rebuilt on each run.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use regex_lite::Regex;
use uuid::Uuid;

use crate::db::repositories::graph::{self, GraphEdge};
use crate::db::Db;
use crate::services::graph_builder::{
    citation_key, provision_key, provision_ref_re, provision_refs,
};

#[derive(Debug, Default, serde::Serialize)]
pub struct LinkStats {
    pub sources_scanned: usize,
    /// Target instruments that produced a usable title signature.
    pub instruments_targetable: usize,
    pub edges_inserted: usize,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct RebuildStats {
    pub sources: usize,
    pub edges_inserted: usize,
}

/// Regenerate the WITHIN-document overlay (`cross_reference` / `non_obstante`)
/// for the whole corpus from EXISTING chunk text + graph nodes — no re-ingest.
///
/// `graph_builder::build_graph` produces this overlay at ingest, so a corpus
/// indexed before the 2026-07-12 provision-kind generalisation has an
/// Article-only (constitution-only) overlay. This runs the SAME extraction
/// (`provision_refs` → provision-head index) over the persisted content, so every
/// document gains its Rule/Section/… overlay without re-OCR/re-embed. Idempotent
/// (wipes the intra-doc overlay first); leaves `contains` + cross-doc edges intact.
/// Run this, then `link_corpus`, after deploying the generalised builder.
pub async fn rebuild_overlay(pool: &Db) -> Result<RebuildStats> {
    let mut stats = RebuildStats::default();
    graph::delete_intra_doc_overlay_edges(pool).await?;

    let source_ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM sources").fetch_all(pool).await?;
    stats.sources = source_ids.len();

    let mut edges: Vec<GraphEdge> = Vec::new();
    for source_id in &source_ids {
        // provision-head index (article-level nodes) for this source
        let heads: Vec<(String, String)> = sqlx::query_as(
            "SELECT gn.citation, gn.id FROM graph_nodes gn JOIN chunks c ON c.id = gn.chunk_id \
             WHERE c.source_id = $1 AND gn.node_type = 'article' AND gn.citation IS NOT NULL",
        )
        .bind(source_id)
        .fetch_all(pool)
        .await?;
        let mut head_index: HashMap<String, String> = HashMap::new();
        for (citation, id) in heads {
            if let Some(k) = citation_key(&citation) {
                head_index.entry(k).or_insert(id);
            }
        }
        if head_index.is_empty() {
            continue; // no provision heads → nothing to reference
        }

        // every node with its citation + text (chunk content)
        let nodes: Vec<(String, Option<String>, String)> = sqlx::query_as(
            "SELECT gn.id, gn.citation, c.content FROM graph_nodes gn \
             JOIN chunks c ON c.id = gn.chunk_id WHERE c.source_id = $1",
        )
        .bind(source_id)
        .fetch_all(pool)
        .await?;

        for (node_id, citation, content) in nodes {
            // the provision this node belongs to — a unit referencing its own
            // provision is not an edge (mirrors graph_builder's enclosing guard).
            let self_key = citation.as_deref().and_then(citation_key);
            let non_obstante = content.to_lowercase().contains("notwithstanding");
            for key in provision_refs(&content) {
                if Some(&key) == self_key.as_ref() {
                    continue;
                }
                if let Some(target) = head_index.get(&key) {
                    if *target == node_id {
                        continue;
                    }
                    edges.push(overlay_edge(&node_id, target, "cross_reference"));
                    if non_obstante {
                        edges.push(overlay_edge(&node_id, target, "non_obstante"));
                    }
                }
            }
        }
    }

    stats.edges_inserted = edges.len();
    if !edges.is_empty() {
        graph::insert_edges(pool, &edges).await?;
    }
    Ok(stats)
}

fn overlay_edge(from: &str, to: &str, edge_type: &str) -> GraphEdge {
    GraphEdge {
        id: Uuid::new_v4().to_string(),
        from_node: from.to_string(),
        to_node: to.to_string(),
        edge_type: edge_type.to_string(),
        weight: 1.0,
    }
}

/// A resolvable target instrument: how to recognise a reference to it in text,
/// and how to resolve a named provision within it.
struct Target {
    source_id: String,
    doc_node_id: String,
    /// Distinctive title matcher, e.g. `(?i)\b(?:sindh\W+|the\W+)?local\W+government\W+act\b`.
    title_re: Regex,
    /// `provision_key` → provision-head node id, for (b) resolution.
    head_index: HashMap<String, String>,
}

/// Leading geographic / article words dropped from the front of a title so a
/// bare "Local Government Act" still matches "Sindh Local Government Act".
const GEO_PREFIX: &[&str] = &["sindh", "the", "of", "government"];
/// Instrument type words — a title signature must include one (name + type).
const TYPE_WORDS: &[&str] = &["act", "ordinance", "rules", "order", "regulations"];

/// Derive a distinctive title regex from a file name, or `None` when the name is
/// too weak to match precisely (no type word, or < 2 core tokens).
fn title_signature(file_name: &str) -> Option<Regex> {
    let base = file_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(file_name);
    // strip a leading numeric prefix like "04_"
    let base = base.trim_start_matches(|c: char| c.is_ascii_digit() || c == '_');
    let tokens: Vec<String> = base
        .split(|c: char| c == '_' || c.is_whitespace() || c == '-')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        // drop year tokens (1900–2099) — they aren't part of the name core
        .filter(|t| !(t.len() == 4 && t.chars().all(|c| c.is_ascii_digit()) && (t.starts_with("19") || t.starts_with("20"))))
        .collect();
    let type_idx = tokens.iter().position(|t| TYPE_WORDS.contains(&t.as_str()))?;
    // drop leading geographic/article words
    let mut start = 0;
    while start < type_idx && GEO_PREFIX.contains(&tokens[start].as_str()) {
        start += 1;
    }
    let core = &tokens[start..=type_idx];
    if core.len() < 2 {
        return None; // e.g. bare "rules" — too generic to match safely
    }
    // Build `(?i)\b(?:sindh\W+|the\W+)?<core joined by \W+>\b`. `\W+` absorbs the
    // spaces/commas/parens that real citations put between title words.
    let joined = core
        .iter()
        .map(|t| regex_escape(t))
        .collect::<Vec<_>>()
        .join(r"\W+");
    Regex::new(&format!(r"(?i)\b(?:sindh\W+|the\W+)?{joined}\b")).ok()
}

/// Minimal regex escaping for the alphanumeric title tokens we join (they never
/// contain regex metacharacters in practice, but be safe).
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if !c.is_ascii_alphanumeric() {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Build the cross-document `cross_reference` overlay across the whole corpus.
pub async fn link_corpus(pool: &Db) -> Result<LinkStats> {
    let mut stats = LinkStats::default();

    // Idempotent: drop any prior cross-doc cross_reference edges (leaves the
    // within-doc overlay from graph_builder intact).
    graph::delete_cross_doc_cross_reference_edges(pool).await?;

    // All indexed sources (id, file_name).
    let sources: Vec<(String, String)> =
        sqlx::query_as::<_, (String, String)>("SELECT id, file_name FROM sources")
            .fetch_all(pool)
            .await?;
    stats.sources_scanned = sources.len();

    // Build the target registry.
    let mut targets: Vec<Target> = Vec::new();
    for (source_id, file_name) in &sources {
        let Some(title_re) = title_signature(file_name) else { continue };
        let doc_node_id: Option<String> = sqlx::query_scalar(
            "SELECT gn.id FROM graph_nodes gn JOIN chunks c ON c.id = gn.chunk_id \
             WHERE c.source_id = $1 AND gn.node_type = 'document' LIMIT 1",
        )
        .bind(source_id)
        .fetch_optional(pool)
        .await?;
        let Some(doc_node_id) = doc_node_id else { continue };
        let heads: Vec<(String, String)> = sqlx::query_as(
            "SELECT gn.citation, gn.id FROM graph_nodes gn JOIN chunks c ON c.id = gn.chunk_id \
             WHERE c.source_id = $1 AND gn.node_type = 'article' AND gn.citation IS NOT NULL",
        )
        .bind(source_id)
        .fetch_all(pool)
        .await?;
        let mut head_index: HashMap<String, String> = HashMap::new();
        for (citation, id) in heads {
            if let Some(k) = citation_key(&citation) {
                head_index.entry(k).or_insert(id);
            }
        }
        targets.push(Target { source_id: source_id.clone(), doc_node_id, title_re, head_index });
    }
    stats.instruments_targetable = targets.len();

    // Scan each source's chunks for references to OTHER instruments.
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for (source_id, _) in &sources {
        // chunk id, content, and the chunk's graph node (the from-node).
        let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT c.id, c.content, \
                    (SELECT gn.id FROM graph_nodes gn WHERE gn.chunk_id = c.id LIMIT 1) \
             FROM chunks c WHERE c.source_id = $1",
        )
        .bind(source_id)
        .fetch_all(pool)
        .await?;

        for (_chunk_id, content, from_node) in rows {
            let Some(from_node) = from_node else { continue };
            for target in &targets {
                if target.source_id == *source_id {
                    continue; // never a self-document edge
                }
                for m in target.title_re.find_iter(&content) {
                    // (b) resolve a provision named just after the title. Snap the
                    // window end to a UTF-8 char boundary (m.end() already is one).
                    let mut tail_end = (m.end() + 80).min(content.len());
                    while tail_end < content.len() && !content.is_char_boundary(tail_end) {
                        tail_end += 1;
                    }
                    let tail = &content[m.end()..tail_end];
                    let to_node = provision_ref_re()
                        .captures(tail)
                        .and_then(|c| {
                            let key = provision_key(
                                c.get(1).unwrap().as_str(),
                                c.get(2).unwrap().as_str(),
                            );
                            target.head_index.get(&key).cloned()
                        })
                        // (a) fallback: the target's document node.
                        .unwrap_or_else(|| target.doc_node_id.clone());

                    if to_node == from_node {
                        continue;
                    }
                    if seen.insert((from_node.clone(), to_node.clone())) {
                        edges.push(GraphEdge {
                            id: Uuid::new_v4().to_string(),
                            from_node: from_node.clone(),
                            to_node,
                            edge_type: "cross_reference".to_string(),
                            weight: 1.0,
                        });
                    }
                }
            }
        }
    }

    stats.edges_inserted = edges.len();
    if !edges.is_empty() {
        graph::insert_edges(pool, &edges).await?;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_signature_matches_act_titles_with_optional_prefix() {
        let re = title_signature("04_Sindh_Local_Government_Act_2013.pdf").unwrap();
        assert!(re.is_match("made under the Local Government Act, 2013"));
        assert!(re.is_match("the Sindh Local Government Act 2013 provides"));
        // a different Act must not match
        assert!(!re.is_match("under the Public Procurement Act, 2009"));
    }

    #[test]
    fn title_signature_needs_a_type_word_and_two_core_tokens() {
        // no type word (Act/Ordinance/Rules/...) → not targetable
        assert!(title_signature("06_Manual_for_Development_Projects_2019_PC-I.pdf").is_none());
        // "Rules of Business": core after dropping geo/prefix collapses to < 2 → None
        assert!(title_signature("01_Sindh_Rules_of_Business_1986.pdf").is_none());
    }

    #[test]
    fn title_signature_tolerates_punctuation_between_words() {
        let re = title_signature("03_Sindh_Public_Procurement_Act_2009.pdf").unwrap();
        assert!(re.is_match("the Sindh Public Procurement Act, 2009 (Act No. of 2010)"));
    }
}
