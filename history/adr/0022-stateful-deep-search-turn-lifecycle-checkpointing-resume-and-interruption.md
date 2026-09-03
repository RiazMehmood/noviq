# ADR-0022: Stateful Deep-Search Turn Lifecycle: Checkpointing, Resume and Interruption

> **Scope**: Document decision clusters, not individual technology choices. Group related decisions that work together (e.g., "Frontend Stack" not separate ADRs for framework, styling, deployment).

- **Status:** Accepted
- **Date:** 2026-07-16
- **Feature:** 025-agentic-deep-search (F026 work)
- **Context:** ADR-0021 replaced the one-pass Deep toggle with a **bounded agentic loop** (≤3 LLM calls, ≤2
  retrieval rounds). It works and is live in production (`ECHO_AGENTIC_DEEP=1`), but it holds **all state in
  memory and runs strictly linearly**, and a Deep turn measurably costs **~137s** (verified in prod: 137s,
  30 citations). Three consequences followed. (1) **Any interruption destroys everything** — a restart, a
  dropped tunnel, or a user cancelling at 130s throws away 3 LLM calls and 7+ CPU embeddings, and the only
  recovery is to re-run the whole turn from scratch. (2) **There was no way to stop a turn at all**: a user who
  saw the loop going down the wrong path had to wait out the full ~137s. (3) **No time-travel** — we could not
  re-run a turn forward from an earlier decision (e.g. "keep the decomposition, redo synthesis") to debug why a
  Deep answer went wrong, which is exactly the debugging loop this feature keeps needing (see the E&D
  wrong-domain and appeal-confusion defects). The user asked specifically for **LangGraph-style checkpoint /
  resume / time-travel / persistence** plus a **stop button**. The binding constraints: we are a single Rust
  binary with a bundled PostgreSQL (ADR-0015), we must not regress the normal path or the live Deep path, and
  we do not want to adopt a Python graph runtime or a distributed workflow engine for one bounded loop.

<!-- Significance checklist (ALL must be true to justify this ADR)
     1) Impact: YES — introduces a durable state model + a new persisted table + resume/branch semantics that
        change how the deep turn is written (node-guarded driver, not a linear fn). Long-term data-model impact.
     2) Alternatives: YES — adopt a graph runtime (LangGraph), event-sourced trace, workflow engine (Temporal),
        in-memory-only cache, or do nothing. Each considered with tradeoffs below.
     3) Scope: YES — cross-cutting: turn lifecycle, scheduler/cancellation, API surface, DB migration, frontend.
     All true. -->

## Decision

Model the bounded Deep loop as a **persisted graph with a durable, resumable, branchable turn lifecycle**,
built from our own primitives rather than an adopted runtime. The cluster:

- **State model** (`services/deep_search/state.rs`): an **ordered `Node` completion marker** —
  `Start < Expanded < RetrievedR1 < CoverageChecked < RetrievedR2 < Synthesized < Done` — so the resume
  predicate is a single comparison: `state.node < Node::X` means "phase X not done yet". Plus a serializable
  `DeepState` (inputs, sub-questions, passages, coverage, pending, partial answer, trace).
- **Persistence** (migration `014_deep_checkpoint.sql`, additive, new table only):
  `deep_checkpoint(id PK, thread_id, seq, node, parent_id, state_json, status, created_at)` +
  `ix_deep_ckpt_thread(thread_id, seq)`. **`parent_id` forms the branch DAG** that makes time-travel possible:
  re-running from an earlier checkpoint appends a new `seq` whose `parent_id` points at the node you branched
  from, so history is preserved rather than overwritten. `state_json` reuses the **`one_ask_session.state_json`
  convention** already established by ADR-0014/0013 — a known, working shape in this codebase.
- **Execution** (`services/deep_search/mod.rs`): `run_agentic_deep` refactored from a linear function into a
  **node-guarded driver (`drive`)**. Each super-step is wrapped in `state.node < Node::X` and writes a
  **best-effort** checkpoint on completion (a checkpoint write failure degrades to the old behavior; it never
  fails the turn). A resumed `DeepState` therefore skips completed phases.
