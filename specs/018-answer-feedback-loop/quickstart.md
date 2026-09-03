# Quickstart — Answer Feedback Loop (US1)

End-to-end verification of the two-question citation feedback. Rust compiles on the GPU VM (no Rust
toolchain on Windows) — see the VM compile runbook in project memory.

## 1. Migration applies

`008_answer_feedback.sql` is auto-discovered by `sqlx::migrate!("./src/db/pg/migrations")`
(`db/schema.rs:53`) on server boot. Verify after deploy:

```bash
psql "$PGURL" -c "\d answer_feedback"          # table + indexes exist
psql "$PGURL" -c "SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 3;"  # 008 present
```

## 2. Backend compiles + unit tests pass (on VM)

```bash
# throwaway worktree reset to the branch, persistent CARGO_TARGET_DIR (per runbook)
cargo check -p noviq-server --features server
cargo test  -p echo-ai answer_feedback -- --nocapture   # insert / supersede / summary
```

Expected: insert creates a current row; second tap on the same message supersedes (old row gets
`superseded_at`, new row has `supersedes`); summary counts only current rows.

## 3. API smoke (curl against the running server)

```bash
TOKEN=…   # from POST /api/login
MSG=…     # message_id from a prior POST /api/query response
CONV=…    # its conversation_id

# accuracy tap
curl -s -X POST localhost:PORT/api/feedback -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d "{\"message_id\":\"$MSG\",\"conversation_id\":\"$CONV\",\"citation_accuracy\":\"all\"}"
# → {"ok":true}

# completeness tap (merges into same current row)
curl -s -X POST localhost:PORT/api/feedback -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d "{\"message_id\":\"$MSG\",\"conversation_id\":\"$CONV\",\"citation_completeness\":\"missing\"}"

# admin aggregate
curl -s localhost:PORT/api/admin/feedback/summary -H "Authorization: Bearer $ADMIN_TOKEN"
# → {"accuracy":{…},"completeness":{…},"total_current":N}
```

Verify current row:

```bash
psql "$PGURL" -c "SELECT citation_accuracy, citation_completeness, superseded_at
                  FROM answer_feedback WHERE message_id='$MSG' AND superseded_at IS NULL;"
# → all | missing | NULL   (one current row, both columns filled)
```

## 4. Browser (officer flow)

1. Log in, ask a legal question, wait for the answer + citations.
2. Under the citations, two chip rows appear:
   - **Citations correct?**  `All citations correct` · `Some citations correct` · `Citations wrong/missing` · `Not sure`
   - **Citations complete?**  `Fully cited` · `Some citations missing` · `Not applicable`
3. Tap one chip in each row → chip highlights (single-select); re-tap a different chip → selection moves.
4. Reload the conversation → prior selection re-hydrates from the current feedback row.
5. Confirm **no network egress** beyond the local server (Principle I / SC-006): the POST goes to
   `/api/feedback` on the same origin only.

## Acceptance checks (US1)

- [ ] `answer_feedback` table + indexes created by migration `008` (additive; older binary still valid).
- [ ] `POST /api/feedback` stores accuracy and completeness, independently, keyed to `message_id`.
- [ ] Re-tap supersedes (history retained; one current row per message+user).
- [ ] Bad enum / missing message_id → `400`; no session → `401`.
- [ ] `GET /api/admin/feedback/summary` returns both distributions; `403` for non-admin.
- [ ] Two chip rows render under every assistant answer; one-tap; selection persists across reload.
- [ ] Zero external network calls for feedback (offline verified).
