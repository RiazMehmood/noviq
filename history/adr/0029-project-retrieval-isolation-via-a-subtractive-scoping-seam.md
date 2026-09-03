# ADR-0029: Project Retrieval Isolation via a Subtractive Scoping Seam

- **Status:** Accepted (2026-08-05 — implemented and verified on the VM; see Evaluator Evidence)
- **Date:** 2026-08-05
- **Feature:** 027-my-projects (cross-cutting: chat retrieval, global search, citation opening, file serving, knowledge graph, share links)
- **Context:** Every retrieval-privacy decision this system has ever made has been **additive**. `visible_source_ids` (`src-tauri/src/db/repositories/sources.rs:97`) answers "what may this user see?" with three OR'd branches — the shared corpus, the user's own documents, this conversation's attachments — each of which only ever ADDS to the set. Feature 027 introduces the first requirement that must **SUBTRACT**: a question asked inside a project must reach that project's documents *and nothing else*, not even the shared corpus the user can otherwise always see, and not even their own documents in other projects. Getting the location of this change wrong is the whole risk: six call sites funnel through that one function, and the reason a Feature 021 admin-disable takes effect everywhere with no re-index is precisely that none of them scopes anywhere else. Feature 027 also adds two adversarial shapes the function has never faced — a client that names which project it is in, and a share-link viewer who is a legitimate user with no claim on the documents.

<!-- Significance checklist
     1) Impact: defines the single legal location for retrieval scoping, permanently.
        Security-critical; every future retrieval feature inherits it. YES
     2) Alternatives: fourth OR'd branch; per-call-site filtering; widening for share links. YES
     3) Scope: cross-cutting — six call sites, five routes, two auth models. YES -->

## Decision

An integrated cluster covering *where* scope is decided, *who* decides it, and *what a share link may reach*.

- **`visible_source_ids` gains an exclusive mode, not a fourth branch.** `RequestScope` gains `project_id: Option<String>` and `include_corpus: bool`. When `project_id` is `Some`, the function returns project memberships only — plus the shared corpus if and only if `include_corpus` — and **nothing else**: no `owner_id` match, no conversation attachments. When `None`, the existing three-branch additive query and the `unrestricted()` early-return are byte-for-byte unchanged, so the desktop path and every existing test are unaffected.
- **Scoping stays in exactly one function.** All six call sites keep funnelling through it: `api/query.rs:116` (chat), `api/documents.rs:569` (knowledge graph), `:672` (citation → file), `:754` (citation → passage), `:864` (global search), `commands/assistant.rs:62` (desktop). No route may filter its own results. This is what makes FR-011 — "chat answering, global search and citation opening are governed by the same decision" — a structural property rather than a promise.
- **Membership, not ownership, decides inside a project.** Exclusive mode does not consult `owner_id`, because a shared project holds another user's `sources` rows (ADR-0028). A source is visible in a project iff it is a member of it, indexed, and not disabled.
- **On a chat turn the client never names a project.** There is no conversation table in this system — `query_sessions`/`query_messages` are unreferenced by the server, and `conversation_id` is a client-minted string on `query_log` with no owning row. Rather than treat that as a gap to fill, the design exploits it: a new `project_chat(conversation_id PK, project_id, owner_id)` lets the **server derive** the project from the `conversation_id` already in the request, then check `owner_id` against the authenticated caller. A forged or guessed conversation id finds no row, or another user's row that the owner check rejects. **The chat path is therefore unwidenable by construction, not by validation.**
- **The four non-chat routes must check, because they cannot derive.** `global_search`, `source_chunks`, `source_file` and `get_knowledge_graph` take `project_id` from the client, so each calls one shared helper, `owned_project(pool, user_id, project_id)`, and returns `404` on failure — the same response as a genuinely absent project, since whether it exists is itself information. This asymmetry with the chat path is explicit, and it carries a **test-per-route** requirement rather than one shared test: a forgotten route fails open.
- **A share-link viewer is authorised per cited chunk, never by widening.** Viewing a shared chat grants the transcript and the specific chunks its turns cited. `visible_source_ids` is neither consulted nor extended. Had a share link added the chat's sources to the viewer's visible set, the viewer could immediately global-search the owner's entire project — because global search scopes through the same function. The narrower authorisation is what keeps a link from becoming project access by the back door.
- **The shared-corpus opt-in is a per-turn flag, with no storage.** `include_corpus` joins `deep`, `widen` and `web` as a per-request field (`api/query.rs:81-83`) — exactly the lifetime the architect chose ("per question, next to the message box"). **No column exists to persist it**, by design: absence of storage is what enforces "returns to excluded next turn".
- **The admin gate is server-side and precedes argument parsing.** `db_config` key `my_projects_enabled`, read through a pure gate function mirroring `services/deep_search/mod.rs:101-109`, checked as the first act of every project route including the share-link read. Removing the nav item is not the gate.

