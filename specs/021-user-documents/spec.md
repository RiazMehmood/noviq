# Feature Specification: User Documents — Ownership-Scoped Retrieval, Uploads, Sharing, and Chat Attachments

**Feature Branch**: `021-user-documents`
**Created**: 2026-07-05
**Status**: Draft
**Input**: User description: "User documents with ownership-scoped retrieval, uploads, sharing, and chat attachments — (1) retrieval respects document ownership: chat/search only retrieves from documents the requesting user may see (their own + shared/global corpus); (2) regular web users can upload and index their own documents from a simple 'My documents' UI; (3) owners/admins can toggle a document/folder between private and shared-with-everyone via an explicit visibility model; (4) users can attach a file inside a chat conversation so the LLM sees its content for that conversation only (conversation-scoped indexing, promotable to My documents)"

## Context

Today the system assumes one owner of knowledge: the administrator indexes the corpus, and
every signed-in user only asks questions over it. Users cannot add documents through the
web interface, there is no way to share a document with colleagues, and — critically — the
answer engine searches **every** indexed document regardless of who owns it, while the
document list correctly hides other people's folders. Ownership therefore exists in the
catalogue but not in retrieval: if user uploads ever opened up under the current behaviour,
one user's confidential file could be quoted, with citations, in another user's answers.
This feature closes that gap first, then builds the user-facing document powers on top of it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Answers only draw from documents I'm allowed to see (Priority: P1)

As a signed-in user, when I ask a question in chat or search, the system consults only the
shared/global corpus plus documents I own (including files I attached to the current
conversation). It never uses another user's private documents to answer me, and never
cites them.

**Why this priority**: This is the confidentiality foundation. Every other story in this
feature adds private user content into the system; without retrieval scoping, each of them
widens a privacy leak. It also stands alone as a fix to today's latent gap.

**Independent Test**: With two ordinary user accounts A and B, index a document privately
as A (via any available means, including operator assistance). Ask B questions whose only
possible source is A's document — B must get a "not found in corpus" style answer with zero
citations to A's document. Ask A the same question — A gets the answer with citations.

**Acceptance Scenarios**:

1. **Given** user A owns a private document and user B does not, **When** B asks a question
   answerable only from that document, **Then** B's answer contains no content or citation
   from it and behaves as if the document does not exist.
2. **Given** the same setup, **When** A asks that question, **Then** A's answer cites A's
   own document normally.
3. **Given** the shared/global corpus, **When** any user (or the administrator) asks a
   question answerable from it, **Then** answers and citations work exactly as they do today
   (no regression in quality, latency, or citation behaviour).
4. **Given** an administrator account, **When** the administrator asks a question, **Then**
   retrieval covers the global corpus and the administrator's own documents — private
   documents of other users are excluded from the administrator's *answers* too (management
   visibility is separate from answer scope).
5. **Given** a user's private document is later shared, **When** another user repeats the
   earlier question, **Then** the document now participates in their answers.

---

### User Story 2 - My documents: upload and manage my own files (Priority: P2)

As an ordinary web user, I have a simple "My documents" area where I can upload files from
my device, watch them get indexed, see their status, and remove them. Documents I upload
are private to me by default and immediately usable in my own chat answers.

**Why this priority**: This is the first visible user power and the reason the scoping in
Story 1 matters. It delivers standalone value: officers can query their own case files
alongside the legal corpus.

**Independent Test**: Sign in as an ordinary user, upload a small set of files, wait for
"indexed" status, ask a question answerable only from one of them, and get a cited answer.
Delete the document and verify it disappears from answers.

**Acceptance Scenarios**:

1. **Given** an ordinary signed-in web user, **When** they open "My documents" and upload
   supported files, **Then** the files are accepted, indexed, and listed with progress and
   a final per-file status (indexed / failed with reason).
2. **Given** a newly indexed personal document, **When** the user asks a related question in
   chat, **Then** the answer can cite that document.
