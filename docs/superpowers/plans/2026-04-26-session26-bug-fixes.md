# Session 26 Bug Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement 6 independent bug fixes: scheduler priority (tab-cancel), 96→256 token limit, NER startup recovery, universal KG entity types + NER prompt, chat scrollbar, and KG relation filter dropdown.

**Architecture:** Fixes 1–4 are Rust backend changes requiring one rebuild; fixes 5–6 are SolidJS frontend changes that hot-reload via Vite. All Rust fixes are independent and can be applied in any order before the single rebuild.

**Tech Stack:** Rust (Tauri backend), SolidJS + TypeScript (frontend), SQLite migrations (sqlx)

**Speed constraint:** Chat response speed is the #1 priority. Fix 1 (scheduler priority) is designed so Chat *can* cancel NER — NER cannot cancel Chat. Fix 2 adds ~+16s only on long summary answers; factual Q&A is unaffected (LLM stops at EOS early). No fix touches the chat inference hot-path beyond the token-limit change.

---

## File Map

| File | Change |
|------|--------|
| `src-tauri/src/services/scheduler.rs` | Add `Batch` variant, `priority()` method, fix `apply_policy_to` |
| `src-tauri/src/services/batch_processor.rs` | Register as `WorkloadKind::Batch` instead of `KgExtraction` |
| `src-tauri/src/services/query_router.rs` | max_tokens 96→256, CONTEXT_TOKEN_BUDGET 640→480 |
| `src-tauri/src/services/indexer.rs` | Extract `run_ner_for_source()` |
| `src-tauri/src/lib.rs` | NER startup recovery block |
| `src-tauri/src/db/migrations/009_universal_entity_types.sql` | New migration |
| `src-tauri/src/services/entity_extractor.rs` | Universal types + new NER prompt |
| `src/components/chat/ChatPanel.tsx` | scrollContainerRef + overflow-y-scroll |
| `src/components/knowledge-graph/GraphCanvas.tsx` | Relation filter dropdown |

---

## Task 1: Scheduler Priority Fix (Fix 1)

**Files:**
- Modify: `src-tauri/src/services/scheduler.rs`
- Modify: `src-tauri/src/services/batch_processor.rs`

### Step A: Add `Batch` to `WorkloadKind` and update `uses_llm`

- [ ] **Step 1: Open `scheduler.rs` and edit the `WorkloadKind` enum (lines 15–21)**

Replace:
```rust
pub enum WorkloadKind {
    Chat,
    Indexing,
    Pass2Ner,
    KgExtraction,
    FileWatcher,
}
```
With:
```rust
pub enum WorkloadKind {
    Chat,
    Batch,
    Indexing,
    Pass2Ner,
    KgExtraction,
    FileWatcher,
}
```

- [ ] **Step 2: Update `uses_llm` to include `Batch` (line 25)**

Replace:
```rust
    pub fn uses_llm(self) -> bool {
        matches!(self, Self::Chat | Self::Pass2Ner | Self::KgExtraction)
    }
```
With:
```rust
    pub fn uses_llm(self) -> bool {
        matches!(self, Self::Chat | Self::Batch | Self::Pass2Ner | Self::KgExtraction)
    }
```

- [ ] **Step 3: Add `priority()` method directly after `uses_llm`**

```rust
    pub fn priority(self) -> u8 {
        match self {
            Self::Chat | Self::Batch => 2,
            Self::KgExtraction | Self::Pass2Ner => 1,
            Self::Indexing | Self::FileWatcher => 0,
        }
    }
```

### Step B: Fix `apply_policy_to` to use priority ordering

- [ ] **Step 4: Replace `apply_policy_to` body (lines 207–233)**

