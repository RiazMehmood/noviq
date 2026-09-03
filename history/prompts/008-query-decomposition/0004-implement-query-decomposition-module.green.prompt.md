---
id: 0004
title: implement query decomposition module
stage: green
date: 2026-06-07
surface: agent
model: claude-opus-4-8
feature: 008-query-decomposition
branch: 008-query-decomposition
user: Riaz Mehmood
command: /sp.implement
labels: [green, query-decomposition, intent-router, capability-gate, tdd]
links:
  spec: specs/008-query-decomposition/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/intent_router/decompose.rs
  - src-tauri/src/services/intent_router/mod.rs
  - CHANGELOG_DEV.md
  - specs/008-query-decomposition/tasks.md
tests:
  - "cargo test -p echo-ai --lib intent_router — 31/31 pass (10 new decompose tests)"
---

## Prompt

Architect: "apply remediation then run /sp.implement" (after /sp.analyze surfaced I1
HIGH + several MEDIUM). Remediation applied to plan/research/contracts/spec/data-model/
tasks, then implementation executed.

## Response snapshot

Implemented Feature 008 in one new module + one dispatch hook.

`intent_router/decompose.rs` (NEW): `decomposition_enabled` (gate: ECHO_DECOMPOSE
override → size≤1.5B from discover_local_gguf label → hardware fallback), `detect_and_split`
(enumeration `(a)(b)(c)` via marker regex with stem+facet & trailing-instruction strip;
multi-article via slots.section_paths>=2 with clause-split + per-article template fallback;
enumeration precedence; <2 or >MAX_SUBQUESTIONS=6 → None), `merge_subanswers` (labelled
blocks, global [n] renumber, synthesize marker when model emits none, not-found verbatim/
uncited). Types: Capability/SubQuestion/SubAnswer/MergedAnswer. Inline `#[cfg(test)]` with
10 tests.

`intent_router/mod.rs::dispatch`: moved scope_ids up; inserted the gated decomposition
block before the single-pass system_prompt build — computes model_size from
discover_local_gguf label, gates, splits, loops route_scoped_with_override SEQUENTIALLY
(cancel.clone()/app.clone(); cancelled → standard cancelled ChatResponse), each sub-question
gets a fresh Intent::Lookup system prompt, merges, maps citations via qr_citation_to_chat,
returns. Single-fact / gate-off path unchanged.

Tests: `cargo test -p echo-ai --lib intent_router` = 31/31 (10 new + 21 prior regressions).
Clean compile (dispatch hook included). I1 remediation honoured — size read from the
discover_local_gguf label, parse_param_size_b (private) not called.

TDD note: given ~minutes-per-build on the NTFS target, tests + impl were authored together
and verified GREEN in one run rather than watching each RED individually; tests are real
and comprehensive (gate matrix, merge renumber/synthesize/not-found, enum split+boundaries,
multi-article split+fallback, single-fact no-split).

## Outcome

- ✅ Impact: code-complete for US1–US4; single-fact path untouched; gated to small models.
- 🧪 Tests: 31/31 intent_router (10 new decompose). T016 gate covered by unit matrix.
- 📁 Files: decompose.rs (new), mod.rs (hook), CHANGELOG_DEV.md, tasks.md (T001-16,18,19 [X]).
- 🔁 Next prompts: MANUAL — T017 (in-app gate check small vs ECHO_DECOMPOSE=0) + T020
  (rebuild `npm run tauri dev`, re-run 15-prompt eval V4; verify Q4/Q9/Q15 flip, single-fact
  unchanged, big-model bypass). Then commit + consider /sp.adr for the gating decision.
- 🧠 Reflection: per-sub-question fresh Lookup prompt is the lever — it gives the 0.5B a
  clean single-provision binding, which is exactly where it scores well.

## Evaluation notes (flywheel)

- Failure modes guarded by tests: single-marker / sub-clause false trigger; citation [n]
  collisions; markerless block; not-found uncited; gate on big/GPU.
- Graders run and results (PASS/FAIL): unit PASS 31/31. Acceptance (15-prompt V4) PENDING.
- Prompt variant: each sub-question uses Intent::Lookup system prompt.
- Next experiment (smallest change to try): run T020 eval; if Q15 sub-answers still drop
  the article number, add a metadata-driven number fast-path for the enumeration case.
</content>