3. **Given** a personal document, **When** the user deletes it, **Then** it no longer appears
   in their list, their answers, or anyone's answers, and its content is removed from the
   system.
4. **Given** an unsupported or oversized file, **When** the user tries to upload it, **Then**
   they get a clear message stating why it was rejected, and other files in the same batch
   still process.
5. **Given** a user at their storage/file allowance, **When** they attempt another upload,
   **Then** the upload is refused with a message showing their current usage and limit.
6. **Given** the administrator, **When** they view system administration, **Then** they can
   see per-user document counts/storage and can remove a user's document (with the action
   recorded in the audit trail).

---

### User Story 3 - Share a document with everyone (Priority: P3)

As the owner of a document (or as an administrator), I can flip a document or a whole
folder between **Private** (only I can use it) and **Shared** (every signed-in user can see
it in the corpus tree and their answers may draw on it). The current visibility is always
displayed, and I can flip it back.

**Why this priority**: Sharing turns personal uploads into team knowledge, but it only
makes sense once private uploads (Story 2) and scoped retrieval (Story 1) exist.

**Independent Test**: User A shares a private document; user B can now see it in their
document tree and receives cited answers from it. A reverts it to private; B loses both
visibility and retrieval.

**Acceptance Scenarios**:

1. **Given** a private document owned by user A, **When** A marks it Shared, **Then** all
   users see it in their document list and it participates in everyone's answers.
2. **Given** a shared document owned by A, **When** A reverts it to Private, **Then** other
   users immediately stop seeing it in lists and answers.
3. **Given** any document, **When** any user views it in a list, **Then** its visibility
   state (Private / Shared) and owner are clearly displayed.
4. **Given** a document owned by A, **When** user B (non-admin) attempts to change its
   visibility, **Then** the change is refused.
5. **Given** an administrator, **When** they change any document's visibility, **Then** the
   change applies and is recorded in the audit trail with the acting user.
6. **Given** the pre-existing global corpus indexed by administrators, **When** this feature
   ships, **Then** those documents appear as Shared and continue to behave exactly as today.

---

### User Story 4 - Attach a file to a conversation (Priority: P4)

As any signed-in user (including the administrator), I can attach a file inside a chat
conversation. The system indexes it for that conversation only: my questions in this
conversation can draw on the attachment (with citations), but the file does not appear in
my or anyone else's document lists and never influences other conversations. If it proves
useful, I can promote it to "My documents" in one action.

**Why this priority**: Highest-convenience, lowest-urgency. It depends on scoped retrieval
(Story 1) and reuses the upload/indexing machinery from Story 2, so it lands last.

**Independent Test**: Attach a document mid-conversation, ask a question about it, get a
cited answer within the same conversation; open a new conversation and confirm the
attachment is invisible there; promote it and confirm it now appears in "My documents".

**Acceptance Scenarios**:

1. **Given** an open conversation, **When** the user attaches a supported file, **Then** the
   conversation shows the attachment with indexing progress and a ready state, and the user
   is told when it can be queried.
2. **Given** a ready attachment, **When** the user asks a question about its content in that
   conversation, **Then** the answer draws on the attachment and cites it, and can still
   draw on the shared corpus and the user's own documents in the same answer.
3. **Given** the same attachment, **When** the user (or another user) asks in a different
   conversation, **Then** the attachment contributes nothing.
4. **Given** a conversation with an attachment, **When** the conversation is deleted, **Then**
   the attachment's content is removed from the system.
5. **Given** a ready attachment, **When** the user promotes it to "My documents", **Then** it
   becomes a normal private document (Story 2 behaviours apply, including later sharing).
6. **Given** an attachment that failed to index, **When** the user views the conversation,
   **Then** the failure and its reason are visible and the user can retry or remove it.

---

### Edge Cases