Replace the entire method:
```rust
    fn apply_policy_to(&self, h: &Arc<WorkloadHandle>, foreground: WorkloadKind) {
        if h.kind.uses_llm() {
            // Two LLM workloads can't share model weights — cancel the non-foreground one.
            // Only applies when the incoming foreground is itself an LLM workload.
            if foreground.uses_llm() {
                h.cancel.cancel();
                h.pause.store(false, Ordering::Release);
                h.notify_resume.notify_waiters();
                if foreground == WorkloadKind::Chat {
                    use tauri::Emitter;
                    if let Some(a) = &self.app {
                        let _ = a.emit("chat://background-paused",
                            serde_json::json!({ "kind": format!("{:?}", h.kind) }));
                    }
                    let _ = self.events.send(SchedulerEvent::BackgroundPaused { kind: h.kind });
                }
            }
            // Non-LLM foreground taking over: don't disturb LLM workloads in progress.
        } else {
            // Non-LLM workloads (Indexing, FileWatcher): always let them run.
            h.pause.store(false, Ordering::Release);
            h.notify_resume.notify_waiters();
        }
    }
```
With:
```rust
    fn apply_policy_to(&self, h: &Arc<WorkloadHandle>, foreground: WorkloadKind) {
        if h.kind.uses_llm() && foreground.uses_llm() {
            // Only cancel a background LLM workload if the new foreground has
            // equal or higher priority. This prevents KgExtraction / Pass2Ner
            // (priority 1) from cancelling Chat / Batch (priority 2).
            if foreground.priority() >= h.kind.priority() {
                h.cancel.cancel();
                h.pause.store(false, Ordering::Release);
                h.notify_resume.notify_waiters();
                if foreground == WorkloadKind::Chat {
                    use tauri::Emitter;
                    if let Some(a) = &self.app {
                        let _ = a.emit("chat://background-paused",
                            serde_json::json!({ "kind": format!("{:?}", h.kind) }));
                    }
                    let _ = self.events.send(SchedulerEvent::BackgroundPaused { kind: h.kind });
                }
            }
            // foreground.priority() < h.kind.priority() → background workload
            // is higher priority (e.g. Chat running, KgExtraction takes foreground)
            // → do NOT cancel it; let it finish.
        } else {
            // Non-LLM workloads (Indexing, FileWatcher): always let them run.
            h.pause.store(false, Ordering::Release);
            h.notify_resume.notify_waiters();
        }
    }
```

### Step C: Update tests

- [ ] **Step 5: Update existing test `set_foreground_cancels_llm_workload` to preserve its intent (Chat cancels NER)**

The existing test at line 244 is still correct — Chat (priority 2) cancels Pass2Ner (priority 1). No change needed.

- [ ] **Step 6: Add new test confirming Chat is NOT cancelled by KgExtraction**

Add after the last test in the `#[cfg(test)]` block:

```rust
    #[tokio::test]
    async fn kg_extraction_does_not_cancel_chat() {
        // KgExtraction (priority 1) must NOT cancel Chat (priority 2).
        let sched = Scheduler::new(None);
        let h = sched.register(WorkloadKind::Chat);
        sched.set_foreground(WorkloadKind::KgExtraction).await;
        assert!(!h.is_cancelled(), "Chat must not be cancelled when KgExtraction takes foreground");
    }

    #[tokio::test]
    async fn batch_not_cancelled_by_kg_extraction() {
        // Batch (priority 2) must NOT be cancelled by KgExtraction (priority 1).
        let sched = Scheduler::new(None);
        let h = sched.register(WorkloadKind::Batch);
        sched.set_foreground(WorkloadKind::KgExtraction).await;
        assert!(!h.is_cancelled(), "Batch must not be cancelled by KgExtraction");
    }

    #[tokio::test]
    async fn chat_cancels_ner() {
        // Chat (priority 2) MUST cancel Pass2Ner (priority 1).
        let sched = Scheduler::new(None);
        let h = sched.register(WorkloadKind::Pass2Ner);
        sched.set_foreground(WorkloadKind::Chat).await;
        assert!(h.is_cancelled(), "Chat must cancel NER so it can acquire the model slot");
    }
```

- [ ] **Step 7: Run scheduler unit tests**

```bash
cd "/media/riaz/New Volume/echo-AI"
cargo test -p echo-ai-lib scheduler -- --nocapture 2>&1 | tail -30
```

Expected: All tests PASS, including the 3 new ones.

### Step D: Register batch as `WorkloadKind::Batch`

- [ ] **Step 8: Edit `batch_processor.rs` lines 438–445**

