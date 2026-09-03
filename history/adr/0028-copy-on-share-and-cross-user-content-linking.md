# ADR-0028: Copy-on-Share and Cross-User Content Linking

- **Status:** Accepted (2026-08-05 — implemented and verified on the VM; see Evaluator Evidence)
- **Date:** 2026-08-05
- **Feature:** 027-my-projects (cross-cutting: sharing, content lifetime, deletion, storage accounting, retrieval scoping)
- **Context:** Feature 027 ("My Projects") gives each user named workspaces holding their own documents, and the architect extended its scope on 2026-08-05 to include sharing — for projects and for individual chats. Sharing forces a question the system has so far avoided: **may one user's indexed content be reachable from another user's workspace?** Migration 022 (`source_links`, 2026-08-04) answered "not yet", in a comment that names this exact decision as owed: *"cross-user linking would make one user inherit another's extraction and leak the fact that the file was already in the system, and that needs a spec + ADR of its own."* Feature 027's spec is that spec; this is that ADR. The architect's chosen sharing semantics — "while it is shared to other users it becomes their project with existing documents also, so it can edit and run other questions too, so it becomes their pipeline, not the original project that remains with the original user only" — resolve the question in a specific and unusual direction: sharing **copies**, it does not grant access.

<!-- Significance checklist
     1) Impact: changes content lifetime, every delete path, storage accounting, and the
        retrieval-scoping predicate. Irreversible in the data it leaves behind. YES
     2) Alternatives: shared access with permission levels; re-index per recipient. YES
     3) Scope: cross-cutting — sharing, deletion, quota, scoping, admin. YES -->

## Decision

An integrated cluster. These parts are not separable — each is forced by the one before it.

- **Sharing semantics: copy, not access.** Sharing a project creates a **new project owned by the recipient**, containing the same documents. The recipient may add, remove, rename and question freely. The original is untouched, in both directions and permanently. There is no shared, concurrently-editable project anywhere in the system, and therefore **no permission levels, no invitations, no roles, no locking, and no concurrent-edit state machine**.
- **Copying is metadata-only.** A copy inserts one `project` row and N `project_source` rows. **Zero** `sources` rows, chunks, embeddings, graph rows or files are created. This is what makes the copy appear in under 5 seconds (SC-003) instead of the minutes a re-index takes.
- **Cross-user linking is therefore permitted, and only through explicit sharing.** `project_source` may name a project owned by user B and a source owned by user A. This supersedes migration 022's per-user restriction **for this path only**: `source_link` remains strictly per-user, and no *implicit* dedup across users is introduced (see Alternatives).
- **Membership, not ownership, is the retrieval authority.** Because a copy references A's `sources` row (whose `owner_id` is A), the existing `owner_id = $1` predicate in `visible_source_ids` can never match B. Project scoping therefore **ignores `owner_id` entirely** — a source is visible in a project iff it is a member of that project, indexed, and not disabled. Consequently `project_source` deliberately carries **no `owner_id` column**, unlike `source_link` where that column is the security guard: the only owner it could record is the *source's*, and any predicate matching it against the caller would break the sharing this decision exists to enable.
- **Content lifetime becomes a union across three tables.** A `sources` row's content is destroyed only when **none** of `notebook_sources` (F001), `source_link` (022) or `project_source` (023) holds a reference. One helper, `source_repo::is_referenced()`, guards every delete path (`api/documents.rs:599-640`, `:1299`).
- **A shared document cannot be recalled** — and this is disclosed at the moment of sharing. The original owner deleting a document, or their whole project, does not remove it from any copy. This follows necessarily from the rule above and is the opposite of what users will expect, so FR-024 makes the disclosure a requirement rather than UI polish.
- **Chat sharing follows the same model at one remove**: a link grants **read** of the transcript and of the specific chunks cited in it; continuing the work means taking a copy (a project + a conversation), which is the project copy above. A share-link viewer is authorised **per cited chunk**, never by widening their document set — see ADR-0029.
- **Storage accounting falls out for free.** `usage_for_owner` sums `file_size_bytes WHERE owner_id = $1`. A copy adds memberships, never rows, so a recipient's allowance is untouched with no additional code — which is also the correct answer, since copying stores no new bytes.
- **`copied_from` is provenance only.** The `project.copied_from` column records where a copy came from and is **never a foreign key** and never read by any scoping or deletion query. An FK with cascade would destroy the recipient's project when the sharer deleted theirs — precisely the failure this decision forbids.

