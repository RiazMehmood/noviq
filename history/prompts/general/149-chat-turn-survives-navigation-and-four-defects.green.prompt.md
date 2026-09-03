---
id: 149
title: Chat turn survives navigation, plus four live-use defects
stage: green
date: 2026-08-11
surface: agent
model: claude-opus-5
feature: general
branch: 027-my-projects
user: Riaz Mehmood
command: (conversational)
labels: [chat, ux, f027, security, prompt, limits]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/stores/chat.ts (new)
  - src/pages/ChatPage.tsx
  - src/components/ConfidentialComposer.tsx
  - src/components/AddProjectDocuments.tsx
  - src/api/transport.ts
  - src-tauri/src/api/uploads.rs
  - src-tauri/src/api/query.rs
  - src-tauri/config/personas/bureaucrat.toml
  - CHANGELOG_DEV.md
tests:
  - "tsc --noEmit: PASS"
  - "vite build: PASS"
  - "cargo: NOT RUN — no Rust toolchain on Windows; VM compile check outstanding"
---

## Prompt

pending work?

as we run a query on chat during its running if we move to other tabs like my
projects or settings etc then return to chat it stopped working. this is not a
good ux i think

and i also want to limit the query characters, so user can not push the whole
book or write thousand of lines in query which bloats the context and returns
nothing, and there is also i found a problem during recent queries also, as user
sends queries it is showing web search options also which was not enabled by the
user and it is always suggesting next step as rule 10 always.

and one more thing i shared a document in my project directory, but it is
visible to all users why? and it is shown inside index documents

## Response snapshot

Reported pending work from the 2026-08-07 resume point, then diagnosed and fixed
five defects found in live use.

1. **Turn lost on navigation.** Every tab is a sibling route under `AppShell`, so
   leaving `/` unmounts `ChatPage`; the running turn lived in component signals,
   so the resolved answer wrote into a disposed scope, never reached the
   localStorage cache, and returning minted a blank chat. The server had
   persisted the turn all along. Fixed by hoisting the in-flight turn into a new
   module-level store (`src/stores/chat.ts`) — chosen by the architect over a
   cheaper rehydrate-on-return guard. Turn is kept running on navigation
   (architect's call). New `generatingHere()` gate keeps another conversation's
   turn from being painted or stopped from the wrong chat.
2. **Project document visible to all users.** Visibility is inherited from the
   indexed FOLDER; an admin's uploads folder is created ownerless, i.e. shared.
   Project uploads now get their own folder owned by the uploader. The server had
   always read a `projectId` upload field — the client never sent it.
3. **Web search stuck ON** after one click (documented as per-turn, never
   cleared). Severe because a web turn with nothing attached skips the corpus
   entirely.
4. **"Rule 10" on every answer** — the persona's offer fragment carried an
   illustrative question and the model recited it verbatim. Second occurrence of
   the same class of failure in that fragment; replaced with a schema and a
   warning comment.
5. **No query length cap** — added 4,000 chars, enforced server-side before the
   audit row and the generation permit, with a composer counter as the courtesy
   half.

## Outcome

- ✅ Impact: chat is usable across tab switches; a project document no longer
  leaks to every user; three answer-quality defects removed.
- 🧪 Tests: TypeScript typecheck + production build pass. Rust NOT compiled —
  flagged to the architect, VM compile check outstanding before deploy.
- 📁 Files: 1 new store, 4 frontend files, 2 Rust files, 1 persona config.
- 🔁 Next prompts: VM compile check + deploy; flip already-shared project
  documents back to private; hide empty "New chat" rows from `list_project_chats`.
- 🧠 Reflection: three of the five defects were STATED INTENT that no code
  enforced — a doc comment saying "per-turn", a persona comment warning against
  fixed examples, an upload path that read a field the client never sent. The
  comment is not the guard.

## Evaluation notes (flywheel)

- Failure modes observed: state scoped to a component that the router unmounts;
  ownership stamped at index time rather than at attach time; an LLM reciting an
  in-prompt example as content.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): `bureaucrat.toml` offer fragment, v3
  (v1 fixed menu → recited; v2 single example → recited; v3 schema only).
- Next experiment (smallest change to try): after deploy, sample a day of
  "Next steps" chips and count distinct provisions offered.