Replace:
```rust
    use crate::services::scheduler::{Scheduler, WorkloadKind};
    ...
        sched.set_foreground(WorkloadKind::KgExtraction).await;
        let h = sched.register(WorkloadKind::KgExtraction);
```
With:
```rust
    use crate::services::scheduler::{Scheduler, WorkloadKind};
    ...
        sched.set_foreground(WorkloadKind::Batch).await;
        let h = sched.register(WorkloadKind::Batch);
```

- [ ] **Step 9: Run `cargo check` — must be clean**

```bash
cd "/media/riaz/New Volume/echo-AI"
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "^error" | head -20
```

Expected: No errors.

---

## Task 2: Response Token Limit Fix (Fix 2)

**Files:**
- Modify: `src-tauri/src/services/query_router.rs`

- [ ] **Step 1: Change `max_tokens` from 96 → 256 at line 205**

In `query_router.rs`, find:
```rust
        match try_llm_infer_streaming(model_slot, &model_path, &prompt, 96, tier, cancel, app).await {
```
Replace with:
```rust
        match try_llm_infer_streaming(model_slot, &model_path, &prompt, 256, tier, cancel, app).await {
```

- [ ] **Step 2: Update `CONTEXT_TOKEN_BUDGET` from 640 → 480 and fix the comment**

Find (lines 21–26):
```rust
/// Context token budget for retrieved passages.
/// n_ctx=1024; MAX_PROMPT_TOKENS=928 (1024-96 response).
/// System prompt ≈200 tokens + ChatML overhead ≈30 tokens → 928−230=698.
/// Using 640 for a safe margin; prevents prompt truncation that caused the
/// model to echo its input instead of generating an answer.
const CONTEXT_TOKEN_BUDGET: usize = 640; // 928 − ~288 (system+overhead)
```
Replace with:
```rust
/// Context token budget for retrieved passages.
/// n_ctx=1024; MAX_PROMPT_TOKENS=768 (1024-256 response).
/// System prompt ≈200 tokens + ChatML overhead ≈30 tokens → 768−230=538.
/// Using 480 for a safe margin; prevents prompt truncation that caused the
/// model to echo its input instead of generating an answer.
const CONTEXT_TOKEN_BUDGET: usize = 480; // 768 − ~288 (system+overhead)
```

- [ ] **Step 3: Run `cargo check`**

```bash
cd "/media/riaz/New Volume/echo-AI"
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "^error" | head -20
```

Expected: No errors.

---

## Task 3: NER Startup Recovery (Fix 3)

**Files:**
- Modify: `src-tauri/src/services/indexer.rs`
- Modify: `src-tauri/src/lib.rs`

### Part A — Extract `run_ner_for_source()`

- [ ] **Step 1: Add `run_ner_for_source()` as a new public function in `indexer.rs`**

Add this function just before the line `// ─── T9/T10: Scheduler-aware batch indexing ──` (around line 517):

