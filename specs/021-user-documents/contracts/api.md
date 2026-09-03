# API Contracts — 021 User Documents

All routes under `/api`, bearer-auth via existing `require_auth` (AuthUser injected).
Command-named POST routes follow the existing `transport.call` convention.

## Changed (behaviour, not shape)

### `POST /query` (alias `/send_message`) — scoped retrieval (US1)
Request/response shape UNCHANGED. Internally builds
`RequestScope { user_id: user.id, conversation_id }` and retrieves only from
shared ∪ own ∪ this conversation's attachments. Citations can never reference an
out-of-scope source (FR-002). Out-of-scope-only answers = today's "not in corpus"
behaviour (FR-003).

### `POST /upload_documents` (multipart) — allowance + defaults (US2)
Existing route. Changes: per-user allowance enforced BEFORE saving
(`413`-style refusal: `{code:"quota_exceeded", used_bytes, used_files, limit_bytes, limit_files}`);
new sources get `owner_id=user, visibility='private'` for every role
(admin uploads through the ADMIN folder flow stay global/shared; an admin using
My-documents upload owns it privately like anyone else).

### `POST /list_indexed_folders`, `/list_folder_sources` — visibility fields (US3)
Response DTOs gain `visibility: "private"|"shared"`, `owner_username: string|null`.
Attachment sources (`conversation_id` set) are NEVER returned by any listing (FR-017).

## New

### `POST /set_visibility` (US3)
```json
{ "folderId": "…" | null, "sourceId": "…" | null, "visibility": "private" | "shared" }
```
Exactly one of folderId/sourceId. Owner or admin only (403 otherwise — FR-013).
Folder form cascades to child sources in one transaction. Audited (`share`/`unshare`).
→ `{ "ok": true, "visibility": "…" }`

### `POST /attach_document` (multipart, US4)
Fields: `conversationId` (text) + one or more `files`.
Enforces allowance; saves to `uploads/<user>/__conv__<id>/`; indexes into stub notebook
`__conv__<conversationId>` with `conversation_id` set. Progress over existing
`GET /index/events` SSE. Audited (`attach`).
→ `[{ "sourceId": "…", "fileName": "…", "status": "processing" }]`

### `POST /list_conversation_attachments` (US4)
`{ "conversationId": "…" }` → attachments for THAT conversation, requesting user only:
`[{ sourceId, fileName, status, failReason? }]` (drives chips/progress/retry UI).

### `POST /promote_attachment` (US4)
`{ "sourceId": "…" }` — owner only. Clears `conversation_id`, re-links to the user's
uploads folder notebook, moves the file; no re-index. Audited (`promote`).
→ `{ "ok": true }`

### `POST /delete_conversation` (US4)
`{ "conversationId": "…" }` — deletes all attachment sources of that conversation
(chunks/embeddings/files) via the existing FK-safe cascade. Called by the web UI when the
user deletes a conversation. Audited (`delete_conversation`). → `{ "ok": true, "removed": n }`

### `POST /my_usage` (US2)
→ `{ "usedBytes": n, "usedFiles": n, "limitBytes": n, "limitFiles": n }`

### `GET /admin/usage` (US2, AdminUser)
→ `[{ "userId": "…", "username": "…", "usedBytes": n, "usedFiles": n }]`

### `POST /delete_my_document` (US2)
`{ "sourceId": "…" }` — owner (or admin) deletes a personal document: full cascade +
file removal. Audited (`delete_document` / `admin_delete_document`).
(Reuses `delete_folder_source` internals; separate route so ordinary users never need
folder-management permissions.)

## Unchanged and explicitly out of scope

- `/add_indexed_folder`, `/add_sources` — admin/desktop corpus management, untouched.
- Entity/graph endpoints gain the same source-id filter internally; shapes unchanged.
- Desktop Tauri commands — no auth model; `RequestScope::unrestricted()`.
