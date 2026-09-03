---
description: "Dependency-ordered task list for vLLM Concurrent Inference Serving (016)"
---

# Tasks: vLLM Concurrent Inference Serving

**Input**: Design documents from `/specs/016-vllm-serving/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅
**ADR**: [ADR-0018](../../history/adr/0018-server-side-inference-engine-vllm-continuous-batching-sidecar.md) (Accepted)

**Tests**: Limited unit tests included **only** where plan.md's Testing section explicitly calls them out (SSE delta mapping + trait/HTTP client). No full TDD requested.

**Organization**: Grouped by user story. US1 (P1) is the MVP. Spike 0 is GREEN (done 2026-06-28), so no task is provisional on the CUDA gate.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1 / US2 / US3 (setup/foundational/polish have no story label)

## Path Conventions

Web/server layout (per plan.md): Rust backend `src-tauri/src/`, deploy assets `deploy/`, eval tooling `tools/`. Inference runtime is **out-of-tree** on the VM under `/opt/noviq` (provisioned, not built by poll-deploy).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Spike gate (done) + scaffolding for the seam, sidecar, and tooling.

- [X] T001 Spike 0 — CUDA-12 forward-compat **and** live serve smoke proven on the R470 vGPU (vllm 0.8.5 served Qwen2.5-0.5B on :8009, `/health` OK + real `/v1/completions`, alongside prod, zero impact). Evidence: `specs/016-vllm-serving/VLLM_SPIKE_RESUME.md`. **GATE PASSED.**
- [X] T002 [P] Create `deploy/vllm/` (`setup-vllm.sh`, `run-vllm.sh`, `noviq-vllm.service`) and `tools/vllm_eval/` (`load_test.sh`; `run_ab.sh` pending T029)
- [ ] T003 [P] Create `src-tauri/src/services/inference/` module skeleton (`mod.rs`, `llamacpp.rs`, `vllm.rs`) and register `pub mod inference;` in `src-tauri/src/services/mod.rs`
- [ ] T004 [P] Add vLLM config keys to the env loader + `.env.example`: `NOVIQ_INFER_ENGINE` (default `llamacpp`), `NOVIQ_VLLM_URL`, `NOVIQ_VLLM_MODEL`, `NOVIQ_VLLM_MAX_MODEL_LEN`, `NOVIQ_VLLM_TIMEOUT_MS` (per data-model.md)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The engine-agnostic seam. Must complete before any story. End state: with `NOVIQ_INFER_ENGINE=llamacpp` the server behaves **identically to today** (regression-safe foundation).

**⚠️ CRITICAL**: No user story work begins until this phase is complete.

- [X] T005 Define the `TextGenerator` trait + `GenParams`, `GenOutcome`, `GenError`, `EngineHealth` types in `src-tauri/src/services/inference/mod.rs`. *VM `cargo check`/`test` PASS.*
- [X] T006 Engine selection: `engine_from_env()` (`NOVIQ_INFER_ENGINE`) + process-wide `vllm_generator()` (warm reqwest pool) in `inference/mod.rs`. *Done.*
- [~] T007 `LlamaCppGenerator` — **superseded by a smaller-diff design**: the seam branches at the generation call sites (`engine_from_env()`), so the **existing direct `ModelSlot` path IS the fallback** (FR-017). A wrapper impl was unnecessary; only `VllmGenerator` implements the trait. (Revisit if the trait ever needs both impls behind one `Arc<dyn>`.)
- [X] T008 Branch the generation call-site `query_router::try_llm_infer_streaming` → `vllm_infer` (raw `/v1/completions`) when `engine=vllm`; retrieval/prompt/no-think/grounding/citation untouched. Also routed `mapreduce.rs` + short-circuited `resolve_active_window` so the GGUF never loads under vLLM. *VM `cargo check` (server) PASS.*
- [X] T009 No startup factory needed (call-site branching); `gen_gate` kept for `llamacpp`, lifted for `vllm` (T014). *Done.*

**Checkpoint**: `cargo check --features server` green on the VM; production `--features llm,server` check running. With `engine=llamacpp` (default) the path is byte-for-byte unchanged.

---

## Phase 3: User Story 1 - Ten officers ask at the same time (Priority: P1) 🎯 MVP

**Goal**: Serve ≥ 10 concurrent users grounded+cited within budget via the vLLM sidecar + continuous batching.

**Independent Test**: Drive 10 simultaneous `/api/query` requests (distinct users); all 10 return correct cited answers, p95 ≤ 10 s, GPU never OOMs (SC-001..004).

- [X] T010 [P] [US1] Write `deploy/vllm/setup-vllm.sh` — one-time out-of-tree provisioning: venv (uv py3.10) + `setuptools wheel`, locally-extracted `cuda-compat-12-x`, `pip install vllm==0.8.5` + pinned `transformers==4.51.3`/`tokenizers==0.21.4`, download `Qwen/Qwen3-8B-AWQ` **and** the GPTQ build into `/opt/noviq/model-library` (carries the Spike 0 gotchas). *Created; runs on the VM at provisioning.*
- [X] T011 [P] [US1] Write `deploy/vllm/run-vllm.sh` — launch flags per `contracts/vllm-openai-api.md`: `--host 127.0.0.1 --port 8000`, `--quantization awq|gptq_marlin`, `--kv-cache-dtype fp8`, `--max-model-len`, `--gpu-memory-utilization`, `--max-num-seqs 16`, `--served-model-name`, `HF_HUB_OFFLINE=1`, `LD_LIBRARY_PATH` compat prepend. *Created; reads tunables from `.env`.*
- [X] T012 [US1] Implement `VllmGenerator::generate_stream` in `src-tauri/src/services/inference/vllm.rs` — `reqwest` POST to `NOVIQ_VLLM_URL/v1/completions` with `stream=true`, raw prompt verbatim, byte-buffered SSE parse → `on_token` → reassemble, `tokio::select!` cancellation. *Done; pending VM `cargo check`.*
- [X] T013 [US1] Map errors in `src-tauri/src/services/inference/vllm.rs`: connect/5xx → `GenError::Unavailable`, request timeout / per-chunk `NOVIQ_VLLM_TIMEOUT_MS` → `GenError::Timeout`, other → `Upstream`. *Done.*
- [X] T014 [US1] Lift the `gen_gate` semaphore when `engine=vllm` (in `src-tauri/src/api/query.rs` — the gate's actual acquire site) so generations run concurrently (vLLM owns batching/admission). *Done.*
- [X] T015 [US1] VRAM tuning pass on the VM — **PASS**: 8B-AWQ resident at `--gpu-memory-utilization 0.88 --max-model-len 8192 --max-num-seqs 16 --kv-cache-dtype fp8` = **11.2/12.1 GB** (~900 MB headroom). Values recorded in `.env` (`NOVIQ_VLLM_*` block, 2026-06-28). No reduction of max-model-len needed.
- [X] T016 [P] [US1] Write `tools/vllm_eval/load_test.sh` — fire 10/20/50 concurrent authenticated `/api/query`; capture p50/p95, throughput (req/min), GPU util+mem (`nvidia-smi`), failure rate. *Created; engine-agnostic (baseline vs vLLM).*
- [~] T017 [US1] **Engine-level SC-001..004 PASS** (maintenance window 2026-06-28, direct `/v1/completions`, real legal prompt): **SC-001** 10/10 (& 20/20) · **SC-002** p95 8.1 s @ N=10 · **SC-003 ~5.6×** (74.1 vs llama.cpp baseline 13.2 req/min) · **SC-004** 0 fail / peak 11.4 GB. **Full `/api/query` end-to-end still pending** — the deployed `noviq-server` is built from `release` (no T008/T014 wiring), so `load_test.sh` through the real stack needs the 016 binary built/deployed (→ T032 cutover, A/B-gated).

**Checkpoint**: 10 concurrent users served within budget — MVP demonstrable (engine still default-off until cutover).

---

## Phase 4: User Story 2 - Answers stream and stay grounded under load (Priority: P2)

**Goal**: Under concurrency, answers still stream progressively and keep valid `[n]` citations + the explicit refusal.

**Independent Test**: With several concurrent requests in flight, one request streams tokens early and ends with valid citations; a no-evidence question returns the refusal, not a fabrication.

- [X] T018 [P] [US2] Unit test SSE delta parsing → reassembly (deltas, partial-line buffering, terminal empty-text, `[DONE]`, comment/blank lines, `/metrics` gauge parse) in `src-tauri/src/services/inference/vllm.rs` tests. *Done; pending VM `cargo test`.*
- [X] T022 [US2] Honor the existing `CancellationToken` mid-stream in `src-tauri/src/services/inference/vllm.rs` (`tokio::select!` on `cancel.cancelled()` aborts the in-flight stream → partial text + `was_cancelled`). *Done (built into T012).*
- [ ] T019 [US2] Verify `VllmGenerator` output flows through the **same** downstream cleanup (`strip_reasoning_spans`, `strip_special_tokens`) and grounding/citation gating in `src-tauri/src/services/query_router.rs` (FR-003/004/014 — no engine-specific leak above the trait)
- [ ] T020 [US2] Confirm SSE deltas surface as existing `chat://` token events (no frontend change); manual progressive-display check while other requests run (SC-006)
- [ ] T021 [US2] Verify the "cannot be determined from the available documents" refusal still fires under load (no fabricated answer) — exercise a no-evidence prompt during a concurrent burst

