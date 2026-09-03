# Legal RAG for Pakistan Government Documents — Design

**Date:** 2026-06-02
**Status:** Approved by user — ready for implementation planning
**Feature branch:** `007-legal-rag-pakistan` (to be created off `master`)
**Supersedes:** `project_echoai_domain_scope.md` ("domain is generic"), the generic-domain clause in `feedback_llm_ir_only_pdf_scanned_images.md`

---

## 1. Background

EchoAI was built as a domain-neutral RAG system, with the Sindh Constitution serving as the showcase corpus through Feature 005 (generic structured importer) and Feature 006 (LLM PDF→JSON importer, mid-flight on its own branch). This design records the project's pivot to a Pakistan government legal RAG product, and the architecture for v1.

The pivot is a **hard pivot**: EchoAI becomes a legal-domain product. Generic-domain prompts, IR schemas, and evaluators get rewritten for legal artifacts. The "domain is generic" memo is retired by the constitution amendment in §2.

## 2. Constitution amendment (`.specify/memory/constitution.md`)

The SDD constitution gets five new principles, in this order of precedence:

1. **Domain principle.** EchoAI is a Pakistan government legal RAG system. Primary artifact types: Constitution, Acts, Ordinances, Rules, Regulations, Policies, and their cross-references. Out of scope: case law / judgments, foreign jurisdictions, private legal practice.
2. **Primary user principle.** Government officer / policy analyst. All defaults (citation format, answer length, export shape, multi-turn behavior) optimize for this persona. Citizen and lawyer personas are supported as secondary modes, not first-class defaults.
3. **Citation principle.** Every answer carries a precise legal citation: instrument name + year + full hierarchical path (Chapter → Section → Sub-section → Clause → Sub-clause) + amendment status as of today. No paraphrase without citation.
4. **Temporal principle.** Latest consolidated text is the primary truth. Each section carries its amendment chain (who inserted/substituted/repealed it, by which Act, when). Point-in-time reconstruction is out of scope for v1.
5. **Cross-reference principle.** Acts ↔ Rules ↔ Policies form a first-class graph, not an afterthought. "Rules under Section X of Act Y" and "Policies citing Section X" are core retrieval surfaces.

The two superseded memos get a `SUPERSEDED — see constitution §X` header but stay in the memory tree for history.

Existing principles (small-diff, ask-clarifiers, MCP-verify, PHR-per-request, ADR-on-significance, no-secrets, citation format `start:end:path`) stay untouched.

## 3. Scope

### 3.1 In-scope (v1 ship)

- **Two-tier indexing.** Default fast path uses the existing Feature 004/005 pipeline (lopdf / pdftotext / OCR → chunks + embeddings + FTS) — no LLM IR build. Cheap, runs in seconds. The deep path runs the Mistral legal-IR pipeline on demand: when an officer is not getting good answers on a document, they click "Deep re-index with Mistral" and the system rebuilds chunks from the legal IR (Section-boundary-respecting), populates amendment events, cross-ref edges, and procedures, then re-embeds and re-indexes FTS. The deep result replaces the fast-path chunks for that document.
- **Sticky tier.** Once a document is `deep`, subsequent re-indexes (including re-uploads with the same sha256) stay deep. The Mistral JSON sidecar is cached on disk and re-used; only changed content re-runs the LLM. To go back to `fast`, the user must explicitly downgrade.
- **Legal IR schema** — replaces the generic Section/Clause/Paragraph node kinds. Produced by the deep path only.
- **Amendment chain extraction** — per-section log of insert/substitute/repeal events, driven by markers like `X[…]`, `[Substituted by Act …]` already present in Pakistan govt PDFs. Deep path only.
- **Cross-reference graph** — typed edges (`cites`, `implements`, `under`, `amends`) between nodes. Deep path only.
- **Officer-grade citation rendering** — single formatter used everywhere. Fast-path docs cite with reduced precision (page / span); deep-path docs cite with full legal path.
- **Legal-tuned retrieval** — section-boundary-respecting chunking (deep only), graph-aware expansion, citation fast-path on the new path format.
- **Intent router in chat** — natural-language queries are classified to one of eight intents and dispatched to a per-intent handler. No mode-switch UI. Handlers requiring deep features detect `index_tier='fast'` on the active document and surface a soft upgrade prompt ("This doc isn't deep-indexed — run Mistral now?") instead of silently returning poor answers.
- **Procedure / process-flow extraction** — a new IR layer attached to procedural sections, with cross-instrument stitching via the graph. Deep path only.
- **Scope picker** — existing default *Both* (folder + notebook); user can narrow to a specific folder doc, whole folder, specific notebook doc, or whole notebook. Every handler respects active scope.

