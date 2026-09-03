---
id: 0010
title: F026 Part 1 — deep-search checkpoint / resume / time-travel
stage: green
date: 2026-07-15
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (chat)
labels: [backend, deep-search, persistence, checkpointing, resume, time-travel, migration]
links:
  spec:
  ticket:
  adr: (suggested) stateful-deep-search-checkpointing
  pr:
files:
  - src-tauri/src/db/pg/migrations/014_deep_checkpoint.sql
  - src-tauri/src/services/deep_search/state.rs
  - src-tauri/src/services/deep_search/checkpoint.rs
  - src-tauri/src/services/deep_search/mod.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - CHANGELOG_DEV.md
tests:
  - "checkpoint.rs: 2 repo unit tests (roundtrip/ordering/next_seq; load-by-id/isolation/delete) — VM compile pending"
  - "existing deep_search unit tests unchanged (types, bounds, sanitize, enumeration)"
---

## Prompt

"built part 1" (after the F026 design: Part 1 = LangGraph-style checkpoint / resume / time-travel / persistence
for the agentic deep-search loop).

## Response snapshot

Grounded it in the migration runner (`sqlx::migrate!("./src/db/pg/migrations")`) and the `one_ask_session`
state_json repo, then built the durable-state foundation and refactored the loop into a resumable driver.

- **Migration `014_deep_checkpoint.sql`** — additive table `deep_checkpoint(id, thread_id, seq, node,
  parent_id, state_json, status, created_at)` + `ix_deep_ckpt_thread`. `parent_id` = the time-travel branch DAG.
- **`state.rs`** — `Node` as an ORDERED completion marker (`state.node < Node::X` means "phase X hasn't run"),
  and a serializable `DeepState` (inputs + subs + passages + coverage + pending + partial answer + trace).
- **`checkpoint.rs`** — the repository (append / latest / load / history / next_seq / delete_thread) + an
  `ECHO_DEEP_CHECKPOINT` flag (default OFF, A/B-safe). 2 repo tests.
- **`mod.rs`** — serde on the loop's types; refactored `run_agentic_deep` into `drive`, where each super-step
  is guarded by `state.node < Node::X` and checkpoints (best-effort) after it. The answer + citations are
  rebuilt deterministically from `state.passages` (`build_material`, shared by synth + finalize), so a turn
  resumed at Synthesized/Done reproduces the same result with no extra model call. Added `resume_deep` (from
  latest, auto-wired at the top of `run_agentic_deep` when the flag is on) and `resume_from` (time-travel,
  branches via next_seq + parent_id). Cancellation now checkpoints `interrupted` → a stopped turn is resumable.
- **`guidance.rs`** — passes a durable `thread_id = conversation_id + ':' + workload_id`.

## Outcome

- ✅ Impact: deep turns become durable + resumable + branchable without a graph runtime — one additive table +
  the existing repo pattern. Flag OFF = byte-identical to today. When ON, an interrupted 135s turn resumes from
  the last node instead of re-embedding every round.
- 🧪 Tests: 2 new repo tests; existing deep_search tests untouched. `test_pool` runs `init_schema` →
  `sqlx::migrate!`, so 014 applies in tests. VM compile pending (Windows can't do the CUDA/native build locally).
- 📁 Files: 014_deep_checkpoint.sql, deep_search/{state,checkpoint,mod}.rs, guidance.rs, CHANGELOG_DEV.md.
- 🔁 Next prompts: VM `cargo test --lib --features llm,server deep_search` → CUDA build → `ECHO_DEEP_CHECKPOINT=1`
  → smoke (interrupt a deep turn, confirm resume continues from the last node, citations identical). Then wire a
  stable FE turn-id for cross-process reconnect-resume, and an admin "↩ resume from node" for time-travel. PHR
  after deploy; ADR `/sp.adr stateful-deep-search-checkpointing`.
- 🧠 Reflection: made `Node` an ordered enum so resume is just "skip phases already done" — no separate resume
  code path, the same `drive` handles fresh + resumed by construction. Rebuilding citations from persisted
  passages (rather than storing them) keeps a resumed answer identical to the straight-through one.

## Evaluation notes (flywheel)

- Failure modes observed: none yet (not compiled locally). Watch on the VM: borrow-checker on the field-level
  splits in `drive` (state.subs/pending immutable vs state.passages mutable) and the `sqlx::migrate!` checksum.
- Graders run and results (PASS/FAIL): pending VM compile + the 2 repo tests.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): flip `ECHO_DEEP_CHECKPOINT=1` on a throwaway thread and assert
  `deep_checkpoint` gets ≤6 rows/turn with monotonic seq and a done-status tail row.
