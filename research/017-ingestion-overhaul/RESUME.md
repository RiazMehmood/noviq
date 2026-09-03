# F017 Ingestion Overhaul — Resume (next session)

**Branch:** `017-ingestion-overhaul` (commit `5c1de4b9`, pushed). Built off `016-vllm-serving`.

## State (end of 2026-06-29 session)

**DONE + validated:**
- **Chunker fix** (`src-tauri/src/services/structure_parser.rs`): abbreviation/number-aware
  sentence split (no "Rs. 10,000" severing) + `long_chunk_threshold` 500→1500 + coalesce
  target/hard_max → 900/1500. Tests 25/25.
- **Mistral/doc_ir removed** from compilation (single deterministic ingestion path). Dormant
  files still on disk: `services/doc_ir/`, `commands/{importer,drafting,one_ask}.rs`,
  `db/repositories/ir_cache.rs` — can be physically deleted later.
- **Re-ingested all 11 corpus docs** (CPU, ~32 min, 8,425 chunks vs 10,208). CDWP §2.6 rule
  now ONE intact chunk (#35). A/B: **Q02 ECNEC→CDWP correct**, refusals 2/5→3/5, FABRICATED
  3→2. Beats reranker arm. Cost: p50 5s→14s (bigger chunks = fuller answers, generation-bound).

**VM live state:** new binary running (`target/release/noviq-server`, built 02:21, CUDA),
engine=vllm/Qwen3, **reranker OFF** (`ECHO_RERANK=0`), `ECHO_QUERY_CACHE=0`,
`ECHO_LOG_DIR=/opt/noviq/echoAI/logs/ab_rerank`. Deploy timer OFF. VM working tree has the
scp'd edits (now also committed on `017`). **PG port is EPHEMERAL** — rediscover each session:
`ss -tlnp | grep -oE '127.0.0.1:[0-9]+' | grep -v 8080`. PG pw `gjA979njVDyusVH1`, db `echo`,
user `postgres`, psql `/opt/noviq/data/pg/install/17.10.0/bin/psql`.

## TODO (tomorrow)

1. **Task #4 — GPU embeddings for admin indexing (flagged).** CPU baseline = **~32 min** for
   11 docs (per-doc timings in CHANGELOG/journalctl). Need: CUDA onnxruntime matching
   R470/CUDA-11.4+compat-12 (current `ORT_DYLIB_PATH=/opt/noviq/.ort-runtime/libonnxruntime.so.1.20.1`
   is CPU-only); wire fastembed EP behind an env flag (admin-only, desktop stays CPU); re-ingest
   on GPU → record GPU time for the CPU-vs-GPU comparison the user wants.
2. **5-timeout pathology** (Q01/Q04/Q16/Q19/Q30) — **ROOT-CAUSED + FIXED (2026-06-29), pending
   VM build/test.** NOT the reviewer loop (OFF). Cause: the `ActionItems` classifier cue
   (`intent_router/classifier.rs:52`) matched a bare "must"; all 5 questions contain "must" →
   misrouted to the whole-corpus ActionItems **map-reduce** (1 LLM call per batch over ~8,425
   chunks) → ran until the client disconnected on BOTH engines. Perfect 1:1 correlation (cue
   fires on exactly those 5, none of the 26 passers). Fix: dropped `musts?|shall do` from the
   cue + regression test in `assistant/intent.rs` + map-reduce safety cap (`mapreduce.rs`,
   `NOVIQ_MAPREDUCE_MAX_BATCHES`/`_BUDGET_SECS`). See CHANGELOG_DEV 2026-06-29.
   **VM TODO:** `cargo test` the two test modules (no Rust on Windows), then re-run the A/B —
   the 5 should now ANSWER (their answer *quality* on these hard boundary/refusal/conflict
   items is a separate question to assess).
3. **Q12** residual "Rs 5,000M" threshold mix-up (minor, ambiguous Q).
4. **Housekeeping:** PHR for this change still owed. Optionally purge the 657MB Mistral GGUF
   (`models/ir_builder/`) + dormant source files. Decide merge path for `017` → `release`.

## How to re-run the A/B (validation)
Scripts on VM at `/opt/noviq/vllm-scripts/`: `run_ab.py`, `eval_set_sindh.json`. Helpers in
this repo's scratchpad were `peek.py`/`ans.py` (re-create if needed). Admin creds `admin` /
`yyVZjNaJFAuv0owFxhSk`. Outputs: `out_awq.jsonl` (baseline), `out_awq_rerank.jsonl` (reranker),
`out_awq_newchunk.jsonl` (chunker fix). Compare: `python3 run_ab.py compare <files...>`.