- **Deterministic rebuild**: the final answer + citations are rebuilt from `state.passages` via a shared
  `build_material` (used by both synthesis and finalize), so **resuming at Synthesized/Done costs zero model
  calls**. This is what makes resume cheap rather than "re-run the expensive bit anyway".
- **Interruption as a first-class state**: cancellation writes an `interrupted` checkpoint, so **a stopped turn
  is a resumable turn**, not a discarded one. The stop path itself is **assembly, not new architecture** — the
  engine already had `Scheduler::request_cancel`, per-workload `CancellationToken`, and cooperative
  `cancelled()` checks at every round/generation boundary. Only two pieces were missing: a
  **conversation→workload map** on the Scheduler (`chat_turns`, with `clear_chat_turn` removing only if the id
  still matches, so a slow tail turn cannot silently disable Stop for a newer turn) and a trigger
  (`POST /api/cancel_turn` + desktop `cancel_turn` + a red Stop (■) in the composer).
- **Rollout**: gated behind **`ECHO_DEEP_CHECKPOINT`, default OFF**. Off = no checkpoint writes, straight-
  through execution. Deployed dormant on release `b58ebfff`.

**Explicitly NOT a full LangGraph runtime.** We adopt the *properties* (durable threads, resume, branching) via
one additive table plus an existing repository pattern — not a graph engine, scheduler, or DSL.

## Consequences

### Positive

- **Interruption stops being destructive.** A restart/disconnect/Stop no longer throws away ~137s of work;
  the turn resumes from its last completed node.
- **Stop is now possible and is fast.** Verified in prod: `cancel_turn` → `{"cancelled":true}`, turn ended at
  **20s vs the 137s uncancelled baseline** — cancelled at the next cooperative boundary.
- **Resume is cheap, not just possible** — rebuilding from `state.passages` means resuming late in the loop
  costs no LLM calls, which is the whole point given the LLM calls are the expensive part.
- **Time-travel gives us a real debugging loop** for the class of defect this feature keeps hitting (wrong-domain
  retrieval, appeal-confusion): branch from `Expanded`, redo synthesis, compare — without re-paying decomposition.
- **Cheap and reversible**: one additive table, one flag, zero new services, no new language runtime, no change
  to the normal path. Off by default = byte-identical behavior.
- **Consistent with the codebase**: reuses the `state_json` upsert convention and the cancellation-token
  registry pattern already proven in F006/F010.

### Negative

- **`state_json` is a schema-less blob.** Changing `DeepState`'s shape can strand old checkpoints; there is no
  versioning field today. Mitigation: checkpoints are ephemeral per-turn data, safe to discard — but a `version`
  column is the obvious follow-up before the flag goes on by default.
- **Write amplification on the hot path**: up to ~6 checkpoint writes per Deep turn. Negligible against a 137s
  turn and a loopback PG, but it is not free, and it is why writes are best-effort rather than transactional.
- **No TTL/housekeeping** — `deep_checkpoint` grows unbounded until a retention job exists.
- **~~`thread_id` is per-turn~~ — CORRECTED 2026-07-16 (this wording understated a real defect).** As first
  shipped, `thread_id = conversation_id + ':' + workload_id` where `workload_id` is a fresh UUID **per turn**,
  so `latest(thread_id)` never matched a prior attempt and **all resume was unreachable dead code** — not
  merely "limited to within a process". The repo unit tests passed because they call the repository directly
  with a fixed `thread_id`; they never exercised the real call path. **Fixed**: `deep_thread_id(conversation_id,
  query_text)` keys the thread on the normalized question, so re-asking an interrupted question resumes it; and
  `latest_resumable()` resumes **only** an `interrupted` latest row, so a completed thread can't replay its
  answer as an accidental cache. **Remaining limit (the original intent of this bullet):** resume is keyed to
  conversation+question, so it survives a re-ask but is not tied to a specific reconnecting turn; a page
  reload mid-turn does not itself resume.
