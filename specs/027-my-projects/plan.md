# Implementation Plan: My Projects

**Branch**: `027-my-projects` | **Date**: 2026-08-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/027-my-projects/spec.md`

## Summary

User-owned document workspaces: a named container holding its own documents and many
chats, where retrieval is scoped to that project's documents **and nothing else** —
optionally widened to the shared legal corpus for a single question. Sharing is
**copy-on-share** for projects and **link-to-view, fork-to-continue** for chats. An
administrator can turn the whole feature off deployment-wide, enforced at the server.

The technical core is one function. `visible_source_ids`
(`src-tauri/src/db/repositories/sources.rs:97`) is the single point through which chat
retrieval, global search, citation opening, file serving and the knowledge graph all
decide what a user may see, and it is **purely additive** — its three branches are OR'd,
so each only ever adds. Project scoping is the first requirement in this system's history
that must **subtract**, so it gains an exclusive mode rather than a fourth branch.
Because all six call sites funnel through that one function, isolation is inherited by
every route for free; implemented anywhere else it would leak through global search.

Two structural facts were verified in code and contradict the design notes: `notebooks` /
`notebook_sources` are live (not dead F001 leftovers) and must not be repurposed, and
**there is no conversation table at all** — `conversation_id` is a client-minted string
with no owning row. See [research.md](./research.md).

## Technical Context

**Language/Version**: Rust 1.87 (`src-tauri`, `noviq-server` bin) + TypeScript/SolidJS
**Primary Dependencies**: axum, sqlx 0.8 (postgres feature, runtime queries), serde,
tokio, uuid. **No new crates.**
**Storage**: Bundled PostgreSQL 17 + pgvector at `{app_data_dir}/pg/data`; additive
migration `023_my_projects.sql`
**Testing**: `cargo test --no-default-features --features server --lib` with
`NOVIQ_TEST_PG_INSTALL_DIR` set; baseline 925 passed / 2 failed
**Target Platform**: Linux server (VM, A40-12Q), browser client over Cloudflare tunnel
**Project Type**: Web — Rust server + SolidJS frontend
**Performance Goals**: project copy < 5 s (SC-003); project question within existing p95
budgets — retrieval gains one `IN (…)` predicate, no new model call
**Constraints**: Additive migrations only; no new model slot; isolation must hold on
every route, not most
**Scale/Scope**: ~10 concurrent users; projects up to 50 documents (SC-010)

## Constitution Check

*GATE: checked before Phase 0, re-checked after Phase 1 design. Constitution v1.1.0.*

| # | Principle | Verdict |
|---|---|---|
| I | Privacy-First, Offline-Always | ⚠️ **Pre-existing divergence, inherited not deepened.** ADR-0025 already accepted that private documents may reach third-party APIs. This feature adds no new outbound path; FR-041 *discloses* the existing behaviour rather than relying on it being untrue. See Complexity Tracking. |
| II | Tiered Intelligence | ✅ No new LLM call. Project scoping narrows the candidate set before Tier 2, so a project question is strictly cheaper than the same question today. |
| III | Single Model Slot | ✅ Untouched. No model is loaded, unloaded or added. |
| IV | SQLite as single source of truth | ⚠️ **Pre-existing divergence.** F011 ported to bundled PostgreSQL + pgvector; CLAUDE.md records the amendment as pending. This feature adds tables to that existing database and creates no new store. See Complexity Tracking. |
| V | Pending Actions Queue | ✅ Not applicable — no tool calls, no shell, no code execution. |
| VI | Persona-as-Config | ✅ No agent persona or tool schema changes. |
| VII | Hardware-Adaptive | ✅ No tier-dependent behaviour; the admin gate degrades the feature visibly and with a message, never silently. |
| VIII | Incremental Feature-App Delivery | ✅ Seven prioritised stories; P1 (Stories 1+2) alone is a usable MVP. Stories 3–7 are independently shippable. |
| IX | Pakistan Government Legal Domain | ✅ Scope unchanged. Projects hold officers' working documents — tender files, inquiries, drafts — which are government working papers, not case law, not a new jurisdiction, not private practice. The shared corpus stays administrator-managed and cannot be added to through a project. |
| X | Officer-First UX | ✅ Directly serves the officer: today a question about "clause 4" pulls Rule 4 from five unrelated instruments. A closed document set is the officer default. The corpus tick is per-question and defaults off, so the officer flow is never widened by accident. |
| XI | Citation Discipline | ✅ Reinforced. FR-016 requires every citation in a mixed answer to declare whether it is the user's own document or enacted law — a *strengthening* of the canonical format, since mistaking a draft for law is the failure this principle exists to prevent. The existing citation format is otherwise unchanged. |
| XII | Latest Text + Amendment Chain | ✅ Corpus documents keep their amendment chain when included via the tick. No point-in-time reconstruction is introduced. |
| XIII | Cross-Reference Graph First-Class | ✅ `get_knowledge_graph` is scoped through the same function as everything else, so a project's subgraph is the project's documents. The graph is not re-derived at query time. |

**Development Constraints**: ✅ Migration 023 is additive (no drops). ✅ Smallest viable
diff — `visible_source_ids` gains one branch; the `unrestricted()` early-return and the
three-branch additive query are untouched. ✅ No secrets. ✅ Every refusal
(not-found, gate-off, allowance-exhausted) carries a human-readable message.

**Gate result**: **PASS.** The two ⚠️ rows are pre-existing divergences already recorded
against F011 and ADR-0025. This feature neither causes nor deepens them, and blocking on
them would block all work on this codebase until the constitution is amended.

**Post-Phase-1 re-check**: unchanged. The design added no crate, no store, no model call
and no outbound path. The one item that moved is Principle XI, which the design
strengthened rather than merely satisfied.

## Project Structure

### Documentation (this feature)

```text
specs/027-my-projects/
├── plan.md              # This file
├── spec.md              # Feature specification
├── research.md          # Phase 0 — R1..R9, all unknowns resolved
├── data-model.md        # Phase 1 — migration 023, RequestScope, content lifetime
├── quickstart.md        # Phase 1 — build, enable, and the tests that matter
├── contracts/
│   └── api.md           # Phase 1 — routes, request/response shapes
├── checklists/
│   └── requirements.md  # Spec quality checklist (passing)
└── tasks.md             # Phase 2 — NOT created by /sp.plan
```

### Source Code (repository root)

```text
src-tauri/src/
├── db/
│   ├── pg/migrations/
│   │   └── 023_my_projects.sql          # NEW — project, project_source,
│   │                                    #       project_chat, chat_share
│   └── repositories/
│       ├── sources.rs                   # CHANGED — RequestScope gains project_id +
│       │                                #   include_corpus; visible_source_ids gains
│       │                                #   exclusive mode; is_referenced() helper
│       ├── projects.rs                  # NEW — CRUD, membership, copy, owned_project
│       └── chat_shares.rs               # NEW — mint, resolve, revoke
├── api/
│   ├── projects.rs                      # NEW — all routes in contracts/api.md
│   ├── router.rs                        # CHANGED — register routes
│   ├── query.rs                         # CHANGED — derive project from conversation;
│   │                                    #   include_corpus per-turn flag
│   ├── documents.rs                     # CHANGED — optional project_id on 4 routes;
│   │                                    #   delete paths consult is_referenced()
│   ├── uploads.rs                       # CHANGED — optional project_id form field
│   └── system.rs                        # CHANGED — get/set_my_projects_enabled
└── services/
    └── projects/mod.rs                  # NEW — pure gate fn, copy orchestration

