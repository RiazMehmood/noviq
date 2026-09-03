# API Contract — Answer Feedback

Two endpoints, mounted in `api/router.rs` alongside the existing `/api/query` surface. Both are
behind the session auth extractor (web); desktop reaches the same handlers via the transport seam.

## POST `/api/feedback`

Submit or update citation feedback for one answered turn. Idempotent per re-tap (supersede).

**Auth**: `AuthUser` (valid session). `user_id` derived server-side from the session — never trusted
from the body.

**Request body** (`application/json`) — mirrors the Tauri invoke arg names so the shared transport
works unchanged:

```jsonc
{
  "message_id": "9c2f…",            // required — TurnResponse.message_id of the rated turn
  "conversation_id": "6ab1…",       // required — for admin JOIN to query_log
  "citation_accuracy": "all",       // optional — one of: all | some | wrong | unsure
  "citation_completeness": "yes"    // optional — one of: yes | missing | na
}
```

At least one of `citation_accuracy` / `citation_completeness` MUST be present. Unknown enum values →
`400`. The two may arrive in separate requests as the user taps each row; the second merges into the
current row (see data-model write semantics).

**Responses**:

| Status | Body | When |
|--------|------|------|
| `200 OK` | `{ "ok": true }` | Stored (insert or supersede). |
| `400 Bad Request` | `{ "error": "…" }` | Missing message_id, both values absent, or bad enum. |
| `401 Unauthorized` | — | No/expired session. |

Storage is best-effort at the DB layer but the endpoint returns `200` only after a successful write;
a DB error returns `500` (the chat turn is unaffected — feedback is a separate request).

## GET `/api/admin/feedback/summary`  (FR-009)

Aggregate distribution of both citation metrics over current feedback rows. Admin-only
(`AdminUser` extractor → `403` for non-admins), matching the existing `/api/admin/*` surface.

**Query params** (optional): `from`, `to` (epoch ms) to bound `created_at`.

**Response** (`200`):

```jsonc
{
  "accuracy":     { "all": 42, "some": 11, "wrong": 6, "unsure": 3 },
  "completeness": { "yes": 40, "missing": 15, "na": 7 },
  "total_current": 62          // distinct current feedback rows in range
}
```

## Frontend transport

`src/api/transport.ts` gains:

```ts
export interface FeedbackInput {
  messageId: string;
  conversationId: string;
  citationAccuracy?: "all" | "some" | "wrong" | "unsure";
  citationCompleteness?: "yes" | "missing" | "na";
}
export async function submitFeedback(f: FeedbackInput): Promise<void>;
// Tauri → invoke("submit_feedback", …); Web → POST /api/feedback.
```

`ChatPage.tsx` renders `<CitationFeedback messageId={resp().message_id}
conversationId={resp().conversation_id} />` under the citations block; the component tracks local
selection state and calls `submitFeedback` on each tap (single-select per row; re-tap allowed).

## Route registration (router.rs)

```rust
// authenticated user routes
.route("/feedback", post(feedback::submit))
// admin routes
.route("/admin/feedback/summary", get(admin::feedback_summary))
```

Desktop command alias (Tauri): `submit_feedback` → same handler via the compat layer, matching how
`send_message` aliases `/api/query`.