## Consequences

### Positive

- **Isolation is inherited by every route for free.** Because global search, citation opening, file serving and the graph all scope through the changed function, they are project-isolated without a line of route-specific code — the same property that let an F021 document-disable take effect everywhere on the next turn with no re-index.
- **The most security-critical path is the one that cannot be attacked.** Chat retrieval — the highest-volume, highest-value path — takes no project identifier from the client at all. There is nothing to forge.
- **The change is small and reversible.** One new branch ahead of an untouched query; setting `project_id: None` restores today's behaviour exactly. The `Default` and `unrestricted()` constructors are unchanged, so the blast radius on existing tests is nil.
- **A project question is cheaper than the same question today**, not more expensive: scoping narrows the candidate set with one `IN (…)` predicate before Tier 2 ever runs (Constitution II).
- **The "one place only" rule is now written down.** Future features inherit a stated invariant rather than rediscovering it, and the reason per-call-site filtering is forbidden is on the record.

### Negative

- ⚠️ **Four routes carry a check that can be forgotten, and forgetting fails open.** Omitting `owned_project` on any of the four turns citation opening or global search into a way to read any project in the database — the exact failure `source_chunks` already documents at `api/documents.rs:745-753`. Mitigated by a required test per route; not mitigated structurally, which is the honest weakness of this design.
- ⚠️ **`visible_source_ids` now has two modes and one function.** Its complexity roughly doubles, and it is the single most security-sensitive function in the codebase. A future maintainer adding a fifth branch could reintroduce additivity inside the exclusive path without any test necessarily catching it unless the negative test is kept.
- **Two authorisation models now coexist**: set-based (`visible_source_ids`) and per-chunk (share links). A reviewer must know which applies where, and the second exists precisely because the first would be wrong.
- **Exclusive mode ignoring `owner_id` looks like a bug.** It is the correct consequence of ADR-0028, and it will read as a missing predicate to anyone who encounters it without that context.
- **Refusals return HTTP 200 on this system**, and "this project has nothing to search" (FR-012) will too — so tests must assert on bodies, not status codes.
- **The empty-project check must fire before retrieval, not after an empty result**, or a project with no documents would fall through to whatever the pipeline does with zero candidates.

## Alternatives Considered

**Alternative A — A fourth OR'd branch in `visible_source_ids`.**
The smallest-looking diff, and structurally impossible. Every existing branch adds; a fourth cannot express "and nothing else". Adding project membership as a fourth branch would make a project's documents visible *in addition to* the shared corpus and the user's other documents — the precise opposite of the requirement.

**Alternative B — Filter the returned `Vec` at each call site.**
*Rejected*: six call sites means six chances to forget, and every forgotten one fails open rather than closed. It also directly contradicts the reason the function exists — the single-point property that makes an admin disable take effect everywhere at once. This is the alternative most likely to be proposed later by someone adding a route in a hurry, which is why it is recorded here.

**Alternative C — A separate `project_visible_source_ids` function.**
*Rejected*: cleaner to read, and it splits the invariant across two functions that must be kept in agreement forever. The first route that calls the wrong one is a silent leak. One function with two modes is harder to read and impossible to call incorrectly.

**Alternative D — Put `project_id` on `query_log` and let the client send it per turn.**
*Rejected*: `query_log` is one row per **turn**, so the client would re-assert the project on every question — exactly the client-supplied scope FR-013 forbids. Rejecting it is what produced the `project_chat` design and, with it, the unwidenable chat path.

**Alternative E — Let a share link widen the viewer's visible set to the chat's sources.**
*Rejected*: simple, and it silently grants the viewer global search over the owner's whole project. This is the concrete form of the architect's rejected "full shared access" option for chat links; the per-chunk authorisation is what implements "view, don't continue".

**Alternative F — Persist the corpus toggle per project.**
Considered and rejected by the architect in favour of per-question. Recorded here because the *implementation* consequence is load-bearing: with no column to store it, "resets every turn" cannot regress.

## References

