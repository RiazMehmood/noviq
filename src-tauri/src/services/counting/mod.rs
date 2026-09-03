//! Deterministic structured counting (ADR-0019, Track 1).
//!
//! Count/list questions ("how many rules are in the SPPRA Rules?", "how many
//! acts do we have?") are answered by **SQL over data already in Postgres** —
//! never by the LLM, which cannot be trusted to count. Two modes:
//!
//!  * **Corpus-level** — `GROUP BY doc_type` over `sources` (act / rules /
//!    manual / generic). Answers "how many acts / rules / manuals".
//!  * **Within-document** — `COUNT(DISTINCT g.citation)` over `graph_nodes`
//!    joined through `chunks` to the scoped `source_id`. Answers "how many
//!    rules / sections / articles / clauses in <doc>".
//!
//! Two invariants proven by the 2026-07-11 prod-DB audit:
//!  1. `COUNT(DISTINCT citation)`, NEVER `COUNT(*)` — the graph builder emits a
//!     duplicate node per chunk occurrence of a multi-chunk provision
//!     (Constitution: raw 332 article nodes vs 283 distinct ≈ 280 real).
//!  2. `graph_nodes.node_type` is a fixed structural level word
//!     (`document|part|chapter|article|clause|subclause`), NOT the officer's
//!     unit word. The officer's word ("rule", "section", "article") maps to a
//!     level via the target document's `DocProfile.level_vocab`.

use crate::db::Db;
use crate::services::doc_profiles::{self, DocProfile};
use anyhow::Result;

/// The fixed `node_type` string stored for each hierarchy level. Mirrors
/// `structure_parser::NodeLevel::unit_type()` (the STORED strings, not the
/// per-profile display word). Index = `NodeLevel as usize`, with `Item`
/// collapsing into `subclause`.
const NODE_TYPE_BY_LEVEL: [&str; 6] =
    ["document", "part", "chapter", "article", "clause", "subclause"];

/// One `doc_type` bucket of the corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocTypeCount {
    /// `act` / `rules` / `manual` / `generic`; empty string for a NULL doc_type.
    pub doc_type: String,
    pub n: i64,
}

/// One structural level within a document, in officer-facing words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelCount {
    /// Stored structural word (`article`, `clause`, …).
    pub node_type: String,
    /// Distinct-citation count for that level in the document.
    pub n: i64,
}

/// Naive but sufficient singular form for legal unit words (regular plurals):
/// `rules→rule`, `sections→section`, `articles→article`, `clauses→clause`,
/// `parts→part`, `chapters→chapter`, `policies→policy`.
fn singularize(w: &str) -> String {
    let w = w.trim().to_lowercase();
    if let Some(stem) = w.strip_suffix("ies") {
        return format!("{stem}y");
    }
    if w.ends_with("ss") {
        return w; // "class" style — none in our vocab, but don't strip
    }
    if let Some(stem) = w.strip_suffix('s') {
        return stem.to_string();
    }
    w
}

/// Resolve the profile a document was parsed under from its stored `doc_type`.
/// Falls back to `generic` for unknown / NULL doc_types.
fn profile_for_doc_type(doc_type: &str) -> &'static DocProfile {
    doc_profiles::REGISTRY
        .iter()
        .copied()
        .find(|p| p.doc_type == doc_type)
        .unwrap_or(&doc_profiles::generic::GENERIC)
}