- **Lesson recorded:** testing the repository is not testing the feature. A checkpoint store with green
  round-trip tests told us nothing about whether resume could ever fire.
- **`resume_from` (time-travel) has no UI trigger** yet; it is reachable in code only (admin "↩ resume from
  node" is the follow-up).
- **The driver is harder to read** than the old linear function: correctness now depends on every super-step
  being correctly node-guarded. A missing guard = silent recomputation or, worse, a skipped phase.
- **transform/mapreduce register their own Chat workloads** and are not in the conversation map, so a running
  *summarize* cannot be stopped yet (acceptable for v1: guidance is the long path).

## Alternatives Considered

- **Alternative A — Adopt LangGraph (or an equivalent graph runtime).** The literal ask. **Rejected**: it is a
  Python framework; our loop is Rust in a single Tauri/server binary (ADR-0015/0017). Adopting it would mean a
  Python sidecar, a second inference/orchestration surface, IPC, and a deployment story — for a loop that is
  **bounded at ≤3 calls and ≤2 rounds** and already written. We wanted LangGraph's *properties*, not its
  runtime. The properties reduce to: durable thread state, a resume predicate, and a parent pointer for
  branching — which is one table.
- **Alternative B — Event-sourced trace (append every event, fold to rebuild).** Attractive: we already emit a
  `DeepTrace`, and it gives perfect history. **Rejected for now**: replay requires a fold function that must
  stay in lockstep with the loop's semantics forever, and our super-steps are coarse (7 nodes, not hundreds of
  events). Snapshotting whole state at 7 coarse nodes is far simpler and the state is small. Revisit if the
  loop ever becomes fine-grained or concurrent.
- **Alternative C — A workflow engine (Temporal/durable-execution).** Gives durability, retries and history
  properly. **Rejected**: enormously disproportionate — an external service and operational surface for one
  bounded in-process loop, in a product that deliberately runs as a self-contained binary with a bundled DB.
- **Alternative D — In-memory checkpoints only (cache the state, no DB).** Would deliver resume-after-cancel
  within a process at near-zero cost. **Rejected**: it does not survive a restart or a redeploy, which is one
  of the three motivating failures, and it gives no time-travel history.
- **Alternative E — Do nothing / stop button only.** Ship cancellation; let cancelled work be discarded.
  **Rejected**: it addresses the symptom (can't stop) while leaving the cost (137s of discarded work) and gives
  no debugging loop. Notably, though, the stop button alone required *no* new architecture — which is why it
  was built first and why cancellation is clustered into this ADR rather than given its own.

## References

- Feature Spec: `specs/025-agentic-deep-search/spec.md`
- Implementation Plan: `specs/025-agentic-deep-search/plan.md`
- Related ADRs: **ADR-0021** (Bounded Agentic Multi-Pass Deep Retrieval — the loop this ADR makes stateful;
  extends, does not supersede); **ADR-0014** (Database Portability Seam) and **ADR-0013** (One-Ask Summary
  Composition — origin of the `state_json` convention reused here); **ADR-0015** (PostgreSQL as single source of
  truth — why migration 014 is additive PG); **ADR-0017** (Server Deployment — why an external runtime/engine
  was rejected).
- Evaluator Evidence: `history/prompts/025-agentic-deep-search/0011-f026-compile-check-and-deploy.green.prompt.md`
  — compile+unit **19/19 green** (the 2 `checkpoint::tests` exercise migration 014 against a real PG); prod
  smoke: Deep **not regressed** by the `drive` refactor (137s, 30 citations, clean answer) and **Stop verified**
  (20s vs 137s). Also PHRs `0009` (stop button) and `0010` (checkpoint/resume). Changelog entries 2026-07-15 /
  2026-07-16 in `CHANGELOG_DEV.md`.
  **Not yet evidenced:** resume itself has never been exercised live (flag off in prod) — the cheapest next
  experiment is `ECHO_DEEP_CHECKPOINT=1` + restart, interrupt one Deep turn, confirm it resumes from last node.
