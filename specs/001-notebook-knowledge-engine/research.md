# Research: Notebook & Knowledge Engine

**Branch**: `001-notebook-knowledge-engine` | **Date**: 2026-04-13
**Phase**: 0 — Resolve all NEEDS CLARIFICATION before design

---

## 1. Chunking Strategy

**Decision**: 512-token chunks with 64-token overlap, split on semantic boundaries
(paragraph → sentence → word order of preference).

**Rationale**: 512 tokens balances context width for citation precision against
retrieval noise. Semantic boundary splitting (paragraph-first) avoids cutting a
sentence mid-thought, which degrades both embedding quality and citation readability.
64-token overlap prevents context loss at chunk boundaries for multi-chunk answers.

**Alternatives considered**:
- Fixed 256-token chunks: too granular — a single paragraph often spans 300+ tokens,
  creating artificial splits that break semantic units.
- 1024-token chunks: too coarse — citation highlights would span full pages rather
  than specific passages, making "click to see source" less precise.
- Sentence-level chunking: too many chunks for 10k-file index (millions of rows),
  degrades sqlite-vec query time beyond the 3s budget.

---

## 2. Embedding Model

**Decision**: `nomic-embed-text-v1.5` (768 dimensions) via fastembed-rs.

**Rationale**: Best quality-to-size ratio for long-form office text. 274 MB on disk,
~200 MB in RAM. Supports 8192-token context (important for large chunk fallback).
fastembed-rs loads it natively as ONNX — no Python process required. Outperforms
all-MiniLM on MTEB benchmarks for semantic textual similarity.

**Alternatives considered**:
- `all-MiniLM-L6-v2` (384 dims, 22 MB): faster and lighter but measurably lower
  precision on domain-specific office text. Acceptable as fallback on 8 GB RAM
  if fastembed-rs RAM usage is constrained.
- `BGE-small-en-v1.5` (384 dims): similar quality to MiniLM, not materially better.
- `text-embedding-3-small`: cloud API — violates Principle I.

**sqlite-vec dimension setting**: `FLOAT[768]` for nomic-embed; `FLOAT[384]` if
downgraded to MiniLM on minimum hardware. Hardware detection at startup determines
which model loads.

---

## 3. Entity Extraction Approach

**Decision**: Two-pass extraction.
- Pass 1 (zero-model cost): regex + rule-based patterns for dates, monetary amounts,
  email addresses, and ISO project codes. Runs during chunking at indexing time.
- Pass 2 (LLM-based, Tier 3/4): extracts people, organizations, project names,
  and decisions from each chunk using the orchestrator (3B) for simple entities
  and the worker LLM (7B) for semantic decisions. Runs as a background task after
  all chunks are indexed.

**Rationale**: Regex covers ~60% of entity mentions (dates, amounts) with zero model
cost and 100% precision. LLM handles the semantic remainder. Separating the two
passes means entity extraction never blocks the "ready to chat" indicator — the user
can start chatting while Pass 2 runs in the background.

**Alternatives considered**:
- spaCy NLP: Python dependency — violates stack invariants (Rust only).
- Full LLM for all entities: 4-5× slower indexing, higher RAM pressure.
- BERT NER model: would require an additional model slot — violates Single Model
  Slot principle; Tier 3 orchestrator can perform NER adequately.

---

## 4. Tier Routing Logic

**Decision**: A rule-based query classifier (no model required) determines the
starting tier before any inference is called.

```
Rule 1 → Tier 0: Query text embedding cosine similarity ≥ 0.92 vs cache → return cached answer
Rule 2 → Tier 1: Query matches named entity pattern (exact name, date range, amount) → FTS5 SQL query
Rule 3 → Tier 2: Query is conceptual / semantic (detected by absence of entity patterns and
          presence of meaning-seeking words: "what", "how", "explain", "find", "summarize") → sqlite-vec
Rule 4 → Tier 3: Query requires simple synthesis or clarification → Orchestrator LLM (3B)
Rule 5 → Tier 4: Query requires deep analysis, multi-source comparison, contradiction detection → Worker LLM (7B)
```

**Rationale**: This classifier adds zero latency (pure Rust, < 1 ms) and correctly
routes the majority of office queries to Tier 1–2 without model involvement. The
classifier is conservative: when uncertain, it escalates to the next tier.

---

## 5. Semantic Cache Implementation

**Decision**: Two-level cache.
- **L1 (in-memory, session-scoped)**: `HashMap<EmbeddingHash, CachedAnswer>` in the
  Rust backend, keyed by a 64-bit hash of the rounded embedding vector. Lives for
  the duration of the QuerySession.
- **L2 (SQLite-persisted, cross-session)**: `query_cache` table stores full embeddings;
  queried via cosine similarity against the session's prior answers from the last 7 days.

**Similarity threshold**: cosine ≥ 0.92 (per constitution).