**Checkpoint**: Streaming + grounding + refusal all hold under concurrency.

---

## Phase 5: User Story 3 - Operator can deploy, observe, recover (Priority: P3)

**Goal**: Engine auto-starts on boot/deploy with the right model, health is observable, and faults degrade gracefully.

**Independent Test**: Reboot the host → engine auto-serves, `/api/health` reports ready; kill the sidecar → user gets a friendly message (or fallback), not a hang.

- [X] T023 [P] [US3] Write `deploy/vllm/noviq-vllm.service` systemd unit: `ExecStart=run-vllm.sh`, `EnvironmentFile=.env`, `User=appadmin`, `Restart=on-failure`, `TimeoutStartSec=180`, `After=network-online.target`, `WantedBy=multi-user.target` (FR-012). *Created; install on the VM.*
- [X] T024 [US3] Add `Wants=`/`After=noviq-vllm` ordering to the `noviq-server` unit (weak/non-cascading — llama.cpp boxes may lack the sidecar unit; FR-011 handles a sidecar crash). *Done; install on VM.*
- [X] T025 [US3] Add an "ensure `noviq-vllm` active" check (no rebuild) to `deploy/deploy.sh` poll-deploy flow — only when `NOVIQ_INFER_ENGINE=vllm`; `systemctl start` if not active. *Done; `bash -n` clean.*
- [X] T026 [US3] Add `engine: { kind, ready, num_running, num_waiting }` to `/api/health` in `src-tauri/src/api/health.rs`, fed by `TextGenerator::health()` (vLLM `/health` + `/metrics`; llamacpp → ready:true, queue N/A). *VM `cargo check` server + llm,server PASS; tests 6/6.*
- [X] T027 [US3] FR-011 graceful handling in `query_router.rs`: engine-aware "service temporarily unavailable" message (not "download a model") + optional `NOVIQ_VLLM_FALLBACK` drop-through to the in-process llama.cpp path (off by default — VRAM contention). *VM-verified.*
- [ ] T028 [US3] Reboot + deploy recovery test (SC-008): confirm engine serves again with no manual steps and health reports ready

