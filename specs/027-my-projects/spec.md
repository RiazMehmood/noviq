# Feature Specification: My Projects

**Feature Branch**: `027-my-projects`
**Created**: 2026-08-05
**Status**: Draft — open questions resolved 2026-08-05
**Input**: User description: "My Projects — user-owned document workspaces. A named, user-created container holding its own documents and many chats, where retrieval is scoped to that project's documents only. Starts empty (none of the 18 shared corpus documents), with an opt-in toggle to also search the shared legal corpus. Admin gate is global on/off via db_config, backend-enforced. Disabling hides and makes unreachable but retains data." Extended in session: sharing for both projects and chats.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Ask questions against only my own documents (Priority: P1)

An officer is working on one matter — a tender file, an inquiry, a draft contract.
They create a project, name it, upload the documents that belong to that matter, and
ask questions. Every answer is drawn from those documents and nothing else. The
shared legal corpus that dominates ordinary chat does not appear in the answer or in
its citations.

**Why this priority**: This is the entire reason the feature exists. Today every
question is answered against the shared corpus plus whatever the user has uploaded,
so a question about "clause 4" can pull Rule 4 from five unrelated statutes. A closed
document set is what makes an answer about *this matter* trustworthy. Without this
story there is no feature; with only this story the feature is already useful.

**Independent Test**: Create a project, upload two documents, ask a question whose
answer also exists in the shared corpus, and confirm every citation resolves to one of
the two uploaded documents. Then ask a question answerable *only* from the shared
corpus and confirm the system says it cannot find it rather than answering.

**Acceptance Scenarios**:

1. **Given** a new project with no documents, **When** the user asks any question,
   **Then** the system reports it has no documents to search and invites the user to
   add some — it does not fall back to the shared corpus.
2. **Given** a project containing only the user's own uploads, **When** the user asks
   a question whose answer appears in both an uploaded document and the shared corpus,
   **Then** every citation points at an uploaded document.
3. **Given** a project belonging to user A, **When** user B is signed in and has not
   been given a copy, **Then** user B cannot see, open, search, or retrieve from that
   project or its documents.
4. **Given** a project chat, **When** the user opens global search, **Then** results
   are limited to that project's documents on the same terms as chat retrieval.

---

### User Story 2 - Keep many conversations inside one project (Priority: P1)

The same matter produces many separate questions over days or weeks. The user opens
their project and sees the chats they have already had inside it, picks one up where
they left off, or starts a new one. All of them share the project's documents.

**Why this priority**: A project that holds one conversation is just a chat with
attachments — which already exists. The container is what distinguishes this feature
from today's attachment flow, and it is what makes uploading documents once worth
doing. P1 alongside Story 1 because the two together are the minimum coherent product.

**Independent Test**: Create a project, hold three separate conversations in it, sign
out and back in, and confirm all three are listed under the project and each replays
its full transcript with citations intact.

**Acceptance Scenarios**:

1. **Given** a project with existing chats, **When** the user opens the project,
   **Then** its chats are listed most-recent first and no chat from outside the project
   appears.
2. **Given** a chat started inside a project, **When** the user signs in on a different
   device, **Then** that chat appears under the same project with its transcript
   intact.
3. **Given** a chat started outside any project, **When** the user views a project,
   **Then** that chat is not listed there.
4. **Given** a new chat started inside a project, **When** the user asks a question,
   **Then** it is answered from the project's documents without the user having to
   re-select or re-attach anything.

---

### User Story 3 - Include the law in one question (Priority: P2)

Most questions are about the officer's own file. Occasionally one needs the law too:
"does clause 9 of this contract comply with the Procurement Rules?" For that one
question the user ticks an option beside the message box, and the answer draws on both
their documents and the shared legal corpus — with the citations making plain which
passage is their own draft and which is enacted law. The next question is back to
project-only.

