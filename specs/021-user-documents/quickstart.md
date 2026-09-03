# Quickstart — verifying 021 User Documents

Manual walkthrough on the deployed web app (two ordinary users A and B + admin).
Mirrors the spec's SC-001…SC-006.

## 1. Privacy (US1 / SC-001) — run FIRST, it's the foundation

1. As **A**: My documents → upload `private-case.pdf` (unique content, e.g. a made-up
   office memo number). Wait for "indexed".
2. As **A**: ask chat about the memo number → answer WITH citation to the file.
3. As **B**: ask the same question → "not in corpus"-style answer, ZERO citations to A's
   file. Repeat with ~5 phrasings.
4. As **admin**: ask the same question → same exclusion as B (admin answers = shared + own).
5. Regression: ask 3 standing corpus questions (e.g. SPPRA Rule 16 direct contracting) as
   A, B, admin → identical quality/citations to pre-feature, latency within ~10%.

## 2. My documents (US2 / SC-003)

1. Fresh ordinary user: sign in → My documents → upload a ≤20-page PDF → progress →
   indexed → cited answer in chat. Stopwatch: under 5 minutes total.
2. Upload an unsupported file (`.exe`) in a batch with a good PDF → clear rejection message,
   PDF still indexes.
3. Delete the document → gone from list; earlier question now returns "not in corpus".
4. Quota: set `NOVIQ_USER_QUOTA_FILES=2` in `.env`, restart, upload 3 files → third refused
   with usage shown. Revert.
5. Admin → usage view shows per-user bytes/files; admin deletes one of A's docs → audit
   trail has `admin_delete_document` with admin as actor.

## 3. Sharing (US3 / SC-004)

1. As **A**: share `private-case.pdf` → **B**'s document tree shows it (Shared badge,
   owner A) within 10 s; B's chat now cites it.
2. As **A**: unshare → B loses listing + retrieval on the next query.
3. As **B**: attempt visibility change on A's doc (API call) → 403.
4. As **admin**: unshare a corpus doc → disappears from A/B listing + retrieval; re-share →
   returns. (Proves no re-index needed either direction.)
5. Check `user_events` rows for every flip.

## 4. Attachments (US4 / SC-005)

1. As **A** in conversation C1: attach `notification.pdf` → chip shows progress → ready
   ≤ 2 min (≤20 pages).
2. Ask about clause content → answer cites the attachment AND can mix corpus citations.
3. Open new conversation C2, ask the same → attachment contributes nothing.
4. As **B**: ask the same → nothing.
5. Promote the attachment → appears in A's My documents (private); C1 still answers
   without double citations.
6. Attach a corrupt PDF → failed chip with reason + retry/remove work.
7. Delete conversation C1 → attachment sources/chunks/file gone
   (verify: `SELECT count(*) FROM sources WHERE conversation_id='C1'` → 0).

## 5. DB spot checks (psql on the VM)

```sql
-- backfill sanity: global corpus stayed shared, nothing lost
SELECT visibility, count(*) FROM sources GROUP BY visibility;
-- invariant I1
SELECT count(*) FROM sources WHERE conversation_id IS NOT NULL
  AND (owner_id IS NULL OR visibility <> 'private');   -- must be 0
-- audit coverage (SC-006)
SELECT event_type, count(*) FROM user_events
 WHERE event_type IN ('share','unshare','attach','promote','delete_document','delete_conversation')
 GROUP BY event_type;
```

DB access runbook: memory / MEMORY.md (ssh <user>@<PRIVATE_IP>, psql as `postgres`, db `echo`).
