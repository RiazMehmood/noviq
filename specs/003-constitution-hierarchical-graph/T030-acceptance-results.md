# T030 — Constitution v1 Acceptance Gate (A1–A4) Results

**Date:** 2026-05-24
**Branch:** `003-constitution-hierarchical-graph`
**DB under test:** `/media/riaz/New Volume/home-overflow/com.echoai.app/echo.db`
**Source:** `constitution_ir.json` (1488 chunks total — 312 IR articles + 1 Preamble parent + 19 Preamble paragraph children + …)
**Verdict:** ✅ **PASS — A1, A2, A3, A4 all green.**

---

## A. Rust harness (`cargo test --test constitution_v1_acceptance`)

```
running 5 tests
test a4_citation_fastpath_resolves_f1_f2_f4_seeds ......... ok
test a3_graph_integrity_nodes_edges_endpoints ............. ok
test a1_structural_counts_and_linkage ..................... ok
test a2_citation_accuracy_every_clause_aligned_with_article  ok
test a4_preamble_fastpath_returns_parent_plus_children .... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured;
                 0 filtered out; finished in 1.99 s
```

CI-ready, deterministic, no LLM dependency. Authoritative gate.

---

## B. `db_inspection/07_hierarchy.sh` — A1 Structural

```
=== G1: chunk counts by unit_type ===
  article    313       ← 312 IR + 1 Preamble parent (Preamble import-gap fix)
  chapter    29
  clause     808       ← 787 IR + 21 Preamble paragraph children (Idea 1)
  document   1
  part       12
  subclause  325
  TOTAL      1488

=== G2: parent linkage by level ===
  article    no_parent=0  has_parent=313
  chapter    no_parent=0  has_parent=29
  clause     no_parent=0  has_parent=808
  document   no_parent=1  has_parent=0     ← the document root
  part       no_parent=0  has_parent=12
  subclause  no_parent=0  has_parent=325

=== G3: orphan check ===
  orphans = 0   (expect 0)
```

---

## C. `db_inspection/09_citation_accuracy.sh` — A2 Citation Accuracy

```
=== I: citation accuracy (A2) — articles / clauses / sub-clauses ===
  checked = 1426
  failed  = 0   (expect 0)
  ALL citations / paths consistent

=== I2: spot-check — a known recovered sub-clause ===
  Article 8(3)(b)  ->  PartII/Ch1/Art-8/cl-(3)/sub-(b)
```

**Note:** the script was patched on 2026-05-24 to exempt `unit_label='Preamble'` rows (parent + paragraph children), which legitimately don't carry an `Article N…` label after Idea 1. The Rust harness already had this exemption.

---

## D. `db_inspection/10_graph_integrity.sh` — A3 Graph Integrity

```
=== J1: graph table counts ===
  graph_nodes  1488
  graph_edges  1626
  chunks       1488   (graph_nodes should equal this)

=== J2: nodes by type ===
  article    313
  chapter    29
  clause     808
  document   1
  part       12
  subclause  325

=== J3: edges by type ===
  contains         1487   ← exactly graph_nodes − 1, the tree skeleton
  cross_reference   116   ← legal overlay (Article→Article references)
  non_obstante       23   ← override semantics

=== J4: integrity ===
  graph_nodes with no backing chunk = 0   (expect 0)
  edges with a dangling endpoint    = 0   (expect 0)
  contains edges = 1487   (matches graph_nodes-1)

=== J5: cross_reference spot-check (first 6) ===
  Article 12(2)  ->  Article 270
  Article 24(3)(f)  ->  Article 253
  Article 28  ->  Article 251
  Article 48(5)(b)  ->  Article 224
  Article 49(1)  ->  Article 41
  Article 58(2)  ->  Article 48
```

All endpoints resolve to real nodes; both edge types present in expected numbers.

---

## E. A4 retrieval — citation fast-path subset

Encoded in the harness as `a4_citation_fastpath_resolves_f1_f2_f4_seeds`:

| Query | Expected `unit_label` of fast-path head | Result |
|---|---|---|
| `Article 10 — Safeguards as to arrest and detention` | `Article 10` | ✅ |
| `what are the four categories of territory under Article 1(2)?` | `Article 1(2)` | ✅ |
| `what is the minimum age in Article 11(3)?` | `Article 11(3)` | ✅ |
| `define Article 19A` | `Article 19A` | ✅ |
| `what does the preamble say` | `Preamble` (parent) + paragraph children | ✅ (≥2 chunks) |

**F3 (high treason) and F5 (founder of Pakistan)** are tracked separately per spec — they require full hybrid retrieval + LLM and were validated by the F-set re-runs in `~/Desktop/test 5 prompts/preamble_v2_idea1.*`.

---

## Cross-cutting observations recorded in this run

- **Preamble shape under Idea 1.** 1 parent (`unit_label='Preamble'`, `unit_path='Preamble'`, body = `"Preamble"` heading marker) + 19 paragraph children (`unit_label='Preamble'`, `unit_path='Preamble/p1'…'Preamble/p19'`). Citation fast-path returns parent + all children via the `unit_path LIKE 'Preamble/%'` clause.
- **NER coverage** at 100% via the Idea-2 iterative loop. 580 non-reference entities extracted over 7 types (`org/concept/location/term/person/date` + 320 `reference`). Quality on 0.5B is uneven (Preamble's "Jinnah" was not extracted as a `person`; some hallucinations like `Karachi`, `Noether, Julius`) — flagged as known v2 follow-up. Does not affect the A1–A4 gate because S3b's entity boost is conservative (1.4×) and S1's salience boost + Idea 1's paragraph splitting already win F5 retrieval without it.

---

## v2 follow-ups (not blocking v1)

1. **NER quality** — switch Pass-2 to a larger local GGUF (1.5B/3B) or cloud (Cerebras/Groq) when corpus accuracy matters for entity-driven queries. Infrastructure (loop, salient-priority, scheduler) is in place; only the model file needs to change.
2. **Scheduler integration for Pass-2** — wire `WorkloadKind::Pass2Ner` end-to-end so the loop pauses mid-chunk on chat instead of waiting for the current chunk to release `ModelSlot`. Currently chat waits up to ~5–10 s for an in-flight NER chunk. Queued as task #13.
3. **`index_constitution` → `index_structured_doc` rename** plus IR-schema generalisation so the same path handles future Acts / Rules / Policies. Queued as task #4.
4. **S4 input-layer feature** — HyDE query expansion + prompt prediction + runtime spelling autocorrect. Queued as task #5.

---

## Verdict

**v1 acceptance gate PASS. Phase 9 (S7) closed. v2 unblocked.**
