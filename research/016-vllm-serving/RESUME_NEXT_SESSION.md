# F016 vLLM — Resume (next session)

**Decision (FINAL, user 2026-06-28): GO WITH vLLM.** Quality A/B passed, vLLM beats
llama.cpp on every axis; SC-002 p95≤10s is hardware-impossible on the time-sliced
A40-12Q and is to be renegotiated, not treated as a vLLM failure.

Read `FINDINGS.md` (this folder) for the full research record + `evidence/`. This file
is just the live state + what's left.

---

## A. Current VM state (branch `016-vllm-serving`, all pushed)

- **016 binary BUILT + RUNNING** under systemd `noviq-server` (incremental CUDA build,
  `target/release/noviq-server`; release binary backed up at `…/noviq-server.release-bak`).
- **`.env` on VM** has: `NOVIQ_INFER_ENGINE=vllm`, `NOVIQ_VLLM_*` (URL/MODEL/MAX_MODEL_LEN=8192/
  GPU_UTIL=0.80/MAX_NUM_SEQS=16/QUANT=awq/TIMEOUT_MS), `NOVIQ_VLLM_LEN_HEADROOM=1024`,
  **`ECHO_QUERY_CACHE=0`** (TESTING — revert for prod), **`RUST_LOG=info,echo_ai=debug`** (TESTING — remove),
  `.env.bak-vllm-window` saved.
- **vLLM sidecar `noviq-vllm`** active on :8000. **Winning config = fp16 KV + chunked prefill
  (V1 engine):** the VM's `/opt/noviq/vllm-scripts/run-vllm.sh` was edited (sed) to
  `--kv-cache-dtype auto` + `--enable-chunked-prefill`, util 0.80, served window 9216.
  ⚠️ **The repo `deploy/vllm/run-vllm.sh` does NOT match** — it still has fp8 + headroom 2048,
  no chunked prefill. **RECONCILE: commit the fp16/chunked/util-0.80 config to the repo script.**
- **Deploy timer STOPPED** (`noviq-deploy.timer`) so the 016 checkout/binary isn't reverted to
  release. **Don't re-enable until 016 is merged to release** (else it rebuilds the release binary
  which lacks the vLLM wiring).
- Scripts + evidence live at **`/opt/noviq/vllm-scripts/`** (run-vllm.sh, run_ab.py, load_test.sh,
  diag_concurrency.sh, out_*.jsonl, *.log). Models cached at `/opt/noviq/model-library/hf-cache`
  (AWQ complete; GPTQ w4a16 re-download was kicked — check `gptq_dl.log`).
- **Sentinel flag** (`/opt/noviq/data/DEPLOYING`): state uncertain — check + clear for prod.
- vLLM was crashing on chunked-prefill+**fp8** (Triton `fp8e4nv` unsupported on sm_86) → that's why
  the working config uses **fp16 KV**. Don't revert to fp8+chunked.
- Harness gotcha (FIXED in repo `load_test.sh`): FR-011 "unavailable" is HTTP 200 → must inspect
  body, else outages look like fast successes. VM copy may be stale — re-extract.

## B. Cutover mechanics still TODO (T032 + ops)

1. **Reconcile `deploy/vllm/run-vllm.sh`** to the winning fp16/chunked/util-0.80 config; push;
   re-extract to `/opt/noviq/vllm-scripts/`.
2. **Re-enable query cache** (remove `ECHO_QUERY_CACHE=0`), **remove `RUST_LOG` debug**.
3. **Merge `016-vllm-serving` → `release`** (so poll-deploy keeps the vLLM wiring), then **re-enable
   `noviq-deploy.timer`**. (Or keep timer off + pinned — decide.)
4. **`systemctl enable noviq-vllm`** (auto-start on boot); confirm `Wants=/After=noviq-vllm` ordering
   is applied on the VM unit (repo `noviq-server.service` has it; VM unit may not).
5. Clear the `DEPLOYING` sentinel; verify `/api/health.engine` = vllm ready.
6. **T035 constitution amendment** (server-only inference-engine exception, ref ADR-0018).
7. Tasks to mark in `specs/016-vllm-serving/tasks.md`: T017 (concurrency measured), T029/T030
   (A/B done — AWQ winner; GPTQ arm optional), T032 (cutover), T035.

