//! Autonomous research pass: per-section corpus retrieval → provision cards
//! (FR-004, research R5).
//!
//! Uses ONLY the `retrieval::knn` seam (ADR-0014). Labels are resolved
//! deterministically from the chunk text via the existing decompose helper —
//! numbers are read, never generated. Cards are the writer's entire citation
//! vocabulary.

use super::doc_type::{self, DocType};
use super::skeleton::Section;
use super::state::{OneAskIntent, ProvisionCard};
use crate::services::drafting::retrieval;
use crate::services::intent_router::decompose;
use crate::db::Db;
use std::collections::HashMap;

/// Max cards contributed per skeleton section (R5).
const CARDS_PER_SECTION: usize = 6;
/// KNN over-fetch factor to survive post-filtering (R1).
const OVERFETCH: usize = 4;
/// Relative relevance cutoff: drop hits whose cosine distance exceeds the
/// section's best hit by more than this margin. KNN always returns k nearest
/// no matter how far — without a cutoff, off-topic provisions (observed:
/// a disability-compensation rule in a development-funds summary) become
/// citation cards. Relative (not absolute) so no per-model calibration.
const DISTANCE_MARGIN: f64 = 0.15;
// Single-section ownership (US3/FR-004,005): a provision is owned by exactly
// ONE section — the first citation-eligible section (skeleton order: Legal
// before Financial) to retrieve it — so a card can render in only one section.
// Enforced below by skipping any chunk already held in `by_chunk`. (Replaces
// the old MAX_SECTIONS_PER_CARD=2, under which live drafts repeated the same
// quote in two sections.)

/// A `unit_path` page/paragraph marker segment ("p2", "p370").
fn is_page_marker(s: &str) -> bool {
    let mut ch = s.chars();
    ch.next() == Some('p') && {
        let rest = ch.as_str();
        !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
    }
}

/// Card label: prefer the chunk's structure-parsed `unit_label` (observed
/// live: content prefix said "Section 2010" — a mis-captured year — while
/// `unit_label` held the real "Section 6.20"). Bare sub-clause labels ("(5)",
/// "(i)") carry no authority alone and defeat the prose echo-allowance
/// (observed: "Rule 27(5)" in prose flagged because the card label was
/// "(i)") — qualify them from the `unit_path` hierarchy ("Rule_27/(5)/(i)/p2"
/// → "Rule 27(5)(i)"). Fall back to parsing the content prefix.
pub(crate) fn card_label(
    unit_label: Option<String>,
    unit_path: Option<String>,
    content: &str,
) -> Option<String> {
    let own = unit_label
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty());
    let Some(own) = own else {
        return decompose::provision_label(content);
    };
    if !own.starts_with('(') {
        return Some(own);
    }
    // Bare sub-clause — qualify from the nearest named ancestor in unit_path.
    if let Some(path) = unit_path {
        let mut segs: Vec<String> = path
            .split('/')
            .map(|s| s.replace('_', " ").trim().to_string())
            .filter(|s| !s.is_empty() && !is_page_marker(s))
            .collect();
        let mut bares: Vec<String> = Vec::new();
        while segs.last().is_some_and(|s| s.starts_with('(')) {
            bares.push(segs.pop().expect("checked non-empty"));
        }
        if let Some(anchor) = segs.pop() {
            bares.reverse();
            return Some(format!("{}{}", anchor, bares.join("")));
        }
    }
    // No named ancestor — the content prefix sometimes carries the context
    // the path lacks; otherwise keep the bare label (still truthful).
    decompose::provision_label(content).or(Some(own))
}

/// H diagnosis (2026-06-13): embedding distances cluster in a razor-thin band
/// (0.2475–0.3100 across a whole draft) and irrelevant money-vocabulary
/// neighbours INTERLEAVE with relevant ones — distance cannot carry the
/// relevance decision. The lexical leg discriminates on SUBJECT vocabulary,
/// so money/magnitude words are excluded from the query.
const LEXICAL_STOP: &[&str] = &[
    "want", "get", "rupees", "rupee", "million", "billion", "crore", "lakh",
    "lakhs", "thousand", "hundred", "amount", "the", "and", "for", "with",
];