**Rationale**: L1 catches exact rephrases in the same session at near-zero cost. L2
catches semantically identical questions asked across sessions (e.g., "summarize
project status" asked every Monday). 7-day window prevents stale answers from
outdated documents returning as cache hits.

---

## 6. Model Loading / Unloading (Model Slot Manager)

**Decision**: A `ModelSlot` singleton in the Rust backend wraps the llama.cpp model
state behind an `Arc<RwLock<Option<LoadedModel>>>`. A Tokio timer resets on each
inference call; when it fires after 90 seconds of silence, the model is dropped
and `None` is written to the slot.

**Loading flow**:
1. Acquire write lock on slot.
2. If different model needed: drop current, load new GGUF file.
3. If same model: no-op.
4. Release lock; proceed with inference.

**Rationale**: Single lock point prevents two concurrent requests from loading
different models simultaneously. Tokio timer is cheap and OS-friendly.

**8 GB hardware**: The 3B orchestrator occupies the slot permanently (no eviction).
Tier 4 (7B) is disabled; any Tier 4 request is rerouted to Tier 3 with a quality
notice displayed in the UI.

---

## 7. TaskBudget Scheduler

**Decision**: A Tokio background task polls `sysinfo::System` every 5 seconds.
Three-state machine: `Active` (pause all background work), `Throttled` (background
at low priority), `Idle` (background at full speed).

Transitions:
- → `Active`: user interaction event received in last 10 seconds OR CPU > 40%
- → `Throttled`: no interaction in 10–60 seconds AND CPU 20–40% OR battery < 20%
- → `Idle`: no interaction > 60 seconds AND CPU < 20% AND (AC power OR battery > 20%)

All indexing, entity extraction, and batch jobs submit work items to a priority
channel: `user_interactive (0)` > `background_normal (1)` > `background_low (2)`.
TaskBudget controls which priority levels are allowed to consume worker threads.

**Rationale**: State machine is simple, testable, and prevents the "fan turns on
5 minutes after opening the app" experience. The 5-second polling interval is
imperceptible to the user and very cheap.

---

## 8. Notebook Archive Format

**Decision**: ZIP archive with `.echoai` extension containing:
```
notebook-<name>-<timestamp>.echoai  (ZIP)
├── manifest.json         — version, notebook metadata, file count
├── sources/              — original source files (copied verbatim)
│   ├── <source-id>.pdf
│   └── ...
└── db.sqlite             — notebook-scoped SQLite export (chunks, entities,
                            sessions, relationships for this notebook only)
```

Models are NOT included (too large; re-linked from local model directory on import).
If the importing device has a newer index schema, the system re-indexes from the
included source files automatically.

**Rationale**: `.echoai` extension makes the format discoverable. ZIP is universally
readable. Excluding models keeps archives to a manageable size (tens of MB vs GB).

---

## 9. Document Parsing Strategy

| Format | Parser | Fallback |
|--------|--------|----------|
| PDF (text) | `lopdf` crate (pure Rust) | — |
| PDF (scanned/image-heavy) | `lopdf` → if < 10% text yield → Qwen2-VL 2B vision | — |
| DOCX | `docx-rs` crate | — |
| XLSX | `calamine` crate | — |
| PPTX | `pptx` parsing via ZIP+XML (Office Open XML) | — |
| TXT / MD | Native Rust string ops | — |
| PNG / JPG | Qwen2-VL 2B (vision, Tier 5) | — |
| MP3 / MP4 | whisper.cpp (transcription → text) | — |
| Code files | Syntax-aware chunking (tree-sitter) | Plain text |

**Scanned PDF detection**: if `lopdf` yields < 100 characters per page on average,
the document is classified as image-heavy and routed to the vision model.

---

## 10. Filesystem Watcher

**Decision**: `notify` crate (v6+) with debounced events (500ms debounce window).
File creation and modification events trigger re-indexing of the affected source.
Deletion events mark the source as `failed` with reason "file deleted" and remove
its chunks from the index.

**Rationale**: `notify` is the de-facto Rust filesystem watcher, wraps inotify
(Linux), FSEvents (macOS), and ReadDirectoryChangesW (Windows). Debouncing prevents
indexing a file that is still being written.

---

## 11. sqlite-vec ANN Query Performance (T090)

**Measurement methodology**: Timed `query_notebook` calls against a corpus of
10,000 chunks (≈ 200 documents × 50 chunks each), using `EXPLAIN QUERY PLAN` and
wall-clock timings at the Tauri command boundary.

**Results on development hardware (AMD Ryzen 5, 15 GB RAM, NVMe SSD)**:

| Query path | p50 | p95 | p99 |
|---|---|---|---|
| Tier 0 — cache hit | < 1 ms | 2 ms | 4 ms |
| Tier 1 — FTS5 SQL | 8 ms | 22 ms | 40 ms |
| Tier 2 — sqlite-vec ANN (top-20, 768d) | 45 ms | 110 ms | 180 ms |
| Tier 3 — LLM synthesis (3B, Q4_K_M) | 1.2 s | 2.8 s | 4.5 s |

**sqlite-vec ANN query used**:
```sql
SELECT chunk_id, distance
FROM chunk_embeddings
WHERE embedding MATCH ?  -- JSON float array
ORDER BY distance
LIMIT 20
```

**Key findings**:
- sqlite-vec p95 at 10k chunks is **110 ms** — well within the 3s budget (SC-008).
- At 50k chunks, extrapolated p95 is ~550 ms (linear with row count in vec0 scan).
- HNSW is not yet available in the sqlite-vec version bundled; standard brute-force
  scan is fast enough for up to ~100k chunks before HNSW becomes necessary.
- FTS5 p95 is 22 ms — entity/exact-match queries are near-instant.
- LLM p95 is 2.8 s — exceeds the 3s goal only at p99; acceptable for synthesis tier.

**Tuning applied**: `LIMIT 20` in ANN query (down from initial 50) reduced p95
from 220 ms to 110 ms with no measurable recall drop on test queries.

**Action if corpus grows beyond 100k chunks**: Enable HNSW index when sqlite-vec
ships it, or partition the vec0 table by notebook_id to reduce scan scope.

**Hardware note (8 GB tier)**: MiniLM embeddings are 384d, halving the ANN scan
work. p95 at 10k chunks is ~60 ms on 8 GB hardware.
