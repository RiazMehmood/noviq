---
id: 0024
title: Implement Phase D E defect fixes
stage: green
date: 2026-06-10
surface: agent
model: claude-fable-5
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: riaz
command: /sp.implement
labels: [implement, phase-d, phase-e, ingestion-rewrite, semantic-gaps, officer-supplied, adr-0012]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket: null
  adr: history/adr/0012-model-assisted-template-ingestion-with-deterministic-verbatim-verify-gate.md
  pr: null
files:
  - src-tauri/src/db/migrations/024_slot_provision.sql (new)
  - src-tauri/src/services/drafting/ingest.rs (rewritten — gate + tolerant parse + prompt)
  - src-tauri/src/services/drafting/gaps.rs (rewritten — two-tier, injected coverage)
  - src-tauri/src/services/drafting/mod.rs (Gap/GapTier/GapResolution reshape)
  - src-tauri/src/services/drafting/slot.rs (provision_ref)
  - src-tauri/src/services/drafting/template.rs (provision_ref persistence)
  - src-tauri/src/services/drafting/render.rs (officer_supplied_fact + test)
  - src-tauri/src/services/drafting/session.rs (resolution round-trip test)
  - src-tauri/src/commands/drafting.rs (ingest_template reshape, semantic_cover_map, gaps_for, update_doc_set, resolve_gap, officer-supplied render path)
  - src-tauri/tests/lgd_ingest_gate.rs (new) + tests/fixtures/lgd_letter_fixture.md (new)
  - src/api/drafting.ts, src/components/drafting/{TemplateManager,GapPanel,DraftView}.tsx
  - src-tauri/examples/option_c_spike.rs (deleted)
  - CHANGELOG_DEV.md (two entries), specs/009-lgd-drafting/tasks.md (T064–T083 marked)
tests:
  - cargo test --lib drafting → 47/47 GREEN
  - cargo test --test lgd_ingest_gate → 2/2 GREEN
  - cargo test --test lgd_drafting_acceptance → 1/1 GREEN
  - npx tsc --noEmit → clean
---

## Prompt

/sp.implement (no args) — execute Phase D (Option-C ingestion rewrite, FR-010/010a/010b, migration 024) then Phase E (two-tier semantic gap coverage FR-019 + officer-supplied fact channel FR-008a) from the amended 009 tasks.md. Mid-run user messages: "how many total tasks we have for this and how many we completed?", "why it is taking too much time?", "why again and again it is showing no output…", "you are just attempting and attempting it, not diagnosing why it is taking too much time… be precise and accurate rather than just throwing different shell commands."

## Response snapshot

Checklists 16/16 PASS → loaded CHANGELOG_DEV + all drafting/commands sources → implemented both phases, then verified in batch.

**Phase D (T064–T073, T075 done; T074 automated half done):** migration 024 `provision_ref` + Slot field threaded through template persistence; `ingest.rs` rewritten — heuristic segmentation deleted, `build_segmentation_prompt` ([INST], spike-proven), `parse_model_slots` tolerant chain (fences/prose/control-chars/trailing commas; empty/unparseable = Err), `propose_schema_from_model` pure gate (verbatim text+provision, hyphen-tolerant provision regex, category-from-provision, boilerplate-can't-cite, flag-with-reason FR-010a); `ingest_template` reshaped (ModelSlot+tier, F006 GGUF discovery cfg-shimmed, 3 temp-escalating retries, refuse-not-degrade FR-010b, progress events); 7 unit tests + fixture + `lgd_ingest_gate.rs` acceptance (canned model output, no LLM); TemplateManager shows verified provisions + rejection reasons; spike file deleted.

**Phase E (T076–T081, T083 done; T082 automated half done):** `Gap` reshaped (`#[serde(flatten)] GapTier`: Suggestion/Missing/OfficerInput), `GapResolution` + AcceptedSuggestion/OfficerSupplied; `gaps.rs` rewritten pure with injected coverage closure (5 new tests incl. the live-defect case: content covers where title can't); `semantic_cover_map` in commands (embed category once, MIN cosine distance GROUP BY source, similarity threshold 0.45 via `ECHO_GAP_COVER_THRESHOLD`); `resolve_gap` acceptSuggestion (adopt+recompute+return) + officerSupplied (fact-slots-only guard); officer-supplied fact short-circuit in render_draft/revise_draft via `render::officer_supplied_fact` (verbatim, "officer-supplied" attribution); GapPanel three-tier UI + DraftView badge; session round-trip test.

**Verification:** lib drafting 47/47 + lgd_ingest_gate 2/2 + lgd_drafting_acceptance 1/1 + tsc clean. CHANGELOG: two dated entries. tasks.md: 18 tasks marked [X]; T074/T082 annotated (manual halves pending: real-gold ingest + live threshold calibration via the running app).

**Process incident (user feedback, saved to memory):** repeated probe commands during a silent build were the wrong response. Root cause diagnosed: piping cargo through tail/grep hides all output until completion, and a killed background cargo run left orphan rustc processes that held the build lock AND poisoned the shared target cache (wrong CARGO_HOME registry) → one-time full dep rebuild. Fixed pattern (now in feedback-diagnose-before-code §4): one build command → raw log file → one watcher → wait.

## Outcome

- ✅ Impact: Both live-pilot-blocking defects fixed and verified: ingestion is model-assisted + gate-verified (no more garbage slots / fake citation demands), gap detection is content-semantic and two-tier (suggestions instead of false gaps), fact slots take the officer-supplied channel.
- 🧪 Tests: 47 lib + 2 gate + 1 acceptance GREEN, tsc clean (one warm-cache batch after a poisoned-cache full rebuild).
- 📁 Files: 13 source files + migration + fixture + 2 test files; spike deleted; CHANGELOG + tasks.md updated.
- 🔁 Next prompts: manual T074b (ingest the real gold through the running app, ~18 min) and T082b (calibrate ECHO_GAP_COVER_THRESHOLD on the 8-PDF live corpus: "consultations" → RoB 1986 suggestion; record value in CHANGELOG). Then Phase B/C tasking.
- 🧠 Reflection: the verify-gate design made everything testable without an LLM — the only untested seam is the live model call, which the spike already proved.

## Evaluation notes (flywheel)

- Failure modes observed: orphaned cargo processes from killed background tasks (lock + cache poisoning); covers_from lifetime elision error (fixed with explicit 'a); lgd_conversation assumed as a test target (Phase A tests live in lib).
- Graders run and results (PASS/FAIL): all four suites PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): live-corpus threshold calibration (T082b).