### 3.2 Out-of-scope (v1)

- Case law / judgments
- Point-in-time queries ("as of date X")
- Foreign jurisdictions
- Drafting / generation of new legal text
- Bilingual (Urdu) ingestion or retrieval
- Audit log of officer answers
- Saved queries / notebooks per matter
- Editable / interactive workflow diagrams

### 3.3 Constraints

- **Default mode is offline.** Production LLM tier is offline (Qwen2.5-0.5B-FT or 3B); Mistral-7B-Instruct (already on disk from 006) is available as a heavier offline-mid model. Cloud providers (Groq, Cerebras) are used for evaluation and regression baselines only, never as the default production path. An explicit per-query "use cloud" toggle stays available.
- **Offline-first is non-negotiable.** A future LAN client–server deployment is anticipated (one box runs models + DB, others connect over LAN), but still without internet access.

## 4. Eight chat intents

A single chat window. The user types a natural-language query; an intent classifier routes it. No mode toggles in the UI.

1. **Lookup** — "show me Article 25" / "Section 6(2)(a) of the RTI Act".
2. **List children** — "what articles are in Chapter 2?" / "list all sections under Part III".
3. **Summarize** — any granularity: clause, section, chapter, instrument.
4. **Difference** — two flavors: amendment diff (before/after on the same section) and instrument diff (e.g. Federal Act vs Sindh counterpart).
5. **Cross-comparison** — N-way comparison across instruments on a chosen dimension (e.g. "compare how Acts X, Y, Z handle penalties").
6. **Action items** — duties, powers, timelines, penalties imposed by an instrument or section, grouped by actor.
7. **Chronological order** — two flavors: amendment timeline of a single section, and thematic timeline of all instruments on a topic ordered by enactment date.
8. **Procedure / process flow** — actor-step path for an officer workflow (e.g. "approval flow for an arms license"), stitched across Act + Rules + Manual where applicable. Linear render by default; "show branches" expands conditional logic on demand.

Each intent has its own handler with its own retrieval shape, prompt, and render template. The chat UI does not branch on intent — only the content inside the bubble differs.

## 5. Architecture

### 5.1 Module layout (new + changed)

```
src-tauri/src/
  services/
    legal_ir/              [NEW] — legal IR pipeline; reuses 006's engine glue
      mod.rs
      types.rs             — NodeKind enum, ProcedureNode, ProcessStep
      schema.rs            — llguidance JSON schemas
      prompts.rs           — Mistral prompts (build, classify, procedure-extract)
      amendment_parser.rs  — deterministic regex pass for X[…] / [Substituted by …]
      xref_extractor.rs    — cites / implements / under edge extraction
      procedure_extractor.rs — procedural-section detection + step extraction
      mistral_builder.rs   — reused from 006, parameterized on schema
      progress_store.rs    — reused from 006 as-is
    intent_router/         [NEW]
      mod.rs
      classifier.rs        — rule-first, small-LLM tiebreak
      slots.rs             — slot extraction (instrument, section path, topic, comparand list)
      handlers/
        lookup.rs
        list_children.rs
        summarize.rs
        difference.rs      — amendment + instrument flavors
        cross_compare.rs
        action_items.rs
        timeline.rs        — amendment + thematic flavors
        procedure.rs
        fallback.rs
    citation/              [NEW]
      mod.rs               — single officer-grade formatter, used everywhere
    query_router.rs        [CHANGED] — delegates to intent_router; scope picker flows through unchanged
    indexer.rs             [CHANGED] — PDFs routed through legal_ir, then xref pass, then procedure pass
    llm.rs                 [CHANGED] — explicit tiering: offline-small / offline-mid / cloud-eval
  db/
    migrations/
      020_legal_ir.sql     [NEW] — graph_nodes legal columns, amendment_events, graph_edges, procedures, procedure_steps
```

### 5.2 Data model — migration 020

