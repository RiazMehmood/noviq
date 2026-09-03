---
id: 0005
title: agentic-deep-search-implement-phase1
stage: green
date: 2026-07-13
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: /sp.implement
labels: [deep-search, agentic-rag, implement, scaffold]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket:
  adr: history/adr/0021-bounded-agentic-multi-pass-deep-retrieval.md
  pr:
files:
  - src-tauri/src/services/mod.rs
  - src-tauri/src/services/deep_search/mod.rs
  - src-tauri/src/services/deep_search/expand.rs
  - src-tauri/src/services/deep_search/coverage.rs
  - src-tauri/src/services/deep_search/assemble.rs
  - src-tauri/src/services/deep_search/verify.rs
  - research/025-agentic-deep-search/README.md
  - research/025-agentic-deep-search/gold.jsonl
tests:
  - deep_search::tests::types_construct_and_coverage_compares — PASS on VM (lib built, 706 filtered out)
---

## Prompt

/sp.implement — execute the tasks.md for feature 025 (checklists PASS 16/16, auto-proceed).

## Response snapshot

Executed the safe foundational slice (Phase 1 + T007), being explicit that Rust can't be compiled locally
(no toolchain) so builds run on the VM.

**Done (T001, T002, T003, T007):**
- Registered `pub mod deep_search;` in `services/mod.rs`.
- `deep_search/mod.rs`: `agentic_enabled()` flag reader (`ECHO_AGENTIC_DEEP`, default OFF, mirrors
  `graph_expand_enabled()`); the in-memory data model (`SubQuestion`, `Coverage`, `CoverageItem`/
  `CoverageReport`, `Passage`, `PassageSet`, `DeepTrace`) per data-model.md; a compile-smoke unit test.
- `expand/coverage/assemble/verify.rs`: documented stubs citing their contracts (impl in US1/US2).
- `research/025-agentic-deep-search/`: A/B eval scaffold (README + placeholder gold.jsonl).

**Deliberately NOT done this turn (need VM build iteration; high-risk):** T004 (extract `retrieve_for_query`
from the hot `route_scoped_with_override`) + T005 guard, T008 coverage spike, and all US1–US4 impl. Writing the
orchestrator + hot-path refactor blind (uncompilable locally) would be irresponsible; these want read-then-write
against real signatures with VM `cargo test` in the loop.

**Verification (GREEN)**: committed to branch `025-agentic-deep-search` (`4f879f22`, no deploy — feature branch
only), pushed; created an isolated VM `git worktree` at `/tmp/f025` (release checkout untouched). Gotchas hit +
fixed: (1) `cargo` not on non-interactive PATH → `source ~/.cargo/env`; (2) a cold `--features cuda` build in a
fresh target dir fails on `llama-cpp-sys-2` because the VM's system `cmake` is 3.16.3 but `GGML_CUDA` needs
≥3.18 (prod deploy avoids this via its already-cached target). Dropped `cuda` → `--features llm,server` (my
module is std-only, no CUDA needed): **lib compiled in 2m41s, `types_construct_and_coverage_compares` PASSED,
706 other tests filtered out, only 8 pre-existing unrelated warnings.** Worktree + logs cleaned up afterward.

## Outcome

- ✅ Impact: Feature module exists + flag + data model; the rest of the loop has a clear, contract-backed home.
  Zero behavior change (nothing routes into it yet); prod safe.
- 🧪 Tests: one compile-smoke unit test; full VM compile+test in progress.
- 📁 Files: services/mod.rs + services/deep_search/*.rs + research/025-agentic-deep-search/*.
- 🔁 Next prompts: continue /sp.implement at T004 (extraction + T005 guard) and T008 (coverage spike) — the two
  highest-risk foundational items — iterating with VM `cargo test`. Then US1 T009–T018.
- 🧠 Reflection: Chose an honest incremental slice over blind-writing 30 tasks of unverifiable Rust into a
  complex hot function. Isolated VM worktree keeps the compile-check off the live release checkout.

## Evaluation notes (flywheel)

- Failure modes observed: `nohup` on a non-interactive ssh shell doesn't source `~/.cargo/env` → `cargo: not
  found`; fixed by sourcing it. Recorded for future VM builds.
- Graders run and results (PASS/FAIL): checklist gate PASS (16/16); compile result pending.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): once the compile is green, do T004 as a pure behavior-preserving
  extraction with T005's snapshot guard BEFORE any new logic, so the normal-path invariant is locked first.
