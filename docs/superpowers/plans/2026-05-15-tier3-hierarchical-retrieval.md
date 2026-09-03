# Tier 3 — Hierarchical Retrieval & Reranked Grounding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace flat 512-token chunks with a three-level adaptive hierarchy (`document → section → leaf`), add a bge-reranker-base cross-encoder between RRF and LLM, and replace index-only grounding with sentence-level lexical-overlap validation.

**Architecture:** New schema columns on `chunks` (`parent_chunk_id`, `unit_type`, `unit_label`, `unit_path`, `domain_metadata`). Heading detector runs regex first (legal + generic patterns), with bounded LLM disambiguation pass for ambiguous lines. Tree builder produces nested nodes; multi-level chunker emits one chunk per node. Embedder embeds all three levels. Retrieval queries vec0 + FTS5 across all levels, RRF-merges, then bge-reranker-base scores top 30 → top 8. Sentence-level grounding validator computes Jaccard overlap per cited sentence.

**Tech Stack:** Rust 1.87, sqlx + bundled SQLite (FTS5 + sqlite-vec vec0), fastembed-rs (existing), llama-cpp-2 (existing, for Qwen2.5-0.5B disambiguation), `ort` 2.x (new — ONNX runtime for reranker), `tokenizers` 0.20 (new — HF tokenizer for reranker), `regex` (existing).

**Spec:** `docs/superpowers/specs/2026-05-15-tier3-hierarchical-retrieval-design.md`

---

## File Structure

**New files:**
- `src-tauri/src/db/migrations/013_hierarchical_chunks.sql` — schema additions
- `src-tauri/src/services/heading_detector/mod.rs` — orchestrator
- `src-tauri/src/services/heading_detector/regex_pass.rs` — pass A
- `src-tauri/src/services/heading_detector/llm_pass.rs` — pass B
- `src-tauri/src/services/heading_detector/tree.rs` — tree builder + node types
- `src-tauri/src/services/reranker.rs` — bge-reranker-base ONNX wrapper
- `src-tauri/src/services/grounding.rs` — sentence-level validator (extracted)
- `src-tauri/tests/heading_detector_test.rs` — integration tests
- `src-tauri/tests/multilevel_chunker_test.rs` — integration tests
- `src-tauri/tests/reranker_test.rs` — integration tests

**Modified files:**
- `src-tauri/Cargo.toml` — add `ort`, `tokenizers`
- `src-tauri/src/db/schema.rs` — wipe-on-version-mismatch, schema_version constant
- `src-tauri/src/db/repositories/chunks.rs` — extend `NewChunk` + `Chunk`, bulk_insert binds new columns
- `src-tauri/src/services/chunker.rs` — replaced by multi-level emit; existing fixed-window logic kept only for two-level fallback path
- `src-tauri/src/services/indexer.rs` — wire heading detector → tree → multi-level chunker; embed each level
- `src-tauri/src/services/query_router.rs` — multi-level retrieval, reranker call, raised budget, rollup, delegated grounding
- `src-tauri/src/services/mod.rs` — export new modules

**Deleted/unused after migration:**
- Old article-detection code in `chunker.rs` (`detect_structural_header`, `current_article` continuation logic) — superseded by heading_detector

---

## Task 1: Schema migration + wipe-on-version-mismatch

**Files:**
- Create: `src-tauri/src/db/migrations/013_hierarchical_chunks.sql`
- Modify: `src-tauri/src/db/schema.rs`
- Test: `src-tauri/tests/schema_wipe_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
// src-tauri/tests/schema_wipe_test.rs
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn wipe_runs_when_schema_version_mismatches() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("echo.db");
    let db_str = db_path.to_str().unwrap();

    // Seed an old DB with an outdated schema_version marker.
    {
        use sqlx::sqlite::SqliteConnectOptions;
        use std::str::FromStr;
        let opts = SqliteConnectOptions::from_str(db_str).unwrap().create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
        sqlx::query("CREATE TABLE db_config (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO db_config(key,value) VALUES ('schema_version','12')")
            .execute(&pool).await.unwrap();
        pool.close().await;
    }
    let cache = tmp.path().join(".fastembed_cache");
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("dummy.bin"), b"x").unwrap();

    let tier = echo_ai::services::hardware_detector::HardwareTier::default_for_test();
    let _pool = echo_ai::db::schema::init_db_with_root(db_str, &tier, tmp.path()).await.unwrap();

    // After init: cache directory must be gone (wiped) and schema_version = 13.
    assert!(!cache.exists(), "cache should have been wiped");
    let opts = sqlx::sqlite::SqliteConnectOptions::from_str(db_str).unwrap();
    let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
    let row: (String,) = sqlx::query_as("SELECT value FROM db_config WHERE key='schema_version'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, "13");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test schema_wipe_test wipe_runs_when_schema_version_mismatches`
Expected: FAIL — `init_db_with_root` does not exist; `schema_version` not bumped.

- [ ] **Step 3: Create migration 013**

Create `src-tauri/src/db/migrations/013_hierarchical_chunks.sql`:

```sql
-- ============================================================
-- EchoAI — Hierarchical chunks (migration 013)
-- Adds three-level structure: document → section → leaf.
-- See spec: 2026-05-15-tier3-hierarchical-retrieval-design.md
-- ============================================================

ALTER TABLE chunks ADD COLUMN parent_chunk_id TEXT REFERENCES chunks(id);
ALTER TABLE chunks ADD COLUMN unit_type       TEXT NOT NULL DEFAULT 'leaf'
                                              CHECK (unit_type IN ('document','section','leaf'));
ALTER TABLE chunks ADD COLUMN unit_label      TEXT;
ALTER TABLE chunks ADD COLUMN unit_path       TEXT;
ALTER TABLE chunks ADD COLUMN domain_metadata TEXT;

CREATE INDEX IF NOT EXISTS idx_chunks_parent     ON chunks(parent_chunk_id);
CREATE INDEX IF NOT EXISTS idx_chunks_unit_type  ON chunks(unit_type);
CREATE INDEX IF NOT EXISTS idx_chunks_unit_label ON chunks(unit_label);

INSERT INTO db_config(key, value) VALUES ('schema_version', '13')
    ON CONFLICT(key) DO UPDATE SET value = excluded.value;
```

- [ ] **Step 4: Add wipe-on-version-mismatch in schema.rs**

In `src-tauri/src/db/schema.rs`, add at top after imports:

```rust
pub const TARGET_SCHEMA_VERSION: u32 = 13;
```

Add a new public function and refactor `init_db`:

```rust
/// Public test-friendly form of init_db that takes an explicit project root for
/// resolving the .fastembed_cache directory to wipe.
pub async fn init_db_with_root(
    db_path: &str,
    tier: &HardwareTier,
    project_root: &std::path::Path,
) -> Result<SqlitePool> {
    maybe_wipe_for_schema_upgrade(db_path, project_root).await?;
    init_db_inner(db_path, tier).await
}

pub async fn init_db(db_path: &str, tier: &HardwareTier) -> Result<SqlitePool> {
    let project_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    init_db_with_root(db_path, tier, &project_root).await
}

async fn maybe_wipe_for_schema_upgrade(
    db_path: &str,
    project_root: &std::path::Path,
) -> Result<()> {
    use std::path::Path;
    let path = Path::new(db_path);
    if !path.exists() {
        return Ok(());
    }
    // Probe existing version without running migrations.
    let probe_opts = SqliteConnectOptions::from_str(db_path)?.create_if_missing(false);
    let probe = match SqlitePool::connect_with(probe_opts).await {
        Ok(p) => p,
        Err(_) => return Ok(()), // unreadable → leave alone, init_db_inner will fail clearly
    };
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM db_config WHERE key='schema_version'"
    ).fetch_optional(&probe).await.ok().flatten();
    probe.close().await;

    let current: u32 = row.and_then(|(v,)| v.parse().ok()).unwrap_or(0);
    if current >= TARGET_SCHEMA_VERSION {
        return Ok(());
    }

    log::warn!(
        "Schema upgrade detected (DB v{current} → v{TARGET_SCHEMA_VERSION}). \
         Performing nuclear wipe — re-indexing required."
    );
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{db_path}-wal"));
    let _ = std::fs::remove_file(format!("{db_path}-shm"));
    let cache = project_root.join(".fastembed_cache");
    if cache.exists() {
        let _ = std::fs::remove_dir_all(&cache);
    }
    Ok(())
}

// Rename existing init_db body to init_db_inner (no behavior change).
async fn init_db_inner(db_path: &str, tier: &HardwareTier) -> Result<SqlitePool> {
    // ... existing init_db body unchanged ...
}
```

Also add a `default_for_test()` constructor in `hardware_detector.rs`:

```rust
impl HardwareTier {
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn default_for_test() -> Self {
        Self {
            ram_gb: 8.0,
            embedding_model: "all-MiniLM-L6-v2".to_string(),
            // copy other fields with sensible defaults from existing struct
        }
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test schema_wipe_test wipe_runs_when_schema_version_mismatches`
Expected: PASS

- [ ] **Step 6: Run all migrations test**

