---
id: 154
title: Audit read-access logging and admin audit tab
stage: green
date: 2026-08-13
surface: agent
model: claude-opus-5
feature: general
branch: 027-my-projects
user: Riaz Mehmood
command: (conversational)
labels: [audit, security, admin-console, compliance, claims-verification]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/db/repositories/audit.rs
  - src-tauri/src/api/audit.rs
  - src-tauri/src/api/documents.rs
  - src-tauri/src/api/mod.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/db/pg/migrations/025_audit_indexes.sql
  - src/pages/Admin.tsx
  - CHANGELOG_DEV.md
  - docs/kimi-k3-cpu-feasibility-2026-08.html
  - history/adr/0020-graph-augmented-retrieval-on-postgres-not-neo4j.md
tests:
  - "frontend: tsc --noEmit — clean"
  - "frontend: npm run build — clean"
  - "frontend: vitest run — 26/26 passed"
  - "backend: cargo check (VM worktree, --features server) — 0 errors"
  - "backend: cargo test --lib api::audit — 7/7 passed"
  - "pre-existing, untouched: tests/ingestion_hardening_acceptance.rs:155 fails to compile (E0061)"
---

## Prompt

(1) "check these features of the app or they ok or needs some changes?" — followed
by nine customer-facing feature claims (on-premises operation; authorized cited
answers; controlled auditable updates; OCR of scanned records; masking of personal
data; role-based access and audit; departmental/project segregation; live meeting
assistance (under development); rule-based document routing (under development))
— "and close this limitations issue."

(2) "explain full audit logging point only in simple way"

(3) "so if we create full audit loggin is it possible?"

(4) "yes build all three and admin tab also for audit and logging name so we can
download the logs datewise in whole or in seperate specific parts."

## Response snapshot

Audited all nine claims against the code. Findings: masking is **frontend-only**
(`src/lib/maskEngine.ts`; no Rust `detect_pii`) so it covers composer input, not
document content; the web-search lane is user-toggleable with **no role gate**
(`api/query.rs:136`, contrast deep search at `:134`) and SearXNG proxies queries
to external engines, so "no data leaves the boundary" needs a carve-out; there is
no department entity in the schema; the meeting-assistant copy promises ASR,
speech output and Sindhi/Urdu, none of which exist.

Then built the audit work. The stated gap (reads unlogged) was real, but checking
turned up a larger one: **`audit_log` had no reader on the server at all** — the
only reader was a Tauri command, and `router.rs` had no audit route, so the trail
was reachable only via psql on the VM.

Shipped: read/denial/search logging behind the `visible_source_ids` decision
point; `AuditRecord`/`AuditFilter` + query/count/export/facets in the repository
(one shared WHERE clause, so screen and download cannot diverge); admin-gated
browse + CSV/JSON export endpoints with calendar-day ranges (`to` = midnight of
the next day, compared `<`); index-only migration 025; and an **Audit Log** tab
with filters, quick spans, paging, row detail and three download buttons.

Separately applied the drafted GPU-limitations redline to its two in-repo targets
(kimi feasibility HTML lines 194/258; ADR-0020 addendum). The three `.docx`
variants on the Desktop remain outstanding — their generator script was lost.

## Outcome

- ✅ Impact: read access, refused reads and corpus searches are now recorded, and
  the audit trail is producible on demand — date-ranged, filtered, or whole — from
  the Admin console instead of only from psql on the VM.
- 🧪 Tests: frontend green (tsc, build, 26/26 vitest); backend `cargo check` +
  `cargo test --lib api::audit` on the VM — 0 errors, **7/7 pass**. Prod verified
  untouched; worktree and 4.6 GB target dir cleaned up. NOT deployed.
- 📁 Files: 7 code files (1 new API module, 1 new migration), 3 docs.
- 🔁 Next prompts: deploy, then behaviour-verify by opening a document,
  attempting a forbidden one and running a search, and confirming all three land
  in the tab and in a date-ranged CSV. Separately: decide retention, and fix the
  pre-existing `ingestion_hardening_acceptance` compile break.
- 🧠 Reflection: the write side of a feature looking complete is not evidence the
  feature works — `audit_log` had been faithfully collecting rows for months with
  no path to read them. ⭐ **Ask "who consumes this?" as a separate question from
  "is this populated?"** And ⭐⭐ **a test run that compiles nothing exits 0** —
  the first VM run reported success on `0 tests, 997 filtered out` because a
  truncated bundle had reverted the module declaration.

## Evaluation notes (flywheel)

- Failure modes observed: a claim-by-claim code audit found two of nine claims
  materially overstated and four needing narrower wording — none of which was
  visible from the feature list alone. Verifying against `src/` rather than
  memory was what caught the frontend-only masking.
- Graders run and results (PASS/FAIL): frontend PASS; backend NOT RUN (blocked on
  toolchain/VM availability, stated rather than assumed).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): decide retention for `audit_log` —
  read events arrive far more often than the mutations it was sized for, and
  nothing prunes it today.