```sql
-- Tag each document with its current index tier (fast | deep). Sticky.
ALTER TABLE documents ADD COLUMN index_tier TEXT NOT NULL DEFAULT 'fast'
  CHECK(index_tier IN ('fast','deep'));

-- Extend graph_nodes (additive; generic 006 rows stay readable)
ALTER TABLE graph_nodes ADD COLUMN instrument_id TEXT;
ALTER TABLE graph_nodes ADD COLUMN legal_path TEXT;            -- "Ch-III/S-12/(2)/(a)"
ALTER TABLE graph_nodes ADD COLUMN in_force INTEGER DEFAULT 1;
ALTER TABLE graph_nodes ADD COLUMN source_page INTEGER;
CREATE INDEX idx_graph_nodes_instrument ON graph_nodes(instrument_id);
CREATE INDEX idx_graph_nodes_path ON graph_nodes(instrument_id, legal_path);

-- Amendment chain (one row per amendment event)
CREATE TABLE amendment_events (
  id INTEGER PRIMARY KEY,
  node_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('inserted','substituted','repealed','omitted')),
  by_instrument TEXT NOT NULL,             -- "Act-XII-of-2021"
  effective_date TEXT,                     -- ISO date if extractable
  source_page INTEGER,
  raw_marker TEXT,                         -- "X[…]" or "[Subs. by Act XII of 2021]"
  FOREIGN KEY (node_id) REFERENCES graph_nodes(id)
);
CREATE INDEX idx_amendment_events_node ON amendment_events(node_id);

-- Typed cross-reference edges
CREATE TABLE graph_edges (
  id INTEGER PRIMARY KEY,
  src_node_id TEXT NOT NULL,
  dst_node_id TEXT NOT NULL,
  edge_kind TEXT NOT NULL CHECK(edge_kind IN ('cites','implements','under','amends')),
  confidence REAL NOT NULL DEFAULT 1.0,
  source_phrase TEXT,
  FOREIGN KEY (src_node_id) REFERENCES graph_nodes(id),
  FOREIGN KEY (dst_node_id) REFERENCES graph_nodes(id)
);
CREATE INDEX idx_graph_edges_src ON graph_edges(src_node_id, edge_kind);
CREATE INDEX idx_graph_edges_dst ON graph_edges(dst_node_id, edge_kind);

-- Procedure layer
CREATE TABLE procedures (
  id INTEGER PRIMARY KEY,
  anchor_node_id TEXT NOT NULL,
  process_name TEXT NOT NULL,
  description TEXT,
  FOREIGN KEY (anchor_node_id) REFERENCES graph_nodes(id)
);

CREATE TABLE procedure_steps (
  id INTEGER PRIMARY KEY,
  procedure_id INTEGER NOT NULL,
  step_order INTEGER NOT NULL,
  actor TEXT NOT NULL,                     -- "Director", "Secretary", "Town Officer", "Municipal Officer", "SHO", "DPO", "DC", "Commissioner"
  action TEXT NOT NULL,
  condition TEXT,                          -- "if amount > 5 lakh"
  branch_of_step_id INTEGER,               -- nullable; non-null = branching child
  timeline TEXT,                           -- "within 30 days"
  source_node_id TEXT NOT NULL,
  FOREIGN KEY (procedure_id) REFERENCES procedures(id),
  FOREIGN KEY (source_node_id) REFERENCES graph_nodes(id)
);
CREATE INDEX idx_procedure_steps_proc ON procedure_steps(procedure_id, step_order);
```

`TARGET_SCHEMA_VERSION` bumps 15 → 16. Strictly additive. Generic 006 graph_nodes rows remain valid (the new columns are nullable / default).

`actor` is free-text TEXT in v1. Designations vary widely across acts/rules/manuals (`Director`, `Secretary`, `Town Officer`, `Municipal Officer`, `SHO`, `DPO`, `DC`, `Commissioner`, etc.). A normalized actor vocabulary with spelling tolerance is a v2 follow-up.

### 5.3 Indexing pipelines (per PDF)

#### Fast path (default — runs at upload time)

```
PDF ──► existing 004/005 extraction (lopdf → pdftotext → OCR)
        │
        ▼
      chunk by paragraph / span
        │
        ▼
      Embed + FTS index
        │
        ▼
      documents row: index_tier = 'fast'
```

No LLM. Runs in seconds. Citations are page-and-span level. Most documents will live here. Officers using the fast path can still ask Lookup, Summarize, and ActionItems queries with reduced citation precision.

#### Deep path (on demand — `deep_reindex_document(doc_id)` command)

Triggered by the officer clicking "Deep re-index with Mistral" on a document whose answers are poor. Runs in the background, cancellable, with progress events.

