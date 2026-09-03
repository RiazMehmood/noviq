---
id: 065
title: Per-viewer visibility, admin private-hiding, empty-folder cleanup
stage: green
date: 2026-07-06
surface: agent
model: claude-fable-5
feature: 021-user-documents
branch: release
user: Riaz Mehmood
command: (conversational)
labels: [f021, sharing, visibility, admin, privacy, backend]
links:
  spec: specs/021-user-documents/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/db/repositories/folders.rs
  - src-tauri/src/api/documents.rs
  - src/pages/Admin.tsx
  - CHANGELOG_DEV.md
tests:
  - VM cargo check --features llm,server,cuda EXIT 0
  - live API verify: admin list_indexed_folders = global corpus only
  - user browser-verified: shared doc in tree, admin list clean
---

## Prompt

admin usage table works, but u used very light color fonts which are not visible at all use dark fonts
after flipping to shared now we can get answer from that document but document is not showing in the
list, and admin list also showing user name which recently added some document but currently they
removed all documents but it is still showing to the admin only their name without any document, so i
think it is not good admin experience and why admin can see all documents from user if they are private
and have no access to view inside them.
shared doc now shows in tree, admin list is clean
save progress upto now, we will start working from this point.

## Response snapshot

§2.5: admin usage table verified via API + browser; contrast fix `ed660ed9` (td cells
inherited near-white global body color on the light card → table text-noviq-ink).

§3 findings, user decisions (hide private from admin COMPLETELY; auto-delete empty
folders), fixes `ddf131d4`:
- list_for_owner: folders with ≥1 per-doc-shared source now listed for everyone
  (My-documents toggle flips sources, not folders — root cause of "answerable but not
  listed"); file_count viewer-scoped (own + shared, attachments excluded).
- list_indexed_folders: admin branch removed — admin uses list_for_owner too.
- list_sources_in_folder / list_folder_sources: rows filtered to own + shared for every
  viewer; closed hole where any authed user could enumerate private file names by folder id.
- delete_if_empty_upload_folder: drops folder row + __folder__ stub notebook when NO
  sources remain under the path (conservative: attachments share the uploads dir);
  called from delete_my_document.
- One-time prod cleanup: ayaz's ghost empty folder row deleted by hand.
Verified live: admin listing = shared global corpus only; DB shows both user docs
private (mehran owner = zarak). User-confirmed in browser.

## Outcome

- ✅ Impact: §3 sharing PASS; admin lists respect privacy; no ghost folders.
- 🧪 Tests: VM cargo check EXIT 0; live API + user browser verification.
- 📁 Files: folders.rs, documents.rs, Admin.tsx, CHANGELOG_DEV.md.
- 🔁 Next prompts: §4 attachments test (script given: notification.pdf KTA Rs 1,850,
  SO(T)-XKV-2214/2026; attach→chip ready no-refresh→cited in-conv→invisible
  elsewhere→Keep promotes→delete-chat cleanup→quota tick), §5 SQL, T030-T033, branch cleanup.
- 🧠 Reflection: per-doc visibility vs per-folder listing mismatch was predictable —
  when two grains of access control exist, every read path must filter at the finer grain.

## Evaluation notes (flywheel)

- Failure modes observed: unfiltered list_sources_in_folder (enumeration hole) found
  only because the listing bug forced reading every read path.
- Graders run and results (PASS/FAIL): cargo check PASS; user browser PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): §4 attachment flow end-to-end.
