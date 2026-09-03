# Phase 1 — Tauri Command Contracts

**Feature:** 007 Pakistan Government Legal RAG
**Date:** 2026-06-02

This document is the canonical reference for the Tauri commands and
event streams Feature 007 adds to the existing app. Existing commands
(upload, chunk, retrieve, etc.) are not redefined here.

---

## C1. `deep_reindex_document`

Triggers the Mistral legal-IR pipeline on a single document.

**Request:**

```ts
deep_reindex_document(doc_id: string): Promise<DeepReindexHandle>;

interface DeepReindexHandle {
  doc_id: string;
  job_id: string;          // opaque; passed to cancel_deep_reindex
  started_at: string;      // ISO timestamp
}
```

**Behaviour:**

1. Look up the document. If `index_tier === 'deep'` already, return success
   immediately with a no-op flag (sticky tier — sha256 cache may still
   refresh embeddings; pipeline decides).
2. Classify the document; if it does not parse as a Pakistan legal
   instrument, return a recoverable error `NotALegalInstrument` so the UI
   can ask the officer to confirm before re-running with
   `force = true`.
3. Register a cancellation token in the existing registry.
4. Run pipeline (classify → IR build → amendment_parser → xref_extractor
   → procedure_extractor → chunks replace → re-embed).
5. Persist `documents.index_tier = 'deep'` only after all passes succeed
   and chunks have been atomically replaced.

**Errors:**

| Code | When | UI handling |
|---|---|---|
| `NotALegalInstrument` | classifier confidence below threshold | confirm prompt; officer can re-call with `force` |
| `RamTierInsufficient` | running tier < 8 GB | refuse with explanation; document stays fast |
| `ModelMissing` | required GGUF not on disk | clear "download required" message |
| `Cancelled` | officer cancelled mid-run | document stays at prior tier; no state corruption |

**Events emitted (`importer://progress` channel, existing):**

```ts
type ProgressEvent =
  | { kind: 'phase_start';   job_id: string; phase: Phase; total?: number }
  | { kind: 'phase_tick';    job_id: string; phase: Phase; current: number; total: number }
  | { kind: 'phase_done';    job_id: string; phase: Phase }
  | { kind: 'completed';     job_id: string; doc_id: string }
  | { kind: 'cancelled';     job_id: string }
  | { kind: 'error';         job_id: string; code: string; message: string };

type Phase =
  | 'classify' | 'ir_build' | 'amendment_parse'
  | 'xref_extract' | 'procedure_extract' | 'chunks_replace' | 'reembed';
```

`phase_tick` is emitted at minimum every 10 seconds during long phases (SC-003).

---

## C2. `cancel_deep_reindex`

```ts
cancel_deep_reindex(job_id: string): Promise<void>;
```

Sets the cancellation token associated with `job_id`. The pipeline
checks the token between chunks / between passes; cancellation MUST
take effect within 5 seconds (SC-003). On cancellation, the document's
prior tier is preserved (no partial deep state visible).

---

## C3. `downgrade_document_to_fast`

```ts
downgrade_document_to_fast(doc_id: string): Promise<void>;
```

Explicit officer action only. Deletes all 007 deep-only artifacts
scoped to the document (amendment_events, graph_edges where the doc is
src or dst, procedures, procedure_steps) in a single transaction, sets
`documents.index_tier = 'fast'`, and re-runs the fast pipeline.

---

## C4. `chat_query` (extends existing)

The existing chat-query command is extended with an explicit scope and
cloud flag.

```ts
chat_query(input: ChatQueryInput): Promise<ChatResponse>;

interface ChatQueryInput {
  text: string;
  scope: Scope;
  use_cloud?: boolean;        // resolved from persistent Settings toggle (default off). Backend MUST honour false silently and MUST route to cloud only when true. Never set true by anything other than the Settings toggle.
  thread_id?: string;
}

type Scope =
  | { kind: 'both' }
  | { kind: 'folder' }
  | { kind: 'folder_doc'; doc_id: string }
  | { kind: 'notebook' }
  | { kind: 'notebook_doc'; doc_id: string };

interface ChatResponse {
  intent: Intent;
  intent_confidence: number;
  bubbles: ChatBubble[];      // usually one; Difference / CrossCompare may return multiple
  upgrade_prompt?: UpgradePrompt;  // present when deep tier would help
}

type Intent =
  | 'lookup' | 'list_children' | 'summarize'
  | 'difference' | 'cross_compare' | 'action_items'
  | 'timeline' | 'procedure' | 'fallback';

interface ChatBubble {
  headline: string;
  body: string;                       // markdown
  citations: Citation[];
  see_also?: Citation[];
  flow?: FlowDiagram;                 // present when intent === 'procedure'
}

interface Citation {
  officer_format: string;             // one canonical string from services/citation
  short_format: string;
  doc_id: string;
  node_id?: string;                   // present on deep tier
  page?: number;                      // present on fast tier
}

interface UpgradePrompt {
  doc_ids: string[];                  // docs that would benefit from Deep re-index
  reason: string;                     // human-readable, e.g. "amendment history needs deep tier"
}

interface FlowDiagram {
  process_name: string;
  steps: FlowStep[];
}

interface FlowStep {
  order: number;
  actor: string;
  action: string;
  condition?: string;
  timeline?: string;
  citation: Citation;
  branches: FlowStep[];               // empty by default; populated when officer expands
}
```

**Latency contracts (SC-011):**
- Fast-tier intents (`lookup`, `list_children`): median ≤ 3 s on offline-mid.
- All other intents: median ≤ 5 s, P95 ≤ 15 s on offline-mid.

---

## C5. `set_scope` (already exists — unchanged contract)

Scope picker state is owned by the existing scope service. `chat_query`
honours the active scope per FR-025 / SC-009.

---

## Constitution alignment

| Command | Pending Actions Queue tier (§V) | Notes |
|---|---|---|
| `deep_reindex_document` | CAUTIOUS | Side effects scoped to one doc; officer-initiated; cancellable; atomic. |
| `cancel_deep_reindex` | READ-ONLY | Sets a flag. |
| `downgrade_document_to_fast` | DANGEROUS | Deletes deep-only artifacts. UI MUST show a "remove all deep data for this document?" confirmation modal. |
| `chat_query` | READ-ONLY | No side effects; even cloud calls require explicit `use_cloud: true` per §I. |
