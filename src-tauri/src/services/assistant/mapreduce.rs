//! T032 — the **map-reduce driver** for whole-/cross-document capabilities
//! (FR-005d, FR-016, research R4).
//!
//! Whole-document work (summarize a book, compare, conflict, whole-doc action
//! items) cannot fit one window. The driver splits the in-scope **atomic**
//! provisions into window-sized batches — no provision is ever split, and every
//! provision lands in exactly one batch (full coverage; nothing silently
//! skipped) — runs a grounded extract pass per batch on the single resident
//! model (Constitution III — sequential, never concurrent), then a final reduce
//! pass that synthesises the per-batch extracts. Each pass obeys the same
//! `prompt ≤ window` guarantee as a single turn. Progress is streamed as
//! `assistant://progress { phase: "map" | "reduce", … }`.
//!
//! The **batcher** ([`plan_batches`]) is pure (a [`TokenCounter`] stand-in makes
//! it unit-testable without a live model) — its full-coverage property is the
//! SC-009 acceptance gate. The async [`run`] driver wires the batches through the
//! single model slot; it is not exercised by the no-LLM acceptance tests but is
//! the production path.

use std::sync::Arc;
use crate::compat::{AppHandle, Emitter};

use crate::services::assistant::context_budget::{EstimateCounter, TokenCounter};
use crate::services::{
    hardware_detector::HardwareTier,
    llm::ModelSlot,
    query_router,
    scheduler::{Scheduler, WorkloadKind},
};

/// One window-sized batch: the indices (into the provisions slice) of the whole
/// provisions assigned to this pass, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub indices: Vec<usize>,
}

/// Pack atomic provisions into window-sized batches for the map phase.
///
/// Invariants (the SC-009 contract):
/// - **No provision is split** — each provision is assigned whole to one batch.
/// - **Full coverage** — every provision appears in exactly one batch, and the
///   batches preserve document order (the union of all indices is `0..N`).
/// - A provision too large to fit even alone (with the per-batch overhead) still
///   gets its own singleton batch rather than being dropped — the allocator's
///   per-pass `prompt ≤ window` safety net handles the degenerate case; nothing
///   is silently skipped.
///
/// `per_batch_overhead` is the token cost the system prompt + instruction add to
/// every pass; `gen_reserve` is carved out for the extract. The per-batch
/// provision budget is `window − gen_reserve − per_batch_overhead`.
pub fn plan_batches<C: TokenCounter>(
    counter: &C,
    provisions: &[String],
    per_batch_overhead: usize,
    window: usize,
    gen_reserve: usize,
) -> Vec<Batch> {
    let budget = window
        .saturating_sub(gen_reserve)
        .saturating_sub(per_batch_overhead)
        .max(1);

    let mut batches: Vec<Batch> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    let mut used = 0usize;

    for (i, p) in provisions.iter().enumerate() {
        let t = counter.count(p);
        // Close the current batch before this provision would overflow it — but
        // never emit an empty batch (an oversized-alone provision starts fresh
        // and becomes its own singleton).
        if !cur.is_empty() && used + t > budget {
            batches.push(Batch { indices: std::mem::take(&mut cur) });
            used = 0;
        }
        cur.push(i);
        used += t;
    }
    if !cur.is_empty() {
        batches.push(Batch { indices: cur });
    }
    batches
}

/// The result of a map-reduce run: the synthesised final answer plus the raw
/// per-batch extracts (kept for tracing / audit).
#[derive(Debug, Clone)]
pub struct MapReduceResult {
    pub answer: String,
    pub batch_extracts: Vec<String>,
    pub batches: usize,
}

