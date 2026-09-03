//! Cross-round assembly — dedup + budget-aware packing (F025, data-model:
//! PassageSet; FR-013).
//!
//! Unions round-1 ∪ round-2 passages, dedups by `chunk_id` (first-seen wins), then
//! orders passages that resolve previously-pending sub-questions (round 2) FIRST so
//! the evidence the second round paid for is never truncated away, and truncates to
//! a char budget / max count.

use super::{Passage, PassageSet};

/// Char budget for the packed passage set (≈ the retrieved-law token budget).
const PACK_CHAR_BUDGET: usize = 16_000;
/// Hard cap on packed passages regardless of budget.
const MAX_PASSAGES: usize = 30;

/// One retrieval hit as returned by `query_router::retrieve_for_query`.
type Hit = (String, String, String, Option<i64>, String);

impl PassageSet {
    /// Fold in one retrieval round's hits, tagging round + originating sub-question.
    pub(crate) fn add_round(&mut self, hits: Vec<Hit>, round: u8, for_sub_question: u8) {
        for (chunk_id, source_id, content, page, label) in hits {
            self.passages.push(Passage { chunk_id, source_id, content, page, label, round, for_sub_question });
        }
    }

    /// Dedup by `chunk_id`, first-seen wins (a round-2 re-find of a round-1 chunk
    /// keeps the earlier entry, so it is not double-counted).
    pub(crate) fn dedup(&mut self) {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        self.passages.retain(|p| seen.insert(p.chunk_id.clone()));
    }

    /// Instrument-affinity ordering (2026-07-18). `Rule 4` collides across at
    /// least five instruments in this corpus; trace `bd0d16e1` asked for Rule 4
    /// "from sindh civil servants efficiency and disciplane rules 1973" and was
    /// answered out of the *Sindh Food Authority Act* because the Compendium
    /// supplied 7 of the 22 assembled passages and the E&D Rules only 5.
    ///
    /// Ordering only — nothing is dropped. `packed()` still truncates to the
    /// budget, so this decides *which* instrument survives truncation and which
    /// passages the LLM sees first. No-op when the query names no statute.
    ///
    /// Must run BEFORE `packed()`: `packed()`'s round sort is stable, so the
    /// instrument order is preserved inside each round group.
    pub(crate) fn prioritize_instrument(&mut self, query: &str, boost: f32) {
        if boost <= 1.0 || self.passages.len() < 2 {
            return;
        }
        let Some((by_file, _top)) = crate::services::query_router::instrument_ranks(
            query,
            self.passages.iter().map(|p| p.label.as_str()),
        ) else {
            return;
        };
        // Stable sort: matching instrument first, existing order kept within a group.
        self.passages.sort_by_key(|p| {
            std::cmp::Reverse(by_file.get(&p.label).copied().unwrap_or(0))
        });
    }

    /// Budget-packed passages, round-2 (gap-filling) FIRST then round-1 in order,
    /// truncated to the char budget / max count (FR-013). Stable within each round.
    pub(crate) fn packed(&self) -> Vec<&Passage> {
        let mut ordered: Vec<&Passage> = self.passages.iter().collect();
        // Stable partition: round ≥ 2 before round 1 (keeps relative order otherwise).
        ordered.sort_by_key(|p| u8::from(p.round < 2));

        let mut out: Vec<&Passage> = Vec::new();
        let mut used = 0usize;
        for p in ordered {
            if out.len() >= MAX_PASSAGES {
                break;
            }
            let c = p.content.len();
            if !out.is_empty() && used + c > PACK_CHAR_BUDGET {
                break;
            }
            used += c;
            out.push(p);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str) -> Hit {
        (id.into(), "src".into(), format!("content-{id}"), None, "lbl".into())
    }

    #[test]
    fn dedup_keeps_first_seen_and_packs_round2_first() {
        let mut ps = PassageSet::default();
        ps.add_round(vec![hit("a"), hit("b")], 1, 1);
        ps.add_round(vec![hit("b"), hit("c")], 2, 2); // "b" is a round-2 re-find
        ps.dedup();
        assert_eq!(ps.passages.len(), 3); // a, b, c

        let packed = ps.packed();
        assert_eq!(packed.len(), 3);
        // "c" is the only pure round-2 passage → packed first.
        assert_eq!(packed[0].chunk_id, "c");
        // "b" kept its first-seen (round 1) tag after dedup.
        let b = ps.passages.iter().find(|p| p.chunk_id == "b").unwrap();
        assert_eq!(b.round, 1);
    }

    /// Reproduces the passage mix from trace `bd0d16e1`: the Compendium supplied
    /// more passages than the E&D Rules, so the Food Authority's "Rule 4" won and
    /// the answer described Food Department designations as disciplinary law.
    #[test]
    fn instrument_priority_puts_the_named_statute_first() {
        let named = |id: &str, file: &str| Passage {
            chunk_id: id.into(), source_id: "s".into(), content: format!("c-{id}"),
            page: None, label: file.into(), round: 1, for_sub_question: 1,
        };
        let mut ps = PassageSet::default();
        for i in 0..7 { ps.passages.push(named(&format!("comp{i}"), "05_Compendium_Rules_under_SLGA_2013.pdf")); }
        for i in 0..5 { ps.passages.push(named(&format!("dfp{i}"), "09_Sindh_Delegation_of_Financial_Powers_2019.pdf")); }
        for i in 0..5 { ps.passages.push(named(&format!("ed{i}"), "Sindh_Civil_Servants_Efficiency_and_Discipline_Rules_1973.pdf")); }

        let before = ps.passages.len();
        // The real query, misspelling included.
        ps.prioritize_instrument(
            "Rule 4(1) sub rule 3 from sindh civil servants efficiency and disciplane rules 1973",
            2.0,
        );
        assert_eq!(ps.passages.len(), before, "ordering only — nothing dropped");
        assert!(
            ps.passages[0].label.starts_with("Sindh_Civil_Servants_Efficiency"),
            "E&D must lead, got {}", ps.passages[0].label,
        );
        // And the ordering survives packing (packed's round sort is stable).
        assert!(ps.packed()[0].label.starts_with("Sindh_Civil_Servants_Efficiency"));
    }

    #[test]
    fn instrument_priority_noops_when_no_statute_named() {
        let mut ps = PassageSet::default();
        ps.add_round(vec![hit("a"), hit("b"), hit("c")], 1, 1);
        let before: Vec<String> = ps.passages.iter().map(|p| p.chunk_id.clone()).collect();
        ps.prioritize_instrument("request for quotations method under rule 16", 2.0);
        let after: Vec<String> = ps.passages.iter().map(|p| p.chunk_id.clone()).collect();
        assert_eq!(before, after, "no statute named → order untouched");
    }

    #[test]
    fn char_budget_truncates_but_keeps_at_least_one() {
        let mut ps = PassageSet::default();
        let big = "x".repeat(PACK_CHAR_BUDGET + 100);
        ps.passages.push(Passage {
            chunk_id: "big".into(), source_id: "s".into(), content: big, page: None, label: "l".into(), round: 1, for_sub_question: 1,
        });
        ps.passages.push(Passage {
            chunk_id: "small".into(), source_id: "s".into(), content: "y".into(), page: None, label: "l".into(), round: 1, for_sub_question: 1,
        });
        let packed = ps.packed();
        assert_eq!(packed.len(), 1); // first (oversized) kept; second exceeds budget
    }
}