/// Subject tokens from the officer's ask ONLY (lower-cased, de-duped, ≥3 chars,
/// non-numeric, money/filler stop-words removed). Section role-hints are
/// EXCLUDED from the lexical leg — they are generic governance vocabulary that
/// matches administrative boilerplate everywhere; they stay on the VECTOR leg.
pub(crate) fn lexical_tokens(intent: &OneAskIntent) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut toks: Vec<String> = Vec::new();
    for t in intent.raw.split(|c: char| !c.is_alphanumeric()) {
        let t = t.to_ascii_lowercase();
        if t.len() >= 3
            && !t.chars().all(|c| c.is_ascii_digit())
            && !LEXICAL_STOP.contains(&t.as_str())
            && seen.insert(t.clone())
        {
            toks.push(t);
        }
    }
    toks
}

/// OR-joined officer subject tokens (no corpus weighting). Retained for tests /
/// callers that have no pool; the live path uses [`discriminative_lexical_query`].
pub(crate) fn lexical_query(intent: &OneAskIntent) -> String {
    lexical_tokens(intent).join(" OR ")
}

/// Max number of discriminative terms kept for the lexical leg.
const LEXICAL_MAX_TERMS: usize = 8;

/// Rare-term selection (Fix #3): `ts_rank_cd` has no IDF, so a chunk matching
/// many COMMON query words ("secretary", "department") outscores one matching
/// the few RARE words that actually identify the provision ("contracting",
/// "distributor"). Given each token's corpus document frequency, keep only the
/// discriminating terms: drop tokens that match nothing (`df == 0`) and
/// ubiquitous tokens (`df > 50%` of the corpus), then keep the rarest
/// `max_terms`. Universal — derived from corpus statistics, no hardcoded list.
pub(crate) fn select_discriminative_terms(
    dfs: &[(String, i64)],
    total: i64,
    max_terms: usize,
) -> Vec<String> {
    let ceiling = ((total as f64) * 0.5) as i64;
    let mut kept: Vec<(String, i64)> = dfs
        .iter()
        .filter(|(_, df)| *df > 0 && *df <= ceiling.max(1))
        .cloned()
        .collect();
    if kept.is_empty() {
        // Everything was zero or ubiquitous — fall back to the rarest non-zero
        // terms so the lexical leg still contributes something.
        kept = dfs.iter().filter(|(_, df)| *df > 0).cloned().collect();
    }
    kept.sort_by_key(|(_, df)| *df); // rarest first
    kept.into_iter().take(max_terms).map(|(t, _)| t).collect()
}

/// Build the lexical query weighted toward the officer's rarest (most
/// discriminating) words, using corpus document frequencies. One DF lookup per
/// draft (section-independent). Falls back to the plain token OR on any DB
/// hiccup so retrieval never breaks.
pub(crate) async fn discriminative_lexical_query(pool: &Db, intent: &OneAskIntent) -> String {
    let toks = lexical_tokens(intent);
    if toks.is_empty() {
        return String::new();
    }
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM chunks")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    if total == 0 {
        return toks.join(" OR ");
    }
    let dfs: Vec<(String, i64)> = sqlx::query_as(
        "SELECT t.tok, count(c.id)::bigint \
         FROM unnest($1::text[]) AS t(tok) \
         LEFT JOIN chunks c ON c.tsv @@ plainto_tsquery('english', t.tok) \
         GROUP BY t.tok",
    )
    .bind(&toks)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    if dfs.is_empty() {
        return toks.join(" OR ");
    }
    let kept = select_discriminative_terms(&dfs, total, LEXICAL_MAX_TERMS);
    if kept.is_empty() {
        toks.join(" OR ")
    } else {
        kept.join(" OR ")
    }
}

/// Reciprocal-rank fusion of the two legs. A chunk present in BOTH lists
/// outranks any single-leg chunk of similar rank — vector-only money
/// neighbours fall out when the lexical leg has real subject matches. Ties
/// keep first-seen (vector) order.
const RRF_K: f64 = 60.0;

pub(crate) fn rrf_merge(
    vector: Vec<retrieval::Hit>,
    lexical: Vec<retrieval::Hit>,
    k: usize,
) -> Vec<retrieval::Hit> {
    let mut score: HashMap<String, f64> = HashMap::new();
    let mut by_id: HashMap<String, retrieval::Hit> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for list in [vector, lexical] {
        for (r, h) in list.into_iter().enumerate() {
            *score.entry(h.chunk_id.clone()).or_insert(0.0) += 1.0 / (RRF_K + r as f64 + 1.0);
            if !by_id.contains_key(&h.chunk_id) {
                order.push(h.chunk_id.clone());
                by_id.insert(h.chunk_id.clone(), h);
            }
        }
    }
    order.sort_by(|a, b| score[b].partial_cmp(&score[a]).unwrap_or(std::cmp::Ordering::Equal));
    order.into_iter().take(k).filter_map(|id| by_id.remove(&id)).collect()
}

