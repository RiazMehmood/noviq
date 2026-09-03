# Quickstart: Validate Structured-Table Capture

## Build & unit test (VM — no local Rust toolchain)
```bash
# on <user>@<PRIVATE_IP>, in a clean checkout of branch 023-structured-table-capture
cd /opt/noviq/.../src-tauri
source ~/.cargo/env && export NOVIQ_TEST_PG_INSTALL_DIR=$HOME/test-pg-install
cargo test --lib --no-default-features --features llm,server,cuda table
# expect: reflow marker round-trip, ExtractedTable emission, table-unit contract (C-1..C-6) tests pass
```

## Acceptance: RoB Schedule-I → 39 (live)
1. Deploy branch; run structure re-extract for RoB:
   `POST /api/reindex_structure` (admin) — re-parses with the new table capture.
2. Verify the whole-table unit + rows exist (echo db; PG port drifts — `ss -tlnp|grep postgres`):
   ```sql
   -- one whole-table unit, N row units for RoB Schedule-I
   SELECT unit_type, count(*) FROM chunks
   WHERE source_id = '<RoB id>' AND unit_type IN ('table','table_row') GROUP BY 1;
   -- expect: table=1 (or a few size-segments), table_row=39
   SELECT count(*) FROM graph_nodes gn JOIN chunks c ON c.id=gn.chunk_id
   WHERE c.source_id='<RoB id>' AND gn.node_type='table_row';   -- expect 39
   ```
3. Ask the assistant:
   - "how many departments are in the Rules of Business?" → **39** (was 12–14) — SC-001
   - "list all departments" → all 39 enumerated, none dropped — SC-001
4. Regenerate + spot-check the whole-document summary covers the department table — SC-005.

## Regression guard (prose-only)
Re-index a prose-only document (e.g. RSO 2000) and confirm its `table`/`table_row` unit count is **0** and its
other chunks are unchanged vs before — SC-004 / C-5 / C-6.

## Rollback
Feature is additive + flag-guardable; if a table capture misbehaves on a document, the whole-table path
falls back to the existing prose reflow (FR-008) — no re-index is destructive (delete-then-insert).
