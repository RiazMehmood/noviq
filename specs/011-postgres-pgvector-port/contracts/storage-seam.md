# Contracts: Storage Seam & Engine Lifecycle (Feature 011)

No REST/GraphQL surface — this feature's contracts are Rust module boundaries
(ADR-0014 seam discipline) plus two Tauri command groups. Signatures are the
contract; implementations may not widen them with engine-specific types.

## C1. Vector retrieval seam — `services/drafting/retrieval.rs` (exists, body swaps)

```rust
/// The ONLY module allowed to contain vector-search SQL.
pub async fn top_k_chunks(
    pool: &Db,                  // type alias: was SqlitePool, becomes PgPool
    query_embedding: &[f32],    // 768-d
    k: usize,
    scope: RetrievalScope,      // existing folder/source scoping
) -> Result<Vec<ScoredChunk>>;  // (chunk_id, distance) ascending — order contract unchanged
```

Contract tests: existing `knn_parity` fixture test (order ≡ exact cosine) reruns
against PG; new live-parity run feeds SC-005.

## C2. Full-text seam — `services/query_router.rs` keyword helpers (exist, bodies swap)

```rust
/// The ONLY module allowed to contain full-text-search SQL.
pub async fn keyword_topk(
    pool: &Db,
    query: &str,                // user text; engine syntax is an internal concern
    k: usize,
    scope: RetrievalScope,
) -> Result<Vec<ScoredChunk>>;
```

RRF fusion + boost stack consume `Vec<ScoredChunk>` and MUST NOT change (FR-003).

## C3. Engine lifecycle — `db/pg/engine.rs` (new)

```rust
pub struct EngineHandle { /* opaque: socket/port, credentials, child guard */ }

pub async fn ensure_running(app_data_dir: &Path, tier: HardwareTier)
    -> Result<EngineHandle, EngineError>;          // init-if-absent → start/adopt → health-check → extension check
pub async fn connect(handle: &EngineHandle) -> Result<Db>;   // pool construction; loopback only
pub async fn shutdown(handle: EngineHandle) -> Result<()>;   // clean stop on app exit

pub enum EngineError {                              // every variant carries `remediation: String` (FR-012)
    InitFailed { .. }, StartFailed { .. }, PortConflict { .. },
    OrphanUnhealthy { .. }, ExtensionMissing { .. }, DataDirCorrupt { .. },
}
```

Invariants: never connects to a non-app-owned instance; never listens beyond
loopback (FR-006); idempotent across crash/restart (FR-005).

## C4. Migration — Tauri commands `commands/migration.rs` (new)

```rust
#[tauri::command] async fn migration_preflight(source: Option<String>)
    -> Result<PreflightReport, String>;   // finds old echo.db, schema version, disk check
#[tauri::command] async fn migration_start(source: String) -> Result<i64, String>;  // job id; resumes if prior job
#[tauri::command] async fn migration_cancel(job_id: i64) -> Result<(), String>;
#[tauri::command] async fn migration_report(job_id: i64) -> Result<MigrationReport, String>;
// progress events on channel "migration://progress": { jobId, phase, table, rowsCopied, totalRows, percent }
```

Field names serialize camelCase (serde rename_all) — TS types in `api/migration.ts`.

## C5. Test provisioning helper — `tests/common/pg.rs` (new)

```rust
/// Ephemeral bundled instance + baseline schema; same engine path as production (FR-015).
pub async fn test_pg() -> (EngineHandle, Db);   // temp dir, dropped on test end
```

All existing suites switch their pool source to this helper; behavioral
assertions unchanged (SC-007).