/// Render a batch's provisions into a numbered context block for the prompt. The
/// `[n]` labels feed the same citation machinery the single-pass path uses.
fn context_for(provisions: &[String], batch: &Batch) -> String {
    batch
        .indices
        .iter()
        .enumerate()
        .map(|(n, &i)| format!("[{}] {}", n + 1, provisions[i]))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Run a whole-document capability via sequential map-reduce over the single
/// model slot. `sys_map` is the system prompt for the per-batch extract passes;
/// `sys_reduce` for the final synthesis. `instruction` is the user's request.
/// Streams `assistant://progress`.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    scheduler: &Arc<Scheduler>,
    app: Option<&AppHandle>, // T009b: None on the headless HTTP path
    sys_map: &str,
    sys_reduce: &str,
    instruction: &str,
    provisions: &[String],
    gen_reserve: usize,
) -> Result<MapReduceResult, String> {
    let emit = |phase: &str, i: usize, total: usize| {
        if let Some(a) = app {
            let _ = a.emit(
                "assistant://progress",
                serde_json::json!({ "phase": phase, "batch": i, "total": total }),
            );
        }
    };

    // F016: route whole-corpus generation to an HTTP engine (vLLM sidecar or remote
    // API) when selected, so these passes never load the GGUF (no llama weights in
    // VRAM beside vLLM).
    let use_vllm = crate::services::inference::engine_from_env().is_http();

    // Resolve the active model + its dynamic window (scales with the model, T029).
    let (rel, _label, _notice) = query_router::select_model_for_tier(tier);
    let model_path = query_router::resolve_model_path(&rel);
    let window: usize = if use_vllm {
        std::env::var("NOVIQ_REMOTE_MAX_MODEL_LEN")
            .or_else(|_| std::env::var("NOVIQ_VLLM_MAX_MODEL_LEN"))
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(8192)
    } else {
        model_slot
            .window_info(&model_path, tier)
            .map_err(|e| e.to_string())?
            .1 as usize
    };
    let max_prompt_tokens = window.saturating_sub(gen_reserve);

    // Per-batch overhead: the system prompt + instruction cost every pass pays
    // (estimate-counted — the live tokenizer caps the prompt as a safety net).
    let overhead = EstimateCounter.count(sys_map) + EstimateCounter.count(instruction);
    let batches = plan_batches(&EstimateCounter, provisions, overhead, window, gen_reserve);
    if batches.is_empty() {
        return Ok(MapReduceResult { answer: String::new(), batch_extracts: vec![], batches: 0 });
    }

    let handle = scheduler.register(WorkloadKind::Chat);
    let cancel = handle.cancellation_token();

    // ── Safety cap (defense-in-depth, 2026-06-29) ─────────────────────────────
    // The real protection against runaway map-reduce is the classifier (a
    // misrouted factual question must never reach this path). This is the
    // backstop: should a future misroute slip through, bound the map phase by a
    // max batch count AND a wall-clock budget so the request degrades gracefully
    // (reduce whatever was extracted, with a coverage note) instead of hanging
    // past the client timeout. Defaults are generous enough for a legitimate
    // single-document summary; override via env for very large scoped jobs.
    let max_batches: usize = std::env::var("NOVIQ_MAPREDUCE_MAX_BATCHES")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(64);
    let budget_secs: u64 = std::env::var("NOVIQ_MAPREDUCE_BUDGET_SECS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(600);
    let started = std::time::Instant::now();

    // ── MAP: one grounded extract pass per batch (sequential — single slot). ──
    let total = batches.len();
    let mut extracts: Vec<String> = Vec::with_capacity(total);
    let mut capped = false;
    for (i, batch) in batches.iter().enumerate() {
        // Stop mapping once either cap is hit — keep the extracts gathered so far
        // and proceed to reduce (graceful degradation, not a hang).
        if i >= max_batches || started.elapsed().as_secs() >= budget_secs {
            log::warn!(
                "mapreduce: safety cap hit at batch {}/{} (max_batches={max_batches}, \
                 budget_secs={budget_secs}, elapsed={}s) — reducing partial extracts",
                i, total, started.elapsed().as_secs()
            );
            capped = true;
            break;
        }
        emit("map", i + 1, total);
        let context = context_for(provisions, batch);
        let prompt = query_router::build_rag_prompt(instruction, &context, &[], sys_map, false);
        let (out, cancelled) = if use_vllm {
            query_router::vllm_infer(&prompt, gen_reserve as u32, Some(cancel.clone()), app.cloned())
                .await
                .map_err(|e| e.to_string())?
        } else {
            model_slot
                .infer_with_cancel_temp(
                    &model_path,
                    &prompt,
                    gen_reserve as u32,
                    window as u32,
                    max_prompt_tokens,
                    tier,
                    Some(cancel.clone()),
                    app.cloned(),
                    Some(0.0),
                    None,
                )
                .map_err(|e| e.to_string())?
        };
        extracts.push(out);
        if cancelled {
            scheduler.unregister(handle.id);
            return Ok(MapReduceResult { answer: extracts.join("\n\n"), batch_extracts: extracts, batches: total });
        }
    }

    // ── REDUCE: synthesise the per-batch extracts into the final answer. ──────
    // A single batch needs no synthesis — its extract IS the answer.
    let answer = if extracts.len() <= 1 {
        extracts.first().cloned().unwrap_or_default()
    } else {
        emit("reduce", total, total);
        let joined = extracts
            .iter()
            .enumerate()
            .map(|(n, e)| format!("[{}] {}", n + 1, e))
            .collect::<Vec<_>>()
            .join("\n\n");
        let prompt = query_router::build_rag_prompt(instruction, &joined, &[], sys_reduce, false);
        let (out, _cancelled) = if use_vllm {
            query_router::vllm_infer(&prompt, gen_reserve as u32, Some(cancel.clone()), app.cloned())
                .await
                .map_err(|e| e.to_string())?
        } else {
            model_slot
                .infer_with_cancel_temp(
                    &model_path,
                    &prompt,
                    gen_reserve as u32,
                    window as u32,
                    max_prompt_tokens,
                    tier,
                    Some(cancel.clone()),
                    app.cloned(),
                    Some(0.0),
                    None,
                )
                .map_err(|e| e.to_string())?
        };
        out
    };

    scheduler.unregister(handle.id);
    let answer = if capped {
        format!(
            "{answer}\n\n_Note: this response covers {} of {} sections — generation was \
             stopped at a safety limit before the full set could be processed._",
            extracts.len(),
            total,
        )
    } else {
        answer
    };
    Ok(MapReduceResult { answer, batch_extracts: extracts, batches: total })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1 token per whitespace word — exact, readable budget math.
    struct WordCounter;
    impl TokenCounter for WordCounter {
        fn count(&self, t: &str) -> usize { t.split_whitespace().count() }
    }

    fn prov(words: usize) -> String {
        std::iter::repeat("x").take(words).collect::<Vec<_>>().join(" ")
    }

    /// Every provision is covered exactly once, in order — no split, none dropped.
    fn assert_full_coverage(provisions: &[String], batches: &[Batch]) {
        let flat: Vec<usize> = batches.iter().flat_map(|b| b.indices.clone()).collect();
        let expected: Vec<usize> = (0..provisions.len()).collect();
        assert_eq!(flat, expected, "batches must cover 0..N in order, exactly once");
    }

    #[test]
    fn batches_cover_every_provision_exactly_once_in_order() {
        let provisions: Vec<String> = (0..20).map(|_| prov(30)).collect();
        // budget = 200 - 20 reserve - 10 overhead = 170 → ~5 provisions/batch.
        let batches = plan_batches(&WordCounter, &provisions, 10, 200, 20);
        assert!(batches.len() > 1, "must span multiple passes: {batches:?}");
        assert_full_coverage(&provisions, &batches);
    }

    #[test]
    fn no_batch_overflows_the_per_pass_budget_except_oversized_singletons() {
        let provisions: Vec<String> = (0..12).map(|_| prov(40)).collect();
        let (window, reserve, overhead) = (200usize, 20usize, 10usize);
        let budget = window - reserve - overhead; // 170
        let batches = plan_batches(&WordCounter, &provisions, overhead, window, reserve);
        for b in &batches {
            let used: usize = b.indices.iter().map(|&i| WordCounter.count(&provisions[i])).sum();
            assert!(
                used <= budget || b.indices.len() == 1,
                "a multi-item batch must fit the budget: used={used} > {budget}",
            );
        }
    }

    #[test]
    fn oversized_provision_gets_its_own_singleton_batch_not_dropped() {
        let provisions = vec![prov(5), prov(500), prov(5)];
        let batches = plan_batches(&WordCounter, &provisions, 10, 200, 20);
        assert_full_coverage(&provisions, &batches);
        // The 500-word provision can't share a batch — it stands alone.
        assert!(batches.iter().any(|b| b.indices == vec![1]), "{batches:?}");
    }

    #[test]
    fn empty_input_yields_no_batches() {
        let batches = plan_batches(&WordCounter, &[], 10, 200, 20);
        assert!(batches.is_empty());
    }

    #[test]
    fn context_for_numbers_provisions_for_citation() {
        let provisions = vec!["Section 1 text".to_string(), "Section 2 text".to_string()];
        let b = Batch { indices: vec![0, 1] };
        let ctx = context_for(&provisions, &b);
        assert!(ctx.contains("[1] Section 1 text"));
        assert!(ctx.contains("[2] Section 2 text"));
    }
}