```
PDF ──► classify_doc_type (offline-small LLM, grammar)
          └─► is it a legal instrument? if not, refuse with reason
        │
        ▼
      chunk by page + Mistral IR build (006 engine, NEW legal schema)
        │  Produces legal IR JSON
        ▼
      persist IR JSON sidecar at {app_data_dir}/ir_cache/<sha>.json
        │
        ▼
      DELETE existing chunks + embeddings + graph_nodes for this doc_id
        │
        ▼
      re-chunk from IR nodes (Section-boundary-respecting)
        │  Produces graph_nodes with instrument_id + legal_path
        ▼
      amendment_parser pass (deterministic, regex-driven)
        │  Writes amendment_events rows
        ▼
      xref_extractor pass (rule-based phrase parser + small-LLM disambiguator)
        │  Writes graph_edges rows (cites / implements / under / amends)
        ▼
      procedure_extractor pass (ONLY on nodes tagged procedural)
        │  Grammar-constrained step extraction with offline model
        ▼
      Writes procedures + procedure_steps
        │
        ▼
      Re-embed + re-FTS-index from new chunks
        │
        ▼
      documents row: index_tier = 'deep'  (sticky)
```

Every pass is resumable via the existing `progress_store` pattern from 006; an 8-hour run that dies overnight picks up where it stopped. JSON sidecar is content-addressed by sha256: an unchanged re-upload skips the LLM entirely and rebuilds chunks from the cached IR.

#### Tier transitions

- `fast → deep`: officer click, runs the deep pipeline above.
- `deep → deep` on re-upload: sha256 hit on the JSON sidecar → skip LLM, re-chunk from cached IR.
- `deep → fast` (downgrade): explicit officer action only; deletes legal IR rows for the doc and re-runs the fast path. Not a routine operation.
- An unchanged re-upload of a `fast` doc stays `fast` — no automatic upgrade.

### 5.4 Query pipeline (per chat message)

```
User query + active scope (Both | Folder | Doc | Notebook | Notebook-doc)
        │
        ▼
  intent_router::classify
    1) rule-based pass (keywords, regexes for "show me Article X", "difference between", etc.)
    2) if low confidence → offline-small LLM tiebreak (grammar-constrained Intent enum)
    3) slot extraction (instrument, section, comparands, topic, date)
        │
        ▼
  Dispatch to IntentHandler
    Each handler:
      - builds its own retrieval query (may expand via graph_edges / amendment_events)
      - respects the active scope filter
      - calls offline-small or offline-mid model with a strict prompt + retrieved context
      - renders via citation:: formatter
        │
        ▼
  Chat bubble: headline · body · citation block · optional "see also" · optional flow diagram
```

The chat UI does not branch on intent; it renders whatever the handler returns.

### 5.5 LLM tiering