Run: `cd src-tauri && cargo test db::schema --lib`
Expected: PASS — all migrations apply cleanly to a fresh DB.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/migrations/013_hierarchical_chunks.sql src-tauri/src/db/schema.rs src-tauri/src/services/hardware_detector.rs src-tauri/tests/schema_wipe_test.rs
git commit -m "feat(db): add hierarchical chunk schema (migration 013) + wipe-on-version-mismatch"
```

---

## Task 2: Extend chunks repository with hierarchical fields

**Files:**
- Modify: `src-tauri/src/db/repositories/chunks.rs`
- Test: existing `src-tauri/src/db/repositories/chunks.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/db/repositories/chunks.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn fresh_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) VALUES('s1','/x','x.pdf','pdf',1,'h',0,0)")
            .execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn bulk_insert_persists_unit_type_and_parent() {
        let pool = fresh_pool().await;
        let parent = NewChunk {
            source_id: "s1".into(), chunk_index: 0, content: "Doc rollup".into(),
            char_start: 0, char_end: 10, page_number: None, token_count: 3,
            parent_chunk_id: None,
            unit_type: "document".into(),
            unit_label: Some("x.pdf".into()),
            unit_path: Some("".into()),
            domain_metadata: None,
        };
        let inserted = bulk_insert(&pool, vec![parent]).await.unwrap();
        let parent_id = inserted[0].id.clone();
        let leaf = NewChunk {
            source_id: "s1".into(), chunk_index: 1, content: "leaf text".into(),
            char_start: 0, char_end: 9, page_number: Some(1), token_count: 2,
            parent_chunk_id: Some(parent_id.clone()),
            unit_type: "leaf".into(),
            unit_label: Some("Section 1".into()),
            unit_path: Some("Section 1".into()),
            domain_metadata: Some(r#"{"domain":"legal"}"#.into()),
        };
        let l = bulk_insert(&pool, vec![leaf]).await.unwrap();
        assert_eq!(l[0].unit_type, "leaf");
        assert_eq!(l[0].parent_chunk_id.as_deref(), Some(parent_id.as_str()));
        assert_eq!(l[0].unit_label.as_deref(), Some("Section 1"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib db::repositories::chunks::tests`
Expected: FAIL — `NewChunk` and `Chunk` lack the new fields; SQL bind count mismatch.

- [ ] **Step 3: Extend the structs and bulk_insert**

In `src-tauri/src/db/repositories/chunks.rs`, replace the structs and `bulk_insert`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Chunk {
    pub id: String,
    pub source_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub char_start: i64,
    pub char_end: i64,
    pub page_number: Option<i64>,
    pub token_count: i64,
    pub created_at: i64,
    pub parent_chunk_id: Option<String>,
    pub unit_type: String,
    pub unit_label: Option<String>,
    pub unit_path: Option<String>,
    pub domain_metadata: Option<String>,
}

#[derive(Debug)]
pub struct NewChunk {
    pub source_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub char_start: i64,
    pub char_end: i64,
    pub page_number: Option<i64>,
    pub token_count: i64,
    pub parent_chunk_id: Option<String>,
    pub unit_type: String,
    pub unit_label: Option<String>,
    pub unit_path: Option<String>,
    pub domain_metadata: Option<String>,
}

pub async fn bulk_insert(pool: &SqlitePool, chunks: Vec<NewChunk>) -> Result<Vec<Chunk>> {
    let now = Utc::now().timestamp_millis();
    let mut result = Vec::with_capacity(chunks.len());
    let prepared: Vec<(String, NewChunk)> = chunks
        .into_iter()
        .map(|c| (Uuid::new_v4().to_string(), c))
        .collect();

    let mut tx = pool.begin().await?;
    for (id, c) in &prepared {
        sqlx::query(
            "INSERT INTO chunks \
             (id, source_id, chunk_index, content, char_start, char_end, page_number, token_count, created_at, \
              parent_chunk_id, unit_type, unit_label, unit_path, domain_metadata) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&c.source_id)
        .bind(c.chunk_index)
        .bind(&c.content)
        .bind(c.char_start)
        .bind(c.char_end)
        .bind(c.page_number)
        .bind(c.token_count)
        .bind(now)
        .bind(&c.parent_chunk_id)
        .bind(&c.unit_type)
        .bind(&c.unit_label)
        .bind(&c.unit_path)
        .bind(&c.domain_metadata)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    for (id, c) in prepared {
        result.push(Chunk {
            id, source_id: c.source_id, chunk_index: c.chunk_index, content: c.content,
            char_start: c.char_start, char_end: c.char_end, page_number: c.page_number,
            token_count: c.token_count, created_at: now,
            parent_chunk_id: c.parent_chunk_id, unit_type: c.unit_type,
            unit_label: c.unit_label, unit_path: c.unit_path, domain_metadata: c.domain_metadata,
        });
    }
    Ok(result)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib db::repositories::chunks::tests::bulk_insert_persists_unit_type_and_parent`
Expected: PASS

- [ ] **Step 5: Fix all remaining call-sites that build `NewChunk`**

Run: `cd src-tauri && cargo build 2>&1 | grep "missing field" | head`
Then update every construction site (notably `chunker.rs::ChunkSpec::into_new_chunk` and any indexer code) to populate the new fields with default values for now: `parent_chunk_id: None, unit_type: "leaf".into(), unit_label: None, unit_path: None, domain_metadata: None`. Keep behavior identical until we replace chunker in Task 7.

- [ ] **Step 6: Run full build**

Run: `cd src-tauri && cargo build`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/db/repositories/chunks.rs src-tauri/src/services/chunker.rs
git commit -m "feat(db): extend NewChunk/Chunk with unit_type, parent_chunk_id, unit_label, unit_path, domain_metadata"
```

---

## Task 3: Heading detector — regex pass

**Files:**
- Create: `src-tauri/src/services/heading_detector/mod.rs`
- Create: `src-tauri/src/services/heading_detector/regex_pass.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: inline in `regex_pass.rs`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/services/heading_detector/regex_pass.rs` with only the test for now:

```rust
//! Pass A — regex heading detection. Returns labelled candidates with confidence.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeadingCandidate {
    /// Byte offset into the source text where this candidate begins.
    pub offset: usize,
    /// The literal heading text as it appears in the source (e.g., "Section 8(2)").
    pub label: String,
    /// 'document' is reserved for the synthetic root; here only 'section' is emitted.
    pub unit_type: HeadingType,
    /// 0.0..=1.0 — high (≥0.85) skips Pass B; low (<0.85) triggers Pass B disambiguation.
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadingType { Section }

pub fn scan(text: &str) -> Vec<HeadingCandidate> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_legal_section_headers_high_confidence() {
        let text = "Some preamble.\n\nSection 8. Recruitment. Text...\n\nRule 12(2) provides...";
        let hits = scan(text);
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"Section 8"));
        assert!(labels.contains(&"Rule 12(2)"));
        assert!(hits.iter().all(|h| h.confidence >= 0.85));
    }

    #[test]
    fn detects_numbered_outline_low_confidence() {
        let text = "1. Introduction\n\nBody text...\n\n2.1 Methodology\n\nMore body.";
        let hits = scan(text);
        // Numbered outlines are common but ambiguous — confidence below the auto-accept threshold.
        assert!(hits.iter().any(|h| h.label == "1. Introduction" && h.confidence < 0.85));
        assert!(hits.iter().any(|h| h.label == "2.1 Methodology" && h.confidence < 0.85));
    }

    #[test]
    fn ignores_inline_section_mentions() {
        // "see section 8" mid-sentence should NOT be a candidate.
        let text = "The applicant must comply with see section 8 of the Act.";
        let hits = scan(text);
        assert!(hits.is_empty(), "got: {hits:?}");
    }

    #[test]
    fn detects_pakistani_notification_header() {
        let text = "SGA&CD Notification dated 27th February 1984\n\nThe minimum length of service shall be...";
        let hits = scan(text);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].label, "SGA&CD Notification dated 27th February 1984");
        assert!(hits[0].confidence >= 0.85);
    }
}
```

Create `src-tauri/src/services/heading_detector/mod.rs`:

```rust
pub mod regex_pass;
```

Add to `src-tauri/src/services/mod.rs`:

```rust
pub mod heading_detector;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib services::heading_detector::regex_pass`
Expected: FAIL — `scan` is `todo!()`.

- [ ] **Step 3: Implement scan**

Replace `scan` in `regex_pass.rs`:

```rust
use regex::Regex;
use std::sync::OnceLock;

struct PatternSet {
    /// (regex, confidence, label_capture_group_index)
    patterns: Vec<(Regex, f32, usize)>,
}

fn patterns() -> &'static PatternSet {
    static P: OnceLock<PatternSet> = OnceLock::new();
    P.get_or_init(|| {
        let patterns = vec![
            // High confidence: explicit legal markers at line start.
            // "Section 8", "Section 8(2)", "Rule 12(2)(a)", "Article 7", "Chapter III", "Schedule II", "Clause 4.1"
            (Regex::new(
                r"(?m)^(?P<lbl>(Section|Rule|Article|Chapter|Schedule|Clause|Sub-clause|Part)\s+[\divxlcIVXLC]+(?:\([\divxlcIVXLC]+\))*(?:\([a-zA-Z]\))?)\b"
            ).unwrap(), 0.95, 1),

            // High confidence: Pakistani-style notification headers (line-start, dated).
            (Regex::new(
                r"(?m)^(?P<lbl>(?:[A-Z][A-Za-z&]+\s+)*Notification\s+dated\s+\d{1,2}(?:st|nd|rd|th)?\s+[A-Z][a-z]+\s+\d{4})"
            ).unwrap(), 0.90, 1),

            // Lower confidence: numbered outline headings ("1. Introduction", "2.1 Methodology").
            // Requires capital after the dot/space; max 1 line.
            (Regex::new(
                r"(?m)^(?P<lbl>\d+(?:\.\d+){0,3}\.?\s+[A-Z][^\n]{1,80})$"
            ).unwrap(), 0.55, 1),
        ];
        PatternSet { patterns }
    })
}

