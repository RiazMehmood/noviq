# ADR-0002: Embedding Model Selection and Schema Strategy

- **Status:** Accepted
- **Date:** 2026-04-13
- **Feature:** 001-notebook-knowledge-engine
- **Context:** EchoAI requires a local embedding model to power Tier 2 (sqlite-vec semantic
  search) and Tier 0 (semantic cache similarity). The choice of embedding model determines:
  (a) the vector dimensionality stored in sqlite-vec's `vec0` virtual table schema,
  (b) retrieval quality across 10,000+ indexed office documents, (c) RAM budget for the
  always-loaded embedder, and (d) whether the first-run experience requires an internet
  connection. Critically, the `vec0` schema dimension (`FLOAT[N]`) is fixed at creation
  time — changing it requires dropping and rebuilding the entire vector index, which for
  large corpora takes hours. This makes the embedding model selection an irreversible
  schema decision that must be resolved before implementation begins.

## Decision

**Primary model**: `nomic-embed-text-v1.5` (768 dimensions, ~274 MB) via `fastembed-rs`.
**Fallback model**: `all-MiniLM-L6-v2` (384 dimensions, ~22 MB) — bundled in installer.
**Schema strategy**: Hardware-detected at first launch; dimension written to a
`db_config` table; all sqlite-vec virtual tables created with the detected dimension.

Components:

- **Model selection at startup** (`hardware_detector.rs`): Detects available RAM.
  If RAM ≥ 12 GB: load nomic-embed (768d). If RAM < 12 GB: load MiniLM (384d).
  Decision is persisted in `db_config` and cannot change without a full re-index.
- **sqlite-vec schema**: `CREATE VIRTUAL TABLE chunk_embeddings USING vec0(chunk_id TEXT PARTITION KEY, embedding FLOAT[{DIM}])` where `{DIM}` is substituted at DB creation time from `db_config`.
- **First-run behaviour**: MiniLM (~22 MB) is bundled with the installer — app works
  fully offline from the very first launch. nomic-embed (~274 MB) is downloaded on
  first launch if the user consents and has internet. If download is declined, MiniLM
  remains the permanent model for that installation.
- **No mid-life model switching**: Changing the embedding model requires user-initiated
  "Re-index everything" action in Settings, which rebuilds the sqlite-vec table from
  scratch. This is surfaced clearly, not triggered automatically.

## Consequences

### Positive

- **nomic-embed quality**: Best-in-class retrieval quality for long-form office text on
  the MTEB benchmark. 8192-token context handles large document chunks without truncation.
  Materially better recall than MiniLM for domain-specific terminology (legal, financial).
- **MiniLM as safe floor**: 22 MB bundled model guarantees full P1–P3 functionality on
  any hardware, including air-gapped machines that will never connect to the internet.
  No user is ever blocked by a download failure.
- **fastembed-rs is pure Rust + ONNX**: No Python process, no separate model server.
  Integrates directly into the Tauri Rust backend. SIMD-optimised inference via ONNX Runtime.
- **Schema dimension locked early**: Baking `DIM` into `db_config` at creation time
  prevents accidental schema mismatches and makes the data model deterministic and testable.
- **Clear upgrade path**: A user on MiniLM who later gets more RAM can trigger a full
  re-index to upgrade to nomic-embed via a single Settings action.

### Negative

- **First-run internet dependency for nomic-embed**: Users who want the best retrieval
  quality must download ~274 MB on first launch. Offline-first users get MiniLM permanently
  unless they manually trigger a re-index after gaining internet access.
- **Schema migration complexity**: If a better embedding model with a different dimension
  is released, migrating requires a full re-index of potentially millions of chunks.
  Partial migrations leave a mixed-dimension corpus that cannot be queried uniformly.
- **Two quality tiers in the field**: Users on MiniLM will experience lower semantic
  search precision than users on nomic-embed. Support requests about search quality may
  be partially explained by embedding model, complicating diagnosis.
- **~200 MB constant RAM for nomic-embed**: Always-loaded embedder consumes 200 MB on
  16 GB machines. MiniLM costs only ~30 MB, which is why it is the 8 GB hardware default.

## Alternatives Considered

### Alternative A: Single fixed model (nomic-embed only, no fallback)

Require nomic-embed for all installations; no MiniLM fallback.

**Why rejected**: Violates constitution Principle I for air-gapped machines. Locks out
users on low-end hardware where 200 MB embedder is too costly. Installer cannot bundle
274 MB without inflating size by 4–5×.

### Alternative B: Single fixed model (MiniLM only, no nomic-embed upgrade)

Bundle MiniLM and never offer nomic-embed upgrade.

**Why rejected**: MiniLM retrieval quality on office documents is measurably lower for
the 10k-file corpus target (15–20% lower recall@5 on MTEB domain benchmarks). SC-005
(semantic search < 3s across 10,000 files) is achievable with both models, but
false-negative rate is higher with MiniLM. Leaves quality improvement on the table for
users with adequate hardware.

### Alternative C: Runtime model switching without re-index

Allow users to swap embedding models without rebuilding the vector index.

**Why rejected**: Mathematically impossible. Embeddings from different models occupy
different geometric spaces. Mixed-model corpora produce nonsensical retrieval results.
Would require maintaining two separate vec0 tables — doubles storage and query
complexity with no clean upgrade path.

### Alternative D: BGE-M3 or E5-large (1024 dimensions)

Use a higher-quality 1024d model for multilingual and cross-lingual retrieval.

**Why rejected**: 1024d vectors double sqlite-vec storage vs 512d. At 10,000 files
× 20 chunks × 4 bytes × 1024 dims ≈ 820 MB for the vec0 table alone. Multilingual
quality is irrelevant for v1 (English-only per spec Assumptions). Deferred to v1.1
when multilingual support is added.

## References

- Feature Spec: `specs/001-notebook-knowledge-engine/spec.md` (FR-017, FR-019, Assumptions)
- Implementation Plan: `specs/001-notebook-knowledge-engine/plan.md` (Technical Context, AD-1)
- Research: `specs/001-notebook-knowledge-engine/research.md` (Decision 2: Embedding Model)
- Data Model: `specs/001-notebook-knowledge-engine/data-model.md` (chunk_embeddings vec0 table)
- Constitution: `.specify/memory/constitution.md` (Principle I: Privacy-First; Principle III: Single Model Slot)
- Related ADRs: ADR-0001 (Model Hosting — shares RAM budget and in-process constraint)
- Evaluator Evidence: `history/prompts/001-notebook-knowledge-engine/003-notebook-engine-plan.plan.prompt.md`
