# Feature Specification: vLLM Concurrent Inference Serving

**Feature Branch**: `016-vllm-serving`
**Created**: 2026-06-27
**Status**: Draft
**Input**: User description: "vLLM concurrent inference serving for the Noviq legal AI server — replace the single serialized llama-cpp generation path with a vLLM continuous-batching engine so the server meets the 10-concurrent-user hard requirement on the 12 GB A40-12Q GPU, serving Qwen3-8B in AWQ/GPTQ with FP8 KV cache, while keeping the RAG retrieval, grounding, and citation pipeline unchanged."

## Overview

The Noviq legal AI server (feature 014) answers Government of Sindh legal questions
from an offline document corpus, grounded with citations. Today the server generates
every answer through a **single, serialized** in-process inference path: one model
held in one slot, one generation at a time (a generation gate of width 1). Under load
this serves only ~3–4 simultaneous users at ~4–6 s each before the rest queue behind
them; a fourth or fifth user can wait tens of seconds. The product has a **hard
requirement of at least 10 concurrent users**.

This feature replaces the server-side generation engine with one that serves many
users **at the same time** (continuous batching), keeping everything else — retrieval,
prompt construction, grounding checks, citation gating, the browser experience, and
fully-offline operation — unchanged. Answer **quality must not regress**; only
**concurrency and tail latency** improve.

This is a **server-only** change. The desktop application is out of scope and keeps
its current single-stream engine.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Ten officers ask questions at the same time (Priority: P1)

During a working session, ten legal officers in different offices each open the
browser client and submit a legal question within the same short window. Every one of
them receives a grounded, cited answer in a reasonable time — nobody is stuck waiting
in a long queue behind the others.

**Why this priority**: This is the entire reason for the feature and the stated hard
requirement. Without it the platform cannot be rolled out beyond a handful of
simultaneous users. It is the MVP — if only this ships, the product is viable.

**Independent Test**: Drive 10 simultaneous question requests (distinct users) at the
server and confirm all 10 return correct, cited answers with tail (p95) latency within
the agreed bound, and that GPU memory never exceeds the card's capacity (no
out-of-memory failures).

**Acceptance Scenarios**:

1. **Given** 10 authenticated users, **When** they each submit a typical legal lookup
   within the same 5-second window, **Then** all 10 receive a grounded answer with at
   least one citation and none is rejected or errored due to capacity.
2. **Given** the same 10-user burst, **When** latency is measured per request, **Then**
   p95 end-to-end latency stays within the agreed target and p50 is comparable to the
   current single-user latency.
3. **Given** sustained 10-user load over several minutes, **When** GPU memory is
   monitored, **Then** it remains within the 12 GB budget with no OOM and no crash.

---

### User Story 2 - Answers stream and stay grounded under load (Priority: P2)

A user submits a question and sees the answer begin to appear quickly (streamed),
with the same grounding and citations as today — even while other users are being
served simultaneously. The "working…" indicator and progressive display behave as
before.

**Why this priority**: Streaming and grounded citations are existing, expected
behaviors. Concurrency must not silently drop them. High value but secondary to simply
serving 10 users at all.

**Independent Test**: With several concurrent requests in flight, confirm a given
request still streams tokens progressively and the final answer carries valid `[n]`
citations that pass the existing grounding/citation checks.

**Acceptance Scenarios**:

1. **Given** other requests are being served concurrently, **When** a user submits a
   question, **Then** the answer streams progressively (first content visible well
   before completion) and ends with valid citations.
2. **Given** an answer with no supporting evidence in the corpus, **When** it is
   generated under load, **Then** the system still returns the explicit
   "cannot be determined from the available documents" response rather than a
   fabricated answer.

---

### User Story 3 - Operator can deploy, observe, and recover the inference engine (Priority: P3)

An operator deploys a new build, restarts the box, or recovers from a fault, and the
inference engine comes back automatically with the correct model and settings. The
operator can see whether the engine is healthy and how loaded it is, using the same
deploy/observability flow as the rest of the server.

**Why this priority**: Operability keeps the system alive in production, but the
feature's core value (US1) can be demonstrated before full operability polish.

**Independent Test**: Restart the host; confirm the inference engine auto-starts,
loads the correct model, and the server reports healthy. Trigger a deploy; confirm the
engine continues serving (or restarts cleanly) without manual intervention.

**Acceptance Scenarios**:

