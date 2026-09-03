# Phase 0 — Research

**Feature:** 007 Pakistan Government Legal RAG
**Date:** 2026-06-02
**Status:** All Technical Context unknowns resolved.

This research phase confirms the technical choices recorded in plan.md against
the spec's success criteria and the constitution's gates. Every "Decision"
below was already explored during brainstorming and Feature 006's
implementation; this document records the chosen path and the rejected
alternatives so the plan does not silently re-litigate them.

---

## R1 — Default offline LLM tier composition

**Decision:** Two locally-resident model classes, time-shared in a single
`ModelSlot` per constitution §III:

- **Orchestrator / small-class:** Qwen2.5-0.5B-FT (fine-tuned) and
  Qwen2.5-3B-Instruct, GGUF Q4_K_M. Used for intent classification tiebreak,
  summarization, action-items extraction, and citation rendering helpers.
- **Worker / mid-class:** Mistral-7B-Instruct-v0.3, GGUF Q4_K_M. Used for IR
  build during Deep re-index, procedure extraction, and the harder intents
  (Difference, CrossCompare).

**Rationale:** Memory (project_echoai_main_llm.md) and Feature 006 work
already validated this stack. Mistral 7B successfully produced legal IR on
chunked Sindh constitution input via llguidance grammar-constrained
sampling. Qwen2.5-3B is the prevailing "orchestrator" model across the
codebase; Qwen2.5-0.5B-FT (fine-tuned) is the cheapest fast-path option for
non-IR LLM work. All three are already on disk under `models/`.

**Alternatives considered:**

- *Llama-3.2-3B as orchestrator.* Rejected: not on disk, no fine-tune, and
  no project-internal eval data versus Qwen2.5-3B.
- *Pure single-model (Mistral 7B for everything).* Rejected: blows the
  fast-tier intent latency budget SC-011 on a CPU-class workstation; loads
  the 7B unnecessarily for trivial intents.

---

## R2 — Cloud LLM access pattern

**Decision:** Groq and Cerebras providers stay wired through the existing
`llm.rs` plugin layer, but production code paths default to offline. An
explicit `--cloud` flag on the chat box (officer-initiated) is the only
non-eval way to dispatch a query to cloud. Cloud calls happen only when
`LlmTier::CloudEval` is requested explicitly — never as a silent fallback.

**Rationale:** Spec FR-027/028/029 and constitution §I together permit
user-explicit cloud calls but forbid silent ones. The existing 22-prompt
eval already exercises Groq llama-3.3-70b at K=20 with 80% OK as the
recorded ceiling; we keep that capability for regression baselines.

**Alternatives considered:**

- *Remove cloud providers entirely.* Rejected: we lose the eval baseline
  and the officer's "give me the best possible answer right now" escape
  hatch. Constitution §I already accommodates explicit-opt-in cloud.
- *Default to cloud, fall back to offline.* Rejected: directly violates
  §I and FR-027.

---

## R3 — Grammar-constrained IR generation

**Decision:** Use `llama-cpp-2`'s `llguidance` sub-feature to enforce the
legal IR JSON schema during Mistral generation, with a six-shape tolerant
parser (six expected envelope shapes × strict-serde / json5 lenient) for
fallback decoding. Failure dumps land under `/tmp/echo-ai-debug/`.

**Rationale:** Memory + 006's eight-round iteration discovered that pure
free-text mode produced unparsable output ~30% of the time on Mistral 7B
even with strict prompts; llguidance brings that to near-zero by
constraining the sampler. The tolerant parser absorbs the residual noise
(literal-newline-in-strings, unknown enum variants → Paragraph fallback,
Mistral `[INST]` artifacts).

**Alternatives considered:**

- *Free-text + retry loop (006 round-1 approach).* Rejected: rejected
  during 006 iteration after empirically measuring the failure rate.
- *Outlines / Guidance external tool.* Rejected: would add a Python
  process boundary, violating constitution §VI's "no Python orchestration
  frameworks" rule.

---

## R4 — Two-tier indexing (fast default + deep on demand)

**Decision:** Documents land in the fast tier by default through the
existing 004/005 pipeline. The "Deep re-index with Mistral" command runs
the legal IR pipeline against the source PDF, atomically replaces the
document's chunks/embeddings/graph_nodes, and persists the JSON IR
sidecar. Sticky tier: re-uploading the same content (sha256 hit) re-uses
the cached IR and stays deep.

**Rationale:** Mistral IR build is ~4–6 min/chunk on a CPU-class machine;
running it on every upload would violate SC-001 (60 s fast index) and
make the product unusable. Officers selectively pay the cost on the
documents that need it. The sha256 cache pattern is already proven by
Feature 006's JSON sidecar handling.