```rust
/// Run NER for a source that is already indexed but has `ner_status='pending'`.
/// Fetches all chunks from DB, runs Pass 1 regex + Pass 2 LLM extraction.
/// Called from startup recovery and can be called from retry paths.
pub async fn run_ner_for_source(
    pool: &SqlitePool,
    source_id: &str,
    file_name: &str,
    app: tauri::AppHandle,
) {
    use crate::services::entity_extractor;
    use tauri::Emitter;

    // Load all chunks for this source.
    let chunks: Vec<(String, String)> = match sqlx::query_as::<_, (String, String)>(
        "SELECT c.id, c.content FROM chunks c WHERE c.source_id = ? ORDER BY c.chunk_index",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            log::warn!("run_ner_for_source({source_id}): failed to load chunks: {e}");
            return;
        }
    };

    if chunks.is_empty() {
        log::info!("run_ner_for_source({source_id}): no chunks found, marking done");
        let _ = sqlx::query("UPDATE sources SET ner_status='done', ner_progress=100 WHERE id=?")
            .bind(source_id)
            .execute(pool)
            .await;
        return;
    }

    // Pass 1: regex NER (fast, always runs).
    for (chunk_id, content) in &chunks {
        if let Err(e) = entity_extractor::extract_pass1(pool, chunk_id, content).await {
            log::warn!("run_ner_for_source Pass 1 failed for chunk {chunk_id}: {e}");
        }
    }

    // Pass 2: LLM-based NER (slow, acquires entity semaphore).
    let pass2_limit: usize = std::env::var("ECHO_PASS2_CHUNK_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &usize| n > 0)
        .unwrap_or(DEFAULT_PASS2_CHUNK_LIMIT);

    let pass2_indices: Vec<usize> = if chunks.len() <= pass2_limit {
        (0..chunks.len()).collect()
    } else {
        let n = chunks.len();
        (0..pass2_limit).map(|i| (i * n) / pass2_limit).collect()
    };

    let _permit = entity_sem().acquire().await.ok();

    use crate::services::llm::ModelSlot;
    use crate::services::hardware_detector::HardwareTier;
    use crate::commands::system::model_info_for_tier;

    let model_slot = app.state::<std::sync::Arc<ModelSlot>>();
    let tier = app.state::<HardwareTier>();
    let (rel_path, _url, _name) = model_info_for_tier(&tier);
    let model_path = crate::services::query_router::resolve_model_path(rel_path);

    let total = pass2_indices.len();

    let _ = sqlx::query(
        "UPDATE sources SET ner_status='running', ner_progress=0, ner_total=? WHERE id=?",
    )
    .bind(total as i64)
    .bind(source_id)
    .execute(pool)
    .await;

    let _ = app.emit("kg://progress", serde_json::json!({
        "source_id": source_id,
        "file_name": file_name,
        "processed": 0,
        "total": total,
        "eta_seconds": null,
    }));

    let relation_types = crate::commands::kg_preferences::load_relation_types(pool).await;
    let t_start = std::time::Instant::now();

    for (done, &i) in pass2_indices.iter().enumerate() {
        let Some((chunk_id, content)) = chunks.get(i) else { continue };
        if let Err(e) = entity_extractor::extract_pass2(
            pool, chunk_id, content, &model_slot, &model_path, &tier, &relation_types,
        ).await {
            log::warn!("run_ner_for_source Pass 2 failed for chunk {chunk_id}: {e}");
        }

        let processed = done + 1;
        let elapsed_secs = t_start.elapsed().as_secs_f64();
        let eta_seconds: Option<u64> = if processed > 0 {
            let rate = elapsed_secs / processed as f64;
            Some((rate * (total - processed) as f64).ceil() as u64)
        } else {
            None
        };

        let progress = ((processed * 100) / total.max(1)) as i64;
        let _ = sqlx::query("UPDATE sources SET ner_progress=? WHERE id=?")
            .bind(progress)
            .bind(source_id)
            .execute(pool)
            .await;

        let _ = app.emit("kg://progress", serde_json::json!({
            "source_id": source_id,
            "file_name": file_name,
            "processed": processed,
            "total": total,
            "eta_seconds": eta_seconds,
        }));

        tokio::task::yield_now().await;
    }

    let _ = sqlx::query("UPDATE sources SET ner_status='done', ner_progress=100 WHERE id=?")
        .bind(source_id)
        .execute(pool)
        .await;

    let _ = app.emit("kg://progress", serde_json::json!({
        "source_id": source_id,
        "file_name": file_name,
        "processed": total,
        "total": total,
        "eta_seconds": 0,
    }));

    log::info!("run_ner_for_source({source_id}): NER complete ({total} chunks sampled)");
}
```

### Part B — Startup recovery block in `lib.rs`

- [ ] **Step 2: Add NER startup recovery after the `reset_stuck_processing` block in `lib.rs`**

Find (lines 217–225):
```rust
    // On startup: reset sources stuck in 'processing' to 'failed' so the user
    // can retry. Auto-re-trigger is unsafe — notebook links may be orphaned
    // after a crash-interrupted delete-and-re-add sequence.
    {
        use crate::db::repositories::sources as source_repo;
        if let Err(e) = source_repo::reset_stuck_processing(&pool).await {
            log::warn!("Failed to reset stuck-processing sources: {e}");
        }
    }
```

Add immediately after that block:
```rust
    // On startup: resume NER for any sources that were indexed but have
    // ner_status='pending' (app restarted mid-NER).
    {
        let pending: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
            "SELECT id, file_name FROM sources WHERE indexing_status = 'indexed' AND ner_status = 'pending'"
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        if !pending.is_empty() {
            log::info!("Startup NER recovery: resuming {} pending source(s)", pending.len());
            let pool2 = pool.clone();
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                for (source_id, file_name) in pending {
                    crate::services::indexer::run_ner_for_source(
                        &pool2, &source_id, &file_name, app2.clone(),
                    ).await;
                }
            });
        }
    }
```

