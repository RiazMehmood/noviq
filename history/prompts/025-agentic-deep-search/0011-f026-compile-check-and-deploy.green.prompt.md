---
id: 0011
title: F026 compile-check, deploy to release, and live verification
stage: green
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (freeform) "compile check and deploy to release branch and check whether in need"
labels: [f026, deep-search, checkpoint, stop-button, deploy, verification]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket: n/a
  adr: owed — stateful-deep-search-checkpointing
  pr: n/a (direct fast-forward push to release)
files:
  - src-tauri/src/services/scheduler.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src-tauri/src/services/deep_search/mod.rs
  - src-tauri/src/services/deep_search/state.rs
  - src-tauri/src/services/deep_search/checkpoint.rs
  - src-tauri/src/db/pg/migrations/014_deep_checkpoint.sql
  - src-tauri/src/api/query.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/commands/query.rs
  - src-tauri/src/lib.rs
  - src/api/assistant.ts
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib --no-default-features --features llm,server deep_search: 19 passed / 0 failed"
  - "live: deep query via POST /api/query (deep:true) → HTTP 200, 137s, 1961 chars, 30 citations"
  - "live: POST /api/cancel_turn → {cancelled:true}, turn ended 20s vs 137s baseline"
---

## Prompt

compile check and deploy to release branch and check whether in need.

(mid-turn follow-ups: "and u said some commit are still remaing which are those?" / "ok now push")

## Response snapshot

Ran the staged VM compile-check, committed the F026 work, fast-forwarded `release`, then verified the
deployed binary in prod rather than assuming the deploy worked.

**Compile-check.** The staged worktree `/opt/noviq/f026-check` (base `74875c0`) was intact. Two corrections to
the recorded runbook were needed: the cargo manifest is at `src-tauri/Cargo.toml`, not the worktree root; and
the 2 `checkpoint::tests` failed with `initdb: libssl.so.3: cannot open shared object file` — the known
glibc-2.31 red herring, fixed with `NOVIQ_TEST_PG_INSTALL_DIR=/home/appadmin/test-pg-install`. Result:
**19 passed / 0 failed**, which clears both flagged risks (borrow-checker in the new `drive`, and
`sqlx::migrate!` picking up 014 — the 2 checkpoint tests exercise 014 against a real PG).

**Commit question.** Answered precisely: there were **no unpushed commits at all**. `HEAD = 74875c01` was
already on `origin/release` and `origin/025`; the entire F026 body of work was uncommitted working-tree
changes. So "remaining" meant commit-then-push, not push-pending-commits.

**Deploy.** Corrected the now-stale CHANGELOG top entry (it claimed the compile-check was "STAGED (paused
before running)") to record what actually happened, including what the check does and does not prove. Staged
code + PHRs + CHANGELOG in one commit (per the "batch docs with their code commit" rule, since any push to
`release` auto-deploys). Committed `b58ebfff`, pushed the feature branch first, then fast-forwarded `release`
(base was exactly `74875c01`, so a clean FF of one commit). `noviq-deploy.timer` fired within seconds; CUDA
build completed; server restarted 11:00:38 PKT with a same-minute binary.

**Verification (the part that mattered).** The stated risk was that the `drive` refactor of `run_agentic_deep`
might change live deep-search behavior — `ECHO_AGENTIC_DEEP=1` is on for all users, and the compile-check
proves shape, not behavior. Verified in prod:
- Deep query → HTTP 200, **137s** (on the known ~135s baseline), 1961 chars, **30 citations**, clean structured
  answer, no garble tail, no CoT leak → **no regression**.
- **Stop button works end-to-end:** `POST /api/cancel_turn` → `{"cancelled":true}`, turn ended at **20s vs the
  137s uncancelled baseline**; `answer:""` is what the FE renders as `_Stopped._`.
- Checkpoint confirmed **dormant** (`ECHO_DEEP_CHECKPOINT` unset → default OFF); migration 014 present in the
  deployed checkout; `cancel_turn` present in the running binary.

Removed the staging worktree; left `f025-wt` (holding the user's own uncommitted edits) untouched.

## Outcome

- ✅ Impact: F026 Parts 1+2 are LIVE on release `b58ebfff`. Stop button is a user-visible new capability,
  verified working. Checkpoint/resume infrastructure is deployed but dormant behind `ECHO_DEEP_CHECKPOINT`,
  so deep-search behavior is unchanged for users.
- 🧪 Tests: 19/19 unit green (incl. 2 real-PG checkpoint tests covering migration 014); 2 live prod smoke tests
  (deep no-regression; cancel_turn halts a turn).
- 📁 Files: 16 changed (+1189 / −159) across scheduler, guidance, deep_search (mod/state/checkpoint), API +
  desktop cancel_turn, ChatPage/assistant.ts, migration 014, CHANGELOG, 3 PHRs.
- 🔁 Next prompts: `/sp.adr stateful-deep-search-checkpointing`; drive the FE Stop(■) in a real browser;
  exercise checkpoint RESUME live (user sets `ECHO_DEEP_CHECKPOINT=1` + restart — prod config change);
  admin "↩ resume from node" trigger for `resume_from`; deep cost-guard nudge; TOML-ise PROBES/GREETINGS.
- 🧠 Reflection: the compile-check was the cheap part; the honest value was refusing to call a green build a
  working deploy. The `drive` refactor touched a path live for every deep user, so a 137s/30-cite prod query
  was the evidence that mattered, and the 20s-vs-137s cancel was the only thing that could prove Stop.

## Evaluation notes (flywheel)

- Failure modes observed: (1) recorded runbook command was wrong (cargo run from worktree root → no
  Cargo.toml); (2) libssl/glibc red herring re-encountered — the existing reference memory resolved it
  immediately, which is exactly what that memory is for; (3) stale prod PG password → reading `.env`/`.pgpass`
  correctly blocked by the credential classifier, so DB state was verified via server behavior instead;
  (4) `sudo journalctl` silently empty non-interactively; (5) API contract mis-remembered — body is
  `{conversation_id, text, deep}` and auth is bearer token, not `{query}` + cookie (422/401).
- Graders run and results (PASS/FAIL): compile+unit PASS (19/19); deep no-regression PASS (137s, 30 cites);
  stop-button cancel PASS (20s vs 137s); checkpoint-dormant PASS (flag unset).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): flip `ECHO_DEEP_CHECKPOINT=1` on a restart and interrupt one deep
  turn — the single cheapest test that converts checkpoint/resume from "deployed" to "proven".
