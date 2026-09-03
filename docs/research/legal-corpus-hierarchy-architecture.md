# Legal & Government Document Corpus — Hierarchical + Graph Retrieval Architecture

Status: design discussion · 2026-05-21 · branch `feature/tier3-hierarchical-retrieval`
Scope: Constitution of Pakistan **and** the broader EchoAI legal/government corpus.

---

## 1. Problem

Indexing `constitution.pdf` through the Tier 3 hierarchical pipeline produced
**1 document / 11 sections / 253 leaves**. Six section labels absorb 93 % of all
leaves (`Article 69` alone = 48 leaves spanning ~60 articles). Diagnosis: the
Tier 3 chunker/detector/schema are **correct**; the failure is **upstream** —
PDF text extraction lost ~93 % of article headings (`‖`, `]]` artifacts; only
11 of ~290 article boundaries survived as detectable). No hierarchy design can
be populated from that text. This is the root cause of incorrect constitutional
answers (the F1–F5 retrieval failures), not the LLM.

Decision: fix the **source/extraction** first, then build a combined
**hierarchical + graph** model, with leaves indexed down to **sub-clause**
granularity.

---

## 2. Target architecture (hierarchy + graph, single version)

### 2.1 Hierarchy levels

```
L0 Document        Constitution of Pakistan
L1 Part            Part I–XII                      (~12)
L2 Chapter         where present                   (~28–32)
L3 Article         Articles 1–280 + lettered ones  (~290–305)
L4 Clause          (1), (2), (3) …                 (~1,300–1,800)
L5 Sub-clause      (a), (b), (c) … + provisos + explanations  (~3,000–5,000)
   Schedules       First–Seventh, parallel subtree  (7 / ~150–250 entries)
```

- **Embed/retrieve/rerank leaf = sub-clause** (L5), per the locked decision —
  finest citable unit, best for deep extraction.
- **Hard rule:** a sub-clause leaf always carries its **article heading +
  clause chapeau** prepended, and any **attached proviso/explanation in-leaf**.
  Never separate a proviso from the rule it modifies (this is what causes
  negation inversions like F3). Structural overlap, not blind token overlap.
- Article-level and Part-level chunks are also embedded, for coarse routing.

### 2.2 Graph overlay (built in the same version)

Edge types over the tree nodes:
- `contains` — tree edges (Part→Chapter→Article→Clause→Sub-clause)
- `cross_reference` — "subject to Article Y", "as in Article 260"
- `non_obstante` — "Notwithstanding anything in Article X…" (override edges)
- `defines` / `defined_by` — term ↔ Article 260 definitions
- `amended_by` — node ↔ amendment (the version dimension)

GraphRAG value: constitutional *reasoning* queries need legally-connected text,
not just topically-similar text. Retrieval = anchor by vector/BM25, then expand
1 hop along these edges.

### 2.3 Metadata per leaf

- **Citation path**: canonical `PartII/Ch1/Art-19A/cl-(1)/sub-(a)` + human
  `"Article 19A(1)(a)"`; `ancestor_ids[]`.
- **Structural**: part/chapter/article numbers + titles (marginal notes),
  clause/sub-clause ids, `unit_type`.
- **Amendment/version**: `inserted_by` / `substituted_by` / `omitted_by`
  (e.g. "18th Amendment, 2010"), `in_force_from`, `is_repealed`.
- **Semantic**: `legal_domain` tags; `cross_references[]`.

The existing Tier 3 schema (`parent_chunk_id`, `unit_type`, `unit_path`,
`domain_metadata`) can already hold all of this — no schema blocker.

### 2.4 Retrieval pipeline

1. **Citation fast-path** — query names "Article 199" → deterministic metadata
   lookup, bypass the reranker. Highest-ROI fix; kills F1/F2/F4.
2. **Hybrid recall** — BM25/FTS5 ⊕ dense vector → RRF fusion over sub-clause
   leaves. (Confirms the pending `fts5.md` decision: **keep FTS5**.)
3. **Rerank** — fine-tuned cross-encoder (zero-shot is domain-blind per eval).
4. **Small-to-big + graph expand** — promote hit to its Article; 1-hop along
   cross-reference / non-obstante / amendment edges.
5. **Generate** — clause-precise `[N]` citations from metadata.

---

## 3. Source acquisition — Constitution of Pakistan

PDF extraction is unfixable cheaply. A clean **structured HTML** source exists
and is far better — explicit Part/Chapter/Article/Clause boundaries for free.

