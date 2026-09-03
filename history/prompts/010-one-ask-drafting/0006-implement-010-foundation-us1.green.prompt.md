---
id: 0006
title: implement 010 foundation us1
stage: green
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 010-one-ask-drafting
branch: 010-one-ask-drafting
user: Riaz Mehmood
command: /sp.implement
labels: [green, implementation, knn-seam, pipeline, us1]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: history/adr/0013-one-ask-summary-composition-architecture.md, history/adr/0014-database-portability-seam-for-postgresql-re-platform.md
  pr: null
files:
  - src-tauri/src/services/drafting/retrieval.rs
  - src-tauri/src/services/drafting/one_ask/{mod,intent,skeleton,research,compose,verify,state}.rs
  - src-tauri/src/db/migrations/025_one_ask_session.sql
  - src-tauri/src/commands/one_ask.rs (+ commands/mod.rs, lib.rs registration)
  - src-tauri/src/services/intent_router/decompose.rs (provision_label → pub)
  - src-tauri/src/services/drafting/mod.rs (module wiring)
  - src-tauri/tests/one_ask_knn_parity.rs
  - src-tauri/tests/one_ask_acceptance.rs
  - src-tauri/tests/fixtures/one_ask/ (corpus.md + canned_writer/×7)
  - src/api/oneAsk.ts
  - src/components/drafting/OneAskView.tsx
  - src/pages/Drafting.tsx (rewired to one-ask, FR-015)
  - specs/010-one-ask-drafting/tasks.md (T001–T021 marked done)
  - CHANGELOG_DEV.md (dated entry)
tests:
  - cargo test --lib drafting → 70/70 (22 new one_ask units)
  - cargo test --test one_ask_knn_parity → 2/2 (R1 gate PASSED — MATCH/k ≡ brute-force cosine)
  - cargo test --test one_ask_acceptance → 3/3 (incl. malicious-writer gate proof)
  - cargo test --test lgd_drafting_acceptance 1/1 + lgd_ingest_gate 2/2 (009 regression clean)
  - npx tsc --noEmit → clean
---

## Prompt

Implement Feature 010 one-ask-drafting per specs/010-one-ask-drafting/tasks.md (38 tasks). Start with Phase 1 Setup + Phase 2 Foundational — T004/T005 knn_parity gates retrieval.rs and must come first within Phase 2; measure real KNN behavior before building dependents. TDD: RED before GREEN per task pairs. No live LLM in any cargo test (canned writer + stub embedder). Honor ADR-0014 seam rules. Use CARGO_TARGET_DIR=/media/riaz/New Volume/echo-AI-target and the established NTFS cargo env. Stop at the US1 checkpoint for review before continuing to US2.

## Response snapshot

T001–T021 complete (Phases 1–2 + all of US1), stopped at the US1 checkpoint as instructed. Highlights: (1) **knn_parity gate PASSED** — first `embedding MATCH ?/k=?` query in the codebase; order ≡ brute-force cosine on normalised vectors incl. indexed/watched-folder filtering; no fallback needed (sqlite-vec 0.1.9 supports k=?). (2) Full two-lane pipeline: regex intent (verbatim figures) → per-section KNN research → provision cards labelled via decompose::provision_label (made pub, T009) → SectionWriter trait (canned in tests, SlotWriter/Mistral-7B in prod) with GLOBAL [CITE:n]/[FIG:n] marker ids (caught+fixed a per-section/global numbering mismatch before tests) → verify.rs splice + verbatim gate + stray-number post-scan; §XI fast-tier disclosure rendering. (3) Acceptance proves the contract: malicious canned output ("Article 99", "Rs. 9 crore") surfaces ONLY as Flag runs. (4) Migration 025 additive, ANSI ON CONFLICT; state tolerates future fields. (5) Commands: start_one_ask (instant return, spawned pipeline, progress/complete events, cancel registry), cancel/get (interrupted → retryable failed). (6) Frontend OneAskView + Drafting.tsx rewired; 009 ceremony removed from routing (FR-015). Deviations noted honestly: RED/GREEN written as same-batch pairs (tests validated against deliberately-wrong inputs — the malicious fixture — rather than strict fail-first); no RAM-tier hard gate (ingest_template precedent; dev machine reports max_model=3 yet runs 7B — T034 revisits).

## Outcome

- ✅ Impact: US1 end-to-end implemented and regression-clean; the 281 s scan path is replaced in the drafting flow by the KNN seam (live timing measured at T035).
- 🧪 Tests: 70 lib + 2 parity + 3 acceptance + 009 suite green; tsc clean; zero live-LLM tests.
- 📁 Files: 14 new, 5 edited (see files list).
- 🔁 Next prompts: review checkpoint → continue /sp.implement US2 (T022–T025), then US3/US4, T034–T038 polish + live gates.
- 🧠 Reflection: the global-vs-per-section marker numbering bug was caught by writing verify and compose against the same data-model invariants — cross-module invariant lists in data-model.md earn their keep.

## Evaluation notes (flywheel)

- Failure modes observed: marker-id scope mismatch (fixed pre-test); rust-analyzer unlinked-file noise until module wiring landed.
- Graders run and results (PASS/FAIL): knn_parity PASS ×3 query seeds; acceptance invariants PASS; 009 regression PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): T035 live run — measure research-stage wall-clock on the 65 GB DB; if vec0 KNN cold-init dominates (~40 s warm-up exists in lib.rs), consider extending the existing warm-up query to the drafting path.