**Why this priority**: Genuinely valuable, and the reason the toggle was specified, but
it depends on Story 1 existing first, and a project that can only search its own
documents is already shippable. Deferring it does not block launch.

**Independent Test**: In a project, ask a question answerable only from the shared
corpus without ticking the option and confirm a not-found response. Ask it again with
the option ticked and confirm it is answered, with shared-corpus citations visually
distinguished from the project's own documents. Ask a third question without ticking
and confirm the corpus is gone again.

**Acceptance Scenarios**:

1. **Given** the option is not ticked, **When** the user asks a question, **Then** no
   shared-corpus passage is retrieved or cited.
2. **Given** the option is ticked for a question, **When** it is answered, **Then** the
   answer may cite both, and each citation is labelled as either a project document or
   a shared-corpus document.
3. **Given** the previous question was asked with the option ticked, **When** the user
   asks the next question, **Then** the option is back to unticked and the answer draws
   on project documents only.
4. **Given** a chat containing both kinds of answer, **When** it is reopened later,
   **Then** a reader can still tell which answers drew on the shared corpus.

---

### User Story 4 - Upload a document once, use it in several projects (Priority: P2)

A user has a document that belongs to more than one matter — a standard set of terms, a
policy circular they refer to often. They add it to a second project without
re-uploading and without waiting for it to be processed again. Removing it from one
project leaves it working in the other.

**Why this priority**: Saves the user real time and the system real work, but the
feature is complete without it — a user can always upload the file twice. Its true
importance is that getting removal wrong here destroys data in a *different* project,
so it must be specified before anyone implements sharing. It is also the mechanism
Story 5 depends on: copying a project must not copy its files.

**Independent Test**: Add one document to two projects, confirm it is immediately usable
in both without reprocessing, remove it from the first, and confirm it still answers
questions in the second.

**Acceptance Scenarios**:

1. **Given** a document already processed for the user, **When** they add it to another
   project, **Then** it becomes searchable there immediately with no reprocessing delay.
2. **Given** a document in two projects, **When** the user removes it from one, **Then**
   it remains fully searchable in the other and its content is retained.
3. **Given** a document in exactly one project, **When** the user removes it from that
   project, **Then** it is no longer retrievable in any of that user's projects.
4. **Given** a project containing documents, **When** the user deletes the whole
   project, **Then** documents shared with a surviving project remain intact there.

---

### User Story 5 - Share a project with a colleague, who gets their own copy (Priority: P2)

An officer has assembled a project for a matter and a colleague needs to work on it
too. They share it. The colleague receives **their own project** containing the same
documents — theirs to add to, remove from, rename and question as they see fit. Nothing
the colleague does touches the original, and nothing the original owner does afterwards
changes the colleague's copy. Two independent lines of work from one starting point.

**Why this priority**: The reason a project is worth assembling is that the work can be
handed on. Copy-on-share is what makes this safe: there is no concurrent editing, no
permission levels, and no way for one user to damage another's work. It is P2 rather
than P1 because Stories 1 and 2 are a complete product without it.

**Independent Test**: User A shares a project with user B. Confirm B has a project with
the same documents that is immediately searchable without reprocessing. Have B add a
document and rename their copy; confirm A sees no change. Have A remove a document;
confirm B's copy still answers questions from it.

**Acceptance Scenarios**:

1. **Given** user A shares a project with user B, **When** B opens it, **Then** B has
   their own project containing the same documents, searchable immediately with no
   reprocessing.
2. **Given** B has a copy, **When** B adds, removes or renames anything in it, **Then**
   user A's project is unchanged.
3. **Given** B has a copy, **When** A adds, removes or renames anything in the original,
   **Then** B's copy is unchanged.
4. **Given** B has a copy, **When** A deletes a document from the original — or deletes
   the whole original project — **Then** B's copy continues to answer questions from
   that document.
5. **Given** user A's project, **When** it is copied to B, **Then** A's chats in it are
   not copied; B starts with an empty conversation list.