1. **Given** a host reboot, **When** the system comes up, **Then** the inference engine
   starts automatically and the server health endpoint reports the engine ready.
2. **Given** a routine deploy, **When** it completes, **Then** the inference engine is
   running the intended model/settings and users can ask questions without an operator
   manually starting anything.
3. **Given** the inference engine is unavailable, **When** a user submits a question,
   **Then** the system returns a clear, friendly "service temporarily unavailable"
   message (or a fallback answer) rather than a raw error or an indefinite hang.

---

### Edge Cases

- **Capacity exceeded**: an 11th+ concurrent request arrives while the engine is at
  full batch — it should be admitted to a short queue and served, or rejected with a
  clear retry message, never silently dropped or allowed to trigger OOM.
- **Oversized context**: a question whose retrieved context exceeds the configured
  maximum context length — the context must be bounded/truncated by the existing budget
  logic so it never overflows the engine's limit.
- **Engine crash mid-stream**: the inference engine dies while streaming an answer —
  the user sees a graceful error and can retry; the engine recovers automatically.
- **Cold start**: the first request after a restart pays a model-load cost — this must
  not be mistaken for a per-request latency and must not error the first user.
- **Model/quantization mismatch**: the configured model file is missing or in an
  unsupported format — the server must fail fast with a clear operator-facing message,
  not serve wrong/garbage answers.
- **Quality regression**: the quantized (AWQ/GPTQ) model produces materially worse
  legal answers than the current path — this must be caught by an A/B quality check
  before cutover, with the option to hold on the current engine.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The server MUST generate chat answers through an inference engine that
  serves multiple concurrent requests simultaneously (continuous batching), replacing
  the current single-at-a-time serialized generation for the server deployment.
- **FR-002**: The system MUST support **at least 10 concurrent users** each receiving a
  grounded, cited answer without capacity-induced failures, within the GPU memory
  budget.
- **FR-003**: The system MUST preserve the existing answer pipeline unchanged around
  generation: hybrid retrieval, prompt construction, no-think handling, grounding
  validation, and citation gating. Only the generation step changes.
- **FR-004**: Generated answers MUST remain grounded and carry valid citations; the
  system MUST still emit the explicit "cannot be determined from the available
  documents" refusal when evidence is insufficient.
- **FR-005**: Answer generation MUST stream progressively to the browser (preserving
  the existing progressive-display / "working…" experience) while other requests are
  served concurrently.
- **FR-006**: The system MUST run **fully offline** — no external model or API calls;
  the model and runtime are provisioned locally on the host.
- **FR-007**: The inference engine MUST be reachable only from the host itself
  (loopback / local socket); it MUST NOT expose a network-accessible inference port.
- **FR-008**: The system MUST operate within the **~12 GB GPU memory budget** of the
  A40-12Q, accounting for both model weights and per-user working memory across the
  full concurrent load, with no out-of-memory failures under the target load.
- **FR-009**: The system MUST serve the Qwen3-8B model in a GPU-optimized quantized
  form suitable for the chosen engine; it MUST NOT require model retraining or
  fine-tuning. Both **AWQ and GPTQ** 4-bit builds MUST be evaluated (quality + latency)
  and the better-performing one selected as the launch format; the loser is dropped.
- **FR-010**: The system MUST bound the maximum context length per request so that
  total memory across the concurrent load stays within budget while meeting the latency
  target (FR-016). The cap MUST remain large enough for current legal RAG contexts —
  **at least ~6k tokens** — with the exact value tuned in planning against the 12 GB +
  ≤10 s p95 envelope.
- **FR-016**: Under the 10-concurrent-user burst, **p95 end-to-end latency MUST be
  ≤ 10 s** and p50 MUST be no worse than the current single-user latency (~5 s).
- **FR-011**: When the inference engine is unavailable, starting, or overloaded, the
  system MUST return a clear, user-friendly message (or a defined fallback) rather than
  a raw error or an indefinite hang.
- **FR-012**: The inference engine MUST start automatically on host boot and after
  deploys, loading the intended model and settings without manual operator steps, and
  integrate with the existing deploy flow.
- **FR-013**: The server health/status surface MUST report whether the inference engine
  is ready and expose enough signal (e.g., readiness, current load/queue) for an
  operator to diagnose problems.