- [ ] **Step 3: Run `cargo check`**

```bash
cd "/media/riaz/New Volume/echo-AI"
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "^error" | head -20
```

Expected: No errors.

---

## Task 4: Universal KG Entity Types + NER Prompt (Fix 4)

**Files:**
- Create: `src-tauri/src/db/migrations/009_universal_entity_types.sql`
- Modify: `src-tauri/src/services/entity_extractor.rs`

### Part A — Database migration

- [ ] **Step 1: Create migration file**

File: `src-tauri/src/db/migrations/009_universal_entity_types.sql`

```sql
-- Migration 009: Replace narrow entity types with a universal schema.
-- Previous types (email, project, decision) were too domain-specific —
-- they matched almost nothing in legal, medical, engineering, or economics documents.
-- New types: person, org, location, concept, term, reference, date, amount.
--
-- Data migration:
--   email     → org  (email addresses usually identify organisations)
--   project   → concept
--   decision  → concept

-- Widen the CHECK constraint to allow all new types.
-- SQLite does not support ALTER COLUMN, so we recreate the entities table.
PRAGMA foreign_keys = OFF;

CREATE TABLE entities_new (
    id             TEXT PRIMARY KEY,
    canonical_value TEXT NOT NULL UNIQUE,
    entity_type    TEXT NOT NULL
                   CHECK (entity_type IN ('person','org','location','concept','term','reference','date','amount')),
    aliases        TEXT,
    created_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO entities_new (id, canonical_value, entity_type, aliases, created_at)
SELECT
    id,
    canonical_value,
    CASE entity_type
        WHEN 'email'    THEN 'org'
        WHEN 'project'  THEN 'concept'
        WHEN 'decision' THEN 'concept'
        ELSE entity_type
    END,
    aliases,
    created_at
FROM entities;

DROP TABLE entities;
ALTER TABLE entities_new RENAME TO entities;

-- Recreate indexes that existed on the original table.
CREATE INDEX IF NOT EXISTS idx_entities_type  ON entities(entity_type);
CREATE INDEX IF NOT EXISTS idx_entities_value ON entities(canonical_value);

PRAGMA foreign_keys = ON;
```

### Part B — Update entity extractor

- [ ] **Step 2: Update the `PASS2_PROMPT` constant in `entity_extractor.rs`**

Find the constant starting at line 85:
```rust
const PASS2_PROMPT: &str = r#"Extract named entities and their relationships from the text below.

Output ONE JSON object per line. No surrounding object, no arrays, no markdown fences.

ONLY these "kind" values are valid: "entity", "rel", "none". Never use "person", "org", "project" or "decision" as a kind — those are entity types.

Entity line — exact shape:
{"kind":"entity","type":"person","value":"Sarah Chen"}
   "type" MUST be one of: person, org, project, decision
   "value" MUST be the canonical name (non-empty)

Relationship line — exact shape:
{"kind":"rel","from":"Sarah Chen","to":"Acme Corp","type":"works_at"}
   "from" and "to" MUST each be the "value" of an entity line you already emitted.
   "type" MUST be one of these snake_case verbs (use the closest match; do not invent new verbs):
   {RELATION_TYPES}

Rules:
- Emit every entity BEFORE any relationship that references it.
- At most 10 entity lines and at most 10 relationship lines.
- Do not emit a relationship unless both its "from" and "to" appear as entity lines above.
- Never leave "from", "to", "type", or "value" empty.
- If the text has nothing to extract, output exactly one line: {"kind":"none"}

TEXT:
{TEXT}

OUTPUT:
"#;
```

