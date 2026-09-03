# Phase 1 Contracts: Assistant Commands

Tauri commands in `commands/assistant.rs`, designed to be wrapped 1:1 over HTTP by the
IBA thin-client server (Spec 014). Streaming uses the existing Tauri event channel
(`assistant://progress`, `assistant://token`) mirrored as SSE on the server.

All commands share invariants: bureaucrat persona, verbatim `[CITE:n]` gate, the context
allocator (`prompt ≤ window`), atomic provisions. Every assistant message carries a
`citations[]` array; any ungrounded claim is replaced by a `flag` + `offer_to_index`.

## `send_message`
Primary turn entrypoint — intent-routed; serves all capabilities.
- **Request**: `{ conversation_id: string, text: string }`
- **Streams**: `assistant://progress { phase }` (classify | retrieve | map | reduce | generate),
  `assistant://token { delta }` (incremental answer).
- **Response**:
  ```json
  {
    "conversation_id": "…",
    "capability": "guidance|draft|summarize_doc|compare|conflict|amendment_chain|action_items",
    "answer": "…plain prose…",
    "citations": [{ "n": 1, "instrument": "…", "path": "Section 9(b)", "verbatim": "…" }],
    "offer": "tailor|null",
    "flags": [{ "missing": "…", "offer_to_index": true }],
    "context_fill": { "utilization": 0.0, "notice": false }
  }
  ```
- **Guarantees**: `answer` numbers/quotes all present in `citations[].verbatim` (FR-006); `Σ prompt ≤ n_ctx` (FR-011); whole-/cross-doc capabilities stream `map`/`reduce` phases (FR-005d).

## `confirm_documents`
Disambiguate the document scope for summarize/compare/conflict/action-items (FR-005e).
- **Request**: `{ conversation_id, candidates: string[] }` → **Response**: `{ confirmed: string[] }`

## `get_amendment_chain`
Deterministic amendment history (FR-005f). No model call for order/dates.
- **Request**: `{ provision_ref: string }`  (e.g., "Sindh Local Government Act 2013, Section 18")
- **Response**: `{ chain: [{ amending_instrument, date, action, citation }], ordered: "oldest_to_recent", narration?: string }`
- **Guarantee**: `chain` derived from `amended_by` edges + `footnote_refs`; `narration` (optional) only restates `chain`.

## `export_draft`
Plain-text export of a produced draft (FR-022).
- **Request**: `{ conversation_id, message_id }` → **Response**: `{ text: string }` (no letterhead/signature)

## `new_chat_from_notice`
Start a fresh conversation carrying pinned facts forward (FR-018b).
- **Request**: `{ from_conversation_id }`
- **Response**: `{ conversation_id, carried_facts: [{ key, value }], recap: string }`
- **Guarantee**: new conversation starts with an empty window + non-superseded `pinned_case_fact` rows + a brief recap.

## `model_status`
Surface the active model + derived window (FR-019/FR-010).
- **Response**: `{ model_file, n_ctx, trained_ctx, ram_clamped: bool, source: "auto|ECHO_N_CTX" }`

## Error / flag shape (shared)
- Ungrounded claim → not emitted; instead `flags[] = { missing, offer_to_index: true }` (FR-007).
- Over-window-by-construction is impossible (allocator asserts pre-build); if a single required provision cannot fit even alone, return `flags[] = { reason: "provision_exceeds_window" }` rather than truncating (edge case).