6. **Given** a copy exists, **When** B asks a question in it, **Then** retrieval is
   scoped to B's copy alone and never reaches back into A's project or any third user's.

---

### User Story 6 - Share a chat by link (Priority: P3)

A user has an answer worth passing on. They share the chat, get a link, and send it to
a colleague. The colleague — signed in to Noviq with their own credentials — opens the
link and reads the whole conversation, and can click any citation to read the passage
it came from. They cannot ask new questions in it. If they want to take the work
further, they take their own copy of the chat and its project, and continue from there.

**Why this priority**: A convenience on top of a working product, and the last thing a
user needs. It is specified now because it is the route by which a project's documents
most easily reach someone the owner did not think about — which is a decision, not an
implementation detail.

**Independent Test**: Share a chat, open the link as a different signed-in user, confirm
the transcript reads in full and citations open. Confirm no new question can be asked in
it. Take a copy, confirm it becomes an independent project and chat. Open the link
signed out, and confirm it is refused.

**Acceptance Scenarios**:

1. **Given** a shared chat link, **When** a signed-in Noviq user opens it, **Then** they
   can read the full transcript and open every citation to its source passage.
2. **Given** a shared chat link, **When** someone who is not signed in to Noviq opens
   it, **Then** they are refused — the link never exposes anything outside Noviq.
3. **Given** a user viewing a shared chat, **When** they try to ask a question in it,
   **Then** they cannot; they are offered their own copy instead.
4. **Given** a user viewing a shared chat, **When** they take a copy, **Then** they
   receive their own project containing the chat's documents and their own copy of the
   conversation, and may continue from there without affecting the original.
5. **Given** a shared chat, **When** the owner stops sharing it, **Then** the link stops
   working for everyone, while copies already taken continue to work.
6. **Given** a shared chat link, **When** the owner later adds turns to the original
   chat, **Then** what a viewer sees is the chat as it stands when they open the link.

---

### User Story 7 - Administrator turns the feature off (Priority: P3)

The administrator decides the deployment is not ready for user-owned workspaces. They
switch My Projects off. Every user's projects disappear from view and cannot be reached
by any route. Nothing is deleted. Switching it back on restores everything exactly as it
was.

**Why this priority**: A release safety valve rather than user value. It matters most at
the moment of launch, so it must exist before the feature is exposed, but it is the last
thing to build.

**Independent Test**: With projects in existence, turn the feature off; confirm the entry
point is gone AND that a direct request for a known project is refused AND that a shared
chat link is refused; turn it back on and confirm every project, document, chat and link
is present and unchanged.

**Acceptance Scenarios**:

1. **Given** the feature is off, **When** any non-administrator uses the application,
   **Then** no entry point to projects is presented anywhere.
2. **Given** the feature is off, **When** a request for a known project is made directly,
   bypassing the interface, **Then** it is refused — hiding the entry point is not the
   enforcement.
3. **Given** the feature is off, **When** a previously shared chat link is opened,
   **Then** it is refused.
4. **Given** the feature is turned off and later on again, **When** a user returns,
   **Then** their projects, documents, chats and share links are all present and
   unchanged.
5. **Given** the feature is off, **When** a user asks a question in ordinary chat,
   **Then** ordinary chat behaves exactly as it did before this feature existed.

---

### Edge Cases

- **Empty project**: a project with no documents must say so plainly rather than
  answering from anywhere else. This is the single most likely way corpus leakage would
  be discovered late.
- **All documents removed mid-conversation**: a chat whose project has been emptied
  behaves as an empty project from the next turn, without corrupting the transcript.
- **Document still processing**: a document added but not yet searchable must be visibly
  pending; answers must not silently omit it as though it were absent.
- **Document failed to process**: shown as failed, not silently missing, so the user does
  not trust an answer drawn from an incomplete document set.