Replace with:
```rust
const PASS2_PROMPT: &str = r#"Extract named entities and their relationships from the text below.

Output ONE JSON object per line. No surrounding object, no arrays, no markdown fences.

ONLY these "kind" values are valid: "entity", "rel", "none". Never use a type name as a kind.

Entity line — exact shape:
{"kind":"entity","type":"person","value":"Sarah Chen"}
   "type" MUST be one of: person, org, location, concept, term, reference, date, amount
   person    — a named individual (e.g. "Albert Einstein", "the Minister")
   org       — a company, institution, government body, or group (e.g. "WHO", "Acme Ltd")
   location  — a place: country, city, region, address (e.g. "Punjab", "Section 14(2)")
   concept   — an abstract idea, policy, right, or named principle (e.g. "due process", "sovereignty")
   term      — a defined technical or domain term (e.g. "force majeure", "EBITDA", "mRNA")
   reference — a cited document, law, standard, article, or formula (e.g. "Article 3", "RFC 2119")
   date      — a specific date or period (e.g. "1 January 2024", "Q3 2023")
   amount    — a quantity with unit (e.g. "$5 million", "30 days", "12%")
   "value" MUST be the canonical name (non-empty)

Relationship line — exact shape:
{"kind":"rel","from":"Sarah Chen","to":"Acme Corp","type":"works_at"}
   "from" and "to" MUST each be the "value" of an entity line you already emitted.
   "type" MUST be one of these snake_case verbs (use the closest match; do not invent new verbs):
   {RELATION_TYPES}

Few-shot examples (mixed domains):

[Legal text]
TEXT: Section 14(2) of the Constitutional Act grants the Supreme Court jurisdiction over interprovincial disputes.
{"kind":"entity","type":"reference","value":"Section 14(2)"}
{"kind":"entity","type":"reference","value":"Constitutional Act"}
{"kind":"entity","type":"org","value":"Supreme Court"}
{"kind":"entity","type":"concept","value":"interprovincial disputes"}
{"kind":"rel","from":"Section 14(2)","to":"Supreme Court","type":"grants_authority_to"}
{"kind":"rel","from":"Constitutional Act","to":"Section 14(2)","type":"contains"}

[Medical text]
TEXT: The RECOVERY trial showed dexamethasone reduced 28-day mortality by 17% in ventilated COVID-19 patients.
{"kind":"entity","type":"reference","value":"RECOVERY trial"}
{"kind":"entity","type":"term","value":"dexamethasone"}
{"kind":"entity","type":"amount","value":"17%"}
{"kind":"entity","type":"term","value":"COVID-19"}
{"kind":"rel","from":"RECOVERY trial","to":"dexamethasone","type":"evaluates"}
{"kind":"rel","from":"dexamethasone","to":"COVID-19","type":"treats"}

[Business text]
TEXT: Riaz Mehmood, CFO of DataSync Ltd, approved the $2.4M acquisition of Apex Analytics on 15 March 2024.
{"kind":"entity","type":"person","value":"Riaz Mehmood"}
{"kind":"entity","type":"org","value":"DataSync Ltd"}
{"kind":"entity","type":"amount","value":"$2.4M"}
{"kind":"entity","type":"org","value":"Apex Analytics"}
{"kind":"entity","type":"date","value":"15 March 2024"}
{"kind":"rel","from":"Riaz Mehmood","to":"DataSync Ltd","type":"works_at"}
{"kind":"rel","from":"DataSync Ltd","to":"Apex Analytics","type":"acquired"}

Rules:
- Emit every entity BEFORE any relationship that references it.
- At most 10 entity lines and at most 10 relationship lines.
- Do not emit a relationship unless both its "from" and "to" appear as entity lines above.
- Never leave "from", "to", "type", or "value" empty.
- If the text has nothing to extract, output exactly one line: {"kind":"none"}

TEXT:
{TEXT}

OUTPUT:
"#;
```

- [ ] **Step 3: Update entity type validation list (line ~207)**

Find:
```rust
                if !evalue.is_empty()
                    && ["person", "org", "project", "decision"].contains(&etype)
```
Replace with:
```rust
                if !evalue.is_empty()
                    && ["person", "org", "location", "concept", "term", "reference", "date", "amount"].contains(&etype)
```

- [ ] **Step 4: Update the `"person" | "org" | "project" | "decision"` match arm (line ~212)**

Find:
```rust
            "person" | "org" | "project" | "decision" => {
                let etype = kind.as_str();
```
Replace with:
```rust
            "person" | "org" | "location" | "concept" | "term" | "reference" | "date" | "amount" => {
                let etype = kind.as_str();
```

