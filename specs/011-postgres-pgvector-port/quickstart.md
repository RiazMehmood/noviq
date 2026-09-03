# Quickstart: Feature 011 — PostgreSQL + pgvector Re-platform

## Prerequisites
- Branch `011-postgres-pgvector-port` (branched off 010 — retrieval.rs seam lives there).
- NTFS rules apply: CARGO_TARGET_DIR + CARGO_HOME set per project convention; never build to root partition.
- First build with the `bundled` feature downloads the PG archive at **compile time** (one-time, needs network at build only).

## 1. Spike gate (do first — proves the offline claim)
```bash
cargo test --test pg_spike -- --nocapture
# Expects: initdb → start (loopback) → CREATE EXTENSION vector → insert/query a vector(768) → stop
# Run once with networking disabled to prove runtime-offline.
```

## 2. Dev run
```bash
npm run tauri dev
# First launch: app provisions {app_data_dir}/pgdata/, starts bundled PG, applies pg baseline.
# Logs show: engine init → health-check → extension vector OK → pool ready.
```

## 3. One-time corpus migration (live 65 GB DB — user runs, per workflow)
Size it first (user shell):
```bash
sqlite3 "/media/riaz/New Volume/home-overflow/com.echoai.app/echo.db" \
  "SELECT COUNT(*) FROM chunk_embeddings; SELECT page_count*page_size/1024/1024/1024.0 FROM pragma_page_count(), pragma_page_size();"
```
Then in-app: Settings → Migration panel → Preflight (disk check + schema version)
→ Start. Progress streams on `migration://progress`; phase order
`preflight → copying → indexing → verifying → done`. Old echo.db is NOT modified;
keep it until the report shows 100% row-count match and you've drafted successfully.

## 4. Acceptance checks (map to SCs)
```bash
# SC-007: existing suites against bundled PG
cargo test --lib drafting && cargo test --test one_ask_acceptance && cargo test --lib query_router

# SC-005: retrieval parity (top-K overlap ≥95% on eval prompt set)
cargo test --test pg_parity -- --nocapture

# SC-001/SC-002: live gates (after migration, user-observed)
# - research lookup ≤15 s each (watch logs/dev-*.log sqlx timings; was 1247–1678 s)
# - full one-ask draft completes, no oomd kill (journalctl -u systemd-oomd -f stays quiet)
```

## 5. Performance knobs (db/pg/settings.rs)
- `shared_buffers`: 256 MB / 512 MB / 1 GB by hardware tier — keep small; model slot owns the RAM.
- `hnsw.ef_search`: raise only if SC-005 parity < 95%.
- Migration-time only: `maintenance_work_mem` raised for the post-COPY HNSW build.

## Gotchas
- Constitution gate: cutover may NOT merge to master before the §IV amendment ADR is accepted (FR-013).
- Don't edit src-tauri files while a `npm run tauri dev` session is live (watcher kills the running app — standing rule).
- A foreign PostgreSQL on the machine is fine: the app uses its own socket/port + data dir and refuses anything else.
- sqlx placeholders are `$N` on PG — `?` compiles to runtime errors only; the port tasks grep for stragglers.
