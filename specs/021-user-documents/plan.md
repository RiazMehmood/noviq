# Implementation Plan: User Documents — Ownership-Scoped Retrieval, Uploads, Sharing, and Chat Attachments

**Branch**: `021-user-documents` | **Date**: 2026-07-05 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/021-user-documents/spec.md`

## Summary

Ownership exists in the catalogue but not in retrieval: `run_turn` scopes every answer to
`all_indexed_source_ids(pool)` (`src-tauri/src/services/assistant/mod.rs:341`), so any
user's "private" upload is retrievable by everyone. This feature (1) makes `sources` the
single scope truth (new `owner_id`, `visibility`, `conversation_id` columns) and filters
every answer-producing retrieval to *shared ∪ own ∪ active-conversation attachments*;
(2) surfaces a "My documents" upload UI to ordinary web users over the already-working
`/api/upload_documents`; (3) adds an explicit Private/Shared toggle (owner or admin, no
re-index); (4) adds conversation attachments as sources bound to one conversation via the
existing indexing pipeline, promotable to My documents. Desktop (single-operator, no auth)
behaviour is unchanged.

## Technical Context

**Language/Version**: Rust 1.87 (workspace `echo-ai`, `src-tauri`, `noviq-server` bin); TypeScript + SolidJS frontend, Tauri 2.0
**Primary Dependencies**: axum (existing `/api` router), sqlx 0.8 (postgres feature, runtime queries), existing indexing pipeline (`services/indexing.rs` `index_folder`/`index_sources`), fastembed 4, existing SSE progress (`/api/index/events`), existing audit trail (`db/repositories/audit.rs`)
**Storage**: Bundled PostgreSQL 17 + pgvector (`{app_data_dir}/pg/data`); additive migration `src-tauri/src/db/pg/migrations/009_sources_ownership.sql`; uploaded files under `{NOVIQ_DATA_DIR}/uploads/<user>/` (existing) + `.../uploads/<user>/__conv__<id>/` (new, attachments)
**Testing**: `cargo test --lib` on the VM (embedded-PG unit tests unrunnable on VM glibc 2.31 → SQL asserts vs live prod PG, as in F018); `tsc --noEmit` + browser walkthrough for frontend
**Target Platform**: Linux GPU VM (headless `noviq-server`, auto-deploy on push to `release`); Windows desktop (Tauri) unaffected
**Project Type**: Web application (Rust backend `src-tauri/src/{api,services,db}`, SolidJS frontend `src/`)
**Performance Goals**: SC-002 median answer latency degradation ≤ 10% (scope filter is one indexed `WHERE` clause + one per-turn id-set query); visibility flip visible ≤ 10 s (SC-004, immediate — next query re-reads scope)
**Constraints**: Additive migrations only; smallest viable diff (scope threads through the EXISTING `scope_source_ids` parameter of `get_context_chunks`/`fts5_retrieve`); single indexing pipeline (uploads queue); no llguidance
**Scale/Scope**: ~10–50 office users, corpus ~10 docs / ~8k chunks today; per-user allowance default 500 MB / 200 files

## Constitution Check

*GATE: evaluated pre-Phase-0 and re-evaluated post-Phase-1.*

| # | Principle | Status | Notes |
|---|-----------|--------|-------|
| I | Privacy-First, Offline-Always | ✅ PASS | Strengthens privacy (scoped retrieval). All processing stays on the VM/device; no new outbound calls. |
| II | Tiered Intelligence | ✅ PASS | No new LLM calls; scoping narrows candidate sets before existing tiers. |
| III | Single Model Slot | ✅ PASS | No model changes. |
| IV | SQLite as Single Source of Truth | ⚠️ DIVERGENCE (inherited) | Prod runs bundled PG 17 + pgvector since F011/F012; documented there, amendment pending. This feature only adds an additive PG migration (009) and mirrors it in the SQLite migrations dir for the desktop build. See Complexity Tracking. |
| V | Pending Actions Queue | ✅ PASS | No new DANGEROUS tool calls; deletes are user-initiated UI actions with confirmation, audited. |
| VI | Persona-as-Config | ✅ PASS | No agent/persona changes. |
| VII | Hardware-Adaptive | ✅ PASS | No tier-dependent behaviour added. |
| VIII | Incremental Delivery | ✅ PASS | US1 alone (scoping) is a shippable privacy fix; US2/US3/US4 layer on independently. |
| IX | PK Government Legal Domain | ✅ PASS | No domain change; user documents are officer case files queried alongside the corpus. |
| X | Officer-First UX | ✅ PASS | "My documents" + attach flow are officer conveniences; corpus defaults untouched. |
| XI | Citation Discipline | ✅ PASS | Citation format unchanged; attachments cite by file name + page/provision path like any fast/deep-indexed source. FR-002 additionally guarantees citations never leave scope. |
| XII | Latest Text + Amendment Chain | ✅ PASS | Untouched. |
| XIII | Cross-Reference Graph | ✅ PASS | Graph untouched; graph-derived context inherits the same source-id scope filter (verified in Phase 1 design — guidance/decompose paths take the per-turn scope set). |

**Gate result**: PASS (with the pre-existing, documented §IV divergence — no new violation introduced by this feature).

## Project Structure

### Documentation (this feature)

```text
specs/021-user-documents/
├── plan.md              # This file
├── research.md          # Phase 0 — decisions D1–D7
├── data-model.md        # Phase 1 — columns, backfill, invariants
├── quickstart.md        # Phase 1 — manual verification walkthrough
├── contracts/
│   └── api.md           # Phase 1 — endpoint contracts (new + changed)
└── tasks.md             # Phase 2 (/sp.tasks — NOT created here)
```

### Source Code (repository root)

```text
src-tauri/src/
├── db/
│   ├── pg/migrations/009_sources_ownership.sql   # NEW — additive: sources.owner_id/visibility/conversation_id, indexed_folders.visibility, backfill
│   ├── migrations/0XX_sources_ownership.sql      # NEW — SQLite mirror for desktop build
│   └── repositories/
│       ├── sources.rs                            # + owner/visibility/conversation fields, visible_source_ids(), usage_for_owner()
│       └── folders.rs                            # + visibility on DTOs, set_visibility (folder cascade)
├── services/
│   ├── assistant/mod.rs                          # run_turn: all_indexed_source_ids → per-request visible scope (RequestScope param)
│   ├── query_router.rs                           # no SQL change — scope arrives via existing scope_source_ids params
│   └── indexing.rs                               # index_sources/index_folder accept owner+visibility+conversation for new rows
└── api/
    ├── query.rs                                  # thread AuthUser + conversation_id into run_turn as RequestScope
    ├── uploads.rs                                # allowance enforcement; attach variant (conversation multipart field)
    ├── documents.rs                              # set_visibility, promote_attachment, delete_conversation, my_usage
    ├── admin.rs                                  # per-user usage aggregate endpoint
    └── router.rs                                 # new routes