- [ ] **Step 5: Update Pass 1 comment at the top of the file (line ~3)**

Find:
```
//! Pass 1 (synchronous, during indexing): regex patterns for dates, amounts, emails.
//! Pass 2 (async background, post-indexing): LLM prompt with structured JSON grammar
//!   for people, orgs, projects, and decisions.
```
Replace with:
```
//! Pass 1 (synchronous, during indexing): regex patterns for dates, amounts, emails.
//! Pass 2 (async background, post-indexing): LLM prompt with structured JSON grammar
//!   for universal entity types: person, org, location, concept, term, reference, date, amount.
```

- [ ] **Step 6: Run `cargo check`**

```bash
cd "/media/riaz/New Volume/echo-AI"
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "^error" | head -20
```

Expected: No errors.

---

## Task 5: Single Rebuild (Fixes 1–4)

- [ ] **Step 1: Verify `cargo check` is clean across all changes**

```bash
cd "/media/riaz/New Volume/echo-AI"
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -E "^error"
```

Expected: Empty output (no errors).

- [ ] **Step 2: Run all unit tests**

```bash
cd "/media/riaz/New Volume/echo-AI"
cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20
```

Expected: All tests pass.

- [ ] **Step 3: Commit Rust fixes**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/services/scheduler.rs \
         src-tauri/src/services/batch_processor.rs \
         src-tauri/src/services/query_router.rs \
         src-tauri/src/services/indexer.rs \
         src-tauri/src/services/entity_extractor.rs \
         src-tauri/src/lib.rs \
         src-tauri/src/db/migrations/009_universal_entity_types.sql
git commit -m "fix: scheduler priority, 256-token limit, NER recovery, universal entity types"
```

---

## Task 6: Chat Scrollbar Fix (Fix 5)

**Files:**
- Modify: `src/components/chat/ChatPanel.tsx`

- [ ] **Step 1: Add `scrollContainerRef` variable (after `messagesEndRef` at line 35)**

Find:
```ts
  let messagesEndRef!: HTMLDivElement;
```
Replace with:
```ts
  let messagesEndRef!: HTMLDivElement;
  let scrollContainerRef!: HTMLDivElement;
```

- [ ] **Step 2: Replace `scrollIntoView` with container-scoped scroll (line 52)**

Find:
```ts
    messagesEndRef?.scrollIntoView({ behavior: "smooth" });
```
Replace with:
```ts
    if (scrollContainerRef) {
      scrollContainerRef.scrollTop = scrollContainerRef.scrollHeight;
    }
```

- [ ] **Step 3: Wire `ref` and change overflow class on the message container (line 98)**

Find:
```tsx
      <div class="flex-1 min-h-0 overflow-y-auto p-4 space-y-4">
```
Replace with:
```tsx
      <div ref={scrollContainerRef} class="flex-1 min-h-0 overflow-y-scroll p-4 space-y-4">
```

- [ ] **Step 4: Remove the now-unused `messagesEndRef` div**

The sentinel `<div ref={messagesEndRef} />` that was placed at the bottom of the message list is no longer needed. Find and remove it. Search for the pattern near the end of the `<For>` block:
```tsx
          <div ref={messagesEndRef} />
```
Delete that line entirely.

- [ ] **Step 5: Verify in browser**

Start the app (`pnpm tauri dev` or existing dev server), open Chat, send 3+ messages, confirm:
- Scrollbar track is always visible (even when fewer messages than viewport height)
- New responses auto-scroll into view without arrow keys
- Page viewport does not jump

- [ ] **Step 6: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src/components/chat/ChatPanel.tsx
git commit -m "fix(chat): always-visible scrollbar + container-scoped auto-scroll"
```

---

## Task 7: KG Relation Filter Dropdown (Fix 6)

**Files:**
- Modify: `src/components/knowledge-graph/GraphCanvas.tsx`

- [ ] **Step 1: Add `selectedRelationTypes` signal**

Find the block where `typeFilter` and other signals are declared (look for `createSignal` calls near the top of the component). Add after the `typeFilter` signal:

```ts
  const [selectedRelationTypes, setSelectedRelationTypes] = createSignal<string[]>([]);
```

