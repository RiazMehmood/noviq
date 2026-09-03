# Contract: Conversational drafting session (FR-027–FR-034)

Tauri commands added in `commands/drafting.rs`. The conversation is an interaction layer over the existing deterministic drafting commands (`start_draft`, `update_doc_set`, `resolve_gap`, `refresh_doc_set`, `render_draft`, `revise_draft`, `finalize_draft`) — it introduces no new generative authority over citations/numbers/facts (FR-028).

## `send_chat_turn`

Officer sends one conversational turn; the system parses it into a `DraftAction`, dispatches to the existing pipeline, persists both the officer turn and the system reply, and (on threshold) compacts.

**Request**
```json
{ "draftId": "string", "text": "string" }
```

**Behaviour**
1. Append the officer turn to `chat_message` (role=`officer`, next `seq`). (FR-031)
2. Parse `text` → `DraftAction` via `chat.rs` (deterministic/regex first):
   - `SetIntent{field,value}` → updates `DraftState.intent`; when intent complete → `start_draft` proposes the doc set.
   - `ConfirmDoc`/`AddDoc`/`RemoveDoc` → `update_doc_set`.
   - `ResolveGap`/`DeclareUnavailable` → `resolve_gap`.
   - `Revise{...}` → `revise_draft` (targeted re-render, FR-024/025).
   - `Finalize` → `finalize_draft` (FR-026).
   - `Unparseable`/`Ambiguous` → no pipeline mutation; system asks a targeted clarifying question (FR-030).
3. Build the system reply text (question, confirmation, or rendered-state summary).
4. Append the system turn to `chat_message` (role=`system`). (FR-031)
5. If live-window tokens > threshold → compact oldest un-summarized turns into `conversation_summary` via one model-slot call (FR-033/034).

**Response**
```json
{
  "reply": "string",                 // system turn shown in the transcript
  "action": "set_intent|confirm_doc|add_doc|remove_doc|resolve_gap|revise|finalize|clarify",
  "state": { "step": "intent|docset|draft", "intent": {}, "confirmedDocs": [], "gaps": [], "blocks": [], "finalized": false }
}
```
`state` mirrors `DraftState` so the live draft/doc panels (FR-029) re-render from one source of truth. Citation-grade invariants hold on every returned `blocks` set (FR-025).

**Errors**: each returns a human-readable message + remediation (Dev Constraints). Examples: no active session for `draftId`; pipeline command failure (surfaced verbatim, draft left unchanged).

## `load_conversation`

Restore the full transcript + current state on resume (FR-032).

**Request**
```json
{ "draftId": "string" }
```

**Response**
```json
{
  "messages": [ { "seq": 0, "role": "officer|system", "content": "string", "createdAt": "ISO-8601" } ],
  "summary": { "text": "string", "coversUptoSeq": 0 },
  "state": { "step": "...", "intent": {}, "confirmedDocs": [], "gaps": [], "blocks": [], "finalized": false }
}
```
Returns all `chat_message` rows (raw turns retained regardless of `summarized`) so the officer sees the complete history, while the model only ever receives `summary + recent turns`.

## Invariants (asserted in tests)

- **I1**: `chat_message` rows are never deleted; compaction only flips `summarized` 0→1. (FR-031)
- **I2**: the model-facing context = `conversation_summary.summary_text` + rows with `seq > covers_upto_seq`; the full transcript is never sent to the model. (FR-033)
- **I3**: no `DraftAction` path lets the conversation emit or mutate a citation, number, or fact — all citations resolve deterministically from chunk metadata. (FR-028, FR-004–FR-008)
- **I4**: compaction preserves decision-relevant state because that state lives in `DraftState` (migration 022), not only in conversational prose. (FR-034)
- **I5**: summarization uses the single resident model slot; no second model is loaded. (Principle III)
