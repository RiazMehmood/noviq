---
id: 134
title: Responsive layout across all surfaces
stage: green
date: 2026-07-31
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (freeform)
labels: [frontend, responsive, mobile, tailwind, solidjs, admin]
links:
  spec: N/A
  ticket: N/A
  adr: N/A
  pr: N/A
files:
  - src/lib/breakpoints.ts (new)
  - src/App.tsx
  - src/index.css
  - src/pages/ChatPage.tsx
  - src/pages/Admin.tsx
  - src/pages/SearchPage.tsx
  - src/pages/SettingsPage.tsx
  - src/components/ComingSoon.tsx
  - src/components/LoginScreen.tsx
  - src/components/DeepSearchNetwork.tsx
  - CHANGELOG_DEV.md
tests:
  - "npx tsc --noEmit: PASS (exit 0)"
  - "npx vite build: PASS (clean; pre-existing tauri dynamic-import warning only)"
  - "built-CSS cascade order (.lg\\:translate-x-0 after .-translate-x-full): PASS"
  - "built-CSS .stack-table rules inside (max-width: 767.98px): PASS"
  - "data-label <-> <th> alignment on the 10-column user table: PASS (9 of 9, 2 opt-outs correct)"
  - "browser render at 390/768/1366: NOT RUN - no dev proxy to /api, needs VPN + prod login"
---

## Prompt

which paddle ocr we are currently using? is it onnx?

still we dont have responsive layout for mobile and tablet and for laptop.

save progress upto now, as we will start work on that next

continue

## Response snapshot

**OCR question** — answered from code. RapidOCR (PP-OCR det/rec models on ONNX
Runtime) is what runs on the deploy VM; `_ocr_backend()`
(`ingestion_sidecar.py:608`) prefers it whenever importable and never probes
`paddle` first, because that import SIGILLs on the AVX-less vCPU. `_ocr_paddleocr`
(3.x) is retained for AVX/GPU hosts. Flagged that `setup-ocr-venv.sh` still
installs only paddle, so a venv rebuilt from it would get the fatal runtime.

**Responsive work** — audit found 6 responsive utilities in 10,269 lines; the root
`h-screen overflow-hidden` meant overflow was *clipped and unreachable*, not just
cramped. Scope confirmed with the architect: all surfaces incl. Admin, down to
390px. Read `CHANGELOG_DEV.md` first per CLAUDE.md and preserved the 2026-07-02
rail semantics (left = sessions, right = documents).

Four parts delivered: (1) `src/lib/breakpoints.ts` + nav hamburger + `.app-viewport`
(`100vh`→`100dvh`); (2) ChatPage rails as slide-over drawers below `lg`, with the
positioning done purely by Tailwind cascade order and JS driving only the
transform; (3) Search/Settings/Login/ComingSoon (= Workflow + Meeting) + the deep
search canvas; (4) Admin — scrolling tab bar, and a new CSS `.stack-table` that
turns the 10-column user table into labelled cards below `md` without duplicating
the row markup into a second JSX branch.

Skipped `KnowledgeGraphPage`, `knowledge-graph/*` and `MeetingPanel` as dead code
(imported nowhere since Workflow replaced that tab), and `OverlayWindow` as a
Tauri-sized popup in a vestigial desktop path — reported rather than silently
narrowed.

## Outcome

- ✅ Impact: every reachable surface now lays out from 390px up. Nothing deployed.
- 🧪 Tests: tsc + build clean; three targeted static verifications of the CSS the
  layout depends on (cascade order, stack-table emission, label/header alignment).
  **No browser render at any width** — `/api` is same-origin with no dev proxy, so
  reaching ChatPage needs the VPN + a prod login.
- 📁 Files: 1 new, 9 modified.
- 🔁 Next prompts: browser-verify at 390/768/1366 before this rides a deploy;
  optionally extend `.tap-target` to older Admin/Settings icon buttons.
- 🧠 Reflection: the audit mattered more than the fix — "not responsive" sounded
  like missing breakpoints, but the clipping root was a worse and different bug.
  Second lesson: a failed grep is not evidence of absence. The check that appeared
  to prove the `lg:` utilities were never emitted was a quoting artifact (CSS
  escapes `:` as `\:`); re-running with fixed strings showed all of them present.
  Nearly acted on a false negative.

## Evaluation notes (flywheel)

- Failure modes observed: (1) the grep quoting false-negative above, caught by
  re-testing with `grep -F` before changing anything; (2) `ConfidentialComposer.tsx`
  contains a raw NUL byte at line 32, so ripgrep classifies it binary and skips it
  — a repo-wide sweep silently missed 565 lines and the file had to be read by
  hand. Reported, not fixed (one-character `\0` change, architect's call).
- Graders run and results (PASS/FAIL): tsc PASS, vite build PASS, 3 CSS assertions
  PASS, browser N/A.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): add a vite dev proxy to `/api` so the
  frontend can be exercised against a running backend without a prod login —
  would have converted this whole task from static analysis to observation.