- **Copying a project whose documents are still processing**: the copy must reflect the
  same pending state rather than appearing complete.
- **Copying a project that is empty**: permitted, and produces an empty project.
- **Copying a project you already have a copy of**: produces a second, separate copy
  rather than merging into or overwriting the first.
- **Original owner deletes a document that a copy still contains**: the copy keeps
  working. A document, once shared, cannot be recalled from copies — see Risks.
- **Original owner's account is removed** while copies of their project exist: the copies
  keep working.
- **Sharing a chat whose project has since been emptied**: the transcript still reads,
  and citations still open, because the transcript records what was actually cited.
- **Taking a copy of a shared chat twice**: produces two separate copies, not a merge.
- **Feature disabled while a user is mid-conversation**: the next turn is refused
  cleanly; the transcript is retained for when it is re-enabled.
- **A shared-corpus document is withdrawn by the administrator** while a question is
  asked with the corpus ticked: it disappears from answers on the next turn, matching
  existing withdrawal behaviour.
- **Duplicate upload**: uploading a file already present in the project does not create a
  second copy or double its weight in retrieval.
- **Deleting a project that holds chats**: the user is told the chats will go with it and
  must confirm; documents held by other projects, including other users' copies, survive.
- **Storage allowance exhausted**: the upload is refused with a clear message naming the
  allowance; existing project documents keep working.
- **Very large project**: retrieval quality must not collapse as a project grows;
  behaviour at scale is a stated success criterion, not an assumption.

## Requirements *(mandatory)*

### Functional Requirements

**Projects and ownership**

- **FR-001**: Users MUST be able to create a project with a name of their choosing,
  rename it, and delete it.
- **FR-002**: Every project MUST have exactly one owner. A project MUST be visible and
  reachable only to its owner — sharing gives another user their own project (FR-020),
  never access to this one.
- **FR-003**: A new project MUST start with no documents — none of the shared corpus
  documents are members of it.
- **FR-004**: Users MUST be able to list their projects, ordered by most recent activity.

**Documents in a project**

- **FR-005**: Users MUST be able to add documents to a project by upload, and remove
  documents from a project.
- **FR-006**: A document MUST be able to belong to more than one project — including
  projects belonging to different users — and adding an already-processed document to a
  further project MUST NOT reprocess it.
- **FR-007**: Removing a document from a project MUST NOT affect its availability in any
  other project that still contains it, including another user's copy. Its content MUST
  be discarded only when it belongs to no project and no conversation anywhere.
- **FR-008**: The system MUST show each project document's processing state (pending,
  ready, failed) so the user knows whether the document set backing an answer is
  complete.
- **FR-009**: Documents a user uploads MUST count against that user's storage allowance,
  and MUST be refused with a clear message when it is exhausted. Documents received by
  copying another user's project MUST NOT count against the recipient's allowance, as no
  additional storage is consumed.

**Retrieval scoping — the security invariant**

- **FR-010**: When answering inside a project without the shared corpus ticked, the system
  MUST retrieve **only** from that project's documents. No shared-corpus document, no
  other project's document — including another copy of the same project — and no other
  user's document may be retrieved or cited.
- **FR-011**: FR-010 MUST hold identically for chat answering, global search, and opening
  a citation — all three must be governed by the same scoping decision, so that no route
  can widen the scope.
- **FR-012**: A project containing no ready documents MUST report that it has nothing to
  search rather than answering from any other source.
- **FR-013**: Scope MUST be determined by the server from the requesting user's identity
  and the project being addressed. A scope asserted by the client MUST NOT be trusted to
  widen what is retrievable.

**Including the shared corpus in a question**

- **FR-014**: Users MUST be able to include the shared legal corpus in a single question,
  chosen at the point of asking, beside the message they are about to send.
- **FR-015**: With the corpus included, retrieval MUST cover the project's documents plus
  the shared corpus, and MUST NOT cover any other project or any other user's documents.