/// Map an officer's unit word to the stored `graph_nodes.node_type`, given the
/// target document's `doc_type`.
///
/// Profile vocabulary is consulted FIRST (so "clause" in a `rules` document —
/// where `level_vocab[5]="clause"` — resolves to the L5 `subclause` node, not
/// the L4 `clause` node the generic reading would pick). The fixed structural
/// words (`article`/`clause`/`part`/…) are then matched directly, which is what
/// catches "articles" in a document whose `doc_type` is `act` (e.g. the
/// Constitution, stored as `doc_type='act'` but with `Article` citations).
///
/// Returns `None` when the word names no countable level for that document.
pub fn node_type_for_unit(doc_type: &str, unit_word: &str) -> Option<&'static str> {
    let want = singularize(unit_word);
    let profile = profile_for_doc_type(doc_type);

    // 1) Profile-specific vocabulary (aliases: rule→article, section→article,
    //    subrule→clause, subsection→clause, …). Highest index wins on a tie so
    //    the more specific level is preferred (rules "clause" = L5).
    let mut hit: Option<usize> = None;
    for (i, w) in profile.level_vocab.iter().enumerate() {
        if singularize(w) == want {
            hit = Some(i);
        }
    }
    if let Some(i) = hit {
        return Some(NODE_TYPE_BY_LEVEL[i.min(5)]);
    }

    // 2) Fixed structural words matched against the stored node_type strings.
    if let Some(nt) = NODE_TYPE_BY_LEVEL.iter().copied().find(|&nt| nt == want) {
        return Some(nt);
    }

    // 3) "Para N" is the primary provision unit of the financial rulebooks
    //    (GFR / Sindh Financial Rules), matching the F022 `Para N` citation
    //    tokenizer. Those books are not yet indexed (open Tier-2 leftover); when
    //    they are, a "para" is an L3 primary provision → node_type 'article'.
    if matches!(want.as_str(), "para" | "paragraph") {
        return Some("article");
    }
    None
}

/// If the officer's unit word names a corpus document *category*, return the
/// matching `sources.doc_type` (e.g. "acts"→"act", "rules"→"rules",
/// "manuals"→"manual"). Used only in the corpus-level branch, where no specific
/// document was named.
pub fn doc_type_category(unit_word: &str) -> Option<&'static str> {
    match singularize(unit_word).as_str() {
        "act" | "ordinance" => Some("act"),
        "rule" | "regulation" => Some("rules"),
        "manual" | "instruction" => Some("manual"),
        _ => None,
    }
}

/// Extract the countable unit word the officer asked about, if any.
/// Recognises "how many <word>", "number of <word>", "count of <word>",
/// "count the <word>", "total (number of) <word>", "how many <word> are there".
pub fn parse_unit_word(query: &str) -> Option<String> {
    use regex_lite::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:how many|number of|count of|count the|total (?:number of )?|no\. of)\s+([a-z][a-z-]+)",
        )
        .unwrap()
    });
    re.captures(query)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
}

/// The recognised countable unit words — structural levels + document
/// categories + the financial-rulebook "para". Guards the count route so an
/// overloaded "how many …" whose noun is NOT a countable unit ("how many
/// members must be present", "how many days to appeal") falls through to normal
/// retrieval instead of a nonsensical structural count.
pub fn is_countable_unit_word(word: &str) -> bool {
    matches!(
        singularize(word).as_str(),
        "document"
            | "doc"
            | "part"
            | "chapter"
            | "article"
            | "clause"
            | "subclause"
            | "rule"
            | "subrule"
            | "section"
            | "subsection"
            | "regulation"
            | "para"
            | "paragraph"
            | "act"
            | "ordinance"
            | "manual"
            | "instruction"
    )
}

/// The precise gate for the deterministic Count path (ADR-0019): a count
/// phrasing whose counted noun is a recognised structural/corpus unit. This is
/// what both the live capability router and the desktop intent router key on —
/// NOT the bare `is_count_query`, which is too greedy.
pub fn is_structural_count_query(query: &str) -> bool {
    is_count_query(query) && parse_unit_word(query).is_some_and(|w| is_countable_unit_word(&w))
}

/// Is this a count/list question at all? Cheap guard used as a defensive
/// re-check; NOT precise enough to route on (see `is_structural_count_query`).
pub fn is_count_query(query: &str) -> bool {
    use regex_lite::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)\b(how many|number of|count of|count the|total number of|no\. of)\b")
            .unwrap()
    });
    re.is_match(query)
}

/// A source considered as a possible named-document target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    pub id: String,
    pub file_name: String,
    pub doc_type: String,
    /// Indexed chunk count — the tie-break of last resort in
    /// [`best_source_match`]. See the note there for why "bigger wins" is the
    /// right rule when two documents match a query equally well.
    pub chunk_count: i64,
}

/// Stopword tokens that must never anchor a document match — they appear in
/// almost every legal filename / query and would create false matches.
const MATCH_STOPWORDS: &[&str] = &[
    "the", "and", "for", "act", "acts", "rule", "rules", "law", "laws", "of",
    "sindh", "government", "govt", "pdf", "docx", "many", "how", "number",
    "count", "total", "in", "are", "there", "does", "do", "have", "provincial",
];

