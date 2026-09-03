---
id: 0013
title: Gate deep search to admins only (config-flagged)
stage: green
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (freeform) "disable deep search for users now only for admins so we can test it smooth … discuss"
labels: [deep-search, gating, rbac, config-flag, rollout]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a (direct fast-forward push to release, a1e17bb6)
files:
  - src-tauri/src/services/deep_search/mod.rs
  - src-tauri/src/api/auth.rs
  - src-tauri/src/api/query.rs
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib --features llm,server deep_search: 20 passed / 0 failed (new: deep_audience_gate)"
  - "tsc --noEmit: clean"
  - "live: admin /me deep_search_enabled=true; temp non-admin /me=false"
  - "live: non-admin crafted deep:true → 44s / 4 cites (normal), NOT 137s / 30 (deep) → downgrade confirmed"
---

## Prompt

disable deep search for users now only for admins so we can test it smooth and after complete testing and
smoothing we will deliver that to users discuss

(clarifications: "now we are client server model not a desktop app any more … dont say again for desktop app
again"; gate control = "Config flag (ECHO_DEEP_AUDIENCE)")

## Response snapshot

Discussed first (the request said "discuss"). Established the gate is already enforceable — `AuthUser { role }`
is in hand at the query handler and `isAdmin()` already exists on the client — so no identity plumbing was
needed. Surfaced two decisions: (1) hardcode admin-only vs a config flag, (2) desktop behaviour. The user chose
the **config flag** and corrected a standing assumption: **Noviq is a client-server product now, not a desktop
app** — recorded to memory ([[project_client-server-not-desktop]]) so desktop stops being raised as a concern.

Built a two-layer gate driven by one server-side source of truth:
- **`deep_search/mod.rs`**: `deep_audience_is_all()` reads `ECHO_DEEP_AUDIENCE` (all/everyone/public → open;
  else admin-only); `deep_enabled_for_role(role)` = admin OR audience-all. The pure decision is factored into
  `deep_enabled(role, audience_is_all)` so it's unit-testable without touching process env (env-var tests are
  racy under parallelism). New test `deep_audience_gate`.
- **`api/query.rs`** (the real gate): `deep = req.deep && deep_enabled_for_role(&user.role)` — a non-admin's
  `deep:true`, from a stale UI or a crafted request, is silently downgraded to a normal search on the line the
  flag is read. The UI toggle is never the enforcement point.
- **`api/auth.rs`**: `/me` advertises `MeResp.deep_search_enabled`, computed by the same helper — one source of
  truth the UI and the enforcement can't disagree on.
- **`ChatPage.tsx`**: `deepEnabled` signal from `me.deep_search_enabled`; the "Deep search mode" menu item is
  `<Show when={deepEnabled()}>`; `deepSearch` forced off for non-admins. Default false, so a `/me` fetch failure
  hides the toggle rather than leaking an unsmoothed feature.

Verified in prod (release `a1e17bb6`, `ECHO_DEEP_AUDIENCE` unset = admin-only): admin `/me` → `true`; a
freshly-created temp non-admin `/me` → `false`, and its crafted `deep:true` completed in **44s with 4
citations** — normal-search behaviour, not the ~137s / 30-citation deep path — proving the server-side
downgrade. Temp user disabled afterward (no hard-delete route exists).

## Outcome

- ✅ Impact: Deep search is now admin-only in prod while it's being smoothed, still exercised live. Public
  rollout is a config flip (`ECHO_DEEP_AUDIENCE=all` + restart), not a redeploy — and reversible.
- 🧪 Tests: 20/20 unit (new `deep_audience_gate`); tsc clean; live /me both roles + live downgrade proof.
- 📁 Files: deep_search/mod.rs (helpers+test), api/auth.rs (/me field), api/query.rs (enforcement),
  ChatPage.tsx (UI gate), CHANGELOG.
- 🔁 Next prompts: when ready to launch — `ECHO_DEEP_AUDIENCE=all` + restart, verify a normal user sees the
  toggle. Before that: the deep-search smoothing backlog (garble tail, appeal-confusion, resume dead-code).
- 🧠 Reflection: the win was refusing to treat a hidden button as the gate — the enforcement had to be on the
  server line that reads `deep`, and the same helper feeds `/me` so the UI can't drift from it. Verified the
  downgrade behaviourally (timing + citation count) rather than trusting the unit test.

## Evaluation notes (flywheel)

- Failure modes observed: `UID` is a readonly bash builtin — using it as a variable in an ssh one-liner
  silently kept it at 1000, so the first user-disable hit the wrong id and "succeeded" (204) while the account
  stayed active (login still 200). Caught it, redid with `TID`, confirmed 401. Lesson recorded to memory.
- Graders run and results (PASS/FAIL): unit PASS (20/20); /me admin PASS (true); /me non-admin PASS (false);
  downgrade PASS (44s/4 cites vs 137s/30).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): flip `ECHO_DEEP_AUDIENCE=all` on a restart and confirm a plain
  user's /me flips to true and the toggle appears — the one check that proves the rollout path before launch.
