# Quickstart — Constitution v1

How to build, run, and validate the v1 feature. Assumes the Tier 3 branch
environment (see `run-dev.sh`, `.cargo/config.toml`).

## Build

```bash
cd "/media/riaz/New Volume/echo-AI"
cargo build --no-default-features --features llm
```

## 1. Acquire the source (dev-time, once)

```bash
cargo run --bin constitution_import -- \
  --out tools/constitution_import/constitute_IR/constitution_ir.json
```

Produces the structured IR from pakistani.org. This is an **offline dev step** —
the shipped app never fetches it. Verify: ~12 parts, ~290 articles in the IR.

## 2. Index

Launch the app and index the constitution; the indexer detects the
`constitution` doc-type profile and runs the structure-aware path (S2–S5).

```bash
./run-dev.sh
# UI → index the constitution source
```

Watch the log for the per-source hierarchy line, e.g.
`(part:12 chapter:29 article:313 clause:808 subclause:325)` — these are the
v1 as-built numbers. `article=313` is the importer's 312 plus the Preamble
parent added by the 2026-05-23 import-gap fix. `clause=808` is the IR's 787
plus the ~21 Preamble paragraph children produced by Idea 1's paragraph
splitter (`structure_parser::split_oversized_leaves`). See §8.1 of
`docs/research/legal-corpus-hierarchy-architecture.md` for the full
as-built table.

## 3. Validate — the v1 acceptance gate (A1–A4)

The **authoritative gate** is the Rust harness — runs in <2 s, no LLM, no
network, no live DB:

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
cargo test --test constitution_v1_acceptance
# expect: test result: ok. 5 passed; 0 failed
```

The bash probes remain as developer diagnostics against the **live** DB —
useful for spot-checking real corpus state but not part of CI:

```bash
cd /home/riaz/Desktop/db_inspection_constitute_v1   # NOTE: v1 suffix

# A1 structural — expect NO parent with >90% of leaves, orphans 0
bash 07_hierarchy.sh           | tee v1_07_hierarchy.txt
bash 08_leaves_per_section.sh  | tee v1_08_leaves.txt

# A2 citation accuracy — checks every article/clause/subclause; expect failed=0
bash 09_citation_accuracy.sh   | tee v1_09_citation.txt

# A3 graph integrity — expect graph_nodes=chunks, contains=N-1, no dangling
bash 10_graph_integrity.sh     | tee v1_10_graph.txt
```

A4 retrieval is split into two parts:

- **Citation-fast-path subset** (F1, F2, F4 + bonus Article 19A + Preamble)
  is covered by the Rust harness above — deterministic, no LLM.
- **Full hybrid retrieval** (F3, F5) requires the running app and an LLM;
  validated in-app by running the F-set against your chosen local /
  cloud model. Captured trace files (e.g.
  `~/Desktop/test 5 prompts/preamble_v2_idea1.*`) are the artefact.

## Pass criteria

| Check | Pass (v1 as-built) |
|---|---|
| A1 | 12 parts · 29 chapters · 313 articles · 808 clauses · 325 sub-clauses · `orphans = 0` · max-children ratio < 0.90 |
| A2 | every article/clause/sub-clause row's `unit_label`, `unit_path`, `domain_metadata.citation`, content lead-in and parent-chain article agree (Preamble rows exempted — they carry their own structural invariants checked in A1) |
| A3 | `graph_nodes = chunks = 1488` · `contains = graph_nodes − 1 = 1487` · `cross_reference = 116` · `non_obstante = 23` · 0 dangling endpoints |
| A4 | F1 (Article 10), F2 (Article 1(2)), F4 (Article 11(3)), Article 19A, and `"what does the preamble say"` all resolve via the citation fast-path to the expected `unit_label` |

All four green → v1 done; v2 (OCR/extraction for the govt corpus) unblocked.

**Full gate evidence:**
`specs/003-constitution-hierarchical-graph/T030-acceptance-results.md`
captures the harness output + bash probe transcripts + cross-cutting
observations from the 2026-05-24 closing run.