- **FR-016**: When an answer draws on both, each citation MUST make plain whether it is
  the user's own document or a shared-corpus document, so a reader never mistakes an
  uploaded draft for enacted law.
- **FR-017**: The choice MUST apply to that question only and MUST return to excluded for
  the next question. It MUST NOT be remembered across turns, chats, or visits.
- **FR-018**: A question asked without the corpus MUST carry no shared-corpus material
  forward from an earlier turn that included it — neither retrieved passages nor
  conversation history standing in for them.
- **FR-019**: An answer that drew on the shared corpus MUST remain identifiable as such
  when the chat is reopened later, so a reader of the transcript can tell which answers
  were project-only.

**Sharing a project — copy, not shared access**

- **FR-020**: Users MUST be able to share a project with another user of the deployment.
  Sharing MUST give the recipient a **new project of their own** containing the same
  documents, of which they are the owner.
- **FR-021**: A copy MUST be fully independent of its original. Changes to either — adding
  or removing documents, renaming, deleting — MUST have no effect on the other, in either
  direction and at any later time.
- **FR-022**: Copying a project MUST NOT reprocess its documents; they MUST be searchable
  in the copy without the delay of a fresh upload.
- **FR-023**: Copying a project MUST NOT copy its chats. The recipient starts with an
  empty conversation list.
- **FR-024**: A document that is deleted by the original owner MUST remain available in
  every copy that contains it. The system MUST make this consequence plain to the user at
  the moment of sharing, since it means a shared document cannot afterwards be recalled.
- **FR-025**: Retrieval in a copy MUST be scoped to that copy alone and MUST NOT reach the
  original project or any other copy (a specific case of FR-010, called out because it is
  the one an implementation is most likely to get wrong).

**Sharing a chat — link to view, copy to continue**

- **FR-026**: Users MUST be able to share one of their chats and receive a link to it, and
  MUST be able to stop sharing it afterwards.
- **FR-027**: A shared chat link MUST be openable only by a user signed in to Noviq with
  their own credentials. It MUST expose nothing to anyone outside Noviq, whether signed
  out or unauthenticated.
- **FR-028**: A user opening a shared chat MUST be able to read the full transcript and
  open each citation to the source passage it came from.
- **FR-029**: A user opening a shared chat MUST NOT be able to ask new questions in it,
  and MUST NOT thereby gain access to the rest of the owner's project.
- **FR-030**: A user opening a shared chat MUST be able to take their own copy — their own
  project containing the chat's documents, and their own copy of the conversation — and
  continue from there under FR-020 and FR-021.
- **FR-031**: When an owner stops sharing a chat, the link MUST stop working for everyone.
  Copies already taken MUST continue to work, since they are independent projects.
- **FR-032**: Sharing a chat MUST tell the user plainly that recipients will be able to
  read the passages cited in it, since that is how a project's documents most easily reach
  someone the owner did not choose individually.

**Chats inside a project**

- **FR-033**: A project MUST hold many chats. Users MUST be able to start a new chat in a
  project, list the project's chats, and reopen any of them with its full transcript and
  citations.
- **FR-034**: A chat MUST belong to at most one project. Chats started outside a project
  MUST remain outside every project.
- **FR-035**: A project's chats MUST follow the user's login rather than the device,
  consistent with existing chat history behaviour.
- **FR-036**: Deleting a project MUST delete its chats, after warning the user, while
  leaving documents held by other projects — including other users' copies — intact.

**Administrative gate**

- **FR-037**: An administrator MUST be able to turn My Projects on or off for the whole
  deployment with a single setting.
- **FR-038**: When off, the system MUST refuse every project operation at the server, for
  every user, regardless of how the request is made, including opening a previously shared
  chat link. Removing the entry point from the interface MUST NOT be the only enforcement.
- **FR-039**: Turning the feature off MUST NOT delete any project, document, chat,
  transcript or share link; turning it back on MUST restore access to all of them
  unchanged.