- [ ] **Step 2: Derive available relation types from actual edges**

Add after the `selectedRelationTypes` signal:
```ts
  const availableRelationTypes = () => {
    const seen = new Set<string>();
    for (const e of allEdges()) seen.add(e.relation_type);
    return Array.from(seen).sort();
  };
```

- [ ] **Step 3: Update `visibleEdges()` to filter by selected relation types**

Find (lines 357–362):
```ts
  const visibleEdges = () => {
    const ids = new Set(visibleNodes().map((n) => n.id));
    return allEdges().filter(
      (e) => ids.has(e.from_entity_id) && ids.has(e.to_entity_id),
    );
  };
```
Replace with:
```ts
  const visibleEdges = () => {
    const ids = new Set(visibleNodes().map((n) => n.id));
    const selected = selectedRelationTypes();
    return allEdges().filter(
      (e) =>
        ids.has(e.from_entity_id) &&
        ids.has(e.to_entity_id) &&
        (selected.length === 0 || selected.includes(e.relation_type)),
    );
  };
```

- [ ] **Step 4: Add relation type filter dropdown to the toolbar**

Find the toolbar section containing the entity-type `<select>` (around line 434–448):
```tsx
            <label class="text-xs text-gray-400">Filter:</label>
            <select
              value={typeFilter()}
              onChange={(e) => {
                setTypeFilter(e.currentTarget.value);
                setSelectedNode(null);
                setTransform(fitTransform(visibleNodes(), WIDTH, HEIGHT));
              }}
              class="text-xs bg-gray-800 border border-gray-700 text-gray-300 rounded px-2 py-1 focus:outline-none focus:ring-1 focus:ring-blue-500"
            >
              <For each={ENTITY_TYPES}>
                {(t) => <option value={t}>{t === "all" ? "All types" : t}</option>}
              </For>
            </select>
```
Add the relation filter immediately after the closing `</select>` tag:
```tsx
            <Show when={availableRelationTypes().length > 0}>
              <label class="text-xs text-gray-400">Relations:</label>
              <select
                multiple
                size={Math.min(4, availableRelationTypes().length)}
                onChange={(e) => {
                  const opts = Array.from(e.currentTarget.selectedOptions).map((o) => o.value);
                  setSelectedRelationTypes(opts);
                  setTransform(fitTransform(visibleNodes(), WIDTH, HEIGHT));
                }}
                class="text-xs bg-gray-800 border border-gray-700 text-gray-300 rounded px-2 py-1 focus:outline-none focus:ring-1 focus:ring-blue-500"
                title="Hold Ctrl/Cmd to select multiple. No selection = show all."
              >
                <For each={availableRelationTypes()}>
                  {(t) => (
                    <option
                      value={t}
                      selected={selectedRelationTypes().includes(t)}
                    >
                      {t}
                    </option>
                  )}
                </For>
              </select>
            </Show>
```

- [ ] **Step 5: Verify in browser**

Open Knowledge Graph tab with extracted edges:
- Relation filter dropdown appears only when edges exist
- Selecting a relation type filters the graph immediately
- No selection = all edges shown
- Ctrl/Cmd+click selects multiple types
- Custom relations defined in preferences but not yet extracted do NOT appear (derived from actual data only)

- [ ] **Step 6: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src/components/knowledge-graph/GraphCanvas.tsx
git commit -m "feat(kg): relation type filter dropdown derived from extracted edges"
```

---

## Acceptance Criteria

- [ ] Chat query in progress when user navigates to KG tab → query completes normally; no auto-cancel
- [ ] Analyze or Batch in progress when user navigates → completes normally
- [ ] KgExtraction taking foreground does NOT cancel Chat or Batch (verified by new unit tests)
- [ ] Chat taking foreground DOES cancel Pass2Ner (model slot freed for chat — speed priority)
- [ ] Summary answers use 256 tokens — no more mid-sentence truncation
- [ ] Sources with `ner_status='pending'` auto-resume NER on app restart
- [ ] NER extracts `person, org, location, concept, term, reference` entities from legal + general docs
- [ ] Chat scrollbar always visible; new responses auto-scroll into view; page viewport never jumps
- [ ] KG edge filter shows only relation types extracted from current graph; filtering is immediate
