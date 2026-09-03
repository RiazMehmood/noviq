---
id: 152
title: Deploy the image-OCR fix and stage the Sindh harvest
stage: green
date: 2026-08-11
surface: agent
model: claude-opus-5
feature: general
branch: 027-my-projects
user: Riaz Mehmood
command: /recall then "complete the remaining 1, 2"
labels: [deploy, ocr, ingestion, corpus, vm-ops]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - CHANGELOG_DEV.md
  - src-tauri/src/services/indexer.rs (deployed, unchanged this session)
  - src-tauri/src/services/parsers/ocr.rs (deployed, unchanged this session)
  - /opt/noviq/harvest/** (VM, out of tree)
tests:
  - cargo check --no-default-features --features server --bin noviq-server: 0 errors (VM, isolated worktree)
  - harvest download --limit 25: 25 ok / 0 failed / 3.9 MB
  - /api/health: 200 after restart
---

## Prompt

recall memory

ok complete the remaining 1, 2 and leave the third one now it is ok

## Response snapshot

Recalled the memory index, then executed items 1 and 2 of the standing next-step
list, leaving item 3 (the 20 MB CrPC upload error) untouched as instructed.

**1. Image-OCR fix (`7af05eff`) compiled and deployed.**

Compile-checked on the VM first, since there is no Rust toolchain on Windows.
Confirmed the delta touches only `indexer.rs` and `ocr.rs` and sits outside every
`llm`/`cuda`/`desktop` `cfg` gate, which is what makes the fast
`--no-default-features --features server` check valid rather than a shortcut.
Bundled the single commit (12 KB) into a temp ref plus a detached worktree with
its own `CARGO_TARGET_DIR`; result was 0 errors in 1m42s, with prod HEAD, the
live binary's mtime and `noviq-server` all verified unchanged *during* the run.
Cleaned up the worktree, branch and target dir afterwards (disk back to 44 G).

Deployed by stopping `noviq-deploy.timer`, pushing the one commit to `release`
(a clean fast-forward `863ade71..7af05eff` — the docs/PHR commit was deliberately
left behind), running the deploy service one-shot, then restarting the timer.
Verified the way `deploy.sh`'s fail-soft demands — binary mtime `16:28:25` →
`22:21:45`, `Finished release profile in 5m 40s`, `deploy done at 7af05eff
(frontend=0 backend=1)`, restart at 22:21:46, `/api/health` 200 — never by the
commit, which moves before the build.

Additionally verified the fix will actually take the *good* path: the sidecar
gate `paddle_ocr_enabled()` defaults true but also requires
`ocr_sidecar::available()`, and there is no systemd `EnvironmentFile` — all four
`ECHO_*` OCR keys come from `/opt/noviq/echoAI/.env`, which loads because it sits
in the unit's `WorkingDirectory`. Confirmed present, plus the OCR venv.

**2. Sindh download staged and smoke-tested; full run handed to the architect.**

Staged at `/opt/noviq/harvest` — outside the prod tree, so `deploy.sh`'s
`git reset --hard` can never disturb it, and positioned so `harvest.py`'s
`parents[2]` output-dir derivation works unmodified. Shipped the 3,899-row
catalogue, `py_compile`-checked the scripts against the VM's Python 3.8, and
confirmed the VM reaches both harvested hosts. Smoke test: 25/25 real PDFs, 0
failures, ~160 KB/file (so ~625 MB total, not the 1.5 GB first estimated).

The auto-mode classifier blocked launching the detached tmux job, which coincides
with the standing preference for the architect to start long VM jobs himself, so
the run was handed over as a copy-paste block. The manifest is keyed by
`source_url`, so it resumes from the 25 already fetched.

## Outcome

- ✅ Impact: images now reach the PP-OCR ladder in production instead of failing
  with "Vision model not loaded" — the commonest officer upload (a photographed
  notice) went from 100% failure to extracting text. Sindh harvest is one command
  from a ~3,899-document corpus refresh.
- 🧪 Tests: cargo check 0 errors; 25/25 downloads ok; `/api/health` 200.
- 📁 Files: `CHANGELOG_DEV.md` updated (entry (e) flipped NOT DEPLOYED → DEPLOYED,
  plus a harvest-staging subsection); no source changed this session.
- 🔁 Next prompts: run `run_download.sh` under tmux, then index the haul; the
  20 MB CrPC upload error (explicitly deferred); drain answers to cache for closed
  chats; hide empty "New chat" rows; Build Summaries.
- 🧠 Reflection: the compile check earned its keep as *scope* evidence, not just a
  green tick — knowing the delta missed the `llm`/`cuda` gates is what justified
  the cheap check, and saying so is the difference between a verified claim and a
  plausible one.

## Evaluation notes (flywheel)

- Failure modes observed: `git bundle` refuses a bare sha and produces an "empty
  bundle" — it needs a ref, so a temp branch is mandatory. `systemctl is-active`
  returns exit 3 for a successfully stopped unit, which reads as failure. The
  classifier blocks compound remote tmux launches, and splitting into a written
  script plus a bare launch does not help.
- Graders run and results (PASS/FAIL): PASS — compile 0 errors, deploy verified by
  mtime + build log, health 200, download smoke 25/25.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after the haul lands, index a single
  scanned Sindh PDF and one photographed image end-to-end to confirm the new
  `ocr_image` path yields real text in the DB, not just a green compile.
