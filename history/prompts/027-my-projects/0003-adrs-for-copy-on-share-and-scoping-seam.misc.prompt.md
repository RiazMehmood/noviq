---
id: 0003
title: ADRs for copy-on-share and scoping seam
stage: misc
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 027-my-projects
user: Riaz Mehmood
command: /sp.adr copy-on-share and cross-user content linking
labels: [my-projects, adr, copy-on-share, retrieval-scoping, supersedes-022]
links:
  spec: specs/027-my-projects/spec.md
  ticket: none
  adr: history/adr/0028-copy-on-share-and-cross-user-content-linking.md, history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md
  pr: none
files:
  - history/adr/0028-copy-on-share-and-cross-user-content-linking.md
  - history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md
  - specs/027-my-projects/plan.md
tests:
  - none (decision-record stage; no code changed)
---

## Prompt

/sp.adr copy-on-share and cross-user content linking

## Response snapshot

Extracted decision clusters from `specs/027-my-projects/plan.md` and its Phase 0/1
artifacts, tested each for significance, and wrote **two** ADRs — the one requested, plus
one more from the plan that passed all three tests independently.

### ADR-0028 — Copy-on-Share and Cross-User Content Linking (the requested one)

Sharing **copies rather than grants access**: the recipient gets a new project of their
own with the same documents, permanently independent in both directions. Copying is
metadata-only (one `project` row + N `project_source` rows; zero `sources` rows, chunks,
embeddings or files), which is what makes it complete in seconds and what makes
independence a property of the data model rather than of discipline.

The cluster holds together because each part forces the next: copy semantics force
cross-user `project_source` rows → which force membership rather than `owner_id` as the
retrieval authority → which forces `project_source` to carry **no** `owner_id` column,
unlike `source_link` where that same column is the security guard → which makes content
lifetime a union across three tables → which makes a shared document **unrecallable**.

⭐ This ADR **supersedes a decision recorded in code**. `022_source_links.sql:20-24` reads:
cross-user linking "would make one user inherit another's extraction and leak the fact
that the file was already in the system, and that needs a spec + ADR of its own." The
hazard it named is *implicit* dedup; copy-on-share is *explicit and owner-initiated*, so
nothing is disclosed the sharer did not choose. Alternative C exists in the ADR
specifically to record that distinction rather than let a future reader think the
restriction was simply dropped.

Four alternatives with rationale: shared access with permission levels (the conventional
model, rejected — it is strictly more machinery and lets one user damage another's work);
re-index per recipient (rejected — makes the <5 s target unreachable); implicit
cross-user dedup (rejected, and distinguished); copy with full file duplication (rejected,
though it was considered seriously because it would make deletion safe again).

### ADR-0029 — Project Retrieval Isolation via a Subtractive Scoping Seam

Written because it passed the significance test on its own, not to pad the output. It
records where scope is decided, who decides it, and what a share link may reach:
exclusive mode in `visible_source_ids` rather than a fourth additive branch; the server
**deriving** the project from `conversation_id` on a chat turn so the client never names
it; `owned_project` on the four routes that cannot derive it; share-link viewers
authorised per cited chunk rather than by widening the visible set.

Six alternatives, including the two most likely to be proposed later by someone in a
hurry — a fourth OR'd branch (structurally impossible: every existing branch adds, and
"and nothing else" cannot be expressed as an OR) and per-call-site filtering (six chances
to forget, each failing open).

Its honest weakness is recorded as a negative consequence rather than buried: four routes
carry a check that **can** be forgotten, and forgetting fails open. The chat path is safe
by construction; the other four are safe only by test.

### Clustering and conflict checks

- Considered merging the two into one ADR. Kept separate because they can diverge: drop
  sharing entirely and ADR-0029 still stands (exclusive mode would simply be free to use
  `owner_id`). They are coupled, not identical, and each is cross-referenced to the other
  at the exact point of coupling.
- Scanned all 22 ADRs on this branch. **No conflicts.** ADR-0015 (PostgreSQL as single
  source of truth) is the store these tables are added to; ADR-0008 (citation integrity)
  is strengthened by FR-016's provenance labelling; ADR-0020 and ADR-0021 are referenced
  as the scoped-route and gate-pattern precedents. The only supersession is the inline
  decision in migration 022, flagged explicitly in ADR-0028's References.

## Outcome

- ✅ Impact: Two Proposed ADRs written before implementation, one of them paying a debt
  the codebase recorded in a migration comment. `plan.md` updated to link them instead of
  recommending them.
- 🧪 Tests: None — decision-record stage, no code changed. Each ADR names the success
  criteria that must eventually serve as its evidence (SC-004/SC-005 for 0028;
  SC-001/006/007/008 for 0029).
- 📁 Files: `history/adr/0028-…md`, `history/adr/0029-…md`, `specs/027-my-projects/plan.md`
- 🔁 Next prompts: `/sp.tasks`.
- 🧠 Reflection: ⚠️ **`create-adr.sh` allocated colliding IDs.** Its `next_id()` scans only
  `history/adr/*.md` **in the working tree**, and ADRs 0023–0027 exist only on other
  branches — so it returned 0023 and 0024, both already taken. Renamed to 0028/0029 after
  confirming 0027 is the highest across all branches. Any future `/sp.adr` on a branch
  cut from `release-deploy` will hit this again; the script would need to consult
  `git log --all` to be correct.

## Evaluation notes (flywheel)

- Failure modes observed: **ID collision from `create-adr.sh`** (see Reflection) — caught
  before writing, by checking `git log --all` for 0023–0027 rather than trusting the
  script's allocation. The two failure modes this command warns about were both avoided:
  no over-granular ADR (two clusters, each spanning several components and files), and
  every alternative carries an explicit rationale (four in 0028, six in 0029).
- Graders run and results (PASS/FAIL): the command's own checklist — clustered not atomic
  **PASS**; at least one alternative with rationale **PASS** (4 and 6); pros and cons for
  chosen and alternatives **PASS**; concise but sufficient **PASS**.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): teach `create-adr.sh` to derive `next_id` from
  `git log --all --diff-filter=A --name-only -- 'history/adr/*'` as well as the working
  tree, so cross-branch ADR numbering stops colliding silently.
