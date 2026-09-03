# Quickstart — Feature 006 (LLM PDF → IR Importer)

For developers picking up the branch.

## Prereqs

- Rust 1.87, Node, the standard EchoAI dev env.
- pdftoppm on PATH (already required by Feature 004).
- GGUF model file at `models/mistral-7b-instruct-v0.3.Q4_K_M.gguf` (~4.4 GB). Without it, the build still runs and the LLM path is skipped — feature still ships, you just hit the rule-based fallback.

## Build + run

```bash
# Backend (NTFS — both vars per feedback_ntfs_cargo_target)
export CARGO_TARGET_DIR="$HOME/.cache/echo-ai-target"
export CARGO_HOME="$HOME/.cache/echo-ai-cargo"
cargo build --manifest-path src-tauri/Cargo.toml

# Frontend + Tauri dev
npm install
npm run tauri dev
```

DB migration `018_llm_ir_importer.sql` runs automatically on first launch against `echo.db`.

## Smoke test the LLM path

1. Drop a structured PDF (e.g. one of the Sindh acts from `data/sindh/`) into a watched folder.
2. Watch `importer://progress` events in devtools (phase cycles `classifying → chunking → converting → validating → indexing`).
3. After import, the doc card should show `extraction_method = llm_ir` and a `quality_score` ≥ 0.95.
4. If `nodes_low_confidence > 0`, an `importer://done` event fires with `outcome: "partial"` and a toast appears. Click into the doc → repair view opens with PDF page + JSON editor.

## Smoke test the fallback

1. Rename the GGUF file (`mv models/mistral-7b-instruct-v0.3.Q4_K_M.gguf /tmp/`).
2. Reload the app. On first import, a one-time `importer://llm-unavailable` event fires.
3. Imports succeed via the rule-based parser (`extraction_method = rules_legacy`) — Feature 005 behaviour, unchanged.

## Smoke test the repair flow

1. Open a doc with `nodes_low_confidence > 0`.
2. Click the badge → repair view opens at the first flagged node.
3. Edit the JSON on the right; the rendered PDF page on the left shows the source.
4. Save → backend re-validates, persists changes, re-embeds only the changed nodes' chunks.
5. Re-import the same PDF (touch its mtime in the watched folder). Verify the user-repaired nodes are preserved (FR-018).

## Cache inspection

```bash
ls -la "$(echo $HOME)/.local/share/echo-AI/ir_cache/" | head
sqlite3 echo.db "SELECT sha256, doc_type, nodes_total, nodes_ok, nodes_low_confidence, orphaned_at FROM ir_cache_index;"
```

## Force the orphan sweep (devtools)

In devtools console: `await __TAURI__.invoke('force_orphan_sweep')` — returns `{ deleted: N }`.

## Tests

```bash
cargo test --manifest-path src-tauri/Cargo.toml doc_ir::          # unit tests for IR types, validator, schema
cargo test --manifest-path src-tauri/Cargo.toml --test llm_ir_importer_acceptance  # P1+P2+P3 acceptance gate
```

`llm_ir_importer_acceptance` is a model-free integration test that uses a **frozen, pre-recorded `DocIr` JSON** as the LLM stub — it does not run Mistral. The real-model golden is a manual run captured in `specs/006-llm-pdf-importer/golden/sindh_csa_1973.docir.json`.

## Tear-down for a clean reproduction

```bash
rm -rf "$(echo $HOME)/.local/share/echo-AI/ir_cache/"
rm -rf "$(echo $HOME)/.local/share/echo-AI/page_cache/"
sqlite3 echo.db "DELETE FROM ir_cache_index; DELETE FROM ir_repair_log;"
```
