# Contract — Notice Assist command & turn flow

NoticeAssist has **no new HTTP endpoint**. It flows through the existing `send_message` /
`POST /api/query` turn, intercepted inside `run_turn` (like transform/additive). The `/` command
is a frontend affordance that produces a normal turn whose text begins with `/help`.

## Frontend → backend

`sendMessage(conversationId, text, sourceIds?)` — unchanged signature (shipped 2026-07-10).
- Entering assist mode: `text` starts with a command token: `/help` | `/reply` | `/audit`
  (optionally followed by the officer's words). The attached notice rides via the existing
  conversation attachment (F021) — `sourceIds` may scope to it, but grounding still uses the
  corpus (R7).
- Continuing the loop: ordinary `sendMessage` turns while a `notice_assist_session` exists for
  the conversation are treated as answers to the current open question.

## Backend routing (in `run_turn`, before capability dispatch)

```
let assist_open = notice_assist::session::load(pool, conversation_id)?.is_some();
if intent::is_notice_assist_command(text) || assist_open {
    return notice_assist::run(pool, inference, embedder, retriever, app,
                              conversation_id, text, &scope_ids).await;
}
```

Precedence: NoticeAssist intercept sits alongside transform/additive (before the capability
`match`). A `/help` token always (re)enters; an open session captures subsequent plain turns.
Exit: session reaches `Done` and the officer sends a non-`/` message not flagged as a revision →
clear/keep per UX (default: keep until `/help` restart or explicit "done").

## `notice_assist::run` phases (server)

1. **Analyze** (first turn or new attachment): call the **local Qwen3-8B** generator (R1) with the
   analysis prompt over the attachment text (R3); tolerant-parse (R2) → `Frame`; persist session.
2. **Gather**: emit the next `Open` `SlotQuestion` (1–2 at a time) as the assistant answer;
   subsequent turns record answers (`Filled`) or gaps ("N/A" → `Gap`).
3. **Ground**: for each `invoked_rule`, retrieve verbatim from the corpus (citation gate); fill
   `CitedRule`.
4. **Draft**: compose the reply via the type template (see extraction-json + templates) and cache
   in `state.drafted_reply`.

Each phase returns the standard `TurnResponse` (answer + citations + flags), so the existing SSE
token/phase streaming, `poll_turn` recovery, and `export_draft` all work unchanged.

## Turn response semantics

- **Gather turns**: `answer` = the question(s); `citations` empty; a `flags` marker
  `notice_assist:gathering` (frontend can render a progress affordance).
- **Draft turn**: `answer` = the full reply; `citations` = the cited rules; `flags` include ⚠
  gap items; exportable via `export_draft`.

## New backend surface (no new command)

- `intent::is_notice_assist_command(text) -> bool` — leading `/help|/reply|/audit` (case-insensitive,
  tolerant of trailing text).
- `services/assistant/capabilities/notice_assist::run(...) -> Result<TurnResponse, String>`.
- `notice_assist::session::{save,load,delete}`.

## Frontend surface

- `ConfidentialComposer`: `/` at input start opens a static command menu (reuse `@` machinery);
  picking a command sets composer text to the token. No new props beyond an optional
  `commands` list; emits nothing to corpus scope.
- `ChatPage`: optional assist-mode banner keyed off the `notice_assist:gathering` flag; questions
  and the final reply render as normal assistant messages (no new message type).
