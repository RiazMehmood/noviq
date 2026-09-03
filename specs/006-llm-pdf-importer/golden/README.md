# Golden DocIr captures

This directory holds frozen `DocIr` JSON files captured from real runs of
`MistralIrBuilder` against the V5 acceptance corpus. They're loaded by
`src-tauri/tests/llm_ir_importer_acceptance.rs` to gate US1 without needing
a live Mistral model in CI.

## Status (2026-05-30)

Currently empty. The Phase-3 acceptance test uses an inline `golden_ir()`
factory inside `llm_ir_importer_acceptance.rs` (a 4-node hand-built fixture)
so the dispatch / cache / persist / graph wiring is verified without Mistral
in the loop. **The real-corpus golden files are captured in T021's manual
validation step**, not as part of Phase 3 stub testing.

## Capture procedure (T015 → T021)

1. Place `mistral-7b-instruct-v0.3.Q4_K_M.gguf` in `models/`.
2. With `MistralIrBuilder` implemented (T021), drop one PDF from
   `data/sindh/` into a watched folder. (Per SC-001 the corpus is the
   6 Sindh PDFs from Feature 004's acceptance set.)
3. Wait for `importer://done`. Read the cache JSON at
   `{app_data_dir}/ir_cache/<sha256>.json`.
4. Copy it to `specs/006-llm-pdf-importer/golden/<short-name>.docir.json`
   (e.g. `sindh_csa_1973.docir.json`).
5. Re-run `cargo test --test llm_ir_importer_acceptance` — once a file
   exists, the acceptance test's `p1_real_golden_*` variant will pick it
   up and assert structural counts against it.

## Files

- *(none yet — captured during T021 manual validation)*