src/                                      # SolidJS frontend
├── pages/
│   ├── ProjectsPage.tsx                 # NEW — list, create, open
│   ├── ProjectPage.tsx                  # NEW — documents + chats + share
│   ├── SharedChatPage.tsx               # NEW — read-only transcript + fork
│   ├── ChatPage.tsx                     # CHANGED — corpus tick; project context
│   └── Admin.tsx                        # CHANGED — the on/off switch
├── components/
│   └── CorpusToggle.tsx                 # NEW — per-question, resets every turn
└── stores/
    └── projects.ts                      # NEW
```

**Structure Decision**: Web application — Rust server under `src-tauri/src/` (the
`noviq-server` binary; the Tauri desktop shell is vestigial per
[research.md](./research.md) R9) and a SolidJS client under `src/`. The layout above
follows the existing `api/` + `db/repositories/` + `services/` split exactly; no new
architectural layer is introduced.

## Implementation Phasing

Ordered so that the isolation invariant is provable before anything depends on it, and so
the most destructive change lands with its test rather than after it.

| Order | Work | Stories | Why here |
|---|---|---|---|
| 1 | Migration 023 + `RequestScope` exclusive mode + `owned_project` | — | Everything rests on this. Land it with the negative test and nothing else. |
| 2 | Project CRUD, membership, chat-turn derivation | US1, US2 | The MVP. Shippable alone. |
| 3 | `is_referenced()` + all delete paths | US4 | ⚠️ Must land **before** sharing. After copy-on-share, a wrong delete destroys another user's data. |
| 4 | Copy-on-share for projects | US5 | Depends on 3. |
| 5 | Per-question corpus tick + citation labelling | US3 | Independent of 4; can swap order. |
| 6 | Chat share links + fork | US6 | Depends on 4 for the fork. |
| 7 | Admin gate | US7 | Last to build, but the gate check is written into every route as it is added — not retrofitted. |
| 8 | Frontend | all | Follows each slice. |

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| **Principle I** — documents may reach third-party APIs | Pre-existing: ADR-0025 accepted this for the whole product. This feature adds no new outbound path, and FR-041 requires disclosing it where a user adds project documents. | Restricting project answering to local inference (spec Question 1 option B) was offered and rejected by the architect: it splits answer quality between projects and ordinary chat, and that difference would itself need explaining. |
| **Principle IV** — PostgreSQL, not SQLite | Pre-existing: F011 ported the whole product; the constitution amendment is recorded as pending in CLAUDE.md. | Adding these four tables to a *separate* SQLite file would create a second source of truth and break the cross-table delete-safety check that FR-024 depends on — a far worse violation of the same principle's intent. |
| **Cross-user `source_link` semantics** — migration 022 deferred exactly this | 022 states cross-user linking "needs a spec + ADR of its own"; this is that spec, and an ADR is proposed below. The hazard 022 named was *implicit* dedup leaking a file's prior existence; copy-on-share is explicit, so nothing is disclosed that the sharer did not choose. | Re-uploading and re-indexing per recipient was rejected: it makes SC-003 (<5 s) impossible, multiplies storage by the number of recipients, and would still not give the independence FR-021 requires. |

## Risks carried into Phase 2

1. **Four routes, one invariant.** Chat derives its project server-side and cannot be
   widened. The other four (`global_search`, `source_chunks`, `source_file`,
   `get_knowledge_graph`) take `project_id` from the client and must each call
   `owned_project` first. **A test per route, not one shared test** — the forgotten route
   fails open.
2. **Delete safety is now cross-user.** Before 023 every reference to a document belonged
   to one user. After it, a delete can destroy another user's data. This is why phase 3
   precedes phase 4.
3. **A shared document cannot be recalled** (FR-024) — correct for a copy model, opposite
   of user expectation. The disclosure at share time is a requirement, not UI polish.
4. **Migration 023 is a one-way deploy.** `ignore_missing = false`: once applied, a
   pre-023 binary fails at startup with `VersionMissing`.
5. **The two pre-existing test failures** include
   `sources::scope_tests::conversation_scope_includes_own_attachment` — in the exact
   module this feature changes. Re-read it before attributing a scope-test failure to
   this work.

## Next

`/sp.tasks` to generate the dependency-ordered task list.

## Architecture Decision Records

Both written 2026-08-05, **Proposed**, before implementation:

- **[ADR-0028 — Copy-on-Share and Cross-User Content Linking](../../history/adr/0028-copy-on-share-and-cross-user-content-linking.md)**
  Sharing copies rather than granting access; cross-user `project_source` links;
  content lifetime as a union across three tables; a shared document cannot be recalled.
  **Supersedes the per-user restriction recorded in `022_source_links.sql:20-24`**, which
  named this ADR as owed.
- **[ADR-0029 — Project Retrieval Isolation via a Subtractive Scoping Seam](../../history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md)**
  Exclusive mode in `visible_source_ids` rather than a fourth additive branch; the server
  derives the project on a chat turn so the client cannot name it; `owned_project` on the
  four routes that must take it from the client; share links authorised per cited chunk.

Numbered 0028/0029 rather than 0023/0024: `create-adr.sh` allocates from files present in
the working tree, and ADRs 0023–0027 exist on other branches. The script's suggestion
would have collided.