- Feature Spec: [specs/027-my-projects/spec.md](../../specs/027-my-projects/spec.md) — FR-010 – FR-019, FR-025, FR-027 – FR-029, FR-037 – FR-040, Risks 1, 3, 5
- Implementation Plan: [specs/027-my-projects/plan.md](../../specs/027-my-projects/plan.md) — Summary, Constitution Check (II, X, XI, XIII), Risks 1 and 5
- Research: [specs/027-my-projects/research.md](../../specs/027-my-projects/research.md) — R1 (six call sites), R2 (no conversation table), R5 (share links), R7 (gate pattern)
- Data model: [specs/027-my-projects/data-model.md](../../specs/027-my-projects/data-model.md) — `RequestScope`, `project_chat`, validation rules
- Quickstart: [specs/027-my-projects/quickstart.md](../../specs/027-my-projects/quickstart.md) — the four-route negative test
- Code: `src-tauri/src/db/repositories/sources.rs:53-121`; call sites `api/query.rs:116`, `api/documents.rs:569,672,754,864`
- Related ADRs: **ADR-0028** (copy-on-share — why exclusive mode must ignore `owner_id`); ADR-0008 (deterministic citation integrity — FR-016's provenance labelling strengthens it); ADR-0020 (graph-augmented retrieval on Postgres — `get_knowledge_graph` is one of the scoped routes); ADR-0021 (bounded agentic multi-pass deep retrieval — deep search's role-gated pattern is the model for the admin gate)
- Evaluator Evidence (accepted 2026-08-05, branch `027-my-projects` @ `8a968f7b`, run in an isolated worktree on the GPU VM):
  - `sources::scope_tests::project_scope_excludes_everything_but_the_project` — **passes**, and was **sabotage-tested**: with the exclusive branch disabled (`if false {`) it fails with `SHARED CORPUS leaked into a project`. This matters more than the pass. A negative test that still passes without the code it guards is testing nothing, and on this feature that was not hypothetical (see the trap below).
  - `sources::scope_tests::project_scope_with_corpus_adds_only_the_corpus`, `::project_scope_still_excludes_disabled_documents`, `::empty_project_returns_nothing` — **pass**. The opt-in adds the corpus and nothing further; a Feature 021 withdraw still subtracts inside a project; an empty project reports nothing rather than falling through to whatever the pipeline does with zero candidates.
  - `sources::scope_tests::unrestricted_returns_everything`, `::user_scope_excludes_foreign_private_and_all_attachments`, `::shared_flip_becomes_visible_without_reindex` — **pass unchanged**, evidencing the claim that `project_id: None` leaves the three-branch additive path byte-for-byte intact.
  - `tests/api_projects.rs::project_scope_isolates_every_route` — **passes**. SC-001 at the route boundary for all four client-supplied-`project_id` routes, asserted on the response body's citations, not on the retrieval helper.
  - `tests/api_projects.rs::share_link_reads_but_never_widens` — **passes**. SC-006/SC-007 including Alternative E's failure mode: a share-link viewer global-searching does **not** see the owner's project documents.
  - `tests/api_projects.rs::gate_closed_refuses_every_project_route` and `services::projects::tests::{gate_closed_by_default, gate_open_when_enabled}` — **pass**. SC-008: server-side refusal, gate closed by default.
  - ⚠️⚠️ **The route tests found a fail-open the compiler and the positive assertions both missed.** `GlobalSearchReq`, `SourceChunksReq` and `KnowledgeGraphReq` are `rename_all = "camelCase"`, so the wire key is `projectId` while `contracts/api.md` documented `project_id`. The mismatch made `Option<String>` deserialize to `None`, which took the **unscoped** branch — a `200` carrying the caller's whole corpus where a `404` was required. Fixed at `be522beb` with `#[serde(default, alias = "project_id")]`. It also meant the *positive* isolation assertions had been passing **vacuously** (an unscoped search over chunk-less sources returns no hits, so nothing appeared to leak); only the "bob must be refused" negative assertion caught it. This is the concrete argument for the test-per-route requirement above, and for `Option<T>` + `rename_all` being a fail-open shape in any security predicate.
  - Suite context: **933 passed / 2 failed** against a 925/2 baseline; migration 023 **2/2**; `api_projects` **5/5**.
  - No grader run: these are structural guarantees, so the acceptance evidence is the test suite rather than an eval set. The end-to-end manual walkthrough (`quickstart.md`) is **not yet done** — it needs a deploy, which has not happened.
