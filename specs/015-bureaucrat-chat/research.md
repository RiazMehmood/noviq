# Phase 0 Research: Unified Bureaucrat Chat Assistant

Resolves the unknowns in plan.md Technical Context. Each item: Decision · Rationale · Alternatives.
Anchored to the existing codebase (verified paths in parentheses).

## R1 — Dynamic `n_ctx` from the model + RAM/VRAM clamp

- **Decision**: At model load, read the GGUF trained context length from `llama-cpp-2` model metadata (the `*.context_length` metadata key, exposed via `LlamaModel::meta_val_str` / model metadata API; fall back to a conservative 4096 if absent). Compute the affordable window from a KV-cache RAM estimate `kv_bytes ≈ n_ctx · n_layers · 2 · n_embd_kv · bytes_per_elem` against a budget fraction of available RAM/VRAM from `hardware_detector`. Set `n_ctx = clamp(min(trained, affordable), 1024, trained)`. `ECHO_N_CTX` overrides outright (clamped to `[1024, trained]`).
- **Rationale**: Replaces the hardcoded per-call-site `n_ctx` (`llm.rs` 2048 analyze / 1024 chat / `query_router` 3584). A swapped-in larger-window model now actually delivers its window; small machines stay safe (Constitution VII). KV-cache RAM — not the model's window — is the true ceiling.
- **Alternatives**: keep fixed `n_ctx` (wastes big models, risks overflow — rejected); trust a config value (drifts from the actual model — rejected).

## R2 — Real-token budgeting (not chars/4)

- **Decision**: Count tokens with the loaded model's tokenizer (`model.str_to_token`, already used in `llm.rs`) for each prompt component during allocation. Cache per-component token counts within a turn. The allocator sums components and asserts `≤ n_ctx − gen_reserve` before building the final prompt.
- **Rationale**: The current `chars/4` estimate (`query_router.rs:861`) is what lets the assembled context overflow the prompt cap and trigger the `llm.rs` head+tail truncation. Real counts make the budget exact → truncation never fires (FR-011).
- **Alternatives**: keep chars/4 (imprecise → overflow — rejected); a separate tokenizer lib (model mismatch — rejected; reuse the loaded model's tokenizer).

## R3 — Intent classification / dispatch

- **Decision**: Deterministic-first classifier in `intent_router` — keyword/marker + lightweight regex over the user turn maps to a capability (guidance · draft · summarize-doc · compare · conflict · amendment-chain · action-items), with a fallback to `guidance`. Document-scoped capabilities parse named documents; if absent/ambiguous → confirm before proceeding (FR-005e). No LLM call for routing (Constitution II — cheapest tier first).
- **Rationale**: Routing must be cheap, deterministic, and predictable so the citation gate / context guarantees are always engaged on the right path; a misroute still cannot fabricate (gate covers it). Reuses the existing `intent_router::decompose` slot/marker machinery.
- **Alternatives**: LLM-based intent classification (cost, nondeterminism — rejected as primary; may assist only on low-confidence fallback later).

## R4 — Map-reduce orchestration within the single model slot

- **Decision**: A sequential map-reduce driver: split the in-scope provisions into window-sized atomic batches (no provision split), run an extract pass per batch on the one resident model (Constitution III — never concurrent), accumulate per-batch grounded extracts, then a final reduce/synthesis pass. Each pass obeys the allocator. Progress is streamed (reuse the existing progress-event channel pattern from F006/one-ask).
- **Rationale**: Whole-document/cross-document capabilities (summarize a book, compare, conflict) cannot fit one window; map-reduce reads everything without truncation (FR-005d, FR-016). Sequential reuse honors the single-slot RAM budget.
- **Alternatives**: bigger context only (fails on huge docs / small machines — rejected); parallel model instances (violates Constitution III — rejected).

## R5 — Pinned case-fact extraction

- **Decision**: Deterministic-leaning extraction of load-bearing situational facts (grade/BPS, length of service, leave type, designation, etc.) via labelled patterns over user turns, stored as structured `pinned_case_fact` rows; surfaced back to the user for implicit confirmation (they appear in the tailored answer). A later restatement supersedes. Kept verbatim in an always-present allocator slot (FR-018).
- **Rationale**: These specifics drive citation-grade guidance and must not be paraphrased away by summarization. Structured + verbatim mirrors the citation discipline for the user's own facts.
- **Alternatives**: rely on the prose conversation summary (lossy — rejected); LLM fact-extraction only (confabulation/format drift — deterministic patterns first, LLM assist later).

## R6 — Amendment-chain data source (deterministic)

- **Decision**: Build the chain from existing structured provenance — `amended_by` graph edges (`db/repositories/graph.rs` edge types include `amended_by`) plus `footnote_refs` (`constitution_ir.rs`) and the F013 amendment-history footnote chunks — ordered oldest→recent by the amending instrument's date. The LLM only narrates the already-ordered, already-cited chain (FR-005f). No model call produces order/dates/instruments.
- **Rationale**: Amendment order is the highest-confabulation-risk surface; it must be structural (Constitution XII). F013 already captures the provenance.
- **Alternatives**: LLM-synthesized chain (rejected — confabulation breaks citation-grade); on-the-fly re-derivation (rejected — Constitution XIII says use the graph).

## R7 — Thinking-model handling

- **Decision**: Detect and strip reasoning spans (`<think>…</think>` and known reasoning delimiters) from (a) the user-facing answer and (b) the citation-gate input, before gating and display. Applied generically at the generation boundary in `llm.rs` so any reasoning model swapped in is handled.
- **Rationale**: The user explicitly wants to test reasoning models; their chain-of-thought must not leak to the officer or pollute the verbatim citation check (FR-009).
- **Alternatives**: per-model config (brittle across hot-swaps — rejected); leave thinking in (leaks to UI + breaks gate — rejected).

## R8 — Conflict detection (candidates only)

- **Decision**: Topic-align provisions across the named documents (embedding similarity over the same retrieval surface), then a per-pair contradiction pass that emits a **candidate** conflict ONLY when both provisions are present and cited; output is framed "potential conflict — review", never an assertion (FR-005g). No conflict is emitted without both grounded citations.
- **Rationale**: Semantic contradiction is genuinely hard; over-claiming breaks trust. Human-judged cited candidates preserve the citation-grade contract.
- **Alternatives**: authoritative conflict verdicts (rejected — confabulation/over-claim risk); pure keyword contradiction (low recall/precision — rejected as sole method).

## R9 — Persona & per-capability prompts as config (Constitution VI)

- **Decision**: Define the bureaucrat persona and per-capability prompt fragments in a TOML file (`config/personas/bureaucrat.toml`); load at runtime. Keep a compact persona variant for small windows.
- **Rationale**: Constitution VI requires persona-as-config, not hardcoded; also lets prompt tuning happen without a rebuild.
- **Alternatives**: hardcoded const strings (violates VI — rejected).

## R10 — Retiring the drafting subsystem safely

- **Decision**: Extract the `[CITE:n]` verbatim gate from `services/drafting/one_ask/verify.rs` into `services/assistant/citation_gate.rs`; remove the slot/template/docset/gap/finalize flow from the user-facing path (commands + frontend), but keep the extracted gate and the export helper (`services/drafting/export.rs`). Old tables remain (additive policy); they are simply unused.
- **Rationale**: Smallest viable diff + don't lose the proven gate; the additive-migration rule forbids table drops.
- **Alternatives**: delete drafting wholesale (loses the gate, risks regressions — rejected); keep both paths (defeats the merge — rejected).
