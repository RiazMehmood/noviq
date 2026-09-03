//! Pass B — LLM disambiguation for ambiguous heading candidates.
//!
//! Bounded budget: <=200 calls per document, <=5% of total paragraph count.
//! Each call is a 1-token YES/NO classification. The actual LLM-backed
//! `Classifier` impl lives at the indexer wiring layer (Task 7) where a
//! `ModelSlot` + model path + `HardwareTier` are available.

use super::regex_pass::HeadingCandidate;

const HARD_CALL_CAP: usize = 200;
const PCT_PARAGRAPH_CAP: f32 = 0.05;
const CONFIDENCE_THRESHOLD: f32 = 0.85;

/// Trait for the disambiguation backend so tests can stub the LLM.
pub trait Classifier: Send + Sync {
    /// Returns true if the line is judged a structural heading.
    fn is_heading(&self, line: &str, surrounding_paragraph: &str) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub struct DisambiguationBudget {
    pub calls_made: usize,
    pub max_calls: usize,
}

pub fn disambiguate(
    candidates: Vec<HeadingCandidate>,
    text: &str,
    paragraph_count: usize,
    classifier: &dyn Classifier,
) -> (Vec<HeadingCandidate>, DisambiguationBudget) {
    let max = std::cmp::min(
        HARD_CALL_CAP,
        (paragraph_count as f32 * PCT_PARAGRAPH_CAP).ceil() as usize,
    );
    let mut budget = DisambiguationBudget { calls_made: 0, max_calls: max };

    let mut accepted: Vec<HeadingCandidate> = Vec::with_capacity(candidates.len());
    for mut c in candidates {
        if c.confidence >= CONFIDENCE_THRESHOLD {
            accepted.push(c);
            continue;
        }
        if budget.calls_made >= budget.max_calls {
            // Out of budget — drop the ambiguous candidate (conservative).
            continue;
        }
        let line_end = text[c.offset..].find('\n').map(|i| c.offset + i).unwrap_or(text.len());
        let line = &text[c.offset..line_end];
        let para_end = text[c.offset..].find("\n\n").map(|i| c.offset + i).unwrap_or(text.len());
        let para = &text[c.offset..para_end];
        budget.calls_made += 1;
        if classifier.is_heading(line, para) {
            c.confidence = 0.85; // promoted
            accepted.push(c);
        }
    }
    (accepted, budget)
}

/// Production classifier — calls the local LLM (resolved by the indexer via
/// `model_info_for_tier`) with a strict YES/NO prompt. Failures (LLM error,
/// model not loaded) are treated as NO (drop the ambiguous candidate),
/// preserving the conservative default.
pub struct LlmClassifier<'a> {
    pub model_slot: &'a crate::services::llm::ModelSlot,
    pub model_path: &'a str,
    pub tier: &'a crate::services::hardware_detector::HardwareTier,
}

impl<'a> Classifier for LlmClassifier<'a> {
    fn is_heading(&self, line: &str, paragraph: &str) -> bool {
        let prompt = format!(
            "You are a document structure classifier. Reply with only YES or NO.\n\
             Is the FIRST LINE below a structural heading (a section/chapter/clause label \
             that introduces the paragraph that follows), as opposed to ordinary running text?\n\n\
             FIRST LINE: {line}\n\n\
             FULL PARAGRAPH:\n{paragraph}\n\n\
             Reply YES or NO:"
        );
        let resp = match self.model_slot.infer(self.model_path, &prompt, 4, self.tier) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("heading classifier LLM call failed: {e}");
                return false;
            }
        };
        let t = resp.trim().to_ascii_uppercase();
        t.starts_with("YES") || t == "Y"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::regex_pass::{HeadingCandidate, HeadingType};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct AlwaysYes;
    impl Classifier for AlwaysYes { fn is_heading(&self, _l: &str, _p: &str) -> bool { true } }
    struct AlwaysNo;
    impl Classifier for AlwaysNo { fn is_heading(&self, _l: &str, _p: &str) -> bool { false } }

    /// AtomicUsize gives us Send+Sync without unsafe.
    struct Counter(AtomicUsize);
    impl Classifier for Counter {
        fn is_heading(&self, _l: &str, _p: &str) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed);
            true
        }
    }

    fn cand(offset: usize, label: &str, conf: f32) -> HeadingCandidate {
        HeadingCandidate { offset, label: label.into(), unit_type: HeadingType::Section, confidence: conf }
    }

    #[test]
    fn high_confidence_skips_classifier() {
        let text = "1. Intro\n\nBody";
        let counter = Counter(AtomicUsize::new(0));
        let (out, b) = disambiguate(vec![cand(0, "Section 8", 0.95)], text, 1, &counter);
        assert_eq!(out.len(), 1);
        assert_eq!(b.calls_made, 0);
        assert_eq!(counter.0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn low_confidence_invokes_classifier_and_keeps_on_yes() {
        let text = "1. Introduction\n\nBody";
        let (out, b) = disambiguate(vec![cand(0, "1. Introduction", 0.55)], text, 1, &AlwaysYes);
        assert_eq!(out.len(), 1);
        assert!(out[0].confidence >= 0.85);
        assert_eq!(b.calls_made, 1);
    }

    #[test]
    fn low_confidence_drops_on_no() {
        let text = "Body";
        let (out, b) = disambiguate(vec![cand(0, "1. xyz", 0.55)], text, 1, &AlwaysNo);
        assert!(out.is_empty());
        assert_eq!(b.calls_made, 1);
    }

    #[test]
    fn budget_cap_drops_excess_ambiguous_candidates() {
        let text = "x".repeat(100);
        let candidates: Vec<_> = (0..50).map(|i| cand(i, &format!("h{i}"), 0.5)).collect();
        let counter = Counter(AtomicUsize::new(0));
        // 5% of 1 paragraph = 1 call max.
        let (out, b) = disambiguate(candidates, &text, 1, &counter);
        assert_eq!(b.max_calls, 1);
        assert_eq!(b.calls_made, 1);
        assert_eq!(out.len(), 1);
    }
}