| Source | Form | Structure | Amendments | Verdict |
|---|---|---|---|---|
| **pakistani.org/pakistan/constitution** | HTML, per-Part/Chapter pages (`part2.ch1.html`) | Article number + marginal note + clauses `(1)(2)` + sub-clauses `(a)(b)` + `Provided that` + `Explanation-I` all typographically distinct | Incorporated **through 27th Amendment** (Nov 2025); separate amendment pages too | ✅ **Primary source** |
| National Assembly (na.gov.pk) | Official PDF | Authoritative but same PDF-extraction problem | Official | ✅ Authority cross-check only |
| CommonLII (commonlii.org/pk/legis/const/1973) | HTML, per-article pages | Per-article granularity | TOC + schedules | ⚠️ Fallback (returned 403 on 2026-05-21) |
| Internet Archive / P&D KP / Bhutto.org | PDF | — | varies | ✗ PDF, skip |

**Plan:** scrape pakistani.org per-Part/Chapter HTML → parse the typographic
markers into the L0–L5 tree → cross-check article text/count against the
na.gov.pk official PDF → use the site's separate amendment pages to populate the
`amended_by` version graph.

---

## 4. Generalization to the broader EchoAI corpus

**Yes — this design generalizes strongly.** Inspected the two Desktop folders:

- `Sindh Govt Documents/` — 6 files
- `Sindh General Administration Documents/` — ~55 files (heavy duplication:
  several instruments appear under 2–3 names with identical byte sizes)

Document types present: **Acts** (Sindh Civil Servants Act, Removal from Service
Special Powers, Transparency & RTI, SLGO 2001) · **Rules** (Rules of Business
1986, Appointment/Promotion/Transfer, Efficiency & Discipline, Appeal, Judicial
Service 1994, Public Procurement 2010, Upper-Age-Relaxation 2024) · **Manuals**
(Secretariat Instructions 2010) · **Schedules & schedule-amendments** ·
**Notifications/Orders** ("…NOMINATED UNDER SECTION 3(5)(vii)") · **Circulars**.

### 4.1 Same structural DNA

Every one of these is a numbered-clause instrument:
`container → numbered provision → sub-provision → clause → sub-clause →
proviso/explanation → schedules`. Only the **level vocabulary** differs:

| Doc type | L1 | L2 | L3 | L4 | L5 |
|---|---|---|---|---|---|
| Constitution | Part | Chapter | Article | Clause | Sub-clause |
| Act / Ordinance | Chapter | — | Section | Sub-section | Clause |
| Rules / Regulations | Part | — | Rule | Sub-rule | Clause |
| Manual | Chapter | — | Instruction / ¶ | — | — |
| Notification / Circular / Order | (mostly flat — a single operative provision that *references into* the graph) |

### 4.2 The graph layer matters MORE here

The constitution is largely one self-contained document. The Sindh corpus is a
**web of instruments** — it *needs* cross-document edges to be coherent:

- `amends` — "rules of business amendment 3" → "rules of business main"
- `supersedes` — "Upper Age Relaxation Rules 2024" → older version
- `issued_under` — "Two Provincial Ministers… under Section 3(5)(vii)" → that
  section of the parent Act/Rules
- `schedule_of` — "Schedule-I Sr.No.7-A (Amendment)" → parent rules
- `references` — a rule citing the Civil Servants Act

A flat-chunked amendment PDF is meaningless without its base document. So
hierarchical + graph is not merely *applicable* to the govt corpus — it is
*required* for it.

### 4.3 What must be generalized in the design

1. **Doc-type profiles** — the structure-aware chunker is parametrized by a
   per-type profile (level vocabulary + heading patterns), not hardcoded to
   "Article". Constitution = profile #1; Act, Rules, Notification = #2–#4.
2. **`document_type` metadata field** — Constitution / Act / Ordinance / Rules /
   Regulations / Manual / Notification / Circular / Order / Schedule-Amendment.
3. **Cross-document graph edges** (§4.2) as first-class — the corpus is a forest
   of trees joined by the graph.
4. **`file_hash` de-duplication** — the Sindh folders have many byte-identical
   duplicates; `sources.file_hash` already exists, use it.

### 4.4 Beyond government

The same model fits any numbered-clause structured document — contracts
(`law-datasets/services_agreement_acme_globex.md` has Section→clause→sub-clause),
standards, policies, technical manuals. This is consistent with the
"EchoAI must not be legal-only" domain-scope mandate.

---

## 5. Sourcing strategy differs by document type

Verified by web search 2026-05-21. The constitution is the **exception**, not
the rule — clean structured sources do **not** exist for the govt corpus.

