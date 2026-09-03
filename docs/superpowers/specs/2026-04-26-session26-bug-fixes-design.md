# Session 26 Bug Fixes — Design Spec

**Date:** 2026-04-26  
**Scope:** 6 independent fixes across Rust backend, NER pipeline, and frontend  
**Rebuild required:** Yes (Rust changes in fixes 1–4)

---

## Fix 1: Tab Navigation Cancels LLM Workloads (Phase 2)

### Problem
`apply_policy_to()` in `scheduler.rs` cancels any running LLM workload when a different LLM takes foreground. Navigating tabs triggers `set_foreground(KgExtraction)` which cancels Chat/Analyze/Batch mid-execution.

Affects: Chat, Analyze, and Batch Processing — all three user-initiated LLM features.

### Design

**Step A — Add `Batch` variant to `WorkloadKind`.**
Currently batch registers as `KgExtraction` (background priority), so `set_foreground(Chat)` cancels it. Analysis doesn't use the scheduler at all. Both need proper registration.

New enum:
```rust
pub enum WorkloadKind {
    Chat,
    Batch,       // NEW — user-initiated batch processing
    Indexing,
    Pass2Ner,
    KgExtraction,
    FileWatcher,
}
```

**Step B — Add `priority()` method:**
```rust
impl WorkloadKind {
    pub fn priority(&self) -> u8 {
        match self {
            Self::Chat | Self::Batch => 2, // user-initiated, never preempted
            Self::KgExtraction | Self::Pass2Ner => 1, // background, yields to user
            Self::Indexing | Self::FileWatcher  => 0, // infra
        }
    }
}
```

**Step C — Change `apply_policy_to` rule:** only cancel workload `h` if `foreground.priority() >= h.kind.priority()`.

- `set_foreground(KgExtraction)` while Chat/Batch running → `1 < 2` → NOT cancelled ✅
- `set_foreground(Chat)` while NER running → `2 >= 1` → NER cancelled ✅ (preserved)
- `set_foreground(Chat)` while Batch running → `2 >= 2` → Batch cancelled (Chat takes over) — acceptable, user explicitly switched

**Step D — Register batch as `WorkloadKind::Batch`** in `batch_processor.rs` (currently uses `KgExtraction`).

**Step E — Analysis:** Analysis engine (`commands/analysis.rs`) currently has no scheduler registration. Add registration as `WorkloadKind::Chat` priority so it isn't cancelled by NER. This is a smaller change — just register + check cancel token.

**Files:** `src-tauri/src/services/scheduler.rs`, `src-tauri/src/services/batch_processor.rs`, `src-tauri/src/commands/analysis.rs`  
**Tests:** Update existing scheduler unit tests to reflect new priority rules.

---

## Fix 2: Response Token Limit (96 → 256)

### Problem
`try_llm_infer_streaming(..., 96, ...)` at `query_router.rs:205` allocates only 96 tokens for LLM response (~384 chars). Summary answers are cut off mid-sentence.

### Design
- Change `max_tokens` arg from `96` → `256` (line 205, query_router.rs)
- Reduce `MAX_PROMPT_TOKENS` from `928` → `768` to maintain headroom within n_ctx=1024

**Response time impact:** ~+25% on long summary answers (decode adds ~0.1s/token × 160 extra tokens ≈ +16s). Specific factual Q&A unaffected (LLM stops at EOS early).

**Files:** `src-tauri/src/services/query_router.rs`

---

## Fix 3: NER Startup Recovery

### Problem
Sources with `indexing_status='indexed'` but `ner_status='pending'` never resume NER after app restart. KG gets stuck at partial progress (e.g. 67%).

### Design

**Part A — Extract `run_ner_for_source()`** from `index_file()` in `indexer.rs`:
```rust
pub async fn run_ner_for_source(
    pool: &SqlitePool,
    source_id: &str,
    file_name: &str,
    app: tauri::AppHandle,
)
```
Fetches chunks from DB, runs Pass 1 regex + Pass 2 LLM NER — same logic as today, callable without re-indexing.

**Part B — Startup recovery block** in `lib.rs` (after existing `reset_stuck_processing`):
```sql
SELECT id, file_name FROM sources
WHERE indexing_status = 'indexed' AND ner_status = 'pending'
```
Spawn `run_ner_for_source` for each result in a background tokio task.