## Consequences

### Positive

- **Independence is a property of the data model, not of discipline.** Two projects pointing at the same immutable content cannot affect one another because neither owns it. FR-021 is true by construction; no code has to remember to preserve it.
- **The entire shared-access problem space disappears**: no permission matrix, no invitation lifecycle, no "what happens when two users edit the same project", no lock contention, no partial-revocation semantics. This is a large reduction in surface area compared with what "sharing" normally costs.
- **Sharing is effectively free at runtime** — a handful of row inserts. No re-extraction, no re-embedding, no duplicate storage, however many recipients.
- **The recipient's quota is correct without special-casing**, because the accounting predicate already keys on the column that does not change.
- **Revocation of a chat link is coherent**: copies already taken keep working because they are independent projects with no reference back, so revocation does not have to reach into anyone else's data.
- **A decision the codebase explicitly owed is now paid.** Migration 022 asked for this document by name; the restriction it recorded is superseded on the record rather than quietly stepped over.

### Negative

- ⚠️ **Deletion can now destroy another user's data.** Before migration 023, every reference to a document belonged to the same user, so "delete my document" could only ever destroy the deleter's own content. After it, a delete path that forgets `is_referenced()` silently destroys content another user's project depends on. This is the single most dangerous consequence in Feature 027, and it is why the plan sequences delete-safety **before** sharing rather than after.
- ⚠️ **A shared document is unrecallable.** A user who shares a project and later deletes a sensitive document has not removed it from any copy. Correct for a copy model; contrary to expectation; mitigated only by disclosure (FR-024). There is no technical remedy available within this model.
- **Copies drift and cannot be reconciled.** There is no update propagation, no notion of an original being "newer", and no merge. A sharer who corrects a document must tell recipients out of band. Accepted deliberately — sync is out of scope.
- **Content outlives its owner's account.** If A's account is removed while copies of A's project exist, the content persists because references remain. Correct, and surprising.
- **`owner_id` now means less than it appears to.** It records who uploaded a document, not who can retrieve it. A future reader of `sources` will likely misread this; the absent `owner_id` on `project_source` is the place that will look like an oversight and is not.
- **Storage attribution and cost accounting become ambiguous** — bytes are charged to the uploader forever, including bytes only a recipient still uses. Acceptable at ~10 users; would need revisiting at scale.
- **Migration 023 is a one-way deploy.** `ignore_missing = false` means a pre-023 binary fails at startup with `VersionMissing` once it has applied.

## Alternatives Considered

**Alternative A — Shared access with permission levels** (viewer / editor / co-owner on one project row).
The conventional model, and the one offered to the architect first. *Rejected*: it requires an ACL evaluated inside the retrieval-scoping predicate, an invitation lifecycle, revocation that must reach live sessions, and answers to every concurrent-edit question — for a deployment of roughly ten officers who work on separate matters. It also makes one user able to damage another's work, which copy-on-share makes structurally impossible. The architect chose the copy model explicitly: *"it becomes their pipeline, not the original project."*

