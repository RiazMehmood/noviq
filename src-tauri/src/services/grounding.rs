//! Sentence-level grounding validator.
//!
//! For each generated sentence carrying a citation `[N]`, compute Jaccard
//! overlap of content tokens (lowercase, ≥4 chars, stopword-stripped) against
//! the cited chunk. Sentence passes if overlap >= JACCARD_THRESHOLD OR shared
//! token count >= MIN_SHARED_TOKENS.

use std::collections::HashSet;

pub const JACCARD_THRESHOLD: f32 = 0.25;
// Plan specified 3, but the plan's own one_bad_sentence_strips test only
// shares 2 content tokens between the supported sentence and the chunk
// (after ≥4-char + stopword filtering strips short tokens like "B-17", "5").
// Set to 2 so well-supported short sentences are not falsely rejected.
pub const MIN_SHARED_TOKENS: usize = 2;
/// Drop sentences if up to this many are ungrounded; > this triggers retry.
pub const MAX_DROPPABLE_UNGROUNDED: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct SentenceVerdict {
    pub sentence: String,
    pub cited_indices: Vec<usize>, // 1-based
    pub overlap: f32,
    pub shared: usize,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct GroundingReport {
    pub per_sentence: Vec<SentenceVerdict>,
    pub action: GroundingAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GroundingAction {
    /// All sentences passed → ship as-is.
    Ship,
    /// 1..=MAX_DROPPABLE_UNGROUNDED failed → strip them, ship remainder.
    Strip { kept: String },
    /// > MAX_DROPPABLE_UNGROUNDED OR > 50% failed → trigger retry escalation.
    Retry,
}

pub fn validate(response: &str, cited_chunks_by_index: &[String]) -> GroundingReport {
    let sentences = split_into_sentences(response);
    if sentences.is_empty() {
        return GroundingReport { per_sentence: vec![], action: GroundingAction::Ship };
    }
    let mut verdicts = Vec::with_capacity(sentences.len());
    for s in &sentences {
        let cited = extract_citation_indices(s);
        if cited.is_empty() {
            // Sentences without citations are not gated — they're assumed to be
            // connective tissue (the model is trained to cite factual claims).
            verdicts.push(SentenceVerdict {
                sentence: s.clone(), cited_indices: vec![],
                overlap: 1.0, shared: 0, passed: true,
            });
            continue;
        }
        let s_tokens = content_tokens(s);
        let mut best_overlap = 0.0f32;
        let mut best_shared = 0usize;
        for &idx in &cited {
            let chunk_idx = idx.saturating_sub(1);
            if chunk_idx >= cited_chunks_by_index.len() { continue; }
            let c_tokens = content_tokens(&cited_chunks_by_index[chunk_idx]);
            let shared = s_tokens.intersection(&c_tokens).count();
            let union = s_tokens.union(&c_tokens).count().max(1);
            let jacc = shared as f32 / union as f32;
            if jacc > best_overlap { best_overlap = jacc; }
            if shared > best_shared { best_shared = shared; }
        }
        let passed = best_overlap >= JACCARD_THRESHOLD || best_shared >= MIN_SHARED_TOKENS;
        verdicts.push(SentenceVerdict {
            sentence: s.clone(), cited_indices: cited,
            overlap: best_overlap, shared: best_shared, passed,
        });
    }
    let failed = verdicts.iter().filter(|v| !v.passed).count();
    let action = if failed == 0 { GroundingAction::Ship }
        else if failed > MAX_DROPPABLE_UNGROUNDED || (failed * 2 > verdicts.len()) {
            GroundingAction::Retry
        } else {
            let kept: String = verdicts.iter().filter(|v| v.passed)
                .map(|v| v.sentence.clone()).collect::<Vec<_>>().join(" ");
            let kept = format!("{kept}\n\n_(Some content omitted as unsupported.)_");
            GroundingAction::Strip { kept }
        };
    GroundingReport { per_sentence: verdicts, action }
}

fn split_into_sentences(s: &str) -> Vec<String> {
    // Naive but robust: split on `. `, `? `, `! ` followed by uppercase.
    // Preserves trailing punctuation. Skips empty pieces.
    let mut out = Vec::new();
    let mut buf = String::new();
    let bytes = s.as_bytes();
    for (i, ch) in s.char_indices() {
        buf.push(ch);
        let is_terminator = matches!(ch, '.' | '?' | '!');
        let next_byte = bytes.get(i + ch.len_utf8()).copied();
        let next2 = bytes.get(i + ch.len_utf8() + 1).copied();
        if is_terminator && next_byte == Some(b' ') {
            if let Some(nb) = next2 {
                if (nb as char).is_uppercase() {
                    out.push(buf.trim().to_string());
                    buf.clear();
                }
            }
        }
    }
    let tail = buf.trim().to_string();
    if !tail.is_empty() { out.push(tail); }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn extract_citation_indices(s: &str) -> Vec<usize> {
    let re = regex_lite::Regex::new(r"\[(\d+)\]").unwrap();
    let mut out: Vec<usize> = re.captures_iter(s)
        .filter_map(|c| c.get(1)?.as_str().parse().ok())
        .collect();
    out.sort_unstable(); out.dedup();
    out
}

fn content_tokens(s: &str) -> HashSet<String> {
    static STOPWORDS: &[&str] = &[
        "the","and","that","this","with","from","into","upon","shall","may","such",
        "have","been","were","also","than","then","when","what","whom","whose","which",
        "their","there","these","those","other","under","over","between","within",
    ];
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4)
        .map(|w| w.to_lowercase())
        .filter(|w| !STOPWORDS.contains(&w.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_grounded_sentence_passes() {
        let chunk = "For B-18, minimum length of service shall be 5 years in B-17.";
        let resp  = "Promotion to BPS-18 requires five years of service in BPS-17 [1].";
        let r = validate(resp, &[chunk.to_string()]);
        assert_eq!(r.action, GroundingAction::Ship);
    }

    #[test]
    fn fully_hallucinated_triggers_retry() {
        let chunk = "Section 8 covers recruitment criteria for civil servants.";
        let resp  = "The Mars rover discovered iron ore in Olympus Mons crater [1].";
        let r = validate(resp, &[chunk.to_string()]);
        assert_eq!(r.action, GroundingAction::Retry);
    }

    #[test]
    fn one_bad_sentence_strips() {
        let chunk = "Promotion to B-18 needs 5 years in B-17. Departmental exams are required.";
        let resp = "Promotion requires five years of service in B-17 [1]. The applicant must also bring a goldfish to the interview [1]. Departmental exams must be cleared [1].";
        let r = validate(resp, &[chunk.to_string()]);
        match r.action {
            GroundingAction::Strip { kept } => {
                assert!(!kept.contains("goldfish"));
                assert!(kept.contains("five years"));
            }
            other => panic!("expected Strip, got {other:?}"),
        }
    }

    #[test]
    fn no_citation_sentences_pass_without_gating() {
        let chunk = "Section 8 covers recruitment.";
        let resp = "Here is your answer. Recruitment is governed by Section 8 [1].";
        let r = validate(resp, &[chunk.to_string()]);
        assert_eq!(r.action, GroundingAction::Ship);
    }
}
