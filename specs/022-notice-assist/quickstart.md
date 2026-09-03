# Quickstart — Notice Assist (/help)

## What it does
Attach a show-cause notice / warning / audit para, type `/help`, and Noviq reads the document
(local Qwen3-8B), asks you only the facts it needs, retrieves the governing rule from the corpus
(verbatim + cited), and drafts a grounded reply + a documents-to-attach checklist — flagging any
weak point honestly.

## Try it (web)
1. Open a chat. Attach the notice (paperclip / `@` the file).
2. Type `/` → pick **/help** (or `/audit` for an audit para) → send.
3. Noviq states the document type it detected and asks its first question.
4. Answer the questions (or reply "N/A" to skip one — it's recorded as a gap).
5. Say "draft it" (or answer everything) → Noviq produces the cited reply + checklist.
6. Copy/export (plain text). Confidential details were masked before anything left the browser.

## Acceptance smoke checks
- `/` opens the command menu; `/help` enters assist mode (US1).
- Two different documents ⇒ different generated questions; no fixed field list (US2).
- An answered question is never re-asked; leaving+returning resumes the session (US3).
- The reply quotes the cited rule verbatim with a citation; a rule missing from the corpus is
  flagged, not invented; gaps show as ⚠ (US4, FR-008/011).
- Audit-para document ⇒ audit-response structure (US5).

## Backend test entry points
- `cargo test --lib -- assistant::capabilities::notice_assist` — extraction tolerant-parse over
  fixture notices (incl. malformed output → fallback), slot-loop transitions, template render,
  grounding gap-flag.
- `intent::is_notice_assist_command` unit tests (`/help` / `/reply` / `/audit`, case + trailing
  text; plain queries do NOT match).
- Requires the PG test engine (VM: `NOVIQ_TEST_PG_INSTALL_DIR`, see
  `reference_vm-pg-tests-glibc`).

## Config knobs
- `NOVIQ_NOTICE_EXTRACT_ENGINE` (default `local`) — pin extraction to the local Qwen3-8B.
- `NOVIQ_NO_THINK=1` — no Qwen thinking tokens on the extraction call.

## Not in v1
Prospective "seek orders in writing" note (prevention), case-law reasoning, auto-submission.
