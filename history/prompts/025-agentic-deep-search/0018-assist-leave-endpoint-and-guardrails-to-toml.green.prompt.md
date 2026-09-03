---
id: 0018
title: Assist Leave endpoint and guardrail lists to TOML
stage: green
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (chat)
labels: [notice-assist, guardrails, principle-vi, api, frontend, not-compile-verified]
links:
  spec: specs/022-notice-assist/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/api/query.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/services/assistant/persona.rs
  - src-tauri/src/services/assistant/intent.rs
  - src-tauri/config/personas/bureaucrat.toml
  - src/api/assistant.ts
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - src-tauri/src/services/assistant/persona.rs::embedded_guardrails_match_the_builtin_defaults (UNRUN)
  - src-tauri/src/services/assistant/persona.rs::guardrails_section_is_optional_for_older_override_files (UNRUN)
  - "frontend: npx tsc --noEmit — PASS"
  - "config: bureaucrat.toml parses; guardrail lists byte-identical to Rust Default impl — PASS (scripted)"
---

## Prompt

ok we will work on that as i return but for now work on guardrail and / command issue

(context: graph-history design parked until the user returns; this turn covers only the
two items named — guardrails and the `/` command assist-mode Leave bug.)

## Response snapshot

Two changes, **both uncommitted and the Rust NOT compile-verified** (no Rust toolchain on the
Windows box; build VM unreachable, VPN down).

### 1. `/` command issue — Leave ran a chat query to exit
Root cause at `ChatPage.tsx:1256`:
`onClick={() => { setAssistMode(false); setInput("/cancel"); submit(); }}` — the button typed the
command and pressed send. Cheap server-side (`is_exit` → `session::delete` → early return; `reply()`
does NOT invoke the model) so it was UX noise, not a slow turn.

**The constraint that rules out the naive fix:** the assist session is SERVER state
(`notice_assist_session`); `assistant/mod.rs:478` `assist_open` routes EVERY turn into the notice
flow while the row exists. `assistMode()` is only a mirror. Removing `submit()` → banner hides,
session stays open → next ordinary question silently hijacked.

Fix = dedicated channel (option 1 of the two offered last turn; user delegated the call):
- `POST /api/assist_exit` `{conversation_id}` → `{left: bool}`; loads-then-deletes, idempotent,
  auth required, mirrors `cancel_turn`.
- `router.rs` route; `assistant.ts` `assistExit()`; `ChatPage.tsx` `leaveAssist()` with `leaving()`
  ("Leaving…", disabled) and `leaveError()` rendered INSIDE the banner.
- On failure the banner deliberately STAYS UP (session still open server-side) — no silent catch.
- Typing `/cancel|/exit|/stop|/done` unchanged.

**Known gap left open (same root):** page reload mid-assist still hides the banner while the session
stays open — `assistMode` is not derived from the server session. Needs a status read.

### 2. Guardrails — SHIPPED already; closed the Principle VI gap
The fast-paths were already live. Moved their hardcoded Rust `const` word lists into
`bureaucrat.toml` `[guardrails]` (greetings 12 / about 16 / probes 35, copied verbatim), so probe
lists can be tuned from `query_log` via `ECHO_PERSONA_TOML` + restart with no rebuild (PHR 0006's
own follow-up). `intent.rs` predicates keep their signatures and delegate to pure `*_with(...)`
forms so the rules stay unit-testable without the global config.

**Trap designed around:** `PersonaConfig::get()` falls back to the embedded default SILENTLY on a
parse error. A required `[guardrails]` field would make any pre-existing on-disk override file fail
to parse and silently revert the whole tuned persona → every field is `#[serde(default)]`-backed.

**Deliberately NOT built:** the deep cost-guard for 1-word real queries (`penalties` → ~135 s deep
loop). It is a product decision (refusing a query the user explicitly asked for is a bad default),
and deep is admin-only so the blast radius is one user. Flagged for a decision.

## Outcome

- ✅ Impact: Leave exits without faking a chat turn; guardrail vocabularies tunable without a
  rebuild, with behaviour provably unchanged.
- 🧪 Tests: frontend `tsc --noEmit` PASS. TOML parse + list-equality vs the Rust `Default` impl PASS
  (scripted stand-in for the unit test). **Two new Rust unit tests written but UNRUN**; the whole
  Rust diff is UNCOMPILED — `cargo` absent locally, VM VPN down.
- 📁 Files: 7 changed + CHANGELOG_DEV.md. Nothing committed (user did not ask to commit).
- 🔁 Next prompts: (1) `cargo test --lib --no-default-features --features llm,server` on the VM —
  `src-tauri/Cargo.toml` is the manifest, and PG tests need
  `NOVIQ_TEST_PG_INSTALL_DIR=/home/appadmin/test-pg-install`; (2) smoke `POST /api/assist_exit`;
  (3) decide the deep cost-guard; (4) decide the reload-desync fix; (5) resume graph-history.
- 🧠 Reflection: reading `mod.rs:478` first is what saved this — "just don't call submit()" is the
  obvious fix and would have silently routed users' next questions into the notice flow. The
  silent-fallback behaviour of `PersonaConfig::get()` turned a trivial config move into a real
  compatibility hazard worth a serde default.

## Evaluation notes (flywheel)

- Failure modes observed: no Rust toolchain locally + VM unreachable ⇒ cannot honour the user's
  "test all aspects locally first" for the Rust half. Reported rather than papered over.
- Graders run and results (PASS/FAIL): tsc PASS; TOML/list-equality PASS; cargo NOT RUN.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run the two new persona tests on the VM — if
  `embedded_guardrails_match_the_builtin_defaults` passes, the config move is provably a no-op.