- Ownership change vs. open conversations: if a document is unshared (or deleted) while
  another user's conversation previously cited it, past answers remain in that user's
  history (they were legitimate at the time) but all *new* answers must exclude it.
- A user is deleted/disabled by the administrator: their private documents and attachments
  must not silently become visible; they remain excluded from everyone's retrieval and are
  disposed of per the administrator's user-removal action.
- The same file uploaded by two different users must remain two independent documents with
  independent ownership — deduplication must never merge visibility.
- A user attaches a file, then promotes it, then shares it: the single document must not be
  double-counted or double-retrieved in that user's conversation.
- Concurrent uploads by many users queue behind the single indexing pipeline: each user must
  still see honest per-file progress and never another user's file names in progress events.
- Very large personal uploads must not starve the shared corpus indexing or chat
  responsiveness for other users (bounded by the per-user allowance).
- A conversation attachment in a conversation that is never deleted: subject to the same
  per-user allowance so abandoned attachments cannot accumulate unbounded storage.
- Retrieval scoping must hold on every answer path (normal chat, follow-ups, drafting,
  search page, any capability that consults the corpus) — not just the primary chat route.

## Requirements *(mandatory)*

### Functional Requirements

**Scoped retrieval (US1)**

- **FR-001**: Every retrieval that feeds an answer for a requesting user MUST be restricted
  to: the shared/global corpus, documents owned by that user, and attachments belonging to
  the active conversation. This applies to all answer-producing surfaces (chat, search,
  drafting, and any capability that consults indexed content).
- **FR-002**: Citations MUST never reference a document outside the requesting user's
  permitted scope at answer time.
- **FR-003**: When the answer exists only outside the user's scope, the system MUST respond
  exactly as it does today for "not in corpus" questions (no hint that a hidden document
  exists).
- **FR-004**: Administrator *answer* scope follows the same rule (global + own documents);
  administrative *management* visibility over all documents is preserved separately.

**My documents (US2)**

- **FR-005**: Ordinary web users MUST be able to upload supported files from their device
  through a dedicated "My documents" surface, without access to any administrator settings.
- **FR-006**: Uploaded documents MUST default to Private and become usable in the owner's
  answers once indexed.
- **FR-007**: The surface MUST show per-file indexing progress and terminal status
  (indexed / failed with a human-readable reason) and allow retry of failures.
- **FR-008**: Owners MUST be able to delete their documents; deletion removes the content
  from storage, listings, and all future retrieval.
- **FR-009**: The system MUST enforce a per-user allowance (storage size and file count),
  with limits configurable by the administrator; uploads beyond the allowance are refused
  with current usage shown. Default allowance: 500 MB and 200 files per user.
- **FR-010**: The administrator MUST be able to view per-user usage and remove any user's
  document; such removals are recorded in the audit trail.
- **FR-011**: Upload, deletion, share/unshare, attachment, and promotion actions MUST be
  recorded in the existing audit trail with the acting user.

**Visibility & sharing (US3)**

- **FR-012**: Every document/folder MUST carry an explicit visibility state: Private
  (owner only) or Shared (all signed-in users). The state and the owner MUST be visible
  wherever the document is listed.
- **FR-013**: Only the owner or an administrator MUST be able to change visibility, in
  either direction, at folder level and at individual-document level.
- **FR-014**: Visibility changes MUST take effect for listings and for retrieval without
  requiring re-indexing, re-upload, or restart.
- **FR-015**: All documents indexed by administrators before this feature MUST be treated
  as Shared with unchanged behaviour; migration MUST NOT require re-indexing.

**Conversation attachments (US4)**

- **FR-016**: Users MUST be able to attach supported files inside a conversation; the
  attachment is indexed and becomes queryable only within that conversation.
- **FR-017**: Attachments MUST NOT appear in any document listing (owner's or others') and
  MUST NOT participate in any other conversation's retrieval.
- **FR-018**: Within its conversation, an attachment participates in answers alongside the
  user's normal scope, and its citations identify it as the attached file.