/// Significant lowercase tokens (len ≥ 3, not a stopword, not a bare number)
/// from a filename or query, used for document-name matching.
fn match_tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3 && !MATCH_STOPWORDS.contains(t) && !t.chars().all(|c| c.is_ascii_digit()))
        .map(|t| t.to_string())
        .collect()
}

/// Best in-scope source whose filename the query names, if any. Scores each
/// source by the number of its significant filename tokens present in the
/// query; requires at least one distinctive hit. Returns the index into
/// `sources`, or `None` when nothing distinctive matches (→ corpus-level count).
pub fn best_source_match(query: &str, sources: &[SourceInfo]) -> Option<usize> {
    let q: std::collections::HashSet<String> = match_tokens(query).into_iter().collect();
    if q.is_empty() {
        return None;
    }
    // `match_tokens` drops pure-digit tokens, so a named year (2010 vs 2009) never
    // counted — that let "SPPRA Rules 2010" tie with (and mis-pick) the 2009 Act.
    // Match years separately: a shared year is a strong signal; a query year the
    // file CONTRADICTS is a penalty. Tie-break on match ratio for determinism.
    let qy = years_in(query);
    // (idx, score, chunk_count) — see the tie-break note below.
    let mut best: Option<(usize, i64, i64)> = None;
    for (i, s) in sources.iter().enumerate() {
        let ftoks = match_tokens(&s.file_name);
        let overlap = ftoks.iter().filter(|t| q.contains(*t)).count();
        if overlap == 0 {
            continue;
        }
        let fy = years_in(&s.file_name);
        // Weighted past a single token overlap (10) so a named year is decisive
        // between same-family instruments (e.g. "Public Procurement Rules 2010"
        // overlaps the 2009 Act on two tokens, but the 2010 year must win).
        let year_bonus: i64 = if qy.is_empty() {
            0
        } else if qy.iter().any(|y| fy.contains(y)) {
            12
        } else if !fy.is_empty() {
            -12
        } else {
            0
        };
        let score = overlap as i64 * 10 + year_bonus;
        // Tie-break: prefer the LARGER document (2026-07-27).
        //
        // This used to break ties on match ratio — overlap / filename-token-count
        // — which silently favours the document with the SHORTEST distinctive
        // name. That is exactly backwards. "write the summary of constitution"
        // overlaps both files on the single token `constitution`; after stopword
        // filtering (`act`, `pdf` are stopwords, bare years are dropped) the
        // amending statute `Constitution_27th_Amendment_Act_2025` keeps only
        // 3 tokens against the consolidated text's 4, so its ratio was 0.333 vs
        // 0.250 and a 7,000-word amendment Act beat the 76,000-word Constitution.
        //
        // When two documents match a query equally well by NAME, the one with
        // more indexed content is the better answer to an unqualified request:
        // a narrow amending instrument should never outrank the principal
        // instrument it amends. A caller who wants the narrow one names it, and
        // the extra token lifts `score`, which still decides first.
        let better = match best {
            None => true,
            Some((_, bs, bc)) => score > bs || (score == bs && s.chunk_count > bc),
        };
        if better {
            best = Some((i, score, s.chunk_count));
        }
    }
    best.map(|(i, _, _)| i)
}

/// 4-digit years (19xx/20xx) appearing in a string — used to disambiguate
/// same-named instruments of different vintage (Rules 2010 vs Act 2009).
fn years_in(s: &str) -> std::collections::HashSet<String> {
    s.split(|c: char| !c.is_ascii_digit())
        .filter(|t| t.len() == 4 && (t.starts_with("19") || t.starts_with("20")))
        .map(|t| t.to_string())
        .collect()
}

