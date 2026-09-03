---
id: 084
title: Verify ADR-0019 counting, fix doc_type + attachment dup, deploy
stage: green
date: 2026-07-11
surface: agent
model: claude-opus-4-8
feature: adr0019-structured-counting
branch: release
user: engr.riaz2010@gmail.com
command: (recall memory and continue remaining work)
labels: [counting, doc-profiles, retrieval, deploy, data-cleanup]
links:
  spec:
  ticket:
  adr: history/adr/0019-structured-counting-and-verbatim-synthesis-retrieval.md
  pr:
files:
  - src-tauri/src/services/counting/mod.rs
  - src-tauri/src/services/assistant/capabilities/count.rs
  - src-tauri/src/services/doc_profiles/mod.rs
  - src-tauri/src/services/structured_importer.rs
  - src-tauri/src/services/assistant/{intent,mod,persona}.rs
  - src/api/assistant.ts
  - CHANGELOG_DEV.md
tests:
  - "cargo check --no-default-features --features llm,server --bin noviq-server: PASS"
  - "cargo test --lib counting:: assistant::intent:: doc_profiles:: : 57 passed / 0 failed"
---

## Prompt

recall memory and start working on remaining work
(followed by, over the session: "still confused by duplication... we only need one
single document at a time, must fix that"; "did you create the architecture diagram,
share the path"; "what we have done yet and what is still pending"; "first push to
release completed work and write memory in sequence. next session: Track 2, then Track
3, then attachment-dedup.")

## Response snapshot

Picked up ADR-0019 Track 1 (deterministic count/list), which was code-complete but
UNVERIFIED on `release`. Moved it to a side branch first (release auto-deploys),
reviewed all wiring against real code, then VM-verified: `cargo check` clean (the only
build blocker was VM cmake 3.16 < 3.18 for ggml-cuda in the check profile — dropped the
irrelevant `cuda` feature; deploy uses newer cmake) and 57 lib tests green.

Diagnosed two prod data bugs while verifying:
- doc_type auto-detection classifies by BODY-WORD frequency → SPPA-2009 (an Act) tagged
  `manual`; and "this Act" body signal is a trap (a rules Compendium says it 47×). Fix:
  title-first classification (`doc_type_from_title` + `detect_profile_titled`),
  authoritative over body scoring, tokenized on non-alnum (regex `\b` fails on `_Act_`).
- "Duplication" = same PDF attached to 3 chats = 3 `sources` rows (dedup keyed on
  file_path not content-hash; attachments carry empty file_hash). Fixed corpus counts to
  exclude `conversation_id IS NULL`; purged 2/3 stale rows; corrected 2 mislabeled rows.

Merged branch → `release` (ff `2f1fb170`), pushed → deploy rebuilt backend OK. Wrote
memory (GraphRAG audit updated; new attachment-content-dedup design captured) + this PHR.

## Outcome

- ✅ Impact: Deterministic count/list LIVE; corpus counts now accurate (rules 6 / act 4 /
  manual 3 / generic 1 = 14 library docs); prod dup + misclassification cleaned.
- 🧪 Tests: 57 lib tests pass; cargo check clean; deploy backend build succeeded.
- 📁 Files: counting module + count capability + title classifier + ingest call + wiring.
- 🔁 Next prompts: Track 2 (verbatim synthesis) → Track 3 (granular traces) →
  attachment content-dedup (spec+ADR first).
- 🧠 Reflection: verifying against prod caught the doc_type + dedup bugs the unit tests
  never could; title-first beats body-frequency and dodged the Compendium regression.

## Evaluation notes (flywheel)

- Failure modes observed: regex `\b` treats `_` as a word char (title tokenizer bug,
  caught by a real-filename test); VM cmake too old for ggml-cuda in check profile.
- Graders run and results (PASS/FAIL): cargo check PASS; 57 lib tests PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): deploy-verify a live "how many rules in the
  SPPRA Rules?" turn now that the count capability is on release.