| Corpus | Online availability | Format | Verdict |
|---|---|---|---|
| **Constitution** | pakistani.org | clean structured **HTML**, amendments to 27th | ✅ scrape HTML |
| **Sindh Acts** (Civil Servants Act, RTI, Procurement Act, SLGO) | sstsindh.gov.pk, sindhlaws.gov.pk "Sindh Code", pas.gov.pk acts DB | **PDF only** on every official portal; pas.gov.pk is a filtered DB (format unconfirmed); third-party HTML (nasirlawsite, SOHRIS) exists for a few, quality unverified | ⚠️ mostly PDF → extraction |
| **Sindh Rules** (Rules of Business 1986, E&D, APT, Procurement Rules, Judicial Service) | sindh.gov.pk, sindhlaws.gov.pk, dept portals | **PDF only**, typeset/scanned | ❌ PDF → extraction |
| **Notifications / Schedule-amendments / Circulars / Orders** (bulk of the General Administration folder) | departmental portals, ad-hoc | **PDF only**, often scanned; no structured online source exists or ever will | ❌ PDF → OCR |

**Conclusion:** the "find a clean source" shortcut works for the **constitution
only**. For the Sindh corpus there is no structured source to scrape — the
layout-aware **OCR / extraction pipeline is the unavoidable gating dependency**.
The structural design (§2–§4) generalizes fully; the sourcing strategy does not.

Both paths feed the same hierarchical + graph model.

---

## 6. Open decisions & dependency order

Dependency order is strict:

1. **Acquire clean structured text** — constitution via pakistani.org scrape;
   govt docs via layout-aware OCR/extraction.
2. **Structure-aware parse** into the L0–L5 tree (profile-driven).
3. **Build graph edges** (intra- and cross-document).
4. **Multi-level embed + hybrid retrieve + graph expand.**

