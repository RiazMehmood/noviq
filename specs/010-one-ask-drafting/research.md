# Research: One-Ask Drafting (010)

**Date**: 2026-06-11 · **Spec**: [spec.md](spec.md)

All Technical Context unknowns resolved below. Each entry: Decision / Rationale / Alternatives considered.

## R1. Whole-corpus retrieval: vec0 KNN MATCH form (FR-014, SC-005)

**Decision**: Replace brute-force `vec_distance_cosine(...) ORDER BY ... LIMIT` scans in the drafting path with sqlite-vec's indexed KNN form, two-step:

1. KNN over the virtual table only: `SELECT chunk_id, distance FROM chunk_embeddings WHERE embedding MATCH ?1 AND k = ?2` (over-fetch, k = 4× the post-filter need).
2. Hydrate + filter in a second query over `chunks`/`sources` (`indexing_status='indexed'`, under a watched folder), preserving KNN order.

**Rationale**: Live measurement 2026-06-11: the brute-force form took **281 s** for one proposal call on the 65 GB production DB (log `dev-20260611-121047.log`). `vec_distance_cosine` as a scalar in ORDER BY forces a full-table scan + JSON-embedding decode per row; the `MATCH ?/k=?` form is the only path that uses vec0's KNN machinery. Joins can't be pushed into a vec0 KNN query, hence two-step with over-fetch.

**Caveat (verify at implementation)**: `chunk_embeddings` is declared `vec0(chunk_id TEXT PARTITION KEY, embedding FLOAT[768])` — no `distance_metric=cosine`, so MATCH ranks by L2. Embeddings are L2-normalised (fastembed), and for unit vectors L2² = 2 − 2·cos, so ranking is identical. Gate with a parity test: KNN top-k ≡ brute-force top-k on a small fixture corpus.

**Alternatives considered**: (a) keep brute force + cache — still O(corpus) per draft, fails SC-005 as corpus grows; (b) rebuild table with `distance_metric=cosine` — needs destructive migration of a 60 GB+ table, forbidden (additive-only) and unnecessary given normalisation; (c) FTS5-only retrieval — loses semantic matching of paraphrased asks.

## R2. Writer engine: Mistral-7B free-text with [CITE:n] markers, no grammar constraint

**Decision**: Demand-load Mistral-7B-Instruct-v0.3 Q4_K_M (already at `models/ir_builder/`, discovered via `mistral_builder::discover_ir_builder_gguf()`) through the existing single ModelSlot. Free-text generation with `[INST]` prompts. The prompt instructs: write connective narrative only; reference provisions exclusively as `[CITE:n]` markers; never write rule/section numbers, quotes, or monetary figures (officer figures appear as `[FIG:n]` placeholders supplied in the prompt). Output parsed with `regex_lite`.

**Rationale**: Option-C spike (2026-06-10) proved 7B free-text is faithful on real Sindh instruments while 3B paraphrased and llguidance grammar mode **hard-crashes** (C++ abort, reproduced twice: F006 IR builder + spike). Marker-based composition keeps the model on the prose lane (Principle XI; the 100%-accuracy contract): citations enter only by deterministic splice.

**Alternatives considered**: (a) llguidance JSON/grammar — dead, crashes; (b) 3B/0.5B writer — proven unfaithful (paraphrasing, confabulated numbers in V4/V5 evals); (c) cloud writer — violates Principle I and the user's explicit offline decision.

## R3. Hallucinated-number defence: deterministic post-scan (FR-005/006, SC-002)

**Decision**: After splicing, run a deterministic verifier over the final text per paragraph:
- Every spliced provision quote must be a verbatim substring of its source chunk (whitespace/hyphen-tolerant normalisation, same approach as the Phase D ingest gate).
- Scan remaining narrative with the provision-number regex (hyphen-tolerant, from `decompose`) and a figure regex (`Rs.`/digits/billion/million/crore/lakh): any number not originating from a provision card label or the officer-figure set ⇒ replace with an inline flag (FR-007) — never shipped silently.

**Rationale**: The model *will* occasionally emit numbers despite instructions (V4: 0.5B said "Article 29" with the correct chunk in context). The gate, not the prompt, is the guarantee — same philosophy as ADR-0012's verbatim-verify gate, now applied at draft level.

**Alternatives considered**: trust-the-prompt (rejected: empirically fails); logit-bias digit ban (rejected: would also ban legitimate paragraph numbering; llama-cpp-2 plumbing complexity for marginal gain over post-scan).

## R4. Composition unit: per-section generation over a built-in skeleton (FR-002/016)

**Decision**: Hardcode the 5-section Sindh summary skeleton as a Rust const: `background_purpose`, `legal_procedural_position`, `financial_aspect`, `proposal_recommendation`, `prayer_for_approval` — each with narrative guidance and retrieval-role hints (e.g. legal section → procedure/powers/routing provisions). Generate **one model call per section** with only that section's provision cards in context; assemble numbered paragraphs afterwards.