## C. User's OPEN quality questions — ANSWER THESE NEXT (with evidence, not assertions)

The user wants a documents-grounded audit before fully trusting answers. They asked:

1. **Which TYPE of questions failed (the 5)?** Q01=boundary, Q04=definition, Q16=boundary,
   Q19=refusal, Q30=conflict. Find what's COMMON (working hypothesis: they retrieve a LARGE context
   → big prompt → grounding-retry / multi-pass → 200s timeout on llama.cpp, 117–180s/500 on vLLM).
   **Bug fixed:** prompt+gen window overflow → vLLM 400 (added 2048→now headroom). **NOT confirmed:**
   the exact slow-path trigger — must READ the code (grounding-retry at `query_router.rs` ~L1348 +
   `validate_grounding`; map-reduce for Compare/Conflict). Confirm before claiming.
2. **Retrieval settings (found in `query_router.rs`, record exact values):**
   - `CONTEXT_TOP_K = 20` (max chunks into LLM context).
   - `ECHO_VEC_K = 24` (vector/Tier-2 top-N), `ECHO_FTS_K = 24` (FTS5/Tier-1 top-N).
   - `ctx_top_k = (ctx_budget/150).clamp(20,400)`; `fetch_k = (ctx_top_k*2).clamp(vec_k,600)`;
     `fetch_limit = fetch_k * pool_mult`.
   - **RRF merge** = `rrf_merge()` combines vec + FTS (find the RRF k constant + formula — was at
     query_router.rs around the "Tier 1 + Tier 2 dual retrieval → RRF merge (FR-039)" block ~L576+).
   - Post-RRF salience boosts: S1 (unit_label), S2 (definitional), S3b (entity-match); entity boost
     via `apply_entity_boost`. Reranker OFF by default (`ECHO_RERANK=1` to enable). Retry escalation
     table (top_k, token_budget) per attempt.
   - **Still to extract:** the exact `rrf_merge` constant (k=60?) and how final top chunks are cut
     (`build_context` + token budget). Read `rrf_merge` + `build_context` fully.
3. **Citation QUALITY — verify, don't assume.** out_awq.jsonl includes a `citations` array per answer
   (instrument/path/verbatim/page). For a sample of questions: take the cited `verbatim` + page and
   **check it against the actual PDF** (`pdftotext -layout /opt/noviq/data/uploads/admin/<file>.pdf`)
   to confirm (a) the citation text really exists in the doc, (b) it supports the claim, (c) the model
   didn't downgrade/skip the "golden" chunk. Report: cite-present vs cite-CORRECT vs confabulated.
4. **Do they get the correct/golden chunks or confabulate?** Compare retrieved chunks (the `citations`
   + the context the server built) against what SHOULD be retrieved for each Q's known ground truth
   (see FINDINGS §9 anchors: DDWP<2000M iff aid<25%, CDWP≤10000M, etc.). Check if the right provision
   chunk is in the top-K and actually cited.

## D. How to work the VM (reminders)

- SSH `<user>@<PRIVATE_IP>`; sudo pw `P@ssw0rd` (NOPASSWD only for `systemctl restart noviq-server`;
  else `echo P@ssw0rd | sudo -S`). VPN drops long foreground SSH → `setsid script &` + poll a log.
- PG: port via `ss -tlnp|grep postgres`, psql `/opt/noviq/data/pg/install/17.10.0/bin/psql`, db `echo`,
  user `postgres`, pw `cat /opt/noviq/data/pg/.pgpass`. SQL: avoid single-quotes through SSH (use `$$`).
- admin creds: `admin` / `yyVZjNaJFAuv0owFxhSk`. load_test/run_ab: **`export USER=admin`** (shell `$USER`
  clobbers the default → 401).
- No Rust on Windows → build/check on VM via worktree + persistent `CARGO_TARGET_DIR` (or main checkout
  since timer is off). Incremental CUDA build ~4 min.