- **FR-019**: Deleting the conversation MUST remove the attachment's indexed content and
  stored file. Attachments count against the owner's per-user allowance while they exist.
- **FR-020**: Users MUST be able to promote an attachment to "My documents" in one action,
  after which it behaves as a normal private document and no longer double-counts or
  double-retrieves within the original conversation.
- **FR-021**: Attachment indexing failures MUST be visible in the conversation with reason,
  retry, and remove options; the conversation remains fully usable while an attachment is
  still indexing or has failed.

### Key Entities

- **Document (source)**: an indexed file. Gains an explicit **owner** (a user, or the
  system/administrator for the pre-existing corpus) and a **visibility** state
  (Private / Shared). Existing lifecycle (uploaded → indexing → indexed/failed) unchanged.
- **Folder / collection**: grouping of documents with the same ownership rules; visibility
  can be set at folder level (applying to its documents) or per document.
- **Conversation attachment**: a document bound to exactly one conversation instead of a
  listing; excluded from all listings and from every other retrieval scope; convertible to
  a normal private document by promotion; deleted with its conversation.
- **User allowance**: per-user storage and file-count budget covering personal documents
  and live attachments; administrator-configurable.
- **Audit event**: existing audit trail extended to cover upload, delete, share/unshare,
  attach, and promote actions.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a two-user privacy test (one user's private document, the other asking 20
  targeted questions about it), zero answers and zero citations expose any of the private
  document's content — 100% exclusion.
- **SC-002**: Existing shared-corpus question quality is unchanged: on the team's standing
  evaluation questions, answers and citations match pre-feature behaviour, and median
  answer latency degrades by no more than 10%.
- **SC-003**: An ordinary user can go from sign-in to a cited answer from their own uploaded
  document in under 5 minutes for a typical (≤20-page) file, without administrator help.
- **SC-004**: A visibility flip (share or unshare) is reflected in other users' listings and
  answers within 10 seconds, with no restart or re-index.
- **SC-005**: A conversation attachment on a typical (≤20-page) file is queryable within
  2 minutes of attaching, and 100% of attachment content is unreachable from any other
  conversation or user in testing.
- **SC-006**: 100% of upload/delete/share/attach/promote actions appear in the audit trail
  with the correct acting user in spot checks.

## Assumptions

- **Sharing audience**: "Shared" means visible to *all* signed-in users of the deployment.
  Group- or person-level sharing (share with only user B) is out of scope for this feature.
- **Sharing is owner-decided and immediate** (user decision 2026-07-05) — no administrator
  approval step. The administrator retains after-the-fact control: unshare or delete any
  document, with every share recorded in the audit trail.
- **Administrator answer scope** (user decision 2026-07-05): administrators' *answers* draw
  only on the shared corpus plus the administrator's own documents — the same privacy rule
  as every user, so "private means private" holds against everyone's chat. Administrators
  retain full management powers (view usage, delete, change visibility) over all documents.
- **Default allowance** of 500 MB / 200 files per user is a starting point; the
  administrator can change it globally. Per-individual overrides are out of scope.
- **Supported file types** for user uploads and attachments are the same set the indexing
  pipeline accepts today; no new formats are introduced by this feature.
- **Desktop app** (single-operator, no auth) is unaffected: it continues to see and manage
  everything, as today.
- **Past conversations are not rewritten**: revoking visibility affects future answers only;
  answer history already delivered to a user is not retroactively redacted.
- **One shared indexing pipeline**: user uploads and attachments queue behind ongoing
  indexing work; the spec requires honest progress reporting, not parallel pipelines.

## Dependencies

- Existing authentication/session model (users and the administrator role) — unchanged.
- Existing indexing pipeline, document tree, citation, and audit-trail mechanisms — reused,
  not redesigned.
- The pre-existing global corpus must migrate to "Shared" without re-indexing (FR-015).
