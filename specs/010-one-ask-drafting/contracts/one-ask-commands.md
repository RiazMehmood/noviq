# Tauri Command Contracts: One-Ask Drafting (010)

**Date**: 2026-06-11 · **Plan**: [../plan.md](../plan.md) · All payloads serde `rename_all = "camelCase"`.

Commands live in `src-tauri/src/commands/one_ask.rs`, registered in `lib.rs`. Long-running work is spawned; progress arrives via events (FR-013). Errors are `Result<_, String>` with human-readable message + remediation (constitution Development Constraints).

## Commands

### `start_one_ask`

Officer submits the one-sentence ask (FR-001). Returns immediately; generation continues in a spawned task.

```ts
startOneAsk(args: { text: string }): Promise<{ draftId: string }>
```

- Validates: non-empty text; corpus indexed (≥1 indexed source under a watched folder) else Err("No indexed documents… Index a folder first.") (edge case: empty corpus).
- Validates: hardware tier can host the 7B writer else Err with remediation (Principle VII).
- Side effects: creates `one_ask_session` row (`status=Researching`), appends officer turn + system acknowledgement to `chat_message`, registers cancel token, spawns pipeline.

### `revise_one_ask`

Free-text revision turn against a `Ready` draft (FR-009, US3).

```ts
reviseOneAsk(args: { draftId: string, instruction: string }): Promise<{ accepted: boolean, target?: string }>
```

- Deterministic turn parse → target section/paragraph ("para 3" → owning section; "add X angle" → best-matching section by hint tokens; unparseable ⇒ `{accepted:false}` + system clarify message in chat — never a model guess).
- Side effects: appends turn to chat; re-runs research(optional, only if instruction adds a topic)+compose+verify for the target section only; events as in start. Rejected if `finalized` or not `Ready`.

### `cancel_one_ask`

```ts
cancelOneAsk(args: { draftId: string }): Promise<void>
```

- Cancels the registered token; pipeline stops between stages/sections; status → `Cancelled`; progress event emitted (FR-013). Idempotent.

### `get_one_ask`

Resume/load (FR-010).

```ts
getOneAsk(args: { draftId: string }): Promise<OneAskView>
loadOneAskSessions(): Promise<{ draftId: string, status: string, updatedAt: string, firstAsk: string }[]>
```

- `OneAskView = { draftId, status, intent, paragraphs, cards, finalized, messages }` (messages from `chat_message`).
- A session interrupted mid-generation loads as `Failed{interrupted}` and is re-runnable via `retry_one_ask` below.

### `retry_one_ask`

```ts
retryOneAsk(args: { draftId: string }): Promise<void>
```

- Re-runs the pipeline from `Researching` reusing the stored intent (edge case: restart mid-generation; never presents a half-draft as finished).

### `finalize_one_ask`

```ts
finalizeOneAsk(args: { draftId: string }): Promise<void>
```

- Sets `finalized=true`; subsequent `revise_one_ask` rejected with remediation ("Start a new draft…"). Copy is frontend-side (clipboard) from the view model (FR-011) and needs no command.

## Events

### `drafting://one-ask-progress`

```ts
{ draftId: string,
  stage: 'researching' | 'composing' | 'verifying' | 'ready' | 'failed' | 'cancelled',
  section?: { index: number, total: number, title: string },   // when composing
  detail?: string,                                              // human-readable
  error?: string }                                              // when failed
```

Emitted at every stage boundary and per section — the officer never sees a silent wait > 10 s (SC-005): composing sections also emit a heartbeat detail at least every 10 s while the model generates.

### `drafting://one-ask-complete`

```ts
{ draftId: string, view: OneAskView }
```

Emitted on `Ready` (after start, retry, or revision).

## Acceptance checks (contract-level)

- [ ] `start_one_ask` returns < 1 s (work is spawned), even on the 65 GB corpus.
- [ ] Progress events cover every stage; gap between consecutive events during generation ≤ 10 s.
- [ ] `OneAskView.paragraphs[].runs[]` never contains an unflagged citation whose `verified != true` (SC-002 surface).
- [ ] Empty corpus / missing model / low RAM each return a distinct human-readable error with remediation.
- [ ] `cancel_one_ask` → status `Cancelled` and a terminal progress event within 5 s of the next stage boundary.
- [ ] Restart during composing → `getOneAsk` shows `failed` (interrupted), `retryOneAsk` succeeds.