**Alternative B — Re-upload and re-index per recipient** (no cross-user linking at all; keep migration 022's restriction intact).
*Rejected*: makes SC-003 (<5 s) unreachable — the recipient waits minutes per document; multiplies storage and embedding cost by the number of recipients; and still would not deliver FR-021's independence any better than linking does, since the copies were already independent. Its only advantage is that deletion stays single-user, which is real but is bought at the cost of the feature.

**Alternative C — Implicit cross-user content dedup** (link any user to any identical bytes already indexed, sharing or not).
*Rejected, and specifically distinguished.* This is the hazard migration 022 named: user B uploads a file, is silently linked to A's row, and thereby learns the file was already in the system. Copy-on-share does not have this property because the linking is **explicit and initiated by the owner** — nothing is disclosed that A did not choose to disclose. This ADR permits cross-user linking *only* through a deliberate share, and leaves `source_link` per-user exactly as 022 built it.

**Alternative D — Copy-on-share with full file duplication** (new `sources` rows, re-embedded).
*Rejected*: identical user-visible semantics to the chosen design, at N× the storage and minutes of latency, and it would make deletion safe again. Considered seriously for that last reason, and rejected because the delete-safety problem is solvable with one helper and one test, whereas the cost is permanent and scales with sharing.

## References

- Feature Spec: [specs/027-my-projects/spec.md](../../specs/027-my-projects/spec.md) — FR-006, FR-007, FR-009, FR-020 – FR-024, Risk 2
- Implementation Plan: [specs/027-my-projects/plan.md](../../specs/027-my-projects/plan.md) — Complexity Tracking row 3, phasing (delete-safety precedes sharing)
- Research: [specs/027-my-projects/research.md](../../specs/027-my-projects/research.md) — R4 (the 022 supersession, verbatim), R6
- Data model: [specs/027-my-projects/data-model.md](../../specs/027-my-projects/data-model.md) — `project_source`, content-lifetime rule
- **Superseded decision**: `src-tauri/src/db/pg/migrations/022_source_links.sql:20-24` — the per-user restriction and its stated request for "a spec + ADR of its own"
- Related ADRs: **ADR-0029** (subtractive scoping seam — the retrieval half of this feature, and where membership-not-ownership is enforced); ADR-0015 (PostgreSQL as single source of truth — the store these tables are added to); ADR-0025 (data boundary: private documents may cross to third-party inference — the decision FR-041 discloses rather than changes; recorded on another branch, not present on `027-my-projects`)
- Evaluator Evidence (accepted 2026-08-05, branch `027-my-projects` @ `8a968f7b`, run in an isolated worktree on the GPU VM):
  - `tests/api_projects.rs::a_copy_is_independent_and_survives_the_original` — **passes**. Produces SC-004 and SC-005 directly: B's copy answers immediately with no reprocessing; edits on either side are invisible to the other; A deleting the document, and then the whole project, leaves B's copy still answering from it. This is the decision's central claim, and it is the assertion that would fail if `is_referenced()` were dropped from a delete path.
  - `tests/migration_023_test.rs::deleting_a_project_cascades_membership_but_not_content` — **passes**. `project_source` rows go, the `sources` row and its chunks stay. The complement of the above at the schema level, so the guarantee does not rest on application code alone.
  - `sources::scope_tests::is_referenced_sees_each_reference_type` — **passes**. Asserts the three-table union (`notebook_sources`, `source_link`, `project_source`) that the content-lifetime rule is stated in; a future reference table added without extending this helper fails here.
  - `sources::scope_tests::project_scope_ignores_source_owner` — **passes**. The "membership, not ownership" clause: a source owned by A is retrievable inside B's copy. It is also the test that would break if someone added the `owner_id` column to `project_source` that this ADR forbids.
  - Suite context: **933 passed / 2 failed** against a 925/2 baseline (delta is exactly the 8 new tests); the two failures are pre-existing and unrelated. Migration 023 **2/2**, `api_projects` **5/5**.
  - ⚠️ Not yet evidenced: SC-003's *wall-clock* <5 s bound for a 50-document copy. The tests prove copying is metadata-only (no `sources`/chunk rows are created), which is the property the bound rests on, but the timing itself is T099 and is unmeasured — the feature is not deployed.
  - Still no grader run: this feature's guarantees are structural, not answer-quality, so the acceptance evidence is the test suite rather than an eval set.