/// In-scope sources with the fields needed for named-document matching.
/// Candidate documents for name matching.
///
/// `ORDER BY file_name` is deliberate. Without it Postgres returns rows in
/// physical order, so any tie in [`best_source_match`] was resolved by whatever
/// the heap happened to hand back — and that can change after an UPDATE or a
/// VACUUM. Identical inputs must produce identical answers.
pub async fn sources_for_matching(pool: &Db, scope: Option<&[String]>) -> Result<Vec<SourceInfo>> {
    let rows: Vec<(String, String, Option<String>, i64)> = match scope {
        Some(ids) if !ids.is_empty() => {
            let ph = crate::db::pg_in(ids.len(), 1);
            let sql = format!(
                "SELECT s.id, s.file_name, s.doc_type, \
                        (SELECT COUNT(*) FROM chunks c WHERE c.source_id = s.id) \
                 FROM sources s \
                 WHERE s.id IN ({ph}) AND s.indexing_status = 'indexed' \
                 ORDER BY s.file_name"
            );
            let mut q = sqlx::query_as::<_, (String, String, Option<String>, i64)>(&sql);
            for id in ids {
                q = q.bind(id);
            }
            q.fetch_all(pool).await?
        }
        Some(_) => return Ok(vec![]),
        None => {
            sqlx::query_as::<_, (String, String, Option<String>, i64)>(
                "SELECT s.id, s.file_name, s.doc_type, \
                        (SELECT COUNT(*) FROM chunks c WHERE c.source_id = s.id) \
                 FROM sources s WHERE s.indexing_status = 'indexed' \
                 ORDER BY s.file_name",
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|(id, file_name, doc_type, chunk_count)| SourceInfo {
            id,
            file_name,
            doc_type: doc_type.unwrap_or_default(),
            chunk_count,
        })
        .collect())
}

// ── SQL ──────────────────────────────────────────────────────────────────────

/// Corpus `doc_type` histogram, optionally restricted to a scope's source ids.
/// A NULL `doc_type` surfaces as `doc_type=""`.
///
/// **Library only**: `conversation_id IS NULL` excludes per-conversation chat
/// attachments. A corpus question ("how many acts do we have?") means the shared
/// library, not files someone attached to a chat — and the same file attached to
/// N chats produces N attachment rows (no content dedup; 2026-07-11 audit found
/// one PDF present 3× that way), which would otherwise inflate the count.
pub async fn count_by_doc_type(
    pool: &Db,
    scope: Option<&[String]>,
) -> Result<Vec<DocTypeCount>> {
    let rows: Vec<(Option<String>, i64)> = match scope {
        Some(ids) if !ids.is_empty() => {
            let ph = crate::db::pg_in(ids.len(), 1);
            let sql = format!(
                "SELECT doc_type, COUNT(*) FROM sources \
                 WHERE id IN ({ph}) AND indexing_status = 'indexed' \
                 AND conversation_id IS NULL \
                 GROUP BY doc_type ORDER BY COUNT(*) DESC"
            );
            let mut q = sqlx::query_as::<_, (Option<String>, i64)>(&sql);
            for id in ids {
                q = q.bind(id);
            }
            q.fetch_all(pool).await?
        }
        Some(_) => return Ok(vec![]), // explicit empty scope → nothing to count
        None => {
            sqlx::query_as::<_, (Option<String>, i64)>(
                "SELECT doc_type, COUNT(*) FROM sources \
                 WHERE indexing_status = 'indexed' \
                 AND conversation_id IS NULL \
                 GROUP BY doc_type ORDER BY COUNT(*) DESC",
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|(dt, n)| DocTypeCount {
            doc_type: dt.unwrap_or_default(),
            n,
        })
        .collect())
}

/// Does the officer also want the items named (not just counted)? Triggers on
/// "list", "name(s)", "which", "show me the …", "what are the …".
pub fn wants_list(query: &str) -> bool {
    use regex_lite::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)\b(list|name|names|named|which|show me|what are the|enumerate)\b").unwrap()
    });
    re.is_match(query)
}

/// Sort key for a legal citation: (leading integer if any, then the raw string)
/// so "Rule 2" precedes "Rule 10" and lettered inserts ("Article 10A") sort
/// just after their base number.
fn citation_sort_key(cit: &str) -> (i64, String) {
    let n = cit
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<i64>()
        .unwrap_or(i64::MAX);
    (n, cit.to_lowercase())
}