**Checkpoint**: Production-operable — auto-start, observable, graceful failure + instant rollback.

---

## Phase 6: Polish, Cutover & Cross-Cutting

**Purpose**: Quality gate, the actual engine flip, offline proof, and governance.

- [ ] T029 [P] Write `tools/vllm_eval/run_ab.sh` — same legal question set (≥ 30 Qs: lookup/definition/cross-ref/refusal) through llama.cpp GGUF (baseline) vs vLLM AWQ vs vLLM GPTQ; score grounding/citation correctness + latency (reuse existing eval sets/validator)
- [ ] T030 Run the A/B no-regression gate; pick the AWQ|GPTQ winner, set `NOVIQ_VLLM_MODEL`, drop the loser (**FR-014 / SC-005 cutover gate** — hold cutover if regression)
- [ ] T031 Verify fully offline during inference — zero external network calls, `HF_HUB_OFFLINE=1`, local weights only (SC-007)
- [ ] T032 Cutover: set `NOVIQ_INFER_ENGINE=vllm` (+ URL/model/max-len) in `/opt/noviq/echoAI/.env`, restart `noviq-server`, confirm `engine.ready=true`; document the one-line `=llamacpp` rollback (FR-017)
- [ ] T033 [P] (Optional, additive) Add nullable `query_log.engine` column + label rows `vllm`/`llamacpp` — only if A/B/monitoring needs it in-DB (additive migration only)
- [ ] T034 [P] Docs: run `quickstart.md` end-to-end validation; append a dated `CHANGELOG_DEV.md` entry (symptom/why, files, cross-cutting)
- [ ] T035 Draft the constitution amendment (Principle III + Tech-Stack Invariant, **server-only** inference-engine exception) referencing ADR-0018; mirror the Postgres precedent

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (P1)**: T001 done; T002–T004 can start immediately (parallel).
- **Foundational (P2)**: depends on Setup — **BLOCKS all user stories**. T005 → T006/T007 → T008/T009.
- **US1 (P3)**: depends on Foundational. Sidecar (T010/T011) ∥ client (T012/T013); then T014; then tune+load (T015→T017).
- **US2 (P4)**: depends on US1 (needs `VllmGenerator`). T018 ∥; T019–T022.
- **US3 (P5)**: depends on US1 (needs sidecar + client). T023 ∥; T024–T028.
- **Polish/Cutover (P6)**: A/B (T029/T030) can run once the sidecar serves (after US1); cutover (T032) after US1+US2+US3 + A/B pass.

