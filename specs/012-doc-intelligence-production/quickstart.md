# Quickstart — Feature 012 (run a wave)

Offline. The expensive step is the NTFS cold build + re-index (~30 min); batch
**one re-index per wave**. The G2 golden gate is cheap (cargo, no LLM) — run it
**before** every attended re-index.

## Build ops (read first — cost hours historically)
- Before any `cargo`, kill any rust-analyzer-parented cargo holding the shared
  target lock: `ps -eo pid,ppid,args | grep '[c]argo'` → kill the RA child.
- Env: `CARGO_HOME="/media/riaz/New Volume/.cargo"` (config pins
  `POSTGRESQL_VERSION=17.10.0` + `ORT_DYLIB_PATH`).
- Prefer standalone-logic verification during iteration; **one** full build at slice end.

## G2 golden gate (cheap, no LLM)
```bash
cd "/media/riaz/New Volume/echo-AI"
cargo test --lib parsers:: heading_detector:: structure_parser:: structured_importer:: drafting::one_ask
```
A diff against a frozen golden fixture (provision label, table rows, chrome
exclusion) is a **release blocker** until reviewed.

## Wave A — US1 → US2 → US3 (one re-index, full re-embed)
1. Implement US1 (chrome), US2 (labels+provenance, migration `003`), US3 (heading).
2. Green G2 locally.
3. Launch app (`./run-dev.sh`), Index-Folder over the Sindh corpus → one re-index + re-embed.
4. Verify on live PG (app running; find port via `ps -eo args | grep '[p]ostgres -D /home/riaz/.local/share/com.echoai.app'`):
   - masthead `Rule 345(3)(b)` / "P a g e" / "Contact Us" → `block_class='chrome'`, excluded from retrieval;
   - no chunk `unit_label` is a bare `(i)` or `m66`; `label_status='unresolved'` where ambiguous;
   - no `Rule <year>` labels;
   - golden provisions unchanged (label = content leading number).

## Wave B — US4 (no re-index)
1. Implement deterministic compose/verify; green `cargo test --lib drafting::one_ask`.
2. Re-run the Canon-scanner ask; confirm zero fabricated gloss, prayer one sentence, all citations G1-verbatim.

## Wave C — US5 (one re-index)
1. Evolve `table.rs` to projection profiles + cross-page stitch; green table fixtures (G2).
2. Re-index; verify Delegation-of-Financial-Powers rows discrete/in-order/verbatim; non-table prose unchanged.

## DB inspection (app down)
Bundled binaries `~/.local/share/echoai-pg/install/17.10.0/bin/`, data dir
`~/.local/share/com.echoai.app/pg/data`, pw `iYwiYOPugJAqQxqA`, db `echo`, user
`postgres`. Start read-only on a temp port, then `pg_ctl ... -m fast stop`.
Schema: `sources(file_name, doc_type, extraction_method)` + `chunks(unit_label,
unit_path, content, block_class, …)`. No `documents` table.