**Rationale**: (a) n_ctx budget — 7B at n_ctx 6144 on CPU can't take 60 chunks + a full draft in one prompt; per-section keeps prompts small and bounded. (b) Targeted revision (FR-009) maps 1:1 — "make paragraph 3 shorter" re-runs only the owning section. (c) Failure isolation — one bad section regenerates without redoing the rest. Skeleton-as-code satisfies FR-016 (no per-case template).

**Alternatives considered**: single mega-prompt (context overflow, no targeted revision); per-paragraph generation (too many 7B calls — minutes each on CPU; sections are the natural unit).

## R5. Research pass: per-section queries → provision cards (FR-004)

**Decision**: Build per-section retrieval queries from the parsed intent (subject + action) combined with each section's role hints. KNN-retrieve top chunks per section (R1), then build **provision cards**: verbatim chunk excerpt + provision label resolved by the existing `decompose` citation helpers (`resolve_provision_label`, hyphen-tolerant) + source document. Cap ~6 cards/section; dedupe across sections by chunk id. Cards are the writer's only citation vocabulary.

**Rationale**: Reuses the proven Fix-B deterministic label-from-chunk path (numbers read from indexed text, never generated). Per-section queries beat one generic query — "delegation of financial powers" chunks should serve the financial section, "rules of business" the routing/legal section, matching how officers actually research.

**Alternatives considered**: one global query K=60 (what 009 propose did — breadth without section alignment); graph-first retrieval via Principle XIII cross-ref graph (valuable enhancement, deferred: graph coverage of the current corpus is partial; noted as a follow-up, not a dependency).

## R6. Intent parsing: deterministic-first (FR-003, Principle II)

**Decision**: Parse the one-sentence ask with `regex_lite` only: figures = number + optional scale word (billion/million/crore/lakh) + optional `Rs.`; action keywords (get/approve/sanction/develop/establish…); subject = the full sentence (used verbatim for retrieval). No model call.

**Rationale**: Tier discipline (Principle II) — a regex satisfies this; the sentence itself is the best retrieval query (embedding handles paraphrase). Figures must be captured verbatim anyway (FR-003), which regex does losslessly.

**Alternatives considered**: 7B intent extraction (extra minutes, adds a hallucination surface for the very figures we must preserve verbatim); 3B orchestrator parse (unnecessary — nothing here needs semantics beyond embedding).

## R7. Persistence: reuse chat_message; new additive table for one-ask state

**Decision**: Conversation turns → existing `chat_message` (migration 023) keyed by draft id. Draft state → new **additive migration 025** `one_ask_session(draft_id TEXT PK, state_json TEXT, updated_at TEXT)` holding the v2 state shape (intent, cards, paragraphs with per-run verification status, status enum). The 009 `draft_session` table is left untouched.

**Rationale**: `draft_session.state_json` holds the 009 `DraftState` shape; mixing two serde shapes in one table invites silent deserialization failures on resume (FR-010). A parallel table is additive (constitution IV + migration rules) and keeps old sessions readable.

**Alternatives considered**: tagged-enum state in `draft_session` (riskier resume path, no benefit); files on disk (violates Principle IV).

## R8. Progress + cancellation (FR-013)

**Decision**: Long-running generation runs as a spawned task; command returns the draft id immediately. Stage events on `drafting://one-ask-progress` (`researching` → `composing §i/5` → `verifying` → `ready|failed|cancelled`), final draft on `drafting://one-ask-complete`. Cancellation via the existing token-registry pattern (as in `indexer::do_index` / `cancel_import`), checked between sections. Model slot: 7B demand-loaded, evicted after idle per Principle III; on RAM tiers that cannot host the 7B, the command fails fast with a human-readable remediation (Principle VII) rather than degrading to a smaller writer.

**Rationale**: The 281 s silent turn in live testing read as a crash; spec makes silence > 10 s a defect (SC-005). Event + spawned-task is the established pattern (F006 importer). Refusing on low tiers beats silently shipping 0.5B-quality prose into a signed government document.

**Alternatives considered**: blocking command with frontend spinner (frozen-screen failure mode, no cancel); auto-fallback to 3B writer (unfaithful, violates SC-002 risk posture).

## R9. Old flow disposition (FR-015)

**Decision**: Remove the template-manager / intent-Q&A / doc-curation / gap-panel UI from the officer drafting path; the drafting tab routes to the new one-ask view. Backend 009 commands, services, and tables remain in place this feature (dead code removal is a later cleanup), and ingested gold instruments stay in `template_schema` for future use as style examples. Style-example few-shot in the writer prompt is **deferred** (optional enhancement) — the skeleton + guidance is sufficient for v1.

**Rationale**: Smallest viable diff (constitution Development Constraints); deleting 009 backend now would churn migrations and tests without officer-visible benefit. Style few-shot needs a true gold *Summary* instrument, which the user has not yet supplied (only the SPSC reference letter exists).

**Alternatives considered**: full 009 teardown now (large diff, blocks rollback); keeping both flows visible (violates FR-015 — officers must see exactly one path).