| Tier | Model | Used for |
|---|---|---|
| `offline-small` | Qwen2.5-0.5B-FT (existing) | Intent classification tiebreak, summarize, action items, citation rendering helpers |
| `offline-mid` | Qwen2.5-3B or Mistral-7B-Instruct (006's existing GGUF) | IR build, procedure extraction, cross-comparison, difference (anything structure-heavy or multi-doc) |
| `cloud-eval` | Groq llama-3.3-70b, Cerebras | Eval baseline, regression testing, optional "use cloud" toggle. Never default. |

`llm.rs` gets an explicit `Tier` parameter on every call site — no implicit model selection.

### 5.6 Engine reuse from 006

006 stays as a frozen generic baseline on its own branch. 007 reuses, without forking:

| Reused from 006 | Touched in 007 |
|---|---|
| `mistral_builder` infer-with-grammar | Re-parameterized on legal schema (no fork) |
| `progress_store` resume | Used as-is for each new pass |
| llguidance binding + sampler factory | Used as-is |
| Classifier scaffold | Repurposed for procedural-section detection |
| Indexer dispatch | Branches on `is_legal_instrument` flag |
| JSON cache on disk | Used as-is |
| Cancel-token registry | Used as-is |

Generic IR (006's NodeKind set) remains a fallback in `indexer::do_index`: if a PDF doesn't classify as a legal instrument, it indexes through the existing 006 generic builder.

### 5.7 Future LAN client–server split

All retrieval + LLM calls go through `services/` traits. UI code does not reach directly into a model or the DB. Splitting into a daemon + thin LAN client is a matter of adding a local HTTP/IPC layer between Tauri commands and `services/`. No design choice in v1 makes this harder.

## 6. Slices and acceptance gates

### Slice 0 — Foundation (1 PR)

- Branch `007-legal-rag-pakistan` off `master`.
- Constitution amendment per §2.
- Migration `020_legal_ir.sql`. `TARGET_SCHEMA_VERSION` 15 → 16.
- Extract 006's engine pieces so `legal_ir/` reuses without copy-paste.
- `services/citation/` skeleton + officer-grade formatter unit-tested against ~12 hand-written golden citations.
- Spec files under `specs/007-legal-rag-pakistan/`: `spec.md`, `plan.md`, `tasks.md`.

**Exit:** `cargo test` green; migration runs clean on a copy of existing `echo.db`; 006 generic-IR flow unaffected.

### Slice A — Legal IR + amendment chain

- `legal_ir::types`, `schema`, `prompts` with the new NodeKind enum.
- `amendment_parser` (deterministic, no LLM): `X[…]`, `[Substituted by Act …]`, `[Inserted by Ordinance …]`, `[Omitted by …]`.
- Indexer dispatch: legal vs generic.
- Per-section chunking respects legal boundaries.

**Eval gate:**
- Re-index Sindh Local Government Act 2013 + Sindh RTI Act 2017 + one Rules instrument under Sindh LG Act.
- ≥ 90% of top-level Sections captured (vs hand-built index).
- 100% of obvious `X[…]` markers found.
- Zero orphan nodes.
- 20-prompt constitution eval: no regression below Groq K=20 baseline of 80% OK.

### Slice B — Intent router + 4 read-side intents

- `intent_router::classifier` (rule-first, small-LLM tiebreak).
- `intent_router::slots`.
- Handlers: `Lookup`, `ListChildren`, `Summarize`, `ActionItems`.
- Scope picker plumbing through every handler.
- All citation rendering via `services/citation/`.

**Eval gate:**
- Build a 40-prompt government-officer eval set (10 per intent), Sindh corpus.
- Offline-small ≥ 60% OK · Offline-mid ≥ 75% OK · Groq ≥ 90% OK (ceiling).
- Citation accuracy ≥ 95% on lookups.

### Slice C — Cross-ref graph + 3 comparison intents

- `xref_extractor` (phrase parser + small-LLM disambiguator) writing `graph_edges`.
- Handlers: `Difference` (both flavors), `CrossCompare` (N-way), `Timeline` (both flavors).
- "See also" surface in chat bubble.

**Eval gate:**
- 30 new prompts (10 per intent); total eval set 70.
- Amendment-diff ≥ 80% OK on offline-mid.
- xref edge precision ≥ 85% on a 100-edge human-graded sample.

### Slice D — Procedure extraction + flow intent

- Procedural-section classifier (heuristic + small-LLM confirm).
- `procedure_extractor` with grammar-constrained step extraction.
- Cross-instrument stitching via `graph_edges` (`implements`, `under`).
- `Procedure` handler + linear flow renderer with on-demand "show branches".

**Eval gate:**
- 15 procedure queries from Sindh corpus (license issuance, RTI request flow, NOC, building plan approval, etc.).
- ≥ 70% return a procedure with all key steps + correct actors + correct citation per step.
- "Show branches" works on at least 3 procedures with known conditional logic.

### Final acceptance gate (end of Slice D = v1 ship)

Single `cargo test --test legal_rag_v1_acceptance` harness, modeled on the constitution_v1 gate:

1. Migration 020 runs clean on a copy of master `echo.db`.
2. Sindh LG Act + Sindh RTI Act + Sindh LG Rules re-index without error; node + edge + procedure counts match goldens within ±5%.
3. Citation formatter produces correct officer-grade strings for 12 golden cases.
4. Intent classifier ≥ 90% accuracy on 70-prompt eval (offline-mid).
5. End-to-end: 5 hand-picked officer queries (one per major intent including Procedure) return correct answer + correct citation on offline-mid.

Separate report (not a CI gate): full 70-prompt eval across offline-small / offline-mid / Groq archived under `specs/007-legal-rag-pakistan/eval/`.

## 7. Risks

1. **Offline-mid quality gap on Difference + CrossCompare.** If Slice C eval misses gate, swap offline-mid default from Qwen 3B to Mistral 7B (already on disk from 006). No architectural change required; decision driven by eval numbers.
2. **Amendment marker variance.** `X[…]` is one of many forms across federal vs provincial acts and across decades. `amendment_parser` regexes will widen iteratively during Slice A; built into the slice work.
3. **Procedure extraction depends on cross-ref graph quality.** If Slice C edge precision is below ~80%, Slice D's cross-instrument stitching breaks. Mitigation: Slice D's eval falls back to single-instrument procedures and flags cross-instrument as a known v1 limitation.

## 8. Follow-ups (v2)

1. Bilingual (Urdu) ingestion + retrieval.
2. LAN client–server deployment (still offline).
3. Saved queries / notebooks per matter, plus audit log of every officer answer.
4. Normalized actor vocabulary with spelling tolerance for `procedure_steps.actor`.
5. Point-in-time queries ("as of date X").
