//! LLM call #2 — coverage-check (F025, contract: coverage-check.md).
//!
//! The one genuinely new component. Given the round-1 passages and the
//! sub-questions, it classifies each sub-question `answered|pending` and returns
//! the pending set that drives retrieval round 2.
//!
//! Refinement validated by the T008 spike: the check judges over the round-1
//! PASSAGES (not a separate freeform draft call) so the loop stays at ≤3 LLM calls
//! (INV-1). Parsing biases to `pending` on any garble/absence/uncertainty
//! (FR-007). The original-query leg (id 0) is excluded from the pending set — it is
//! always retrieved in round 1, and the spike showed it skews to `pending`.

use tokio_util::sync::CancellationToken;

use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::ModelSlot;
use crate::services::query_router::generate_direct;

use super::{Coverage, CoverageItem, CoverageReport, PassageSet, SubQuestion};

/// Max chars of passage material fed to the coverage check (keeps call #2 short).
const COVERAGE_CHAR_BUDGET: usize = 12_000;

/// Classify each sub-question answered/pending from the round-1 passages.
pub(crate) async fn coverage_check(
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    subs: &[SubQuestion],
    passages: &PassageSet,
    cancel: Option<CancellationToken>,
) -> CoverageReport {
    let sub_list =
        subs.iter().map(|s| format!("{}: {}", s.id, s.text)).collect::<Vec<_>>().join("\n");

    let mut material = String::new();
    for p in &passages.passages {
        material.push_str(p.content.trim());
        material.push_str("\n---\n");
        if material.len() > COVERAGE_CHAR_BUDGET {
            break;
        }
    }

    let prompt = format!(
        "For EACH sub-question, decide whether the PASSAGES below contain enough to substantively \
         answer it (\"answered\") or not yet (\"pending\"). Output ONLY JSON, no other text: \
         {{\"coverage\":[{{\"id\":<int>,\"status\":\"answered or pending\"}}]}}\n\n\
         SUB-QUESTIONS:\n{sub_list}\n\nPASSAGES:\n{material}\n/no_think"
    );
    let raw = generate_direct(model_slot, tier, &prompt, 256, cancel, None).await.unwrap_or_default();
    parse_coverage(&raw, subs)
}

/// Parse the coverage JSON into a report, biasing every unknown/missing/garbled
/// sub-question to `Pending` (FR-007).
fn parse_coverage(raw: &str, subs: &[SubQuestion]) -> CoverageReport {
    let map = extract_status_map(raw);
    subs.iter()
        .map(|s| {
            let status = match map.get(&s.id).map(String::as_str) {
                Some("answered") => Coverage::Answered,
                _ => Coverage::Pending, // pending | unknown | missing → refetch
            };
            CoverageItem { sub_question_id: s.id, status }
        })
        .collect()
}

fn extract_status_map(raw: &str) -> std::collections::HashMap<u8, String> {
    let mut m = std::collections::HashMap::new();
    let (Some(s), Some(e)) = (raw.find('{'), raw.rfind('}')) else {
        return m;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw[s..=e]) else {
        return m;
    };
    if let Some(arr) = v.get("coverage").and_then(|x| x.as_array()) {
        for it in arr {
            if let (Some(id), Some(st)) =
                (it.get("id").and_then(|x| x.as_u64()), it.get("status").and_then(|x| x.as_str()))
            {
                m.insert(id as u8, st.trim().to_lowercase());
            }
        }
    }
    m
}

/// Sub-question ids still pending after coverage — the round-2 fetch set. Excludes
/// the original-query leg (id 0), which is always retrieved in round 1.
pub(crate) fn pending_ids(report: &CoverageReport) -> Vec<u8> {
    report
        .iter()
        .filter(|c| c.sub_question_id != 0 && c.status == Coverage::Pending)
        .map(|c| c.sub_question_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subs() -> Vec<SubQuestion> {
        vec![
            SubQuestion { id: 0, text: "orig".into(), is_original: true },
            SubQuestion { id: 1, text: "a".into(), is_original: false },
            SubQuestion { id: 2, text: "b".into(), is_original: false },
        ]
    }

    #[test]
    fn missing_and_unknown_bias_to_pending() {
        // id1 answered; id2 absent from the report → pending.
        let raw = "{\"coverage\":[{\"id\":1,\"status\":\"answered\"},{\"id\":0,\"status\":\"pending\"}]}";
        let rep = parse_coverage(raw, &subs());
        assert_eq!(rep[1].status, Coverage::Answered);
        assert_eq!(rep[2].status, Coverage::Pending);
        // id0 excluded from pending even though flagged pending.
        assert_eq!(pending_ids(&rep), vec![2]);
    }

    #[test]
    fn garbage_makes_all_pending_except_original() {
        let rep = parse_coverage("not json", &subs());
        assert_eq!(pending_ids(&rep), vec![1, 2]);
    }
}