- **FR-014**: Before cutover, the system MUST be validated for **no answer-quality
  regression** versus the current engine on a representative set of legal questions; if
  a material regression is found, cutover MUST be held.
- **FR-017**: The current in-process llama.cpp generation path MUST be **retained as a
  config-switchable fallback** after cutover, so a vLLM outage or quality regression can
  be reverted to the known-good engine without a redeploy.
- **FR-015**: The design MUST NOT preclude later horizontal scale-out (more replicas /
  more GPU) even though launch targets a single GPU.

### Key Entities *(include if feature involves data)*

- **Inference Engine**: the local service that loads the model and produces tokens;
  serves many requests concurrently; has a memory budget, a max-context limit, a
  readiness state, and a current load.
- **Served Model**: the Qwen3-8B legal chat model in its GPU-optimized quantized form;
  has a format, a file location on the host, and a memory footprint.
- **Generation Request**: one user turn handed to the engine — carries the constructed
  prompt, generation limits, and streaming preference; maps 1:1 to a chat turn.
- **Concurrency/Queue State**: how many requests are in flight vs. queued; drives
  admission, fallback, and the capacity-exceeded behavior.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With **10 users** submitting typical legal questions within the same
  5-second window, **100% receive a grounded, cited answer** with no capacity failures.
- **SC-002**: Under the 10-user burst, **p95 end-to-end latency is ≤ 10 s** and p50
  latency is no worse than the current single-user latency (~5 s).
- **SC-003**: **Throughput improves by at least 2.5×** in served requests-per-minute
  versus the current serialized path at 10 concurrent users (the current path is
  effectively flat under concurrency).
- **SC-004**: **GPU memory stays within 12 GB** across the full 10-user sustained load
  with **zero out-of-memory events** over a multi-minute soak.
- **SC-005**: **No answer-quality regression**: on a representative legal question set,
  the new engine matches or exceeds the current engine on grounding/citation correctness
  (no increase in hallucinated or missing citations).
- **SC-006**: Answers **stream** — first content is visible to the user in well under
  the full generation time — even while other requests are being served.
- **SC-007**: The platform remains **fully offline** — verified zero external network
  calls during inference.
- **SC-008**: After a host reboot or deploy, the inference engine is **serving again
  with no manual steps**, and the health surface reports it ready.

## Assumptions

- The existing RAG retrieval, prompt building, grounding, and citation logic are correct
  and reused as-is; this feature swaps only the generation engine.
- The corpus and PostgreSQL/pgvector retrieval store are unchanged.
- Qwen3-8B (already the chosen chat model, with thinking disabled) remains the model;
  only its serving format changes to a GPU-optimized quantization.
- A pre-quantized AWQ/GPTQ Qwen3-8B is obtainable for one-time offline provisioning; no
  retraining is required.
- "10 concurrent users" means 10 in-flight question turns; bursts slightly above that
  are handled by short queuing, not by guaranteeing unlimited parallelism.
- The target GPU remains the single A40-12Q ~12 GB slice at launch.

## Dependencies & Risks

- **CUDA / driver compatibility (HIGH)**: the host driver is 470 (CUDA toolkit ≤ 11.4);
  modern vLLM builds typically require newer CUDA. Resolving this (driver/toolkit
  upgrade, a containerized runtime, or a vLLM version compatible with the available
  CUDA) is a hard prerequisite and the largest risk to delivery. To be settled in
  planning/research.
- **Memory headroom (MEDIUM)**: 12 GB is tight; meeting 10 users depends on the
  quantization + reduced-precision working memory + context cap all landing within
  budget. Requires empirical VRAM validation.
- **Quality of quantized model (MEDIUM)**: 4-bit GPU quantization could shift legal
  answer quality; mitigated by the FR-014 A/B gate before cutover.
- **Operational footprint (LOW-MEDIUM)**: introducing a separate runtime alongside the
  existing server adds deploy/observability surface; mitigated by auto-start + health
  integration (FR-012/013).

## Out of Scope (Non-Goals)

- The desktop application (keeps its current single-stream engine).
- Model fine-tuning, a domain reranker, GraphRAG changes, the evaluation framework, and
  caching strategy — these are separate items in the architecture brief.
- Any change to the PostgreSQL/pgvector retrieval corpus or schema.
- Multi-GPU / distributed inference at launch (must remain possible later — FR-015).
- The public ingress / VPN / tunnel layer (unchanged from feature 014).
