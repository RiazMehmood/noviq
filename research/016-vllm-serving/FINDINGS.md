# F016 — vLLM Concurrent Inference Serving: Research Findings

> Living research record for the vLLM cutover investigation on the IBA GPU VM.
> Structured as a technical report: methodology, experiments, measured data,
> analysis, and open questions. Updated chronologically as work proceeds.
>
> **Status as of 2026-06-28:** engine wired + VM-verified; A/B no-regression gate
> **passed** (quality); concurrency bottleneck **fully root-caused** (§7) and the best
> Ampere-compatible config **measured** (§5.6, §8): fp16-KV + chunked-prefill gives
> **p95 17.8 s, 2.56× baseline (SC-003 ✅)** but **SC-002 p95 ≤ 10 s is unachievable on
> this time-sliced vGPU** — vLLM still dominates llama.cpp on every axis. Cutover is a
> defensible *improvement* with a renegotiated latency target (§8); rollback is one env
> flip. Production currently on the 016 binary with `engine=vllm` for testing.

---

## 1. Abstract

We replace echoAI's serialized single-slot llama.cpp generator (`gen_gate =
Semaphore::new(1)`) with a vLLM continuous-batching sidecar to meet a hard
requirement of **10 concurrent users** answering grounded, cited legal questions
on a **12 GB time-sliced A40-12Q vGPU**. We prove CUDA-12 forward-compat on the
R470 driver without a host upgrade; verify the 8B-AWQ model fits; measure a
**5.6× engine-level throughput** gain over llama.cpp; and run a 30-question
no-regression A/B over the indexed corpus showing **no quality regression**.
Two deployment bugs were found and fixed (co-resident VRAM OOM; prompt+gen window
overflow). A residual finding: at the **full `/api/query` stack**, 10-concurrent
latency does **not** meet the p95 ≤ 10 s target — the bottleneck is vLLM's
concurrent-slot scheduling under a compute-saturated time-sliced vGPU, not
retrieval. Diagnosis and remaining levers are documented.

---

## 2. Problem & motivation

- Production `noviq-server` runs one in-process llama.cpp `ModelSlot` behind a
  width-1 semaphore (`gen_gate`). Generation is **fully serialized**: ~3–4 users
  at 4–6 s realistically; 10 concurrent users queue.
- **Measured baseline (llama.cpp GGUF, Qwen3-8B-Q4_K_M, via `/api/query`, 10
  concurrent):** 10/10 served, **p50 22.8 s, p95 45.5 s**, **13.2 req/min**,
  peak GPU 7.8 GB. Confirms the serialization cliff.
- Goal (spec SC-001..004): 10 concurrent served, **p95 ≤ 10 s**, **≥ 2.5×**
  throughput vs baseline, **0 OOM**.

---

## 3. Environment

| Component | Value |
|-----------|-------|
| GPU | NVIDIA A40-12Q — **time-sliced vGPU**, 12107 MiB, cap sm_86 |
| Host driver | R470.223.02 (native CUDA ≤ 11.4) |
| OS | Ubuntu 20.04.6 focal, x86_64 |
| CUDA-12 forward-compat | `cuda-compat-12-4` (libcuda.so.550) extracted locally, `LD_LIBRARY_PATH` prepend — **no host driver change** |
| Python | CPython 3.10 via `uv` (system 3.8 too old for vLLM ≥ 0.8.5) |
| vLLM | 0.8.5, pinned `transformers==4.51.3` / `tokenizers==0.21.4` (5.x breaks vLLM 0.8.5 tokenizer cache) |
| Inference model (vLLM) | `Qwen/Qwen3-8B-AWQ` (primary); `RedHatAI/Qwen3-8B-quantized.w4a16` (GPTQ, A/B alt) |
| Baseline model (llama.cpp) | Qwen3-8B-Q4_K_M GGUF |
| KV cache | fp8 (storage-only on Ampere; halves KV) |
| Provisioning | out-of-tree under `/opt/noviq/vllm` (survives `git reset`); scripts in `/opt/noviq/vllm-scripts/`; loopback systemd sidecar `noviq-vllm` on :8000 |

---

## 4. Architecture & methodology

- **Seam:** a `TextGenerator` trait (`services/inference/`) with a `VllmGenerator`
  (raw `/v1/completions`, `stream=true`, SSE→token, `tokio::select!` cancel). The
  smallest-diff wiring **branches at the generation call sites** on
  `engine_from_env()` (`NOVIQ_INFER_ENGINE`, default `llamacpp`); the existing
  direct `ModelSlot` path is the fallback (FR-017). The GGUF is **never loaded**
  under vLLM (no llama weights beside vLLM).
- `gen_gate` is **lifted** when `engine=vllm` (vLLM owns batching/admission).
- The hand-built ChatML + no-think prompt is sent **byte-verbatim** (no server-side
  chat-template re-render) so grounding/citation behaviour is identical above the
  trait.
- **Measurement harnesses** (`tools/vllm_eval/`): `load_test.sh` (N-concurrent
  `/api/query`, p50/p95/throughput/peak-GPU), `run_ab.py` (per-engine eval run +
  behavioural scoring + cross-engine compare). A correlated diagnostic
  (`diag_concurrency.sh`) samples vLLM `num_requests_running/waiting` during a
  burst.
- **Discipline:** decompose `/api/query` into components (embedding → pgvector →
  rerank → prompt/window build → vLLM generation → grounding/citation); form a
  hypothesis per component; confirm with a single targeted measurement before
  moving on.

---

## 5. Experiments & results

### 5.1 Spike 0 — CUDA-12 forward-compat (GATE)
`torch 2.4.1+cu121` reports `cuda.is_available()=True` + ran a real GPU matmul on
the A40-12Q via the extracted `cuda-compat-12-4`; `vllm 0.8.5` imports + serves.
**No IBA-IT driver upgrade required.** vLLM 0.8.5 served Qwen2.5-0.5B on :8009
alongside production (zero impact). **PASS.**

### 5.2 T015 — model fit
| Setting | Server state | vLLM resident | Free | Result |
|---------|-------------|--------------|------|--------|
| util 0.88, len 8192 | **stopped** | 11.2 / 12.1 GB | ~0.9 GB | fits; `/health` 200 |
| util 0.88, len 8192 | **running** (co-resident) | 10.5 GB + ~1 GB server | 254 MB | **CUDA OOM at first forward pass** (Bug 1) |
| util 0.72, len 8192/10240 | running | 9.3 GB | ~2.8 GB | stable |
| util 0.80, len 9216 | running | 10.2 GB | ~1.9 GB | stable |

### 5.3 Engine-level concurrency — direct `/v1/completions` (server stopped, util 0.88, len 8192)
Real ChatML legal prompt, 400 tokens.

| N | served | p50 | p95 | wall | throughput | peak GPU |
|---|--------|-----|-----|------|-----------|----------|
| 10 | 10/10 | 8086 ms | 8088 ms | 8.1 s | **74.1 req/min** | 11.4 GB |
| 20 | 20/20 | 9426 ms | 16410 ms | 16.4 s | 73.0 req/min | 11.4 GB |

vs llama.cpp baseline 13.2 req/min ⇒ **~5.6×** (SC-003 ≥ 2.5× ✅), p95 8.1 s ≤ 10 s
✅, 0 OOM ✅. **This is the vGPU's batching ceiling when it has the whole slice.**

### 5.4 A/B no-regression — 30 complex/ambiguous Qs from the indexed PDFs (FR-014)
Eval set `eval_set_sindh.json`: multi-hop cross-document, threshold-boundary,
ambiguous-scope, and refusal-trap (out-of-corpus + fabricated-section) questions
grounded in the 11 indexed Sindh/federal governance documents (10,208 chunks).
Scored behaviourally (citation presence, grounding-fallback, refusal correctness).

| Arm | answer-arm CITED | refuse-arm correct | fabrications | serial p50 / p95² | hard failures |
|-----|------------------|--------------------|--------------|-------------------|---------------|
| llama.cpp GGUF | 20/20 | 2/5 clean refuse | Q06 Punjab, Q14 tax, Q29 honorarium | 4.3 s / **22.2 s** | **5 (Q01/Q04/Q16/Q19/Q30) 200 s timeout** |
| vLLM AWQ | 19/20¹ | 2/5 clean refuse | Q06, Q14, Q29 (same) | 5.1 s / **9.4 s** | **5 (same Qs) timeout** |

¹ the one delta is Q05 (an *ambiguous* question) where AWQ refused/asked to
disambiguate instead of guessing — arguably better, not a regression.
² serial (1-at-a-time) A/B latency, timeouts excluded. **Notable:** even serial,
vLLM's p95 (9.4 s) beats llama.cpp's (22.2 s) — llama.cpp suffers per-query
context-rebuild spikes (Q02 127 s, Q17 57 s) while vLLM stays a steady 5–9 s. So
vLLM improves **consistency**, not just concurrency. Raw: `data/ab_compare.txt`.

**Verdict:** Both engines fabricate on the *same* out-of-corpus questions (where the
model has parametric knowledge: Punjab PDWP limit, Sindh sales-tax rate, councillor
honorarium) and correctly refuse the *truly* fabricated ones (no such SPPRA §47-B;
no specified penalty). Both fail the *same* 5 pathological questions. **No quality
regression from the engine swap.** The headline llama.cpp hallucination — Q06
inventing "Rs 10,000 million … NEC decision 24 May 2012, Annex-20" for Punjab with a
fake `[1]` — is reproduced, not worsened, by vLLM.

### 5.5 Full-stack concurrency — `/api/query`, 10 concurrent, co-resident
Early exploratory runs (cache state mixed). The 0.80 row was later re-confirmed clean.

| util | window | p50 | p95 | throughput | peak running (vLLM) | GPU util |
|------|--------|-----|-----|-----------|---------------------|----------|
| 0.72 | 10240 | 21.8 s | 28.6 s | 20.9 req/min | 4 | 95–98% |
| 0.80 | 9216 | **15.0 s** | 25.8 s | — | **6** | 95–98% |

### 5.6 Clean concurrency comparison — cache **OFF**, hardened harness, 10 concurrent
The rigorous run: `ECHO_QUERY_CACHE=0`, harness rejects FR-011/empty answers (HTTP 200
errors no longer scored as fast successes), all 10/10 verified real generations.

| KV / scheduler | engine | p50 | p95 | throughput | vs llama.cpp¹ | SC-002 (p95≤10s) | SC-003 (≥2.5×) |
|----------------|--------|-----|-----|-----------|---------------|------------------|----------------|
| **fp8 KV, no chunked prefill** | V0 | 15.0 s | **25.9 s** | 23.2 req/min | 1.76× | ❌ | ❌ |
| **fp16 KV, chunked prefill** | V1 | 14.0 s | **17.8 s** | **33.8 req/min** | **2.56×** | ❌ | **✅** |

¹ llama.cpp baseline 13.2 req/min. Enabling chunked prefill (only possible with fp16 KV
on Ampere — see §7) shrinks the second wave: **p95 −31% (25.9→17.8 s), throughput +46%**,
crossing the SC-003 2.5× gate. **SC-002 (p95 ≤ 10 s) is still missed (17.8 s)** — the
time-sliced vGPU cannot render 10 concurrent grounded 8B answers in ≤10 s; the engine-only
8 s (§5.3) required the *whole* slice + simultaneous arrival + fp8. fp16 cuts KV capacity
(worst-case concurrency 4.23x→1.43x for full-window) but 10 short legal queries still fit
(peak 10.5 GB, 0 OOM).

Both **FAIL SC-002** (p95 ≤ 10 s) and the 0.72 run **FAILS SC-003** (1.58× vs 2.5×).
Raising util 0.72→0.80 raised concurrent slots 4→6 and cut p50 21.8→15.0 s.

---

## 6. Bugs found & fixes

**Bug 1 — co-resident VRAM OOM.** The standalone fit test (util 0.88, server
*stopped*) was misleading. With `noviq-server` co-resident (~1 GB CUDA context),
util 0.88 left only 254 MB free → CUDA OOM on the 320 MB forward-pass activation at
the *first* inference (`/health` had passed). **Fix:** lower `gpu-memory-utilization`
to leave headroom for the co-resident process (0.72, then 0.80).

**Bug 2 — prompt+gen window overflow → vLLM 400.** The server budgets prompt +
completion to `NOVIQ_VLLM_MAX_MODEL_LEN` (8192) using *its* token estimate; vLLM
enforces the same sum with the *exact* Qwen tokenizer. On dense legal prompts the two
counts differ by a few hundred tokens, overflowing a zero-margin window → vLLM 400
("maximum context length is 8192 … you requested N"). Hit 5/30 large-context Qs.
**Fix:** decouple the served window from the budget — `run-vllm.sh` serves
`MAX_LEN + NOVIQ_VLLM_LEN_HEADROOM` (default 2048) so the mismatch can't overflow.
Cleared the 400s. *Proper long-term fix (follow-up): clamp `max_tokens` against the
real prompt token count in `vllm.rs`.*

**Bug 3 (pre-existing, engine-independent) — 5 pathological questions.**
Q01/Q04/Q16/Q19/Q30 hit a slow multi-pass path (grounding-retry / map-reduce):
**200 s timeout on llama.cpp**, 117–180 s timeout/500 on vLLM. They fail on *current
production* too. Not caused by the engine swap; tracked as a separate RAG-pipeline
defect.

---

## 7. Root-cause analysis — full-stack concurrency

**Component decomposition of `/api/query` (single request ≈ 6.3 s):**
- Generation dominates (~5–6 s for ~300 output tokens at this vGPU's ~50 tok/s);
  retrieval is small (~1 s). `query_log` records only total `latency_ms` (no split).
- **Embedder** (`services/embedder.rs`): a single global `Mutex<Option<TextEmbedding>>`
  held across the whole ONNX forward pass → embeddings serialize. But each query
  embedding is ~0.2–0.3 s, and `TextEmbedding::try_new` uses the **default (CPU)**
  execution provider (no CUDA provider configured), so embedding (a) is a minor
  latency contributor and (b) does **not** contend for the vGPU. **Secondary.**

**Primary bottleneck (measured, `diag_concurrency.sh`):** vLLM admits only **4
(util 0.72) → 6 (util 0.80)** concurrent sequences with the remainder *waiting*,
GPU pinned at **95–98%**. The 10 requests therefore process in **~2–3 waves**,
which fully accounts for p50 15–22 s.

**Why the cap is ~6, not KV:** 6 short requests (~650 tokens each = ~3,900 tokens)
use a tiny fraction of the ~39,000-token KV budget at util 0.80 — so **KV memory is
not the limiter at that point**. With the GPU compute-saturated on a *time-sliced*
vGPU slice, the ceiling is **compute/scheduler-bound**, not memory-bound. The
engine-level test reached 10-in-8 s only because the server was stopped, giving vLLM
the *entire* vGPU slice and util 0.88.

**RESOLVED — the ~6-slot cap mechanism (confirmed from vLLM config + a crash test):**

vLLM 0.8.5 initialises the **V0 engine** with **`chunked_prefill_enabled=False`** for
this AWQ/fp8 config. Without chunked prefill, a scheduler step is *either* all-prefill
*or* all-decode — it **cannot interleave** prefilling a new request with an in-progress
decode batch. Requests arrive **staggered** (natural user arrival + the serialized
embedder spreading retrieval completion over ~1–2 s); since one decode cycle is ~5–6 s,
the late arrivals land while the first batch is mid-decode and must **wait a full decode
cycle** before vLLM will run a prefill step for them. That is the measured 2-wave pattern
(`run=6 wait=4` held ~9 s, then the remaining 4), and it fully explains p50 15–22 s.
KV is *not* the limiter (6 short requests = ~3,900 of ~39,000 token-slots).

**The standard fix — `--enable-chunked-prefill` — is BLOCKED on this hardware.**
Enabling it crashed the engine on the first inference with a Triton compile error:
`ValueError: type fp8e4nv not supported in this architecture (supported: fp8e4b15,
fp8e5)`. The chunked-prefill attention kernel requires **fp8 *compute* (`fp8e4nv`)**,
which **Ampere sm_86 does not have** (FP8 on Ampere is *storage-only*, per spec/research
R-FP8) — and it collides with our `--kv-cache-dtype fp8`. So on the A40-12Q, chunked
prefill and fp8 KV are mutually exclusive.

**Measurement-integrity note:** `load_test.sh`/`diag_concurrency.sh` originally counted
HTTP 200 as success, but the FR-011 "service temporarily unavailable" message is *also*
HTTP 200 (with an error body). When the chunked-prefill crash took vLLM down, the harness
reported bogus ~2–3 s "successes". The harness was hardened to reject the unavailable
marker / empty answers; the trustworthy vLLM numbers (util 0.72 = 21.8 s, util 0.80 =
15.0 s) are slow real generations with sampled `num_running`, not error fast-fails.

---

## 8. Conclusions & decision

**The concurrency bottleneck is fully diagnosed and the best achievable config is measured.**

1. **Mechanism (resolved):** vLLM V0 + `chunked_prefill_enabled=False` cannot interleave
   prefill with an in-flight decode batch; staggered arrival ⇒ late requests wait a full
   ~5–6 s decode cycle ⇒ 2-wave processing ⇒ p95 25.9 s. (KV is *not* the limit.)
2. **Best Ampere-compatible config (measured §5.6):** `--enable-chunked-prefill` +
   `--kv-cache-dtype auto` (fp16) → vLLM V1 engine → **p95 17.8 s, 33.8 req/min, 2.56×
   baseline**. This is the recommended vLLM serving config on this card.
3. **Gate outcome vs the 10-user target:**
   | Gate | fp8/no-chunked | fp16/chunked | llama.cpp (today) |
   |------|----------------|--------------|-------------------|
   | quality no-regression (FR-014) | ✅ | ✅ | (baseline) |
   | SC-003 ≥ 2.5× throughput | ❌ 1.76× | **✅ 2.56×** | 1.0× |
   | SC-002 p95 ≤ 10 s | ❌ 25.9 s | ❌ **17.8 s** | ❌ 45.5 s |
   | SC-001 10/10 served, SC-004 0 OOM | ✅ | ✅ | ✅ (but serialized) |
4. **Hardware reality:** SC-002 (p95 ≤ 10 s @ 10 simultaneous users) is **not achievable**
   for a grounded 8B answer on this *time-sliced* A40-12Q slice — the engine-only 8 s
   required the whole slice, fp8, and simultaneous arrival. The slice's compute saturates
   (95–98%) at ~6 concurrent decodes. vLLM nonetheless **dominates llama.cpp on every axis**
   (p95 17.8 vs 45.5 s; throughput 2.56×; no gen_gate serialization; far better tail
   consistency — see §5.4).

**Recommendation (cutover, FR-014/SC-005):** quality gate **passed**; throughput gate
(SC-003) **passed** with the fp16/chunked config; the absolute-latency gate (SC-002)
**cannot be met on this vGPU** and should be **renegotiated to the hardware-realistic
floor (~18 s p95 @ 10 users)** rather than treated as a vLLM failure — vLLM is strictly
better than the llama.cpp status quo. Cutover is defensible *as an improvement* with that
caveat documented; rollback stays one env flip (`NOVIQ_INFER_ENGINE=llamacpp`).

**Residual follow-ups (independent of this decision):** (a) drop the embedder `Mutex` so
retrieval parallelises (clusters arrivals → helps any scheduler); (b) `vllm.rs` max_tokens
clamp (Bug 2 proper fix); (c) the pre-existing 5-question multi-pass pathology (Bug 3);
(d) GPTQ A/B arm; (e) a dedicated (non-time-sliced) GPU slice would change the SC-002 verdict.
5. **Follow-ups:** GPTQ A/B arm (download was re-kicked); `vllm.rs` max_tokens clamp
   (Bug 2 proper fix); the pre-existing 5-question multi-pass pathology (Bug 3).

---

## 9. Appendix — measurement provenance

- Baseline / arms: `/opt/noviq/vllm-scripts/{out_llamacpp.jsonl,out_awq.jsonl,
  baseline.log,loadtest_vllm.log,diag_conc.log}`.
- Eval set + scorer + harness: `tools/vllm_eval/{eval_set_sindh.json,run_ab.py,load_test.sh}`.
- Provisioning + launch: `deploy/vllm/{setup-vllm.sh,run-vllm.sh,noviq-vllm.service}`.
- Corpus: 11 indexed PDFs (Sindh Rules of Business 1986; SPPRA Rules 2010/2013;
  Sindh Public Procurement Act 2009; Sindh LG Act 2013 + Compendium; Manual for
  Development Projects 2019 PC-I; Delegation of Financial Powers 2019; SPSC
  Regulations 2023; Civil Servants APT Rules 1974; Constitution; Secretariat
  Instruction Manual 2010), 10,208 chunks.
- Ground-truth anchors used to author questions: DDWP ≤ Rs 2,000 M *iff* foreign aid
  < 25% (else → CDWP); CDWP ≤ Rs 10,000 M (beyond → ECNEC); DDSC barred from FEC/aid
  schemes; PDWP highest provincial body; SPPRA petty/RFQ/Rs 1 M advertising boundary;
  US$10 M ICB threshold; financial powers by officer Category I–IV.