src/
├── pages/SettingsPage.tsx                        # "My documents" section for ordinary web users (upload/list/delete/share/usage)
├── pages/ChatPage.tsx                            # attach button, attachment chips (progress/ready/failed/promote), delete-conversation cleanup call
├── components/DocumentTree.tsx                   # visibility badge (Private/Shared + owner)
└── api/transport.ts                              # attachDocument(), setVisibility(), myUsage(), promoteAttachment(), deleteConversation()
```

**Structure Decision**: Existing two-part layout (Rust `src-tauri`, SolidJS `src/`) — no new
crates, no new top-level directories. The retrieval seam is deliberately the *existing*
`scope_source_ids` parameter that `get_context_chunks`/`fts5_retrieve` already accept
(query_router.rs:759/786/1393/1400), so query SQL is untouched; only the set fed into it
changes per request.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| §IV SQLite → bundled PostgreSQL (inherited from F011/F012, amendment still pending) | Prod DB is PG; this feature must add its columns where retrieval actually runs | Reverting to SQLite is a separate (rejected) migration project; skipping the SQLite mirror would break the desktop build |
| Denormalized visibility on `sources` (folder flip = bulk UPDATE of child sources) | Retrieval must be one indexed WHERE clause on the table it already joins; folder-level joins in 4 retrieval paths would spread the privacy invariant across more SQL | Normalized (join `indexed_folders` at query time via `notebook_sources`) rejected: 3-table join in every retrieval path + the privacy rule living in N queries instead of one id-set function |
