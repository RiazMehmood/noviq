# Contract: Tauri commands (frontend ↔ backend)

All commands return `Result<T, String>` where `String` is a human-readable error. Frontend is SolidJS.

## 1. Import progress + lifecycle

### Event: `importer://progress`

Emitted by `MistralIrBuilder` during `build()`, by `indexer` during persist, and by the partial-re-index path after repair.

```ts
type ImporterProgress = {
  source_id: string;
  phase: "classifying" | "chunking" | "converting" | "validating" | "indexing" | "repairing";
  current: number;           // section index, 1-based
  total: number;             // total sections in this doc
  retries: number;           // retries on the current section
  eta_seconds: number | null;
  message: string | null;    // human-readable status line
};
```

### Event: `importer://done`

Emitted once per document on terminal completion (success, partial-success, or failure).

```ts
type ImporterDone = {
  source_id: string;
  outcome: "ok" | "partial" | "failed" | "cancelled";
  nodes_total: number;
  nodes_ok: number;
  nodes_low_confidence: number;
  extraction_method: "llm_ir" | "rules_legacy" | "constitution_ir" | "unprocessable";
  quality_score: number;     // 0..1
};
```

### Event: `importer://llm-unavailable`

Emitted once per app session, when an import would have used the LLM path but the GGUF is missing.

```ts
type LlmUnavailable = { reason: "model_missing" | "model_load_failed" | "hardware_tier"; remediation: string };
```

### Event: `importer://queued`

Emitted when a new import lands in the queue, and re-emitted for every still-waiting entry whenever the queue advances (the head finishes / cancels). Lets the UI render "Queued — N ahead" without polling.

```ts
type ImporterQueued = {
  source_id: string;
  position: number;          // 0 = currently running; 1+ = waiting (1 = next up)
  queue_length: number;      // total entries in the queue including the running one
};
```

### Command: `cancel_import`

```ts
invoke<void>("cancel_import", { source_id: string });
```

Sets the document's `CancellationToken`; in-flight LLM build returns `IrBuildError::Cancelled`; partial state is rolled back.

---

## 2. Repair view

### Command: `get_doc_ir`

```ts
invoke<DocIrPayload>("get_doc_ir", { source_id: string });

type DocIrPayload = {
  doc_ir: DocIr;             // full IR
  pdf_path: string;
  page_count: number;
};
```

### Command: `get_low_confidence_nodes`

```ts
invoke<NodeRef[]>("get_low_confidence_nodes", { source_id: string });

type NodeRef = { node_id: string; page: number; number: string | null; title: string | null; quality_error: string | null };
```

### Command: `render_pdf_page`

```ts
invoke<string>("render_pdf_page", { source_id: string, page: number });
// returns absolute path to PNG in {app_data_dir}/page_cache/<sha>/p<N>.png
```

Idempotent: returns cached PNG if present, else rasterizes via pdftoppm at 150 DPI.

### Command: `save_repair`

```ts
invoke<SaveRepairResult>("save_repair", {
  source_id: string,
  edits: NodeEdit[],
});

type NodeEdit =
  | { op: "update", node_id: string, patch: Partial<StructureNode> }
  | { op: "insert_sibling", after_node_id: string, node: StructureNode }
  | { op: "delete", node_id: string };

type SaveRepairResult = {
  applied: number;            // count of edits persisted
  rejected: { node_id: string, reason: string }[];
  reindexed_chunk_ids: string[];
};
```

Validates each edit against the same schema+semantic rules. Rejected edits are returned with `reason`; applied edits are persisted, the cache JSON is rewritten, and changed nodes' chunks + graph edges are re-embedded/re-built (R8 in research.md).

### Command: `list_docs_needing_review`

```ts
invoke<DocReviewItem[]>("list_docs_needing_review");

type DocReviewItem = {
  source_id: string;
  title: string;
  nodes_low_confidence: number;
  quality_score: number;
};
```

Powers the doc-card badges and the notification toast count.

---

## 3. Cache maintenance (devtools-only; not user-facing in v1)

### Command: `ir_cache_stats`

```ts
invoke<{ total_records: number, orphaned: number, total_bytes: number }>("ir_cache_stats");
```

### Command: `force_orphan_sweep`

```ts
invoke<{ deleted: number }>("force_orphan_sweep");
```

Runs the sweep immediately, ignoring the daily timer.