### User Story Independence

- **US1** is self-contained (the MVP) — deliverable without US2/US3 polish (engine stays default-off).
- **US2** hardens streaming/grounding US1 already enables — independently testable.
- **US3** adds operability around the same sidecar — independently testable.

### Parallel Opportunities

- Setup: T002, T003, T004 in parallel.
- US1: T010 ∥ T011 (deploy scripts) ∥ T016 (load script) while T012/T013 (vllm.rs) proceed.
- US3: T023 (unit file) ∥ other story work.
- Polish: T029, T033, T034 in parallel.

---

## Parallel Example: User Story 1

```bash
# Provisioning + tooling (different files, no deps) in parallel:
Task: "deploy/vllm/setup-vllm.sh — venv + cuda-compat + download AWQ & GPTQ"   # T010
Task: "deploy/vllm/run-vllm.sh — vllm serve launch flags"                       # T011
Task: "tools/vllm_eval/load_test.sh — 10/20/50 concurrency harness"            # T016
# Meanwhile the Rust client (single file, sequential within itself):
Task: "VllmGenerator::generate_stream in src-tauri/src/services/inference/vllm.rs"  # T012 → T013
```

---

## Implementation Strategy

### MVP First (User Story 1)

1. Phase 1 Setup (T001 done; T002–T004).
2. Phase 2 Foundational (T005–T009) — **regression-safe seam, engine still llamacpp**.
3. Phase 3 US1 (T010–T017) — sidecar + vLLM client + lift gate + load test.
4. **STOP & VALIDATE**: 10-user load test passes SC-001..004.

### Incremental Delivery

US1 (concurrency) → US2 (streaming/grounding under load) → US3 (operability) → Polish/Cutover (A/B gate → flip `NOVIQ_INFER_ENGINE=vllm` → docs → constitution amendment). Each increment keeps the llama.cpp fallback one env flip away.

---

## Notes

- Inference runtime stays **out-of-tree** on the VM (survives `git reset --hard`); poll-deploy only gains an "ensure active" check (T025), never rebuilds vLLM.
- Cutover (T032) is the only step that changes user-facing behavior; everything before it is dark-shipped behind `NOVIQ_INFER_ENGINE=llamacpp`.
- T017/T030 need a brief **maintenance window** — Qwen3-8B-AWQ won't fit alongside production's ~6.3 GB; stop `noviq-server` first to free VRAM.
- [P] = different files, no incomplete-task deps. Commit after each task or logical group.