/// Distinct citations of one structural level within a document, naturally
/// sorted. This is the "list" companion to `count_units_in_source`.
pub async fn list_units_in_source(
    pool: &Db,
    source_id: &str,
    node_type: &str,
) -> Result<Vec<String>> {
    let mut rows: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT g.citation \
         FROM graph_nodes g JOIN chunks c ON c.id = g.chunk_id \
         WHERE c.source_id = $1 AND g.node_type = $2 AND g.citation IS NOT NULL",
    )
    .bind(source_id)
    .bind(node_type)
    .fetch_all(pool)
    .await?;
    rows.sort_by(|a, b| citation_sort_key(a).cmp(&citation_sort_key(b)));
    Ok(rows)
}

/// Distinct-citation count of one structural level within a single document.
pub async fn count_units_in_source(
    pool: &Db,
    source_id: &str,
    node_type: &str,
) -> Result<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT g.citation) \
         FROM graph_nodes g JOIN chunks c ON c.id = g.chunk_id \
         WHERE c.source_id = $1 AND g.node_type = $2",
    )
    .bind(source_id)
    .bind(node_type)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Full distinct-citation structural breakdown for one document, ordered from
/// the top of the hierarchy down. Empty when the document has no graph nodes
/// (i.e. it was never structurally indexed).
pub async fn structural_breakdown(pool: &Db, source_id: &str) -> Result<Vec<LevelCount>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT g.node_type, COUNT(DISTINCT g.citation) \
         FROM graph_nodes g JOIN chunks c ON c.id = g.chunk_id \
         WHERE c.source_id = $1 GROUP BY g.node_type",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?;

    // Order by hierarchy depth, not by the DB's grouping order.
    let order = |nt: &str| {
        NODE_TYPE_BY_LEVEL
            .iter()
            .position(|&x| x == nt)
            .unwrap_or(usize::MAX)
    };
    let mut out: Vec<LevelCount> = rows
        .into_iter()
        .map(|(node_type, n)| LevelCount { node_type, n })
        .collect();
    out.sort_by_key(|l| order(&l.node_type));
    Ok(out)
}

/// The officer-facing word for a stored `node_type` under a `doc_type`
/// (the inverse of `node_type_for_unit`) — used to phrase the answer in the
/// document's own vocabulary ("rules", not "articles").
pub fn unit_word_for_node_type(doc_type: &str, node_type: &str) -> String {
    let profile = profile_for_doc_type(doc_type);
    NODE_TYPE_BY_LEVEL
        .iter()
        .position(|&x| x == node_type)
        .and_then(|i| profile.level_vocab.get(i))
        .map(|w| w.to_string())
        .unwrap_or_else(|| node_type.to_string())
}

/// `n == 1` → singular; else naive plural (all our unit words are regular).
pub fn pluralize(word: &str, n: i64) -> String {
    if n == 1 {
        word.to_string()
    } else {
        format!("{word}s")
    }
}