/// Keep only hits within `DISTANCE_MARGIN` of the section's best hit.
fn relevant_hits(hits: Vec<retrieval::Hit>) -> Vec<retrieval::Hit> {
    let Some(best) = hits.first().map(|h| h.distance) else {
        return hits;
    };
    hits.into_iter()
        .filter(|h| h.distance <= best + DISTANCE_MARGIN)
        .collect()
}

/// Embedder injected as a closure so tests run without fastembed.
pub type EmbedFn<'a> = &'a (dyn Fn(&str) -> Result<Vec<f32>, String> + Send + Sync);

/// Live research commentary for the progress UI (FR-013): fraction of the
/// research pass complete (0.0–1.0) plus a human-readable note.
pub type NoteFn<'a> = &'a (dyn Fn(f32, String) + Send + Sync);

/// Build the retrieval query for one section: the officer's sentence plus the
/// section's (doc-type-resolved) role hints (paraphrase matching is the
/// embedder's job).
pub fn section_query(intent: &OneAskIntent, role_hints: &[&str]) -> String {
    format!("{} {}", intent.raw, role_hints.join(" "))
}

/// Retrieve provision cards for every skeleton section, deduped by chunk
/// across sections (section_ids merged), capped per section.
pub async fn research(
    pool: &Db,
    embed: EmbedFn<'_>,
    intent: &OneAskIntent,
    skeleton: &[Section],
    doc_type: DocType,
    on_note: NoteFn<'_>,
) -> Result<Vec<ProvisionCard>, String> {
    let mut by_chunk: HashMap<String, ProvisionCard> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    // Fix #3: weight the lexical leg toward the officer's rarest (most
    // discriminating) words. Section-independent, so compute once.
    let lex_q = discriminative_lexical_query(pool, intent).await;

    let total = skeleton.len().max(1);
    for (si, section) in skeleton.iter().enumerate() {
        // US2/US3: only citation-eligible sections (cap > 0) own provision
        // cards. Background / Proposal / Prayer draft prose only, so they are
        // not searched and receive no cards.
        if section.max_citations == 0 {
            continue;
        }
        let frac = si as f32 / total as f32;
        on_note(
            frac,
            format!("Searching the indexed documents for “{}” material…", section.title),
        );
        let query = section_query(intent, doc_type::effective_role_hints(doc_type, section));
        let emb = embed(&query)?;
        // Hybrid retrieval (H): vector leg (margin as safety net only) +
        // lexical subject-vocabulary leg, RRF-merged. Both legs fetch deeper
        // than the per-section card budget so fusion has room to reorder.
        let raw_hits =
            retrieval::knn(pool, &emb, CARDS_PER_SECTION * 2, OVERFETCH).await?;
        if let Some(best) = raw_hits.first().map(|h| h.distance) {
            for h in &raw_hits {
                log::info!(
                    "one_ask research [{}] vec dist={:.4} best={:.4} {} chunk={} :: {}",
                    section.title,
                    h.distance,
                    best,
                    if h.distance <= best + DISTANCE_MARGIN { "KEEP" } else { "DROP" },
                    h.chunk_id,
                    h.content.chars().take(70).collect::<String>().replace('\n', " "),
                );
            }
        }
        let vec_hits = relevant_hits(raw_hits);
        let lex_hits =
            retrieval::lexical(pool, &lex_q, CARDS_PER_SECTION * 2).await?;
        for (r, h) in lex_hits.iter().enumerate() {
            log::info!(
                "one_ask research [{}] lex rank={} score={:.4} chunk={} :: {}",
                section.title,
                r + 1,
                h.distance,
                h.chunk_id,
                h.content.chars().take(70).collect::<String>().replace('\n', " "),
            );
        }
        let hits = rrf_merge(vec_hits, lex_hits, CARDS_PER_SECTION);
        for (r, h) in hits.iter().enumerate() {
            log::info!(
                "one_ask research [{}] RRF pick rank={} chunk={}",
                section.title,
                r + 1,
                h.chunk_id,
            );
        }
        if hits.is_empty() {
            on_note(
                frac,
                format!("No new passages for “{}” — moving on.", section.title),
            );
        }
        // Keep at most this section's citation cap, in fusion-rank order. A
        // chunk already owned by an earlier eligible section is skipped
        // (MAX_SECTIONS_PER_CARD = 1: single-section ownership, no cross-section
        // duplication). No anti-starvation force-adopt — a section with no fresh
        // on-topic provision stays uncited (thin over wrong; US6 precision).
        let mut kept = 0usize;
        for hit in hits {
            if kept >= section.max_citations as usize {
                break;
            }
            if by_chunk.contains_key(&hit.chunk_id) {
                continue;
            }
            let (page, unit_label, unit_path): (Option<i64>, Option<String>, Option<String>) =
                sqlx::query_as::<_, (Option<i64>, Option<String>, Option<String>)>(
                    "SELECT page_number, unit_label, unit_path FROM chunks WHERE id = $1",
                )
                .bind(&hit.chunk_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?
                .unwrap_or((None, None, None));
            let label = card_label(unit_label, unit_path, &hit.content);
            on_note(
                frac,
                match &label {
                    Some(l) => format!(
                        "Found {l} in “{}” — noting it as a citation candidate.",
                        hit.file_name
                    ),
                    None => format!("Found a related passage in “{}” — reading it…", hit.file_name),
                },
            );
            order.push(hit.chunk_id.clone());
            by_chunk.insert(
                hit.chunk_id.clone(),
                ProvisionCard {
                    chunk_id: hit.chunk_id,
                    source_id: hit.source_id,
                    source_title: hit.file_name,
                    excerpt: hit.content,
                    label,
                    page,
                    section_ids: vec![section.id],
                },
            );
            kept += 1;
        }
    }

    Ok(order
        .into_iter()
        .filter_map(|id| by_chunk.remove(&id))
        .collect())
}

/// The cards relevant to one section as `(global_1_based_id, card)` pairs —
/// marker ids in prompts are GLOBAL so `verify` can resolve them against the
/// full card vec.
pub fn cards_for_section<'a>(
    cards: &'a [ProvisionCard],
    section: &Section,
) -> Vec<(usize, &'a ProvisionCard)> {
    cards
        .iter()
        .enumerate()
        .filter(|(_, c)| c.section_ids.contains(&section.id))
        .map(|(i, c)| (i + 1, c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, distance: f64) -> retrieval::Hit {
        retrieval::Hit {
            chunk_id: id.into(),
            source_id: "s1".into(),
            file_name: "f.pdf".into(),
            file_path: "/c/f.pdf".into(),
            content: "Rule 1 text".into(),
            distance,
        }
    }

    #[test]
    fn relevant_hits_drops_far_tail_keeps_near_cluster() {
        let hits = vec![
            hit("a", 0.40),
            hit("b", 0.48),
            hit("c", 0.54),
            hit("d", 0.71), // > 0.40 + 0.15 — the off-topic tail
            hit("e", 0.90),
        ];
        let kept: Vec<String> = relevant_hits(hits).into_iter().map(|h| h.chunk_id).collect();
        assert_eq!(kept, vec!["a", "b", "c"]);
    }

    #[test]
    fn lexical_query_keeps_officer_subject_drops_money_and_role_hints() {
        let intent = OneAskIntent {
            raw: "I want to get 4 billion rupees to develop schools".into(),
            figures: vec![],
            action: Some("get".into()),
        };
        let q = lexical_query(&intent);
        assert!(q.contains("develop"), "{q}");
        assert!(q.contains("schools"), "{q}");
        assert!(q.contains(" OR "), "{q}");
        for money in ["billion", "rupees", "want"] {
            assert!(!q.split(" OR ").any(|t| t == money), "money/filler token leaked: {q}");
        }
        // Universal property: generic role-hint governance vocabulary must NOT
        // appear in the lexical leg (it buries discriminative subject nouns —
        // see fn doc). None of these are in `intent.raw`, so they can only have
        // come from role-hints.
        for hint in ["rules", "business", "competent", "authority", "routing", "approval", "procedure"] {
            assert!(!q.split(" OR ").any(|t| t == hint), "role-hint token leaked: {q}");
        }
    }

    #[test]
    fn discriminative_terms_prefers_rare_drops_common_and_zero() {
        // Fix #3: keep the words that identify the provision, drop the noise.
        let dfs = vec![
            ("secretary".to_string(), 1500),
            ("department".to_string(), 2000),
            ("procurement".to_string(), 800),
            ("contracting".to_string(), 50),
            ("distributor".to_string(), 10),
            ("canon".to_string(), 0),         // matches nothing
            ("ubiquitous".to_string(), 5000), // > 50% of 8000
        ];
        let kept = select_discriminative_terms(&dfs, 8000, 8);
        assert!(kept.contains(&"distributor".to_string()), "{kept:?}");
        assert!(kept.contains(&"contracting".to_string()), "{kept:?}");
        assert!(!kept.contains(&"canon".to_string()), "zero-df dropped: {kept:?}");
        assert!(!kept.contains(&"ubiquitous".to_string()), "ubiquitous dropped: {kept:?}");
        assert_eq!(kept[0], "distributor", "rarest first: {kept:?}");
    }

    #[test]
    fn discriminative_terms_caps_and_falls_back() {
        let many: Vec<(String, i64)> = (0..12).map(|i| (format!("t{i}"), (i + 1) as i64)).collect();
        assert_eq!(select_discriminative_terms(&many, 1000, 8).len(), 8, "cap to max_terms");
        // All ubiquitous (> 50% of total) → fall back to rarest non-zero.
        let all_common = vec![("a".to_string(), 900), ("b".to_string(), 950)];
        assert_eq!(
            select_discriminative_terms(&all_common, 1000, 8),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn rrf_merge_prefers_both_leg_hits_over_single_leg() {
        // "b" is rank 2 in BOTH legs; "a" is vector-only rank 1 (the live
        // forest-bond shape). Fusion must put "b" first.
        let vector = vec![hit("a", 0.25), hit("b", 0.26), hit("c", 0.27)];
        let lexical = vec![hit("d", 0.9), hit("b", 0.8), hit("e", 0.7)];
        let merged: Vec<String> = rrf_merge(vector, lexical, 3)
            .into_iter()
            .map(|h| h.chunk_id)
            .collect();
        assert_eq!(merged[0], "b", "{merged:?}");
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn rrf_merge_handles_empty_lexical_leg() {
        let vector = vec![hit("a", 0.25), hit("b", 0.26)];
        let merged: Vec<String> = rrf_merge(vector, vec![], 6)
            .into_iter()
            .map(|h| h.chunk_id)
            .collect();
        assert_eq!(merged, vec!["a", "b"]);
    }

    #[test]
    fn card_label_prefers_unit_label_falls_back_to_content_prefix() {
        assert_eq!(
            card_label(
                Some("Section 6.20".into()),
                Some("Section_8/Section_6.20/p1".into()),
                "Section 2010 Development project proposals which exceed",
            ),
            Some("Section 6.20".into())
        );
        assert_eq!(
            card_label(None, None, "Section 31 Islamic way of life (1) Steps shall be taken"),
            Some("Section 31".into())
        );
        // Blank unit_label → content fallback; nothing parsable → None.
        assert_eq!(
            card_label(Some("  ".into()), None, "Section 31 Islamic way of life"),
            Some("Section 31".into())
        );
        assert_eq!(card_label(Some(String::new()), None, "no label here"), None);
    }

    #[test]
    fn bare_subclause_label_qualified_from_unit_path() {
        // Observed live: label "(i)" / "(5)" carries no authority and defeats
        // the prose echo-allowance — qualify from the path hierarchy.
        assert_eq!(
            card_label(
                Some("(i)".into()),
                Some("Rule_27/(5)/(i)/p2".into()),
                "(5) Rs.400,000,000/ - (Rupees four hundred Million)",
            ),
            Some("Rule 27(5)(i)".into())
        );
        assert_eq!(
            card_label(Some("(e)".into()), Some("Part_II/(1)/(e)/p67".into()), "x"),
            Some("Part II(1)(e)".into())
        );
        // No named ancestor in the path → content-prefix fallback…
        assert_eq!(
            card_label(Some("(1)".into()), Some("(1)/p2".into()), "Rule 8 (1) Steps shall"),
            Some("Rule 8".into())
        );
        // …and when the content has no label either, keep the bare label.
        assert_eq!(
            card_label(Some("(v)".into()), Some("(v)/p12".into()), "plain prose text"),
            Some("(v)".into())
        );
    }

    #[test]
    fn relevant_hits_keeps_all_when_tight_and_handles_empty() {
        let hits = vec![hit("a", 0.50), hit("b", 0.55), hit("c", 0.60)];
        assert_eq!(relevant_hits(hits).len(), 3);
        assert!(relevant_hits(vec![]).is_empty());
    }
}
