---
id: 076
title: Composer document scope — checkboxes, @-mention, attachment-aware retrieval
stage: green
date: 2026-07-10
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: Riaz Mehmood
command: general
labels: [retrieval, scoping, attachments, frontend, backend]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/services/assistant/intent.rs
  - src-tauri/src/db/repositories/sources.rs
  - src-tauri/src/api/query.rs
  - src-tauri/src/commands/assistant.rs
  - src/api/assistant.ts
  - src/components/ConfidentialComposer.tsx
  - src/components/DocumentTree.tsx
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib -- assistant::intent (attachment_only_excludes_law_seeking_questions, detects_attachment_references) — pending VM run"
  - "cargo test --lib -- assistant::tests::narrow_to_selection — pending VM run"
  - "npm run build (tsc + vite) — EXIT 0"
---

## Prompt

ok lets fix one by one, first attachment and document selection, as user says from
this so it knows it is from attached document or anything attached or given in the
chat, or user says about attached document, so it shows and points to attached
document, and one more thing i want to identify is that as user writes a document
name so it autocomplete full name of document and directly point to that document
only not from other corpus, and one another bug is that as we dont select any
document only gives attached document it is still searching into those chunk of
documents which are not selected as this check box selection and unselection is
not applying properly and not strick tiking to that specific document make it fix
also.

(mid-turn) one more thing to notice is that if a user attached a notification or
showcause or audit para document and asks which laws support me to answer these
question will this only search for attached document or whole corpus to answer
user query?

(mid-turn) can we use hyde approach to understand user queries? after user query
we use llm to understand user query and expand that accordingly?

## Response snapshot

Diagnosed three linked bugs and fixed the first two-and-a-half in one foundation:
1. Right-rail checkboxes were cosmetic — `<DocumentTree />` had no
   `onSelectionChange` handler and no request-path parameter carried a subset.
   Added `selected_source_ids` through `send_message`/`/api/query` → `run_turn`,
   intersected with the F021 `visible_source_ids` privacy set via new
   `narrow_to_selection()` (narrows only, never widens). Wired the tree callbacks
   in ChatPage; `sendMessage` now sends the subset only when not "all".
2. Attachment references ("summarize this document", "from this") now resolve to
   the conversation's live attachment via `intent::references_attachment()` +
   `sources::attachment_source_ids()`, scoping strictly to it. GATED by
   `intent::seeks_legal_authority()` so "which laws support this show-cause/audit
   para?" keeps the corpus in scope (attachment is premise; law is in corpus).
3. @-mention typeahead in `ConfidentialComposer` (documents/onMentions props):
   "@" opens a doc-name picker; picking scopes to that source only. Ids derived
   statelessly from the text. "Answering only from: <chips>" strip shows scope.

Frontend build passed (tsc+vite EXIT 0). Rust deferred to VM (no Windows
toolchain). HyDE deferred as a separate retrieval-quality follow-up.

## Outcome

- ✅ Impact: checkbox scoping now actually restricts retrieval; attachment
  references answer from the attached doc; named docs scope via @-mention; the
  attachment-vs-corpus ambiguity (law-seeking questions) is resolved correctly.
- 🧪 Tests: frontend build EXIT 0; new Rust unit tests written (intent gates +
  narrow_to_selection) — run on VM before deploy.
- 📁 Files: 5 Rust, 4 TS, + CHANGELOG.
- 🔁 Next prompts: VM `cargo check`/`cargo test`; then HyDE query-expansion design.
- 🧠 Reflection: one scope-plumbing foundation served all three bugs; the
  law-seeking gate was the non-obvious correctness point (strict-attachment would
  have broken "which laws apply to this notice").

## Evaluation notes (flywheel)

- Failure modes observed: strict attachment-only scoping would wrongly starve
  law-lookup questions that reference an attachment; fixed with a legal-authority
  gate.
- Graders run and results (PASS/FAIL): npm build PASS; cargo PENDING (VM).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): add HyDE — LLM rewrites/expands the
  query before embedding, feeding hybrid retrieval; A/B vs current.
