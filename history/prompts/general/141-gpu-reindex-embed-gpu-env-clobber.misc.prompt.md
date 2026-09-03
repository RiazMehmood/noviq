---
id: 141
title: GPU re-index CPU-embedded because .env clobbered the drop-in
stage: misc
date: 2026-08-04
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: "patch both copies"
labels: [ops, gpu-reindex, embeddings, env-precedence, canary]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - deploy/gpu-reindex.sh
  - CHANGELOG_DEV.md
  - /opt/noviq/gpu-reindex.sh (VM, out of tree)
tests:
  - "bash -n on the generated VM script: PASS"
  - "diff repo body vs live VM copy before patching: identical"
  - "diff after patching: exactly the two intended hunks, nothing else"
  - "post-install md5 matches the locally generated file"
---

## Prompt

reindexing done

...followed by, after the diagnosis was reported:

patch both copies

## Response snapshot

Checked the re-index before running the corpus gap check, and it had not
happened. `noviq-gpu-reindex.service` started 19:28:46 and failed at 19:33:54
with exit 1, aborting at the canary. One of nineteen sources was re-indexed
(the canary, APT Rules 1974, 194 chunks) and the server journal for that window
shows `Embedding model ready (gpu=false)` — it went through on the CPU.

Root cause, confirmed rather than inferred: `.env` line 31 is
`ECHO_EMBED_GPU=0`, and the app force-loads `.env` at startup, so it beats the
systemd drop-in where the script sets `ECHO_EMBED_GPU=1`. That is the same
precedence the script already documents and works around for `ORT_DYLIB_PATH`
three lines earlier; `ECHO_EMBED_GPU` simply never got the same treatment,
because the key was not in `.env` on 2026-07-29 when the last run succeeded.

Patched both copies:

1. §3 now `sed`s `ECHO_EMBED_GPU=1` into `.env` next to `ORT_DYLIB_PATH`
   (`.env.bak.gpu-reindex` already restores both, so `restore()` was untouched).
2. The canary now treats `Embedding model ready (gpu=false)` as a decisive
   FAIL — `embedder.rs:112` prints it unconditionally, so it settles in seconds
   instead of burning the full 300s ceiling with chat down.

Verified the repo copy and the live VM copy were byte-identical (repo = VM plus
a 26-line reference header) before editing, generated the VM-shaped file from
the patched repo copy, `bash -n`'d it, backed up the live script to
`gpu-reindex.sh.bak.2026-08-04`, installed, and confirmed the md5 matches.

## Outcome

- ✅ Impact: a GPU re-index will now actually embed on the GPU; a future
  regression of the same class fails in seconds instead of five minutes.
- 🧪 Tests: syntax check + before/after diffs + md5 equality (see front matter).
  Not exercised end to end — that requires an actual re-index run.
- 📁 Files: `deploy/gpu-reindex.sh`, `CHANGELOG_DEV.md`, `/opt/noviq/gpu-reindex.sh`.
- 🔁 Next prompts: fire the re-index (architect's call, takes chat down);
  then `scripts/corpus_gap_check.sql`; then Build Summaries; then re-run the
  finance eval.
- 🧠 Reflection: the canary was right both times it fired. The 2026-07-25
  incident it was built for was the *same* failure — a silent CPU fallback
  during a "GPU re-embed" — arriving by a different route.

## Evaluation notes (flywheel)

- Failure modes observed: a completed-looking operation that did nothing
  (exit 1 with a clean self-healing restore, so the system looked healthy);
  configuration precedence where the losing layer reports no error at all.
- Graders run and results (PASS/FAIL): n/a — ops change, no grader.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): audit anything else relying on a
  drop-in `Environment=` for a key that `.env` also defines — the override is
  dead the moment the key appears in `.env`, silently.