Decided:
- **Graph store — SQLite tables in `echo.db`** (`graph_nodes`, `graph_edges`),
  traversed with recursive CTEs. Constitution Principle IV ("single `echo.db`;
  MUST NOT create separate database files") forbids a dedicated store. CozoDB
  was reconsidered against production scale and rejected — see §7.1.
- **v1 scope — constitution only** (clean pakistani.org source available now);
  Act/Rules profiles deferred to v2 behind the OCR phase.

Open decisions:
- OCR/extraction engine for the govt corpus — v2, ties into the OCR phase.

---

## 7. Constitution v1 — scope & acceptance

### 7.1 Graph storage — SQLite tables, and why (CozoDB reconsidered)

The graph lives as two tables in `echo.db` — `graph_nodes` and `graph_edges` —
traversed with recursive CTEs. CozoDB / KuzuDB / Oxigraph were evaluated and
rejected:

- **Constitution Principle IV** mandates a single `echo.db` and forbids separate
  database files. A dedicated store would require a constitution amendment.
- **Scale analysis settles it.** Production (~2,000 docs) ≈ ~1M graph nodes /
  ~3M edges. But graph use at *retrieval time* is **local neighbourhood
  expansion** — from ~20 retrieved seed leaves, expand 1–2 hops. Cost scales
  with (seed set × hops × branching factor), **not** total corpus size. With a
  composite index on `graph_edges(from_node, edge_type)`, 1-hop expansion is
  ~20 index seeks (sub-millisecond) whether the graph holds 1.8k or 10M nodes.
- CozoDB only pays off for **whole-graph analytics** (PageRank, community
  detection) or deep unbounded recursive reasoning — neither of which legal RAG
  retrieval performs. Corpus-wide analytics, if ever needed, are precomputed in
  an offline batch job and materialised as node columns.
- A second storage engine also adds WAL/lock-contention risk (this project has
  already fought an indexer pool-deadlock).

Engineering rules that keep SQLite-graph fast at production scale: composite
edge indexes; recursive CTEs bounded to depth ≤ 2–3 with a visited-set dedup;
no unbounded traversal; analytics offline-only.

### 7.2 v1 — in scope

End-to-end hierarchical + graph indexing of the **constitution only**, from the
clean pakistani.org source, proven by an acceptance test. Builds on the existing
Tier 3 branch (schema, chunker, reranker, grounding, query_router all reused).

| Stage | Work |
|---|---|
| S1 | **Source importer** — scrape pakistani.org per-Part/Chapter HTML → clean structured intermediate (one-off; *not* the generic pipeline). |
| S2 | **Structure-aware parse** — a `constitution` doc-type profile (level vocabulary + heading patterns) parses the IR into the L0–L5 tree. Replaces regex-on-dirty-text. |
| S3 | **Persist** — extend `unit_type` vocabulary to part/chapter/article/clause/subclause; write tree with `parent_chunk_id`, `unit_path`, citation metadata. Reuses Tier 3 schema. |
| S4 | **Graph** — integrate `cozo`; build `contains` / `cross_reference` / `non_obstante` / `amended_by` edges into `echo-graph.db`. |
| S5 | **Embed** — multi-level: sub-clause leaves (heading + chapeau prepended) + article + part chunks → existing sqlite-vec. |
| S6 | **Retrieve** — citation fast-path → hybrid BM25/FTS5 ⊕ vector RRF over sub-clause leaves → reranker → small-to-big to article → 1-hop Cozo graph expand → clause-precise citations. |
| S7 | **Acceptance harness** — §7.4. |

### 7.3 v1 — out of scope (→ v2)

Generic OCR/layout-aware extraction · Sindh Act/Rules doc-type profiles ·
reranker fine-tune (separate track; v1 uses the current reranker) · multi-hop
(>1) graph reasoning.

### 7.4 v1 acceptance test — the gate before v2

v1 must pass before v2/OCR work begins. Four checks, extending the
`db_inspection` harness:

1. **Structural** — re-run the `07`/`08` hierarchy scripts. Expect ~12 Parts,
   ~30 Chapters, **~290 Articles**, ~1,500 Clauses, ~3–5k sub-clause leaves; **no
   parent absorbing >90% of leaves**; `orphans = 0`. Contrast: current `1/11/253`.
2. **Citation accuracy** — sample N leaves; each leaf's `citation_path` must
   match the article it actually contains (today's failure: Article-95 text
   stamped `Article 69`).
3. **Graph integrity** — `cross_reference` edges resolve to real article nodes;
   `amended_by` edges present; spot-check a known non-obstante article.
4. **Retrieval** — re-run the F1–F5 Tier 3 failure queries. v1 PASS = the
   article-number queries (F1/F2/F4) retrieve the correct article via the
   citation fast-path. F3/F5 (reasoning) should improve but may still need the
   fine-tuned reranker — tracked separately, not a v1 blocker.
```

---

## §8 — v1 as-built (2026-05-24)

Recorded at the close of S7 acceptance. Where the shipped numbers drift from
§7 estimates, the cause is called out inline. Full T030 gate output:
[`specs/003-constitution-hierarchical-graph/T030-acceptance-results.md`](../../specs/003-constitution-hierarchical-graph/T030-acceptance-results.md).

### 8.1 Chunk counts vs the §7.4 estimate

| Unit type | §7.4 estimate | v1 as-built | Why |
|---|---|---|---|
| document | 1 | **1** | — |
| part | ~12 | **12** | — |
| chapter | ~30 | **29** | — |
| article | ~290 | **313** | importer recovered 312 from the IR; +1 is the Preamble parent added by the 2026-05-23 import-gap fix (the IR's `preamble` field was being silently dropped at tree assembly) |
| clause | ~1,500 | **808** | the importer's IR has 787 clause-rank units; the +21 are Idea 1 paragraph splits of the Preamble (2775 chars / 14+ paragraphs → clause-level children) |
| subclause | ~3,000–5,000 | **325** | the IR's actual sub-clause leaf count — §7.4's "3–5k" was a worst-case ceiling, not a target |
| **Total chunks** | — | **1,488** | unchanged baseline shape |

`orphans = 0` · max-children ratio < 0.90 · one document root with NULL parent.

### 8.2 Graph as-built

| Layer | Count | Note |
|---|---|---|
| `graph_nodes` | 1,488 | 1-1 with chunks (every chunk gets a node) |
| `contains` edges | 1,487 | tree skeleton: `graph_nodes − 1` |
| `cross_reference` edges | 116 | every "Article N" mention in another unit's text |
| `non_obstante` edges | 23 | "Notwithstanding anything…" overrides |
| **Total edges** | **1,626** | no dangling endpoints |

### 8.3 Retrieval boost stack (post-A4)

The S1–S6 plan envisioned hierarchical-only retrieval + citation fast-path.
The actual shipped stack added four post-RRF boost layers and one
indexing-time change to close A4 across all 15 F-set prompts:

1. **Citation fast-path** (S6, T025) — query mentioning `Article N(…)` or `Preamble` bypasses vec/FTS/reranker entirely.
2. **Idea 1 — paragraph splitting at indexing time** (2026-05-23) — any leaf chunk > `ECHO_LONG_CHUNK_THRESHOLD` (default 1000) splits into clause-level paragraph children under an article-level parent. The Preamble (2775 chars) is the only chunk that splits in the current corpus; future Acts/Policies with long openings will benefit identically. Doc-agnostic — no per-doc-type code.
3. **S1 salience boost** — `DocProfile.salient_units` lists labels that get a 1.3× post-RRF score multiplier. Constitution declares `["Preamble"]`.
4. **S2 definitional-question classifier** — regex on English question stems (`who is`, `what is the meaning of`, `define X`, …). When matched: vec_k and fts_k double; salience multiplier escalates to 1.6×.
5. **S3a NER on constitution chunks** — removed the v1-scope bypass; Pass-1 regex runs at index time, Pass-2 LLM runs incrementally (see Idea 2).
6. **S3b entity-NP boost** — capitalised noun phrases in the query JOIN `entities`+`entity_mentions`; matched chunks get 1.4× score multiplier.

### 8.4 NER infrastructure — Idea 2 incremental coverage

- Migration `015_ner_pass2_progress.sql` added `chunks.ner_pass2_done`.
- `run_ner_loop_for_source` (in `services/indexer.rs`) processes
  `ECHO_PASS2_BATCH_PCT` (default 10%) of pending chunks per batch, sleeps
  `ECHO_PASS2_SLEEP_SECS` (default 30s) between batches, runs until 100% covered.
- `select_next_pass2_batch` always includes pending salient chunks (Preamble)
  in every batch via the same `is_salient_unit` helper S1 uses.
- Startup hook `resume_pending_pass2_ner` resumes incomplete sources across
  app restarts.
- NER model picked via `query_router::select_model_for_tier` —
  auto-discovers any `*.gguf` in `models/`, no hardcoded filename.

**Known v2 follow-up:** NER **quality** on local 0.5B is uneven on legal
English (Preamble's "Quaid-i-Azam Mohammad Ali Jinnah" was not extracted as
a `person`; some hallucinations like `Karachi`, `Noether, Julius`). The Idea
2 *infrastructure* is sound — only the model file needs upgrading (drop
1.5B or 3B GGUF into `models/`, `UPDATE chunks SET ner_pass2_done = 0`, let
the loop redo it). S3b boost is conservative (1.4×) so noisy entity data
does not produce wrong answers; salience boost + Idea 1 paragraph splitting
already carry F5 retrieval without entity matches.

### 8.5 Build-context cap regression (2026-05-23)

The `build_context()` per-chunk cap (`ECHO_PER_CHUNK_CAP_FRAC=0.25`, so 600
tokens at default budget 2400) silently dropped a single-chunk result that
was over the cap — exactly the case that the citation fast-path produced
for `"preamble"` queries before Idea 1. Fix: skip the cap when the chunk
is the first in the candidate list (mirrors the existing budget-overflow
override one branch below). Documented in
`src-tauri/src/services/query_router.rs::build_context` with the test
`context_builder_admits_single_oversized_citation_chunk` as the regression
net.

### 8.6 Acceptance gate (S7)

The §7.4 plan called for four checks. They now exist as `cargo test --test
constitution_v1_acceptance` (5 tests, ~1.6 s, no LLM dependency,
in-memory SqlitePool). Bash probes
(`~/Desktop/db_inspection_constitute_v1/07_hierarchy.sh`,
`09_citation_accuracy.sh`, `10_graph_integrity.sh`) are kept as developer
diagnostics and were patched on 2026-05-24 to exempt
`unit_label = 'Preamble'` rows for parity with the Rust harness.

A1/A2/A3 deterministic; A4 covers the citation-fast-path subset (F1/F2/F4
+ Article 19A + Preamble parent-plus-children). F3/F5 require full hybrid
retrieval + LLM and are validated by the F-set re-runs in
`~/Desktop/test 5 prompts/preamble_v2_idea1.*` (3B passes both after Idea 1).

### 8.7 What's queued for v2

| ID | Item | Why deferred |
|---|---|---|
| #4 | `index_constitution → index_structured_doc` rename + IR-schema generalisation | Architectural cleanup; doesn't affect retrieval; v2 needs it before adding Acts/Rules |
| #5 | S4 HyDE query expansion + prompt prediction + spelling autocorrect | Heavier LLM path; needs its own design pass |
| #13 | Wire Pass-2 NER through the foreground scheduler properly | Currently chat waits up to one in-flight chunk (~5–10 s) for `ModelSlot` |
| NER quality | Switch Pass-2 to 1.5B/3B local or cloud | Infrastructure ready; only the model file changes |