Fixes: restart-mid-NER case, file-watcher-added-file NER starvation.

**Files:** `src-tauri/src/services/indexer.rs`, `src-tauri/src/lib.rs`

---

## Fix 4: KG Entity Types — Universal Schema

### Problem
Current entity types (`person, org, date, project, decision, amount, email`) are too narrow. Pass 2 LLM returns 0 entities from legal, medical, engineering, and economics documents. System must work for all domains.

### Design

**Part A — New migration** replacing narrow types with universal ones:

| Keep | Drop | Add |
|------|------|-----|
| person, org, date, amount | email, project, decision | location, concept, term, reference |

`reference` covers: articles, sections, RFCs, standards, laws, formulas, code files — any cited external item across all domains.

Data migration: existing `email` → `org`, `project`/`decision` → `concept`.

**Part B — NER prompt redesign** in `entity_extractor.rs`:
- Replace single-domain examples with **mixed-domain few-shot examples** (legal + medical + business snippet)
- Relationship examples across domains so the 3B model generalises the extraction pattern
- Keep output format identical (one entity/relation per line)

**Files:** `src-tauri/src/db/migrations/009_universal_entity_types.sql`, `src-tauri/src/services/entity_extractor.rs`

---

## Fix 5: Chat Scrollbar

### Problem
Scrollbar disappears after 2–3 chat messages. Response renders below visible area, requiring arrow keys to reach. Root cause: `scrollIntoView` scrolls the page viewport, not the message container, corrupting the container's scroll state.

### Design
Two changes in `ChatPanel.tsx`:

1. Add `scrollContainerRef` and replace `scrollIntoView` with:
```ts
scrollContainerRef.scrollTop = scrollContainerRef.scrollHeight;
```

2. Change container class: `overflow-y-auto` → `overflow-y-scroll`

`overflow-y-scroll` keeps the track always visible (fixes GTK overlay scrollbar hiding).  
`scrollTop = scrollHeight` scrolls within container only — never touches page viewport.

**Files:** `src/components/chat/ChatPanel.tsx`

---

## Fix 6: KG Relation Filter Dropdown

### Problem
`RelationTypesPanel` (gear icon) saves NER extraction preferences but has no effect on the displayed graph. No way to filter visible edges by relation type.

### Design

**Separation of concerns:**
- `RelationTypesPanel` — extraction preferences only (no change to its role)
- New edge-type filter — display-only, derived from actual extracted data

**GraphCanvas.tsx changes:**
- Add `selectedRelationTypes` signal, default `[]` (= show all)
- Derive filter options from `allEdges()` (deduplicated `relation_type` values) — only shows types that actually exist in the graph
- Update `visibleEdges()`:
```ts
const visibleEdges = () => allEdges().filter(e =>
  ids.has(e.from_entity_id) && ids.has(e.to_entity_id) &&
  (selectedRelationTypes().length === 0 ||
   selectedRelationTypes().includes(e.relation_type))
);
```
- Add dropdown UI alongside existing entity-type filter

**Key rule:** Filter options come from actual extracted edges — custom relations defined but not yet extracted do NOT appear in the filter until NER runs and finds them.

**Files:** `src/components/knowledge-graph/GraphCanvas.tsx`

---

## Implementation Order

| # | Fix | Type | Rebuild |
|---|-----|------|---------|
| 1 | Tab cancel (scheduler priority) | Rust | Yes |
| 2 | 96→256 token limit | Rust | Yes |
| 3 | NER startup recovery | Rust | Yes |
| 4 | KG entity types + NER prompt | Rust | Yes |
| 5 | Chat scrollbar | Frontend | No |
| 6 | KG relation filter | Frontend | No |

**Strategy:** Fixes 1–4 in one batch → single rebuild. Fixes 5–6 after rebuild (hot-reloaded by Vite).

---

## Acceptance Criteria

- [ ] Chat/Analyze/Batch complete normally when user navigates to KG tab mid-execution
- [ ] Summary answers are not cut off mid-sentence
- [ ] Sources with `ner_status='pending'` auto-resume NER on app restart
- [ ] NER extracts non-date entities from legal, business, and general documents
- [ ] Chat scrollbar always visible; new responses auto-scroll into view
- [ ] KG edge filter dropdown shows only extracted relation types; filtering updates graph immediately