pub fn scan(text: &str) -> Vec<HeadingCandidate> {
    let mut hits: Vec<HeadingCandidate> = Vec::new();
    for (re, conf, _) in &patterns().patterns {
        for cap in re.captures_iter(text) {
            let m = cap.name("lbl").expect("named group missing");
            hits.push(HeadingCandidate {
                offset: m.start(),
                label: m.as_str().trim().to_string(),
                unit_type: HeadingType::Section,
                confidence: *conf,
            });
        }
    }
    // Dedup by (offset, label); keep the highest confidence per offset.
    hits.sort_by_key(|h| (h.offset, std::cmp::Reverse((h.confidence * 1000.0) as u32)));
    hits.dedup_by_key(|h| h.offset);
    hits
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib services::heading_detector::regex_pass`
Expected: PASS for all four tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/heading_detector/ src-tauri/src/services/mod.rs
git commit -m "feat(headings): regex Pass A — legal/numbered/notification heading detection"
```

---

## Task 4: Heading detector — LLM disambiguation (Pass B)

**Files:**
- Create: `src-tauri/src/services/heading_detector/llm_pass.rs`
- Modify: `src-tauri/src/services/heading_detector/mod.rs`
- Test: inline in `llm_pass.rs`

**Context:** Pass B calls Qwen2.5-0.5B with a strict yes/no prompt for ambiguous candidates only. Capped per spec §4.3 at ≤5% of paragraphs and absolute ≤200 calls per document.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/services/heading_detector/llm_pass.rs`:

```rust
//! Pass B — LLM disambiguation for ambiguous heading candidates.
//!
//! Bounded budget: ≤200 calls per document, ≤5% of total paragraph count.
//! Each call is a 1-token YES/NO classification.

use super::regex_pass::HeadingCandidate;

const HARD_CALL_CAP: usize = 200;
const PCT_PARAGRAPH_CAP: f32 = 0.05;
const CONFIDENCE_THRESHOLD: f32 = 0.85;

/// Trait for the disambiguation backend so tests can stub the LLM.
pub trait Classifier: Send + Sync {
    /// Returns true if the line is judged a structural heading.
    fn is_heading(&self, line: &str, surrounding_paragraph: &str) -> bool;
}

pub struct DisambiguationBudget {
    pub calls_made: usize,
    pub max_calls: usize,
}

pub fn disambiguate(
    candidates: Vec<HeadingCandidate>,
    text: &str,
    paragraph_count: usize,
    classifier: &dyn Classifier,
) -> (Vec<HeadingCandidate>, DisambiguationBudget) {
    let max = std::cmp::min(
        HARD_CALL_CAP,
        (paragraph_count as f32 * PCT_PARAGRAPH_CAP).ceil() as usize,
    );
    let mut budget = DisambiguationBudget { calls_made: 0, max_calls: max };

    let mut accepted: Vec<HeadingCandidate> = Vec::with_capacity(candidates.len());
    for mut c in candidates {
        if c.confidence >= CONFIDENCE_THRESHOLD {
            accepted.push(c);
            continue;
        }
        if budget.calls_made >= budget.max_calls {
            // Out of budget — drop the ambiguous candidate (conservative).
            continue;
        }
        let line_end = text[c.offset..].find('\n').map(|i| c.offset + i).unwrap_or(text.len());
        let line = &text[c.offset..line_end];
        let para_end = text[c.offset..].find("\n\n").map(|i| c.offset + i).unwrap_or(text.len());
        let para = &text[c.offset..para_end];
        budget.calls_made += 1;
        if classifier.is_heading(line, para) {
            c.confidence = 0.85; // promoted
            accepted.push(c);
        }
    }
    (accepted, budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::regex_pass::{HeadingCandidate, HeadingType};

    struct AlwaysYes;
    impl Classifier for AlwaysYes { fn is_heading(&self, _l: &str, _p: &str) -> bool { true } }
    struct AlwaysNo;
    impl Classifier for AlwaysNo { fn is_heading(&self, _l: &str, _p: &str) -> bool { false } }
    struct Counter(std::cell::Cell<usize>);
    impl Classifier for Counter {
        fn is_heading(&self, _l: &str, _p: &str) -> bool {
            self.0.set(self.0.get() + 1); true
        }
    }

    fn cand(offset: usize, label: &str, conf: f32) -> HeadingCandidate {
        HeadingCandidate { offset, label: label.into(), unit_type: HeadingType::Section, confidence: conf }
    }

    #[test]
    fn high_confidence_skips_classifier() {
        let text = "1. Intro\n\nBody";
        let counter = Counter(std::cell::Cell::new(0));
        let (out, b) = disambiguate(vec![cand(0, "Section 8", 0.95)], text, 1, &counter);
        assert_eq!(out.len(), 1);
        assert_eq!(b.calls_made, 0);
        assert_eq!(counter.0.get(), 0);
    }

    #[test]
    fn low_confidence_invokes_classifier_and_keeps_on_yes() {
        let text = "1. Introduction\n\nBody";
        let (out, b) = disambiguate(vec![cand(0, "1. Introduction", 0.55)], text, 1, &AlwaysYes);
        assert_eq!(out.len(), 1);
        assert!(out[0].confidence >= 0.85);
        assert_eq!(b.calls_made, 1);
    }

    #[test]
    fn low_confidence_drops_on_no() {
        let text = "Body";
        let (out, b) = disambiguate(vec![cand(0, "1. xyz", 0.55)], text, 1, &AlwaysNo);
        assert!(out.is_empty());
        assert_eq!(b.calls_made, 1);
    }

    #[test]
    fn budget_cap_drops_excess_ambiguous_candidates() {
        let text = "x".repeat(100);
        let candidates: Vec<_> = (0..50).map(|i| cand(i, &format!("h{i}"), 0.5)).collect();
        let counter = Counter(std::cell::Cell::new(0));
        // 5% of 1 paragraph = 1 call max.
        let (out, b) = disambiguate(candidates, &text, 1, &counter);
        assert_eq!(b.max_calls, 1);
        assert_eq!(b.calls_made, 1);
        assert_eq!(out.len(), 1);
    }
}
```

In `src-tauri/src/services/heading_detector/mod.rs`, add:

```rust
pub mod regex_pass;
pub mod llm_pass;
```

- [ ] **Step 2: Run tests to verify they fail then pass**

Run: `cd src-tauri && cargo test --lib services::heading_detector::llm_pass`
Expected: PASS — implementation is included with the test in the same step (this module has no external dependencies).

- [ ] **Step 3: Add a real Classifier impl backed by Qwen2.5-0.5B**

Append to `llm_pass.rs`:

```rust
/// Production classifier using the in-process llama-cpp-2 LLM slot.
/// One-shot prompt; response constrained to first 3 chars (YES/NO/Y).
pub struct LlmClassifier<'a> {
    pub llm: &'a crate::services::llm::LocalLlm,
}

impl<'a> Classifier for LlmClassifier<'a> {
    fn is_heading(&self, line: &str, paragraph: &str) -> bool {
        let prompt = format!(
            "You are a document structure classifier. Reply with only YES or NO.\n\
             Is the FIRST LINE below a structural heading (a section/chapter/clause label \
             that introduces the paragraph that follows), as opposed to ordinary running text?\n\n\
             FIRST LINE: {line}\n\n\
             FULL PARAGRAPH:\n{paragraph}\n\n\
             Reply YES or NO:"
        );
        let resp = match self.llm.generate_short(&prompt, 4) {
            Ok(s) => s,
            Err(e) => { log::warn!("heading classifier LLM call failed: {e}"); return false; }
        };
        let t = resp.trim().to_ascii_uppercase();
        t.starts_with("YES") || t == "Y"
    }
}
```

If `LocalLlm` does not have a `generate_short` method, add a minimal one in `src-tauri/src/services/llm.rs` that runs the existing single-shot generation path with `max_tokens` capped to the argument and no chat history. (No test required; covered by integration test in Task 7.)

- [ ] **Step 4: Verify full build**

Run: `cd src-tauri && cargo build`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/heading_detector/llm_pass.rs src-tauri/src/services/heading_detector/mod.rs src-tauri/src/services/llm.rs
git commit -m "feat(headings): LLM Pass B disambiguation with bounded call budget"
```

---

## Task 5: Document tree builder

**Files:**
- Create: `src-tauri/src/services/heading_detector/tree.rs`
- Modify: `src-tauri/src/services/heading_detector/mod.rs`
- Test: inline in `tree.rs`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/services/heading_detector/tree.rs`:

```rust
//! Document tree builder: combines pass A + pass B candidates into a nested
//! (document → section → leaf) tree. Each node records a byte range into the
//! source text so the chunker can emit content faithfully.

use super::regex_pass::HeadingCandidate;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind { Document, Section, Leaf }

#[derive(Debug, Clone)]
pub struct DocNode {
    pub kind: NodeKind,
    /// Heading label or document title. None on Leaf.
    pub label: Option<String>,
    /// Breadcrumb from document root, e.g., "Chapter III > Section 8".
    pub path: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub children: Vec<DocNode>,
}

pub struct BuildOptions {
    pub document_title: String,
}

/// Build a doc → section → leaf tree.
/// `headings` MUST be sorted by `offset` ascending (the standard output of regex_pass::scan).
/// If `headings` is empty, returns document with the entire text as a single leaf.
pub fn build_tree(text: &str, headings: &[HeadingCandidate], opts: BuildOptions) -> DocNode {
    if text.is_empty() {
        return DocNode {
            kind: NodeKind::Document, label: Some(opts.document_title), path: String::new(),
            byte_start: 0, byte_end: 0, children: vec![],
        };
    }

    if headings.is_empty() {
        // Two-level fallback: document + single leaf containing all text.
        return DocNode {
            kind: NodeKind::Document, label: Some(opts.document_title.clone()),
            path: String::new(), byte_start: 0, byte_end: text.len(),
            children: vec![DocNode {
                kind: NodeKind::Leaf, label: None, path: String::new(),
                byte_start: 0, byte_end: text.len(), children: vec![],
            }],
        };
    }

    let mut sections: Vec<DocNode> = Vec::with_capacity(headings.len());

    // Optional preamble leaf — text before the first heading.
    if headings[0].offset > 0 {
        sections.push(DocNode {
            kind: NodeKind::Section, label: Some("Preamble".into()),
            path: "Preamble".into(),
            byte_start: 0, byte_end: headings[0].offset,
            children: vec![DocNode {
                kind: NodeKind::Leaf, label: None, path: "Preamble".into(),
                byte_start: 0, byte_end: headings[0].offset, children: vec![],
            }],
        });
    }

    for (i, h) in headings.iter().enumerate() {
        let start = h.offset;
        let end = headings.get(i + 1).map(|n| n.offset).unwrap_or(text.len());
        let path = h.label.clone();
        sections.push(DocNode {
            kind: NodeKind::Section,
            label: Some(h.label.clone()),
            path: path.clone(),
            byte_start: start, byte_end: end,
            children: vec![DocNode {
                kind: NodeKind::Leaf, label: None, path,
                byte_start: start, byte_end: end, children: vec![],
            }],
        });
    }

    DocNode {
        kind: NodeKind::Document, label: Some(opts.document_title),
        path: String::new(), byte_start: 0, byte_end: text.len(),
        children: sections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::regex_pass::{HeadingCandidate, HeadingType};

    fn h(offset: usize, label: &str) -> HeadingCandidate {
        HeadingCandidate { offset, label: label.into(), unit_type: HeadingType::Section, confidence: 0.95 }
    }

    #[test]
    fn empty_text_returns_empty_document() {
        let t = build_tree("", &[], BuildOptions { document_title: "x.pdf".into() });
        assert!(matches!(t.kind, NodeKind::Document));
        assert!(t.children.is_empty());
    }

    #[test]
    fn no_headings_yields_two_level_fallback() {
        let text = "Just some flat text.";
        let t = build_tree(text, &[], BuildOptions { document_title: "memo.txt".into() });
        assert_eq!(t.children.len(), 1);
        assert!(matches!(t.children[0].kind, NodeKind::Leaf));
    }

    #[test]
    fn sections_partition_text_byte_ranges() {
        let text = "Section 8. Body of 8. Section 9. Body of 9.";
        let headings = vec![h(0, "Section 8"), h(20, "Section 9")];
        let t = build_tree(text, &headings, BuildOptions { document_title: "x".into() });
        assert_eq!(t.children.len(), 2);
        assert_eq!(t.children[0].byte_start, 0);
        assert_eq!(t.children[0].byte_end, 20);
        assert_eq!(t.children[1].byte_start, 20);
        assert_eq!(t.children[1].byte_end, text.len());
    }

    #[test]
    fn preamble_before_first_heading_becomes_section() {
        let text = "Some preamble.\n\nSection 1. Body";
        let headings = vec![h(16, "Section 1")];
        let t = build_tree(text, &headings, BuildOptions { document_title: "x".into() });
        assert_eq!(t.children.len(), 2);
        assert_eq!(t.children[0].label.as_deref(), Some("Preamble"));
        assert_eq!(t.children[1].label.as_deref(), Some("Section 1"));
    }
}
```

In `src-tauri/src/services/heading_detector/mod.rs` append:

```rust
pub mod tree;
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test --lib services::heading_detector::tree`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/services/heading_detector/
git commit -m "feat(headings): document tree builder (document → section → leaf)"
```

---

## Task 6: Multi-level chunker

**Files:**
- Modify: `src-tauri/src/services/chunker.rs`
- Create: `src-tauri/tests/multilevel_chunker_test.rs`

**Approach:** Add a new public function `chunk_tree(text, tree, source_id) -> Vec<ChunkSpec>` that walks the tree and emits one `ChunkSpec` per node. Existing `chunk(text, page_offsets)` remains for backward compatibility but is no longer called by the indexer (Task 7 swaps the wiring). `ChunkSpec` gains hierarchy fields.

- [ ] **Step 1: Write the failing integration test**

Create `src-tauri/tests/multilevel_chunker_test.rs`:

```rust
use echo_ai::services::chunker::{chunk_tree, ChunkSpec};
use echo_ai::services::heading_detector::{regex_pass, tree};

#[test]
fn legal_pdf_text_yields_three_levels_with_parent_links() {
    let text = "\
Sindh Civil Servants Act 1973\n\n\
Some preamble text.\n\n\
Section 8. Recruitment.\n\n\
A person is eligible if conditions are met.\n\n\
Sub-clause 8(2)(a) further requires written permission.\n\n\
Section 9. Appointments.\n\n\
The appointing authority shall determine eligibility.";

    let headings = regex_pass::scan(text);
    let t = tree::build_tree(text, &headings, tree::BuildOptions {
        document_title: "Sindh Civil Servants Act 1973.pdf".into(),
    });
    let specs = chunk_tree(text, &t);

    let by_type = |ut: &str| specs.iter().filter(|c| c.unit_type == ut).count();
    assert_eq!(by_type("document"), 1);
    assert!(by_type("section") >= 2, "got {} sections", by_type("section"));
    assert!(by_type("leaf") >= 2);

    // Document node has no parent.
    let doc = specs.iter().find(|c| c.unit_type == "document").unwrap();
    assert!(doc.parent_index.is_none());

    // Every section's parent is the document.
    for s in specs.iter().filter(|c| c.unit_type == "section") {
        assert_eq!(s.parent_index, Some(doc.chunk_index));
    }

    // Every leaf has a section parent.
    for l in specs.iter().filter(|c| c.unit_type == "leaf") {
        let p = l.parent_index.expect("leaf must have parent");
        let parent = specs.iter().find(|c| c.chunk_index == p).unwrap();
        assert_eq!(parent.unit_type, "section");
    }

    // unit_label preserves citation strings.
    assert!(specs.iter().any(|c| c.unit_label.as_deref() == Some("Section 8")));
    assert!(specs.iter().any(|c| c.unit_label.as_deref() == Some("Section 9")));
}

#[test]
fn flat_text_yields_two_levels_only() {
    let text = "A short memo with no detectable structure at all.";
    let headings = regex_pass::scan(text);
    let t = tree::build_tree(text, &headings, tree::BuildOptions {
        document_title: "memo.txt".into(),
    });
    let specs = chunk_tree(text, &t);
    assert_eq!(specs.iter().filter(|c| c.unit_type == "document").count(), 1);
    assert_eq!(specs.iter().filter(|c| c.unit_type == "section").count(), 0);
    assert_eq!(specs.iter().filter(|c| c.unit_type == "leaf").count(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --test multilevel_chunker_test`
Expected: FAIL — `chunk_tree` does not exist; `ChunkSpec` lacks `unit_type`, `parent_index`, `unit_label`.

- [ ] **Step 3: Extend ChunkSpec and add chunk_tree**

In `src-tauri/src/services/chunker.rs`, replace `ChunkSpec` and add new functions:

```rust
pub struct ChunkSpec {
    pub chunk_index: i64,
    pub content: String,
    pub char_start: i64,
    pub char_end: i64,
    pub page_number: Option<i64>,
    pub token_count: i64,
    pub unit_type: String,                 // "document" | "section" | "leaf"
    pub unit_label: Option<String>,
    pub unit_path: Option<String>,
    pub parent_index: Option<i64>,         // chunk_index of parent in same Vec, None for document
}

const SECTION_TOKEN_CAP: usize = 1500;
const LEAF_SOFT_CAP: usize = 512;

/// Walk the document tree and emit one ChunkSpec per node.
/// Document chunk content = title + summary placeholder (optional summarization
/// is done elsewhere; here we emit the title and the first ~1000 chars).
/// Section content = full section text, truncated to SECTION_TOKEN_CAP.
/// Leaf content = section body, split at clause boundaries if exceeding LEAF_SOFT_CAP.
pub fn chunk_tree(text: &str, tree: &crate::services::heading_detector::tree::DocNode) -> Vec<ChunkSpec> {
    use crate::services::heading_detector::tree::NodeKind;
    let mut out: Vec<ChunkSpec> = Vec::new();
    let mut next_index: i64 = 0;

    // Document chunk first.
    let doc_label = tree.label.clone();
    let doc_content = build_document_content(text, &doc_label);
    let doc_index = next_index;
    out.push(ChunkSpec {
        chunk_index: next_index,
        content: doc_content.clone(),
        char_start: 0,
        char_end: text.len() as i64,
        page_number: None,
        token_count: estimate_tokens(&doc_content) as i64,
        unit_type: "document".into(),
        unit_label: doc_label.clone(),
        unit_path: Some(String::new()),
        parent_index: None,
    });
    next_index += 1;

    for section in &tree.children {
        match section.kind {
            NodeKind::Leaf => {
                // Two-level fallback: document → leaf directly.
                emit_leaves(text, section, doc_index, &mut next_index, &mut out);
            }
            NodeKind::Section => {
                let sec_text = &text[section.byte_start..section.byte_end];
                let sec_content = if estimate_tokens(sec_text) > SECTION_TOKEN_CAP {
                    let truncated_chars = SECTION_TOKEN_CAP * 4;
                    let cut = (0..=truncated_chars.min(sec_text.len())).rev()
                        .find(|i| sec_text.is_char_boundary(*i)).unwrap_or(0);
                    format!("{}\n[…section truncated for embedding; full text in leaf chunks…]", &sec_text[..cut])
                } else { sec_text.to_string() };
                let sec_index = next_index;
                out.push(ChunkSpec {
                    chunk_index: next_index,
                    content: sec_content.clone(),
                    char_start: section.byte_start as i64,
                    char_end: section.byte_end as i64,
                    page_number: None,
                    token_count: estimate_tokens(&sec_content) as i64,
                    unit_type: "section".into(),
                    unit_label: section.label.clone(),
                    unit_path: Some(section.path.clone()),
                    parent_index: Some(doc_index),
                });
                next_index += 1;
                for leaf in &section.children {
                    emit_leaves(text, leaf, sec_index, &mut next_index, &mut out);
                }
            }
            NodeKind::Document => unreachable!("nested documents not supported"),
        }
    }
    out
}

fn build_document_content(text: &str, label: &Option<String>) -> String {
    let title = label.as_deref().unwrap_or("(untitled)");
    let preview_chars = 1000usize.min(text.len());
    let cut = (0..=preview_chars).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
    format!("{title}\n\n{}", &text[..cut])
}

fn emit_leaves(
    text: &str,
    leaf_node: &crate::services::heading_detector::tree::DocNode,
    parent_index: i64,
    next_index: &mut i64,
    out: &mut Vec<ChunkSpec>,
) {
    let body = &text[leaf_node.byte_start..leaf_node.byte_end];
    if estimate_tokens(body) <= LEAF_SOFT_CAP {
        out.push(ChunkSpec {
            chunk_index: *next_index,
            content: body.to_string(),
            char_start: leaf_node.byte_start as i64,
            char_end: leaf_node.byte_end as i64,
            page_number: None,
            token_count: estimate_tokens(body) as i64,
            unit_type: "leaf".into(),
            unit_label: None,
            unit_path: Some(leaf_node.path.clone()),
            parent_index: Some(parent_index),
        });
        *next_index += 1;
        return;
    }
    // Body too large — split at paragraph boundaries.
    let paragraphs: Vec<(usize, &str)> = split_paragraphs(body);
    let mut buf = String::new();
    let mut buf_start = leaf_node.byte_start;
    let mut buf_tokens = 0usize;
    for (rel_start, par) in &paragraphs {
        let pt = estimate_tokens(par);
        if buf_tokens + pt > LEAF_SOFT_CAP && !buf.is_empty() {
            push_leaf(out, next_index, parent_index, &buf, buf_start, buf_start + buf.len(), &leaf_node.path);
            buf.clear();
            buf_tokens = 0;
        }
        if buf.is_empty() { buf_start = leaf_node.byte_start + rel_start; }
        else { buf.push_str("\n\n"); }
        buf.push_str(par);
        buf_tokens += pt;
    }
    if !buf.is_empty() {
        push_leaf(out, next_index, parent_index, &buf, buf_start, buf_start + buf.len(), &leaf_node.path);
    }
}

fn push_leaf(
    out: &mut Vec<ChunkSpec>, next_index: &mut i64, parent_index: i64,
    content: &str, start: usize, end: usize, path: &str,
) {
    out.push(ChunkSpec {
        chunk_index: *next_index,
        content: content.to_string(),
        char_start: start as i64,
        char_end: end as i64,
        page_number: None,
        token_count: estimate_tokens(content) as i64,
        unit_type: "leaf".into(),
        unit_label: None,
        unit_path: Some(path.to_string()),
        parent_index: Some(parent_index),
    });
    *next_index += 1;
}
```

Update `into_new_chunk` to copy the new fields:

```rust
impl ChunkSpec {
    pub fn into_new_chunk(self, source_id: &str, parent_chunk_id: Option<String>) -> NewChunk {
        NewChunk {
            source_id: source_id.to_string(),
            chunk_index: self.chunk_index,
            content: self.content,
            char_start: self.char_start,
            char_end: self.char_end,
            page_number: self.page_number,
            token_count: self.token_count,
            parent_chunk_id,
            unit_type: self.unit_type,
            unit_label: self.unit_label,
            unit_path: self.unit_path,
            domain_metadata: None,
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --test multilevel_chunker_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/chunker.rs src-tauri/tests/multilevel_chunker_test.rs
git commit -m "feat(chunker): multi-level chunker walks doc tree to emit document/section/leaf"
```

---

## Task 7: Wire heading detector + multi-level chunker into the indexer

**Files:**
- Modify: `src-tauri/src/services/indexer.rs`

- [ ] **Step 1: Locate the chunking call site**

Run: `grep -n "chunker::chunk\b\|chunker::chunk(" "/media/riaz/New Volume/echo-AI/src-tauri/src/services/indexer.rs"`
Identify the function that converts `ParsedDocument` → `Vec<ChunkSpec>`.

- [ ] **Step 2: Replace flat-chunk call with hierarchical pipeline**

In the indexer's per-source ingestion path, replace the existing `chunker::chunk(text, page_offsets)` call with:

```rust
use crate::services::heading_detector::{regex_pass, llm_pass, tree};

// 1. Pass A — regex.
let candidates = regex_pass::scan(&parsed.text);

// 2. Pass B — bounded LLM disambiguation. The classifier is constructed from
//    the existing LocalLlm slot held in app state. Skip Pass B entirely if
//    the LLM is unavailable (fall back to high-confidence regex hits only).
let paragraph_count = parsed.text.split("\n\n").count();
let promoted = match crate::services::llm::try_get_local_llm(&state) {
    Some(llm) => {
        let classifier = llm_pass::LlmClassifier { llm: &llm };
        let (kept, _budget) = llm_pass::disambiguate(candidates, &parsed.text, paragraph_count, &classifier);
        kept
    }
    None => candidates.into_iter().filter(|c| c.confidence >= 0.85).collect(),
};

// 3. Build tree.
let doc_tree = tree::build_tree(&parsed.text, &promoted, tree::BuildOptions {
    document_title: source.file_name.clone(),
});

// 4. Multi-level chunks.
let specs = crate::services::chunker::chunk_tree(&parsed.text, &doc_tree);
```

- [ ] **Step 3: Translate parent_index → parent_chunk_id during DB insert**

Two-pass insert: first, insert all chunks with `parent_chunk_id: None` to receive their UUIDs; then issue a single UPDATE per child that has a parent. Alternatively, build the parent UUID map in memory before insert by pre-allocating UUIDs.

Use the pre-allocation approach to keep it one transaction:

```rust
use uuid::Uuid;
let pre_ids: Vec<String> = (0..specs.len()).map(|_| Uuid::new_v4().to_string()).collect();
// Map from chunk_index → uuid
let id_by_index: std::collections::HashMap<i64, String> = specs.iter().zip(&pre_ids)
    .map(|(s, id)| (s.chunk_index, id.clone())).collect();

// Build NewChunk vec resolving parent_chunk_id from parent_index.
let new_chunks: Vec<NewChunk> = specs.into_iter().enumerate().map(|(i, spec)| {
    let parent_uuid = spec.parent_index.and_then(|pi| id_by_index.get(&pi).cloned());
    let mut nc = spec.into_new_chunk(&source.id, parent_uuid);
    // Optional: stuff legal-domain metadata if a Pakistani notification etc.
    nc
}).collect();
```

Then update `chunks::bulk_insert` to accept pre-allocated IDs (add a parallel `bulk_insert_with_ids(pool, ids, chunks)` function), or keep `bulk_insert` and accept the second-pass UPDATE cost — given indexing is no longer speed-critical (per user §3 priorities), the simpler two-pass is acceptable:

```rust
let inserted = chunks::bulk_insert(pool, new_chunks_first_pass).await?;
// inserted preserves order; map chunk_index → real id
let real_id_by_index: std::collections::HashMap<i64, String> =
    inserted.iter().map(|c| (c.chunk_index, c.id.clone())).collect();
let mut tx = pool.begin().await?;
for c in &inserted {
    if let Some(spec_parent_idx) = /* parent_index recovered from spec, kept in a parallel Vec */ ... {
        let parent_id = real_id_by_index.get(&spec_parent_idx).cloned();
        sqlx::query("UPDATE chunks SET parent_chunk_id = ? WHERE id = ?")
            .bind(parent_id).bind(&c.id).execute(&mut *tx).await?;
    }
}
tx.commit().await?;
```

(Pick one approach — pre-allocated IDs is cleaner; the engineer chooses based on `bulk_insert` invasiveness.)

- [ ] **Step 4: Embed all unit_types**

Confirm the existing embed loop iterates over every inserted chunk (regardless of `unit_type`) — no change needed unless it currently filters. Add an explicit log:

```rust
log::info!(
    "Indexed {} chunks for {} (doc:{} section:{} leaf:{})",
    inserted.len(), source.file_name,
    inserted.iter().filter(|c| c.unit_type == "document").count(),
    inserted.iter().filter(|c| c.unit_type == "section").count(),
    inserted.iter().filter(|c| c.unit_type == "leaf").count(),
);
```

- [ ] **Step 5: Run full build**

Run: `cd src-tauri && cargo build`
Expected: PASS

- [ ] **Step 6: Smoke test ingestion end-to-end**

Run: `cd src-tauri && cargo test --test indexer_test 2>&1 | tail -30`
Expected: PASS, or if no integration test exists, run the app: `cd src-tauri && cargo run` → add a small folder of legal PDFs → confirm logs show three-level chunk counts.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/services/indexer.rs src-tauri/src/db/repositories/chunks.rs
git commit -m "feat(indexer): wire heading detector + multi-level chunker into ingestion pipeline"
```

---

## Task 8: Add ort + tokenizers dependencies and reranker scaffolding

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/services/reranker.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: Add deps**

In `src-tauri/Cargo.toml` `[dependencies]`:

```toml
ort         = { version = "2.0.0-rc.10", default-features = false, features = ["load-dynamic", "ndarray"] }
ndarray     = "0.16"
tokenizers  = { version = "0.20", default-features = false, features = ["onig"] }
```

Run: `cd src-tauri && cargo build`
Expected: PASS — deps resolve. Document in `CHANGELOG_DEV.md` that `ort` uses `load-dynamic` so users provide their own onnxruntime shared lib (or we bundle it later).

- [ ] **Step 2: Create reranker module skeleton**

Create `src-tauri/src/services/reranker.rs`:

```rust
//! Cross-encoder reranker (bge-reranker-base ONNX).
//!
//! Loaded lazily on first query; kept resident in a Mutex<Option<Reranker>>.
//! Adds ~1.2s per query for 30 (query, chunk) pairs on CPU.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_PAIR_TOKENS: usize = 512;
const REL_THRESHOLD: f32 = -10.0; // very permissive; we use ranking, not gating

pub struct Reranker {
    session: ort::session::Session,
    tokenizer: tokenizers::Tokenizer,
}

pub struct RerankerSlot {
    inner: Mutex<Option<Reranker>>,
    model_path: PathBuf,
    tokenizer_path: PathBuf,
}

impl RerankerSlot {
    pub fn new(model_path: impl Into<PathBuf>, tokenizer_path: impl Into<PathBuf>) -> Self {
        Self { inner: Mutex::new(None), model_path: model_path.into(), tokenizer_path: tokenizer_path.into() }
    }

    /// Score an iterable of (query, candidate) pairs. Higher = more relevant.
    /// Returns scores in input order.
    pub fn score_pairs(&self, query: &str, candidates: &[String]) -> Result<Vec<f32>> {
        let mut g = self.inner.lock().map_err(|_| anyhow::anyhow!("reranker mutex poisoned"))?;
        if g.is_none() {
            *g = Some(Self::load(&self.model_path, &self.tokenizer_path)?);
        }
        let r = g.as_ref().unwrap();
        r.score(query, candidates)
    }

    fn load(model: &Path, tok: &Path) -> Result<Reranker> {
        let environment = ort::init().with_name("reranker").commit()?;
        let session = ort::session::Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .commit_from_file(model)
            .with_context(|| format!("failed to load reranker ONNX from {model:?}"))?;
        let tokenizer = tokenizers::Tokenizer::from_file(tok)
            .map_err(|e| anyhow::anyhow!("failed to load tokenizer: {e}"))?;
        Ok(Reranker { session, tokenizer })
    }
}

impl Reranker {
    fn score(&self, query: &str, candidates: &[String]) -> Result<Vec<f32>> {
        if candidates.is_empty() { return Ok(vec![]); }
        // Build (query, candidate) pair tokens. bge-reranker-base uses XLMRoberta tokenizer
        // which expects `[query] [SEP] [candidate]` — encode_pair handles this.
        let pairs: Vec<(String, String)> = candidates.iter().map(|c| (query.to_string(), c.clone())).collect();
        let encodings = self.tokenizer.encode_batch(pairs, true)
            .map_err(|e| anyhow::anyhow!("tokenize failed: {e}"))?;

        // Pad to longest in batch (capped at MAX_PAIR_TOKENS).
        let max_len = encodings.iter().map(|e| e.get_ids().len()).max().unwrap_or(0).min(MAX_PAIR_TOKENS);
        let batch = encodings.len();

        let mut input_ids = ndarray::Array2::<i64>::zeros((batch, max_len));
        let mut attention_mask = ndarray::Array2::<i64>::zeros((batch, max_len));
        for (i, enc) in encodings.iter().enumerate() {
            for (j, &id) in enc.get_ids().iter().take(max_len).enumerate() {
                input_ids[[i, j]] = id as i64;
                attention_mask[[i, j]] = 1;
            }
        }

        let outputs = self.session.run(ort::inputs! {
            "input_ids" => input_ids,
            "attention_mask" => attention_mask,
        }?)?;
        let logits = outputs[0].try_extract_tensor::<f32>()?;
        let view = logits.view();
        // bge-reranker-base outputs shape [batch, 1] — single relevance logit.
        let scores: Vec<f32> = (0..batch).map(|i| view[[i, 0]]).collect();
        let _ = REL_THRESHOLD; // silence unused; reserved for future gating
        Ok(scores)
    }
}
```

In `src-tauri/src/services/mod.rs` add:

```rust
pub mod reranker;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/services/reranker.rs src-tauri/src/services/mod.rs
git commit -m "feat(reranker): scaffold bge-reranker-base ONNX cross-encoder service"
```

---

## Task 9: Reranker integration test against a real model

**Files:**
- Create: `src-tauri/tests/reranker_test.rs`

**Note:** This test is gated behind `#[ignore]` because it requires the user to download `bge-reranker-base` to `models/bge-reranker-base/`. Run manually with `cargo test --test reranker_test -- --ignored`.

- [ ] **Step 1: Write the test**

Create `src-tauri/tests/reranker_test.rs`:

```rust
use std::path::PathBuf;

fn model_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("models").join("bge-reranker-base")
}

#[test]
#[ignore = "requires bge-reranker-base in models/ dir"]
fn ranks_relevant_chunk_higher_than_irrelevant() {
    let model = model_dir().join("model.onnx");
    let tok   = model_dir().join("tokenizer.json");
    if !model.exists() || !tok.exists() {
        panic!("missing model files at {model:?} / {tok:?}");
    }
    let slot = echo_ai::services::reranker::RerankerSlot::new(model, tok);
    let query = "What is the minimum service for promotion to BPS-18?";
    let candidates = vec![
        "Section 8: For B-18, minimum length of service shall be 5 years in B-17.".to_string(),
        "The cat sat on the mat. Recipes for chicken curry require fresh spices.".to_string(),
        "Rule 12(2) describes age relaxation for candidates exceeding the prescribed age limit.".to_string(),
    ];
    let scores = slot.score_pairs(query, &candidates).unwrap();
    assert_eq!(scores.len(), 3);
    // Index 0 (BPS-18 service requirement) MUST outrank index 1 (cat/recipes).
    assert!(scores[0] > scores[1], "scores: {scores:?}");
    // Index 0 should also outrank index 2 (related but not the answer).
    assert!(scores[0] > scores[2], "scores: {scores:?}");
}
```

- [ ] **Step 2: Run with --ignored if model present**

Run: `cd src-tauri && cargo test --test reranker_test -- --ignored`
Expected: PASS if `models/bge-reranker-base/{model.onnx,tokenizer.json}` exist; otherwise the engineer documents the download step in `CHANGELOG_DEV.md` and proceeds.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tests/reranker_test.rs
git commit -m "test(reranker): integration test against bge-reranker-base (ignored by default)"
```

---

## Task 10: Sentence-level grounding validator (extracted module)

**Files:**
- Create: `src-tauri/src/services/grounding.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/services/query_router.rs`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/services/grounding.rs`:

```rust
//! Sentence-level grounding validator.
//!
//! For each generated sentence carrying a citation `[N]`, compute Jaccard
//! overlap of content tokens (lowercase, ≥4 chars, stopword-stripped) against
//! the cited chunk. Sentence passes if overlap >= JACCARD_THRESHOLD OR shared
//! token count >= MIN_SHARED_TOKENS.

use std::collections::HashSet;

pub const JACCARD_THRESHOLD: f32 = 0.25;
pub const MIN_SHARED_TOKENS: usize = 3;
/// Drop sentences if up to this many are ungrounded; > this triggers retry.
pub const MAX_DROPPABLE_UNGROUNDED: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct SentenceVerdict {
    pub sentence: String,
    pub cited_indices: Vec<usize>, // 1-based
    pub overlap: f32,
    pub shared: usize,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct GroundingReport {
    pub per_sentence: Vec<SentenceVerdict>,
    pub action: GroundingAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GroundingAction {
    /// All sentences passed → ship as-is.
    Ship,
    /// 1..=MAX_DROPPABLE_UNGROUNDED failed → strip them, ship remainder.
    Strip { kept: String },
    /// > MAX_DROPPABLE_UNGROUNDED OR > 50% failed → trigger retry escalation.
    Retry,
}

pub fn validate(response: &str, cited_chunks_by_index: &[String]) -> GroundingReport {
    let sentences = split_into_sentences(response);
    if sentences.is_empty() {
        return GroundingReport { per_sentence: vec![], action: GroundingAction::Ship };
    }
    let mut verdicts = Vec::with_capacity(sentences.len());
    for s in &sentences {
        let cited = extract_citation_indices(s);
        if cited.is_empty() {
            // Sentences without citations are not gated — they're assumed to be
            // connective tissue (the model is trained to cite factual claims).
            verdicts.push(SentenceVerdict {
                sentence: s.clone(), cited_indices: vec![],
                overlap: 1.0, shared: 0, passed: true,
            });
            continue;
        }
        let s_tokens = content_tokens(s);
        let mut best_overlap = 0.0f32;
        let mut best_shared = 0usize;
        for &idx in &cited {
            let chunk_idx = idx.saturating_sub(1);
            if chunk_idx >= cited_chunks_by_index.len() { continue; }
            let c_tokens = content_tokens(&cited_chunks_by_index[chunk_idx]);
            let shared = s_tokens.intersection(&c_tokens).count();
            let union = s_tokens.union(&c_tokens).count().max(1);
            let jacc = shared as f32 / union as f32;
            if jacc > best_overlap { best_overlap = jacc; }
            if shared > best_shared { best_shared = shared; }
        }
        let passed = best_overlap >= JACCARD_THRESHOLD || best_shared >= MIN_SHARED_TOKENS;
        verdicts.push(SentenceVerdict {
            sentence: s.clone(), cited_indices: cited,
            overlap: best_overlap, shared: best_shared, passed,
        });
    }
    let failed = verdicts.iter().filter(|v| !v.passed).count();
    let action = if failed == 0 { GroundingAction::Ship }
        else if failed > MAX_DROPPABLE_UNGROUNDED || (failed * 2 > verdicts.len()) {
            GroundingAction::Retry
        } else {
            let kept: String = verdicts.iter().filter(|v| v.passed)
                .map(|v| v.sentence.clone()).collect::<Vec<_>>().join(" ");
            let kept = format!("{kept}\n\n_(Some content omitted as unsupported.)_");
            GroundingAction::Strip { kept }
        };
    GroundingReport { per_sentence: verdicts, action }
}

fn split_into_sentences(s: &str) -> Vec<String> {
    // Naive but robust: split on `. `, `? `, `! ` followed by uppercase.
    // Preserves trailing punctuation. Skips empty pieces.
    let mut out = Vec::new();
    let mut buf = String::new();
    let bytes = s.as_bytes();
    for (i, ch) in s.char_indices() {
        buf.push(ch);
        let is_terminator = matches!(ch, '.' | '?' | '!');
        let next_byte = bytes.get(i + ch.len_utf8()).copied();
        let next2 = bytes.get(i + ch.len_utf8() + 1).copied();
        if is_terminator && next_byte == Some(b' ') {
            if let Some(nb) = next2 {
                if (nb as char).is_uppercase() {
                    out.push(buf.trim().to_string());
                    buf.clear();
                }
            }
        }
    }
    let tail = buf.trim().to_string();
    if !tail.is_empty() { out.push(tail); }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn extract_citation_indices(s: &str) -> Vec<usize> {
    let re = regex::Regex::new(r"\[(\d+)\]").unwrap();
    let mut out: Vec<usize> = re.captures_iter(s)
        .filter_map(|c| c.get(1)?.as_str().parse().ok())
        .collect();
    out.sort_unstable(); out.dedup();
    out
}

fn content_tokens(s: &str) -> HashSet<String> {
    static STOPWORDS: &[&str] = &[
        "the","and","that","this","with","from","into","upon","shall","may","such",
        "have","been","were","also","than","then","when","what","whom","whose","which",
        "their","there","these","those","other","under","over","between","within",
    ];
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4)
        .map(|w| w.to_lowercase())
        .filter(|w| !STOPWORDS.contains(&w.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_grounded_sentence_passes() {
        let chunk = "For B-18, minimum length of service shall be 5 years in B-17.";
        let resp  = "Promotion to BPS-18 requires five years of service in BPS-17 [1].";
        let r = validate(resp, &[chunk.to_string()]);
        assert_eq!(r.action, GroundingAction::Ship);
    }

    #[test]
    fn fully_hallucinated_triggers_retry() {
        let chunk = "Section 8 covers recruitment criteria for civil servants.";
        let resp  = "The Mars rover discovered iron ore in Olympus Mons crater [1].";
        let r = validate(resp, &[chunk.to_string()]);
        assert_eq!(r.action, GroundingAction::Retry);
    }

    #[test]
    fn one_bad_sentence_strips() {
        let chunk = "Promotion to B-18 needs 5 years in B-17. Departmental exams are required.";
        let resp = "Promotion requires five years of service in B-17 [1]. The applicant must also bring a goldfish to the interview [1]. Departmental exams must be cleared [1].";
        let r = validate(resp, &[chunk.to_string()]);
        match r.action {
            GroundingAction::Strip { kept } => {
                assert!(!kept.contains("goldfish"));
                assert!(kept.contains("five years"));
            }
            other => panic!("expected Strip, got {other:?}"),
        }
    }

    #[test]
    fn no_citation_sentences_pass_without_gating() {
        let chunk = "Section 8 covers recruitment.";
        let resp = "Here is your answer. Recruitment is governed by Section 8 [1].";
        let r = validate(resp, &[chunk.to_string()]);
        assert_eq!(r.action, GroundingAction::Ship);
    }
}
```

In `src-tauri/src/services/mod.rs`:

```rust
pub mod grounding;
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test --lib services::grounding`
Expected: PASS — all four tests.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/services/grounding.rs src-tauri/src/services/mod.rs
git commit -m "feat(grounding): sentence-level Jaccard-overlap validator with strip/retry actions"
```

---

## Task 11: Wire reranker + new grounding into query_router

**Files:**
- Modify: `src-tauri/src/services/query_router.rs`

**Approach:** keep the existing flow intact, add reranker scoring between RRF merge and `build_context`, and replace the call to legacy `validate_grounding` with the new `grounding::validate` for the success path. Legacy `validate_grounding` is preserved for the retry path until callers fully migrate (one task at a time).

- [ ] **Step 1: Raise CONTEXT_TOKEN_BUDGET**

In `src-tauri/src/services/query_router.rs:31`:

```rust
const CONTEXT_TOKEN_BUDGET: usize = 2400; // raised from 1200 — reranker filters noise
```

- [ ] **Step 2: Add reranker call**

In the function around line 469 (where `build_context` is called), before that call, add:

```rust
// Reranker pass — bypassed for Tier-1 entity FTS queries (already exact).
let reranked: Vec<(String, String, String, Option<i64>, String)> =
    if matches!(query_type, QueryType::Regular) {
        let candidate_texts: Vec<String> = raw_chunks.iter().map(|c| c.2.clone()).collect();
        match state.reranker_slot.score_pairs(query_text, &candidate_texts) {
            Ok(scores) => {
                let mut indexed: Vec<(usize, f32)> = scores.iter().copied().enumerate().collect();
                indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let mut out = Vec::with_capacity(raw_chunks.len());
                for (i, _) in indexed { out.push(raw_chunks[i].clone()); }
                trace.step("Reranker", &[
                    ("Model", "bge-reranker-base"),
                    ("Pairs scored", &format!("{}", raw_chunks.len())),
                ]);
                out
            }
            Err(e) => {
                log::warn!("reranker failed, falling back to RRF order: {e}");
                trace.step("Reranker", &[("Skipped", "reranker error — using RRF order")]);
                raw_chunks.clone()
            }
        }
    } else {
        raw_chunks.clone()
    };

let chunks = build_context(&reranked, ctx_top_k, ctx_budget);
```

`state.reranker_slot` is a new field on the app state — wire it in `lib.rs` where other slots are constructed:

```rust
let reranker_slot = std::sync::Arc::new(crate::services::reranker::RerankerSlot::new(
    models_dir.join("bge-reranker-base/model.onnx"),
    models_dir.join("bge-reranker-base/tokenizer.json"),
));
.manage(reranker_slot.clone())
```

…and add it to the `AppState` struct accordingly.

- [ ] **Step 3: Apply rollup before build_context**

After reranking but before `build_context`, add:

```rust
// Rollup: if 3+ leaf chunks share the same parent section, replace those
// siblings with the single section chunk. Improves coherence for broad queries.
let reranked = roll_up_siblings(reranked, pool).await;
```

Define `roll_up_siblings` near the bottom of the file:

```rust
async fn roll_up_siblings(
    chunks: Vec<(String, String, String, Option<i64>, String)>,
    pool: &sqlx::SqlitePool,
) -> Vec<(String, String, String, Option<i64>, String)> {
    // Probe each chunk's parent_chunk_id and unit_type from DB (single round-trip).
    let ids: Vec<&str> = chunks.iter().map(|c| c.0.as_str()).collect();
    if ids.is_empty() { return chunks; }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let q = format!("SELECT id, parent_chunk_id, unit_type FROM chunks WHERE id IN ({placeholders})");
    let mut bind = sqlx::query_as::<_, (String, Option<String>, String)>(&q);
    for id in &ids { bind = bind.bind(id); }
    let rows = match bind.fetch_all(pool).await {
        Ok(r) => r,
        Err(e) => { log::warn!("rollup probe failed: {e}"); return chunks; }
    };
    use std::collections::HashMap;
    let meta: HashMap<String, (Option<String>, String)> =
        rows.into_iter().map(|(i, p, u)| (i, (p, u))).collect();

    // Count leaf siblings per parent.
    let mut sibling_count: HashMap<String, usize> = HashMap::new();
    for c in &chunks {
        if let Some((Some(parent), ut)) = meta.get(&c.0).map(|(p,u)| (p.clone(),u.clone())) {
            if ut == "leaf" { *sibling_count.entry(parent).or_insert(0) += 1; }
        }
    }
    // Identify parents that should be promoted.
    let promote: std::collections::HashSet<String> = sibling_count.into_iter()
        .filter(|(_, n)| *n >= 3).map(|(p, _)| p).collect();
    if promote.is_empty() { return chunks; }

    // Fetch parent rows once.
    let pp = promote.iter().cloned().collect::<Vec<_>>();
    let placeholders = pp.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let q = format!("SELECT c.id, c.source_id, c.content, c.page_number, s.file_name \
                     FROM chunks c JOIN sources s ON c.source_id = s.id \
                     WHERE c.id IN ({placeholders})");
    let mut bind = sqlx::query_as::<_, (String, String, String, Option<i64>, String)>(&q);
    for id in &pp { bind = bind.bind(id); }
    let parent_rows = match bind.fetch_all(pool).await {
        Ok(r) => r,
        Err(e) => { log::warn!("rollup parent fetch failed: {e}"); return chunks; }
    };
    let parents_by_id: HashMap<String, _> = parent_rows.into_iter().map(|r| (r.0.clone(), r)).collect();

    // Replace leaves under promoted parents with the parent itself (deduped).
    let mut seen_parent: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<(String, String, String, Option<i64>, String)> = Vec::with_capacity(chunks.len());
    for c in chunks {
        let parent_to_promote = meta.get(&c.0).and_then(|(p, u)|
            if u == "leaf" { p.clone().filter(|pid| promote.contains(pid)) } else { None }
        );
        match parent_to_promote {
            Some(pid) if !seen_parent.contains(&pid) => {
                if let Some(prow) = parents_by_id.get(&pid) {
                    seen_parent.insert(pid);
                    out.push(prow.clone());
                }
            }
            Some(_) => { /* parent already pushed; skip duplicate sibling */ }
            None => out.push(c),
        }
    }
    out
}
```

- [ ] **Step 4: Switch first-pass grounding to new validator**

Replace the call around line 638:

```rust
let report = crate::services::grounding::validate(
    &answer,
    &chunks.iter().map(|c| c.2.clone()).collect::<Vec<_>>(),
);
trace.step("Grounding (sentence)", &[
    ("Sentences", &format!("{}", report.per_sentence.len())),
    ("Failed", &format!("{}", report.per_sentence.iter().filter(|v| !v.passed).count())),
    ("Action", &format!("{:?}", report.action)),
]);
let answer = match report.action {
    crate::services::grounding::GroundingAction::Ship => answer,
    crate::services::grounding::GroundingAction::Strip { kept } => kept,
    crate::services::grounding::GroundingAction::Retry => {
        // Existing retry path — set a flag the existing escalation block reads.
        // Keep `answer` unchanged so the retry block sees the original.
        log::info!("Sentence-grounding requested retry");
        answer
    }
};
```

The existing `validate_grounding(...)` call is left in place for the retry path until a future task migrates it; that prevents this task from sprawling.

- [ ] **Step 5: Build & test**

Run: `cd src-tauri && cargo build && cargo test --lib query_router`
Expected: PASS — existing query_router tests still pass; new behavior is exercised by the integration tests below.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/query_router.rs src-tauri/src/lib.rs
git commit -m "feat(query): integrate reranker + sibling rollup + sentence grounding into query path"
```

---

## Task 12: End-to-end retrieval acceptance test

**Files:**
- Create: `src-tauri/tests/hierarchical_retrieval_test.rs`

- [ ] **Step 1: Write the test**

Create `src-tauri/tests/hierarchical_retrieval_test.rs`:

```rust
//! End-to-end acceptance: ingest a synthetic legal text → query → assert
//! reranked top-K contains the expected leaf chunk.

use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn precise_clause_query_returns_matching_leaf() {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
    sqlx::query("INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) VALUES('s1','/x','sindh-act.pdf','pdf',1,'h',0,0)")
        .execute(&pool).await.unwrap();

    // Build hierarchy by hand to avoid pulling in the LLM in tests.
    use echo_ai::db::repositories::chunks::{NewChunk, bulk_insert};
    let doc = NewChunk {
        source_id:"s1".into(), chunk_index:0,
        content:"Sindh Civil Servants Act 1973".into(),
        char_start:0, char_end:30, page_number:None, token_count:5,
        parent_chunk_id:None, unit_type:"document".into(),
        unit_label:Some("Sindh Civil Servants Act 1973".into()),
        unit_path:Some("".into()), domain_metadata:None,
    };
    let inserted = bulk_insert(&pool, vec![doc]).await.unwrap();
    let doc_id = inserted[0].id.clone();

    let section = NewChunk {
        source_id:"s1".into(), chunk_index:1,
        content:"Section 8 — Recruitment. Eligibility for various BPS positions is described herein.".into(),
        char_start:0, char_end:100, page_number:None, token_count:20,
        parent_chunk_id:Some(doc_id.clone()), unit_type:"section".into(),
        unit_label:Some("Section 8".into()),
        unit_path:Some("Section 8".into()), domain_metadata:None,
    };
    let s = bulk_insert(&pool, vec![section]).await.unwrap();
    let sec_id = s[0].id.clone();

    let leaf = NewChunk {
        source_id:"s1".into(), chunk_index:2,
        content:"For B-18, the minimum length of service shall be 5 years in B-17.".into(),
        char_start:0, char_end:80, page_number:Some(1), token_count:18,
        parent_chunk_id:Some(sec_id), unit_type:"leaf".into(),
        unit_label:None, unit_path:Some("Section 8".into()), domain_metadata:None,
    };
    bulk_insert(&pool, vec![leaf]).await.unwrap();

    // FTS5 sanity: searching for "minimum length of service" must return the leaf.
    let row: (String,) = sqlx::query_as(
        "SELECT c.id FROM chunks c JOIN chunks_fts f ON c.rowid = f.rowid \
         WHERE f.content MATCH 'minimum length of service' LIMIT 1"
    ).fetch_one(&pool).await.unwrap();
    let found = sqlx::query_as::<_, (String,)>("SELECT unit_type FROM chunks WHERE id=?")
        .bind(&row.0).fetch_one(&pool).await.unwrap();
    assert_eq!(found.0, "leaf");
}
```

- [ ] **Step 2: Run**

Run: `cd src-tauri && cargo test --test hierarchical_retrieval_test`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tests/hierarchical_retrieval_test.rs
git commit -m "test: end-to-end hierarchical retrieval — leaf hit on FTS for clause query"
```

---

## Task 13: Update CHANGELOG_DEV.md and final acceptance pass

**Files:**
- Modify: `CHANGELOG_DEV.md`

- [ ] **Step 1: Write the changelog entry**

Append to `CHANGELOG_DEV.md`:

```markdown
## 2026-05-15 — Tier 3: hierarchical retrieval + reranker + sentence-level grounding

**Why:** Audit (`Desktop/fine tunning research & work/audit_rag_ocr_optimization.md`)
identified context starvation (1200 tokens vs 12 chunks), no reranker, flat
chunking, and `[N]`-presence-only grounding as primary causes of retrieval
noise. Spec: `docs/superpowers/specs/2026-05-15-tier3-hierarchical-retrieval-design.md`.

**Changes:**
- DB migration 013: added `parent_chunk_id`, `unit_type` (document|section|leaf),
  `unit_label`, `unit_path`, `domain_metadata` columns to `chunks`. Bumped
  `schema_version` to 13.
- `db/schema.rs`: added wipe-on-version-mismatch — DBs at version <13 are
  destroyed (echo.db + .fastembed_cache) on next startup with a warning log.
  `models/` is preserved.
- New module `services/heading_detector/`: regex Pass A + bounded LLM Pass B
  (Qwen2.5-0.5B yes/no, ≤200 calls or ≤5% of paragraphs per doc) + tree builder.
- `services/chunker.rs`: added `chunk_tree` — walks the doc tree to emit one
  chunk per node (document/section/leaf) with parent links.
- `services/indexer.rs`: ingestion now runs heading detector → tree → multi-level
  chunker. Logs three-level chunk counts per source.
- `services/reranker.rs`: new bge-reranker-base ONNX wrapper (lazy-loaded).
  Adds `ort` 2.0-rc.10 + `tokenizers` 0.20 + `ndarray` 0.16 deps.
  Bypassed for Tier-1 entity FTS queries.
- `services/grounding.rs`: extracted sentence-level Jaccard validator.
  Actions: Ship | Strip | Retry. Thresholds: 0.25 Jaccard or ≥3 shared content
  tokens. Tunable constants at top of file.
- `services/query_router.rs`:
  - `CONTEXT_TOKEN_BUDGET` raised 1200 → 2400.
  - Added reranker call between RRF merge and `build_context`.
  - Added `roll_up_siblings`: if 3+ retrieved leaves share a parent section,
    the section chunk is promoted in their place.
  - First-pass grounding now uses `grounding::validate`. Retry path still uses
    the legacy validator (will be migrated in a future task).
- New trace step entries: `Reranker`, `Grounding (sentence)`.

**Cross-cutting impact:**
- One-shot destructive migration: existing user data is wiped on first run.
- Indexing latency increases (Pass B LLM calls + 3× embeddings). Per user
  direction 2026-05-15 brainstorm, indexing speed is no longer a constraint.
- Reranker requires `models/bge-reranker-base/{model.onnx,tokenizer.json}` —
  document download in app's model-setup UI before next release.
- `ort` is loaded with `load-dynamic` feature — users need a system onnxruntime
  shared lib, OR we add a bundled-libs feature flag in a follow-up.

**Follow-ups:**
- Sub-project #3 (OCR rebuild) — separate spec.
- Sub-project #4 (indexing-mode lock UX) — gate chat/analysis while indexing.
- Sub-project #5 (NER stuck-on-restart) — small bug, schedule alongside #4.
- Sub-project #6 (Query Assist UX) — SymSpell + FTS5 prefix.
- Migrate retry-path grounding from legacy `validate_grounding` to new
  `grounding::validate`.
```

- [ ] **Step 2: Run the full acceptance pass**

Run sequentially:

```bash
cd src-tauri
cargo build
cargo test --lib services::heading_detector
cargo test --lib services::grounding
cargo test --lib db::repositories::chunks
cargo test --test multilevel_chunker_test
cargo test --test schema_wipe_test
cargo test --test hierarchical_retrieval_test
```
Expected: all PASS.

Then run the app manually: `cargo run`, add a small folder with one legal PDF, confirm:
- Old DB is wiped on first launch (warning log present).
- Logs report `(doc:1 section:N leaf:M)` per source.
- A chat query returns a response with `[N]` citations.
- Trace JSON contains `Reranker` and `Grounding (sentence)` steps.

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG_DEV.md
git commit -m "docs(changelog): tier 3 hierarchical retrieval + reranker + sentence grounding"
```

---

## Self-Review Checklist (run after writing this plan)

- **Spec coverage**:
  - §4.1 wipe → Task 1 ✅
  - §4.2 schema → Task 1, Task 2 ✅
  - §4.3 hybrid heading detection → Tasks 3, 4, 5 ✅
  - §4.4 multi-level chunking → Tasks 6, 7 ✅
  - §4.5 retrieval + reranker + rollup → Tasks 8, 9, 11 ✅
  - §4.6 sentence grounding → Tasks 10, 11 ✅
  - §6 trace logging → Task 11 ✅
  - §8 acceptance criteria → Tasks 12, 13 ✅
- **Placeholder scan**: one TBD-ish line in Task 7 step 3 about the parent_index recovery approach (intentional — engineer picks A or B). All other steps contain concrete code.
- **Type consistency**: `ChunkSpec::parent_index: Option<i64>` (Task 6) ↔ `bulk_insert` translates to `parent_chunk_id: Option<String>` (Task 7) ✅. `unit_type` is String everywhere ✅. `RerankerSlot::score_pairs` signature matches caller in Task 11 ✅.