**Alternatives considered:**

- *Always-deep indexing.* Rejected: blows SC-001 and SC-011, makes the
  product unusable for casual exploration.
- *Always-fast (no deep tier).* Rejected: forecloses the intents that
  give the product its officer-grade value (Difference, Timeline,
  Procedure).
- *Auto-upgrade on first deep-required query.* Rejected: surprises the
  officer with a multi-minute background job they did not consent to.

---

## R5 — Section-boundary chunking on the deep tier

**Decision:** On Deep re-index, chunks are derived from legal IR nodes
rather than from page boundaries — one chunk per Section (with optional
sub-section / clause merging when the section is short). Each chunk
carries `instrument_id`, `legal_path`, and an in-memory `node_id` link
back to `graph_nodes`.

**Rationale:** Section-boundary chunking is what makes officer-grade
citations precise (FR-022 / SC-004). Page-boundary chunking is fine for
the fast tier but loses hierarchical structure mid-Section.

**Alternatives considered:**

- *Keep page-boundary chunking even on deep tier.* Rejected: defeats the
  purpose of deep upgrade.
- *Sub-section as the chunk unit.* Rejected: over-fragments. Many
  sections have meaningful inter-clause context that should stay
  together at retrieval time.

---

## R6 — Amendment marker parser

**Decision:** Deterministic regex pass running after IR build. Patterns
matched in v1 (widened iteratively during Slice A as we re-index real
PDFs): `X[…]`, `[Substituted by Act … of …]`, `[Subs. by Act … of …]`,
`[Inserted by Act … of …]`, `[Ins. by …]`, `[Omitted by …]`, `[Repealed
by …]`. Unrecognized markers log a `kind = 'unparsed'` event with the raw
text preserved (per spec edge case + Principle XII).

**Rationale:** Markers are a deterministic syntactic feature — no LLM
needed. The 006 Sindh constitution work already proved that ≥ 90% of
markers in published Pakistan instruments use one of those forms.

**Alternatives considered:**

- *LLM-driven amendment extraction.* Rejected: deterministic pass is
  faster, cheaper, and easier to debug.
- *Strict pattern set, fail document on unknown marker.* Rejected:
  violates the spec's tolerance requirement and would block the entire
  Sindh corpus on any one unusual variant.

---

## R7 — Cross-reference edge extraction

**Decision:** Two-stage. Stage 1 — rule-based phrase parser scans every
node's text for known patterns ("section X of the Y Act, YYYY", "Rule X
of the Y Rules, YYYY", "under section X of …", "as amended by Act X of
YYYY"). Stage 2 — small-LLM disambiguator resolves the matched phrase to
a concrete `dst_node_id` by querying the existing graph. Each edge
carries a `confidence` (1.0 for unambiguous regex matches, < 1.0 for
LLM-resolved).

**Rationale:** Regex catches the obvious majority cheaply; the LLM only
runs on residue where multiple candidate destinations exist (e.g., two
Acts with the same short title). This keeps Slice C's offline-mid
budget reasonable.

**Alternatives considered:**

- *Pure LLM extraction.* Rejected: too expensive per node, hard to debug
  edge precision in the 100-edge eval gate.
- *Pure regex, no disambiguator.* Rejected: high false-positive rate on
  short-title collisions; would fail Slice C's ≥ 85% precision gate.

---

## R8 — Procedure extraction (Slice D)

**Decision:** Two-stage similar to xref. Stage 1 — heuristic
procedural-section classifier (title keywords: "procedure", "manner of",
"application for", "grant of", "approval of"; structural cues:
nested-list-heavy sections with imperative verbs and actor nouns).
Stage 2 — grammar-constrained Mistral 7B extraction emitting
`ProcessStep[]` with actor, action, condition, branch_of, timeline,
source_node_id. Cross-instrument stitching uses `graph_edges`
(`implements`, `under`).

**Rationale:** Heuristic classification avoids running Mistral on every
single node. Grammar-constrained extraction has the same correctness
profile as the IR build (006-proven). Cross-instrument stitching reuses
the Slice C graph instead of re-deriving relationships.

**Alternatives considered:**

- *Pure LLM classification of every node.* Rejected: too expensive at
  ~50k chunks-per-machine scale (SC-012).
- *Hard-coded department-by-department procedure templates.* Rejected:
  brittle, doesn't generalize, and the spec requires deriving procedures
  from the source instruments.

---

## R9 — Intent classifier