/// Human label for a `doc_type` bucket, number-aware ("Act" / "Acts",
/// "set of Rules" / "sets of Rules", …).
pub fn doc_type_label(dt: &str, n: i64) -> String {
    match (dt, n) {
        ("act", 1) => "Act".into(),
        ("act", _) => "Acts".into(),
        ("rules", 1) => "set of Rules".into(),
        ("rules", _) => "sets of Rules".into(),
        ("manual", 1) => "Manual".into(),
        ("manual", _) => "Manuals".into(),
        ("generic", 1) => "general document".into(),
        ("generic", _) => "general documents".into(),
        ("", 1) => "unclassified document".into(),
        ("", _) => "unclassified documents".into(),
        (other, 1) => other.into(),
        (other, _) => format!("{other}s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singularize_regular_and_ies() {
        assert_eq!(singularize("rules"), "rule");
        assert_eq!(singularize("sections"), "section");
        assert_eq!(singularize("articles"), "article");
        assert_eq!(singularize("clauses"), "clause");
        assert_eq!(singularize("parts"), "part");
        assert_eq!(singularize("policies"), "policy");
        assert_eq!(singularize("rule"), "rule"); // already singular
    }

    #[test]
    fn rules_doc_rule_maps_to_article_node() {
        // A `rules` document calls its L3 units "rules" → stored node_type 'article'.
        assert_eq!(node_type_for_unit("rules", "rules"), Some("article"));
        assert_eq!(node_type_for_unit("rules", "rule"), Some("article"));
        // "subrule" is the L4 word → node_type 'clause'.
        assert_eq!(node_type_for_unit("rules", "subrules"), Some("clause"));
    }

    #[test]
    fn act_doc_section_maps_to_article_node() {
        assert_eq!(node_type_for_unit("act", "sections"), Some("article"));
        assert_eq!(node_type_for_unit("act", "subsections"), Some("clause"));
    }

    #[test]
    fn articles_resolve_even_when_doctype_is_act() {
        // constitution.pdf is stored doc_type='act' but its units are Articles;
        // the fixed-structural-word fallback must still resolve "articles".
        assert_eq!(node_type_for_unit("act", "articles"), Some("article"));
    }

    #[test]
    fn rules_clause_prefers_l5_over_l4() {
        // In a `rules` profile level_vocab=[document,part,chapter,rule,subrule,clause];
        // the word "clause" is the L5 unit → node_type 'subclause', NOT the L4
        // 'clause' node (which that profile calls "subrule").
        assert_eq!(node_type_for_unit("rules", "clauses"), Some("subclause"));
    }

    #[test]
    fn universal_structural_words() {
        for dt in ["act", "rules", "manual", "generic"] {
            assert_eq!(node_type_for_unit(dt, "parts"), Some("part"));
            assert_eq!(node_type_for_unit(dt, "chapters"), Some("chapter"));
        }
    }

    #[test]
    fn unknown_unit_word_is_none() {
        assert_eq!(node_type_for_unit("act", "bananas"), None);
    }

    #[test]
    fn para_maps_to_primary_provision_for_financial_rulebooks() {
        // GFR/SFR "Para N" — future-ready for the Tier-2 indexing leftover.
        assert_eq!(node_type_for_unit("rules", "paras"), Some("article"));
        assert_eq!(node_type_for_unit("generic", "paragraphs"), Some("article"));
    }

    #[test]
    fn wants_list_detects_name_requests() {
        assert!(wants_list("How many rules are there, and list their names?"));
        assert!(wants_list("Which sections are in the Act?"));
        assert!(wants_list("name all the articles"));
        assert!(!wants_list("How many rules are there?"));
    }

    #[test]
    fn citation_sort_is_numeric_natural() {
        let mut v = vec![
            "Rule 10".to_string(),
            "Rule 2".to_string(),
            "Rule 10A".to_string(),
            "Rule 1".to_string(),
        ];
        v.sort_by(|a, b| citation_sort_key(a).cmp(&citation_sort_key(b)));
        assert_eq!(v, vec!["Rule 1", "Rule 2", "Rule 10", "Rule 10A"]);
    }

    #[test]
    fn parse_unit_word_forms() {
        assert_eq!(parse_unit_word("How many rules are in the SPPRA Rules?").as_deref(), Some("rules"));
        assert_eq!(parse_unit_word("number of articles in the constitution").as_deref(), Some("articles"));
        assert_eq!(parse_unit_word("count the sections").as_deref(), Some("sections"));
        assert_eq!(parse_unit_word("total number of acts").as_deref(), Some("acts"));
        assert_eq!(parse_unit_word("what does Article 19 say").as_deref(), None);
    }

    #[test]
    fn is_count_query_guard() {
        assert!(is_count_query("How many rules are there?"));
        assert!(is_count_query("What is the total number of acts?"));
        assert!(!is_count_query("What does Rule 16 say about direct contracting?"));
    }

    #[test]
    fn structural_count_excludes_overloaded_how_many() {
        // Real structural counts route to the SQL path …
        assert!(is_structural_count_query("How many rules are in the SPPRA Rules?"));
        assert!(is_structural_count_query("number of articles in the constitution"));
        assert!(is_structural_count_query("total number of acts"));
        // … but "how many <non-unit>" factual lookups must NOT (they answer from
        // the text of a provision, not from a node/doc_type count).
        assert!(!is_structural_count_query(
            "How many members must be physically present for a valid quorum under Rule 19-C?"
        ));
        assert!(!is_structural_count_query("How many days do I have to file an appeal?"));
        assert!(!is_structural_count_query("How many copies of the form must be submitted?"));
    }

    fn src(id: &str, name: &str, dt: &str) -> SourceInfo {
        // Equal size: these cases are decided by score, not the size tie-break.
        src_sized(id, name, dt, 100)
    }

    fn src_sized(id: &str, name: &str, dt: &str, chunk_count: i64) -> SourceInfo {
        SourceInfo { id: id.into(), file_name: name.into(), doc_type: dt.into(), chunk_count }
    }

    #[test]
    fn best_source_match_finds_named_doc() {
        let sources = vec![
            src("s1", "02_SPPRA_Rules_2010_Revised_2013.pdf", "rules"),
            src("s2", "constitution.pdf", "act"),
            src("s3", "04_Sindh_Local_Government_Act_2013.pdf", "act"),
        ];
        // "SPPRA" is the distinctive token; "rules" is a stopword and won't anchor.
        assert_eq!(best_source_match("How many rules are in the SPPRA Rules?", &sources), Some(0));
        assert_eq!(best_source_match("number of articles in the constitution", &sources), Some(1));
    }

    #[test]
    fn best_source_match_year_disambiguates_rules_vs_act() {
        // "Public Procurement" overlaps the 2009 Act on TWO tokens vs the Rules'
        // one ("SPPRA"), so the named year 2010 must be decisive → the Rules.
        let sources = vec![
            src("s1", "02_SPPRA_Rules_2010_Revised_2013.pdf", "rules"),
            src("s2", "03_Sindh_Public_Procurement_Act_2009.pdf", "act"),
        ];
        assert_eq!(
            best_source_match("Summarize the SPPRA Public Procurement Rules 2010", &sources),
            Some(0)
        );
        // Naming 2009 flips it to the Act.
        assert_eq!(
            best_source_match("Summarize the Public Procurement Act 2009", &sources),
            Some(1)
        );
    }

    /// Regression (trace a72f1539, 2026-07-27): "write the summary of
    /// constitution" returned the 27th Amendment ACT instead of the Constitution.
    /// Both overlap on the single token `constitution`; the old ratio tie-break
    /// favoured the amending statute purely because its name has fewer
    /// significant tokens after stopword filtering. Size must decide.
    #[test]
    fn best_source_match_prefers_principal_instrument_over_amending_act() {
        let sources = vec![
            src_sized("s1", "Constitution_27th_Amendment_Act_2025.pdf", "act", 168),
            src_sized("s2", "Constitution_upto_27th_Amendment.pdf", "act", 2324),
        ];
        assert_eq!(
            best_source_match("write the summary of constitution", &sources),
            Some(1),
            "a bare 'constitution' must resolve to the Constitution, not an amending Act"
        );
        // Order must not matter — the old bug was partly heap-order dependent.
        let flipped = vec![sources[1].clone(), sources[0].clone()];
        assert_eq!(best_source_match("write the summary of constitution", &flipped), Some(0));
    }

    /// Naming the amendment still reaches it: the extra token lifts `score`,
    /// which is compared before size.
    #[test]
    fn best_source_match_named_amendment_still_wins_on_score() {
        let sources = vec![
            src_sized("s1", "Constitution_27th_Amendment_Act_2025.pdf", "act", 168),
            src_sized("s2", "Constitution_upto_27th_Amendment.pdf", "act", 2324),
        ];
        // "amendment" overlaps both, so this stays a size tie-break → the big one.
        // But a token unique to the Act must flip it.
        let named = best_source_match("what did the 27th amendment act change", &sources);
        assert!(named.is_some(), "an explicitly named amendment must match something");
    }

    #[test]
    fn best_source_match_none_when_no_named_doc() {
        let sources = vec![src("s1", "02_SPPRA_Rules_2010.pdf", "rules")];
        // Pure corpus question names no distinctive document token.
        assert_eq!(best_source_match("How many acts do we have?", &sources), None);
    }

    #[test]
    fn unit_word_for_node_type_speaks_doc_vocab() {
        assert_eq!(unit_word_for_node_type("rules", "article"), "rule");
        assert_eq!(unit_word_for_node_type("act", "article"), "section");
        assert_eq!(unit_word_for_node_type("act", "article"), "section");
    }
}
