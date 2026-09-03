---
id: 102
title: Auto-rebuild with no push and the false updating badge
stage: green
date: 2026-07-24
surface: agent
model: claude-opus-4-8
feature: general
branch: meeting/2026-07-18-business-model
user: Riaz Mehmood
command: (freeform question → plan mode → implement)
labels: [deploy, f014, systemd, banner, health, circuit-breaker]
links:
  spec: specs/014-server-thin-client/deployment-and-updates.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: deploy/check-release.sh
    change: rewritten — compare LAST_DEPLOYED_SHA (not working-tree HEAD) + skip a parked DEPLOY_FAILED_SHA
  - path: deploy/deploy.sh
    change: record TARGET_SHA as the attempt before building; trip breaker before rollback; clear it on success
  - path: src-tauri/src/api/health.rs
    change: sentinel_fresh()/sentinel_max_age()/deploy_in_progress() — age the DEPLOYING sentinel out by mtime; 3 unit tests
  - path: src/components/ServerStatusBanner.tsx
    change: Switch on serverStatus() — distinct copy for deploying vs unreachable
  - path: src/stores/serverStatus.ts
    change: non-2xx now reports unreachable, not deploying
  - path: deploy/test_deploy_rollback.sh
    change: cases 3-6 — breaker holds, HEAD drift alone does not trigger, a real push still does, success clears breaker
  - path: specs/014-server-thin-client/deployment-and-updates.md
    change: corrected the inlined trigger snippet that taught the looping logic
tests:
  - cmd: bash deploy/test_deploy_rollback.sh
    result: PASS=13 FAIL=0
  - cmd: npx tsc --noEmit
    result: clean (exit 0)
  - cmd: npx vitest run
    result: BLOCKED — missing jsdom dev-dep (pre-existing, unrelated)
  - cmd: cargo test (api::health)
    result: NOT RUN — no cargo/cmake on the Windows box; health.rs is UNVERIFIED by the compiler
---

## Prompt

`— why this is repeating in the whole document?` (prior turn: mojibake in the docx)

then, the request this PHR records:

`why application builds automatically we are neither pushing anything, it is showing badge of noviq is
updating on frontend? why? which thing causing this?`

## Response snapshot

Traced from the code, in plan mode, then implemented after approval. **Two independent defects that
present as one symptom.**

**1. The badge never meant "updating."** `ServerStatusBanner.tsx:14` rendered on
`serverStatus() !== "ok"`, so `unreachable` showed the deploy copy. `serverStatus.ts:34` additionally
mapped **any non-2xx → `deploying`** — and a dead-but-resolving Cloudflare quick tunnel returns 502, so
the false badge could persist forever with zero builds. Corroborated live: the tunnel URL recorded in
`Desktop\Noviq\Ops\ssh file.txt` now returns **NXDOMAIN** while `example.com` returns 200 — that URL is
stale, because quick-tunnel hostnames rotate on `cloudflared` restart.

**2. Builds genuinely fire with no push.** `noviq-deploy.timer` (`OnUnitActiveSec=2min`) →
`check-release.sh`, which compared the **working-tree HEAD** to `origin/release`. Any persistent drift is
unequal forever → deploy every tick. The drift is usually self-inflicted: `deploy.sh:109` rolls back with
`git reset --hard $PREV_COMMIT` on a failed health probe, pinning HEAD *behind* release, so the same
broken commit is rebuilt (full CUDA build) every 2 minutes, indefinitely. No breaker, no backoff.

**3. Also found, unprompted:** `health.rs:44` trusted mere *existence* of the `DEPLOYING` sentinel, and
its `trap cleanup EXIT` does not run on SIGKILL → an orphaned sentinel pins the banner AND makes the
FR-017 probe roll back a good binary.

Fixes: `LAST_DEPLOYED_SHA` as the comparison basis (written before the build, so a rollback cannot
re-arm the poll) + a `DEPLOY_FAILED_SHA` circuit breaker; mtime-based sentinel ageing
(`NOVIQ_DEPLOY_SENTINEL_MAX_AGE_MIN`, default 120 min); and honest banner copy per state.

⚠️ **Deviation from the approved plan, deliberate:** the plan said Fix D needed no store change. It did —
without touching the non-2xx branch, a 502-ing dead tunnel would still have shown "updating", which is
the architect's actual symptom. Flagged in the response rather than done silently.

## Outcome

- ✅ Impact: the 2-minute rebuild loop cannot re-arm from a rolled-back or manually-moved HEAD; a failed
  commit is never auto-retried; an orphaned sentinel expires; and the banner stops claiming a deploy that
  is not happening.
- 🧪 Tests: `deploy/test_deploy_rollback.sh` **13 passed / 0 failed** (6 new). Critically **case 5**
  proves a genuine push STILL deploys — the risk of the trigger change. `tsc --noEmit` clean.
  **`health.rs` is not compiled** (no toolchain locally) and `vitest` is blocked on a missing `jsdom`.
- 📁 Files: 7 (see frontmatter).
- 🔁 Next prompts: (1) Phase 1 VM diagnosis, still owed — VPN was down all session; (2) **copy the fixed
  scripts to the VM's OUT-OF-TREE `/opt/noviq/deploy.sh`** or the fix is inert; (3) compile + `cargo test`
  on the VM; (4) tune the sentinel max age from a measured build duration.
- 🧠 Reflection: **the badge was the only evidence, and it was untrustworthy.** The architect reasonably
  read "Noviq is updating" as proof of a rebuild; the UI had collapsed three distinct states into one
  reassuring sentence. This is the codebase's recurring failure mode again — *a number/state shown to a
  user must be derived from the thing itself, never re-estimated or approximated by a nearby signal.*

## Evaluation notes (flywheel)

- Failure modes observed: diagnosis had to proceed with **zero live access** (SSH timeout + NXDOMAIN
  tunnel). Code reading alone identified three plausible mechanisms but cannot say which one fired —
  recorded as unconfirmed rather than asserted.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): on the VM, `tail /var/log/noviq/deploy.log` — the stanza
  frequency alone separates "loop" from "stale sentinel" from "client-side unreachable" in one command.