- **FR-040**: When off, ordinary chat, global search and the shared corpus MUST behave
  exactly as they did before this feature existed.

**Privacy disclosure**

- **FR-041**: The system MUST state plainly, where the user adds documents to a project,
  that project documents are handled exactly as any other uploaded document — including
  being sent outside the deployment for processing where the deployment is configured
  that way. A workspace named "My Projects" MUST NOT be allowed to imply a
  confidentiality guarantee that is not made.

### Key Entities

- **Project**: A named container owned by exactly one user. Has a name, an owner, a
  creation time and a last-activity time. Holds document memberships and chats. Never
  visible to any other user; sharing produces a second, independent project.
- **Project document membership**: The fact that a given document belongs to a given
  project, with the time it was added. Many-to-many and cross-user: one document may be a
  member of several projects belonging to several users, and removing one membership
  leaves the others standing. The document's content survives while any membership
  remains.
- **Project chat**: A conversation that belongs to a project. Carries the transcript and
  citations already recorded for conversations today; the new facts are which project it
  belongs to and whether it is currently shared.
- **Chat share link**: The fact that a chat is currently readable by signed-in users who
  hold its link. Revocable by the owner; independent of any copy already taken from it.
- **Feature availability setting**: A single deployment-wide value recording whether My
  Projects is on or off, readable by the server on every project request.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a project with the corpus not ticked, **100%** of citations across a
  50-question test set resolve to documents in that project — zero shared-corpus, zero
  other-project and zero cross-user passages, measured over chat answers, global search
  results and citation opening alike.
- **SC-002**: A user with documents in hand can create a project, add them, and get a
  first answer in under **5 minutes**, without help.
- **SC-003**: Adding an already-processed document to a second project — or receiving a
  copied project — makes it searchable in under **5 seconds**, versus the minutes a fresh
  upload takes.
- **SC-004**: Removing a document from one project leaves it answering questions in every
  other project that contains it, including other users' copies, in **100%** of removal
  cases.
- **SC-005**: Across a test set of changes made to an original project after copying —
  additions, removals, renames, deletions — **0** are observable in the copy, and vice
  versa.
- **SC-006**: **100%** of attempts to open a shared chat link while not signed in to
  Noviq are refused.
- **SC-007**: **0** questions can be asked in a chat opened via a share link by a user
  who is not its owner.
- **SC-008**: With the feature turned off, **every** attempted project operation is
  refused, including shared chat links and requests that bypass the interface entirely.
- **SC-009**: Turning the feature off and on again returns **100%** of projects,
  documents, chats and links unchanged.
- **SC-010**: Answer quality in a project of **50** documents is no worse than in a
  project of 5, measured on a fixed question set — scoping must not degrade as a project
  grows.
- **SC-011**: With the corpus ticked, **100%** of citations are correctly labelled as
  either the user's own document or shared-corpus law, on a mixed-source test set.
- **SC-012**: Ordinary chat, global search and shared-corpus answers show **no measurable
  change** against their existing benchmark after this feature ships.

## Resolved Decisions

Decided by the architect on 2026-08-05. Recorded so they are not re-litigated.

| # | Question | Decision |
|---|---|---|
| 1 | Corpus access | Project-only, with the shared corpus includable per question. |
| 2 | Structure | A project is a container holding many chats, not one chat. |
| 3 | Reuse | Index once, link to many. Deletion is link-aware. |
| 4 | Admin disable | Hidden and unreachable, data retained, fully reversible. |
| 5 | Gate scope | Deployment-wide on/off only; per-user allowlist deferred. |
| 6 | Privacy promise | **Disclose, do not change behaviour** (FR-041). Project documents are handled exactly as uploads are today; the interface says so where documents are added. |
| 7 | Toggle granularity | **Per question, defaulting to excluded** (FR-014, FR-017). The control sits beside the message box and does not persist. |
| 8 | Project sharing | **Copy, not shared access** (FR-020, FR-021). The recipient gets their own project with the same documents and becomes its owner; the original is untouched. No permission levels, no concurrent editing. |
| 9 | Chat sharing | **Link to view, copy to continue** (FR-026 – FR-031). Signed-in Noviq users read the transcript and open citations; to go further they take their own copy. |
| 10 | Share recipients | **Signed-in Noviq users only.** Links never expose anything outside Noviq. |