**Decision:** Rule-first classifier covering ~80% of officer queries
through regex/keyword patterns matched to each intent (e.g.,
`"^(show|give|read)\s+(me\s+)?(section|article|rule)\s+\d"` → Lookup;
`"\b(difference|differ|changed|amendment)\b"` → Difference;
`"\b(procedure|approval flow|process flow|approval path)\b"` →
Procedure; etc.). Tiebreak on low confidence dispatches to Qwen2.5-0.5B
with a grammar-constrained enum output. Fallback intent handles
genuinely-ambiguous queries with a "did you mean ..." chip surface.

**Rationale:** Rule-first handles the bulk of officer phrasing
cheaply (sub-50 ms) and stays on the small-LLM tier. Fits SC-011 fast-tier
latency budgets even on Tier-1 hardware.

**Alternatives considered:**

- *LLM-only classifier.* Rejected: latency hit on every query
  unnecessary when the majority of phrasing is patternable.
- *Embedding-similarity-to-prototype intents.* Rejected: harder to debug
  than regex when officers report misclassification; less explainable.

---

## R10 — Single canonical citation format

**Decision:** All citation rendering routes through
`services::citation::LegalCitation::format_*`. Two public methods:
`format_officer()` (long form for chat bubbles, "see also", exports) and
`format_short()` (inline pill form). Both implemented in Slice 0 with 12
golden tests; no other module in the codebase produces citation strings.

**Rationale:** Constitution §XI (NON-NEGOTIABLE) demands one canonical
format. Centralization makes future changes (e.g., adding parallel
citations for lawyers) a single-file edit.

**Alternatives considered:**

- *Per-handler citation rendering.* Rejected: drift inevitable; spec
  FR-024 explicitly forbids.

---

## R11 — Quality-floor models on small-class

**Decision:** Qwen2.5-0.5B-FT is acceptable as the fast-tier
classification/citation-helper model. Qwen2.5-3B is the floor for
summarization and action-items extraction. Mistral 7B is the floor for
IR build, procedure extraction, Difference, CrossCompare.

**Rationale:** Memory: 22-prompt eval put 0.5B-FT at 1/22 OK on hard
queries vs Groq 16/22. The 0.5B is fine for classification (one-of-N
enums under grammar) but cannot carry harder generative tasks alone.
3B + 7B together can; both already on disk.

**Alternatives considered:**

- *Make 0.5B the default for everything.* Rejected: quality cliff
  documented in eval.
- *Make 7B the default for everything.* Rejected: latency cliff (SC-011)
  and violates constitution §III's bounded-RAM intent.

---

## R12 — Officer eval set construction

**Decision:** Build a 70-prompt officer eval set incrementally across
Slices B/C/D, anchored on the Sindh Local Government Act 2013, the
Sindh RTI Act 2017, and Sindh LG Rules. Each intent gets 10 prompts.
Eval runs on offline-small, offline-mid, and cloud (Groq) baselines and
the results are archived under `specs/007-legal-rag-pakistan/eval/`.

**Rationale:** Memory: prior 20-prompt set on the Sindh constitution
proved 80% OK on cloud is the realistic ceiling. The 70-prompt set
extends that pattern to the eight intents.

**Alternatives considered:**

- *Use the existing 20-prompt set as-is.* Rejected: it's constitution-
  only and doesn't exercise the eight intents.
- *Larger eval set (200+ prompts).* Rejected: diminishing returns
  before Slice D ships.

---

## R13 — Existing-engine inheritance

**Decision:** The 006 engine (`mistral_builder`, `progress_store`,
llguidance binding, classifier scaffold, JSON sidecar cache,
cancel-token registry) is inherited as-is. `legal_ir/` re-parameterizes
`mistral_builder` on the new schema rather than forking it.

**Rationale:** Avoids duplicate code; lets the eventual 006 full-doc
verification roll forward to 007 automatically.

**Alternatives considered:**

- *Fork the 006 engine into 007.* Rejected: doubles maintenance.
- *Wait for 006 to merge cleanly to master before starting 007.*
  Rejected per user decision (see CHANGELOG 2026-06-02): 006 is
  "finished enough" for 007 to build on its branch state.

---

## Outstanding (deferred to planning phase)

These were marked "low impact" during `/sp.clarify` and are now resolved
inline as plan-level decisions:

- **Fast-index UX progress feedback.** Existing per-document spinner
  badge stays; on Deep re-index, the dedicated `DeepReindexProgress`
  component renders a per-pass progress bar (5 passes — IR build,
  amendment_parser, xref_extractor, procedure_extractor, re-embed). No
  spec change required.
- **JSON sidecar input format.** Officers may drop a hand-crafted IR
  JSON sidecar alongside the PDF (same `{app_data_dir}/ir_cache/<sha>.json`
  path the deep pipeline writes). Detection happens at upload time; if
  the sidecar parses against the legal-IR schema, the document boots
  directly into the deep tier (no Mistral run). This is purely additive
  and lands as part of Slice A.