## Assumptions

- **Copies do not consume the recipient's storage allowance**, since copying adds no
  stored bytes. Documents the recipient uploads themselves do.
- **Projects do not change how documents are processed.** A document added to a project is
  processed exactly as an uploaded document is today.
- **The shared corpus stays administrator-managed.** Users cannot add documents to the
  shared corpus through a project, and a project document never becomes shared corpus.
- **Withdrawal still wins.** A shared-corpus document the administrator has withdrawn stays
  out of project answers even when the corpus is ticked.
- **No per-user allowlist for the feature.** The gate is deployment-wide on or off.
- **Administrators do not read project contents.** The gate controls availability of the
  feature, not access to any user's documents.
- **A share link is not a secret in itself** — anyone holding it who is also signed in to
  Noviq may read the chat. Restricting a link to named recipients is not in scope.

## Dependencies

- Existing user accounts and sign-in, including the administrator role, which is what
  makes "signed-in Noviq users only" enforceable for share links.
- The existing document processing pipeline and per-user storage allowance.
- The existing single point through which chat retrieval, global search and citation
  opening decide what a user may see — this feature's isolation guarantee (FR-011)
  depends on that remaining the only such point.
- The existing per-user duplicate-content handling, which is what makes both "add once,
  use in many" and copy-on-share possible without reprocessing. Copy-on-share extends it
  across users for the first time.
- The existing account-bound chat history, which project chats extend rather than replace.
- The existing administrator settings mechanism used by comparable deployment-wide
  switches.

## Risks

1. **Retrieval scoping is the security invariant, and it now has more ways to fail.**
   Chat answering, global search and citation opening must all be governed by one
   decision (FR-011), and copies of the same project must not see each other (FR-025). A
   negative test is required, not optional: a project question must return zero
   shared-corpus and zero other-project passages.
2. **A shared document cannot be recalled.** Because content survives while any project
   still holds it (FR-007), a user who shares a project and later deletes a document has
   not removed it from the copy. This is correct for a copy model and wrong for what
   users will expect, so FR-024 requires saying so at the moment of sharing.
3. **Chat share links are the widest door.** A link plus a Noviq account grants reading of
   every passage cited in that chat (FR-028). This is what makes a shared answer checkable
   and is also how a project's documents reach people the owner never named — hence the
   disclosure in FR-032 and the revocation in FR-031.
4. **The name promises more than the system delivers.** "My Projects" reads as private,
   while an accepted decision permits documents to be sent to third-party services and
   masking covers only what the user types. Decision 6 is to disclose rather than change
   behaviour, which places the entire weight on the wording of FR-041 being clear.
5. **The admin gate must be enforced at the server** (FR-038). A hidden entry point is not
   a gate — a lesson already learned once on this system.

## Out of Scope

- Shared, concurrently editable projects, permission levels, or invitations. Sharing is
  copy-on-share; two users never edit one project.
- Keeping a copy in sync with its original, or any notion of an update propagating.
- Per-user or per-group enablement of the feature.
- Share links usable outside Noviq, or by anyone not signed in.
- Restricting a share link to named recipients, or expiring it automatically.
- Folder hierarchies, tags, or nesting inside a project.
- Moving an existing chat into a project after the fact.
- Per-project storage quotas separate from the per-user allowance.
- Users contributing documents to the shared corpus.
- Administrator visibility into the contents of users' projects.
