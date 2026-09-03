---
id: 160
title: Push everything to release, and give project chats their own example prompts
stage: general
date: 2026-08-17
surface: agent
model: claude-opus-5
feature: general
branch: deploy-idx
user: Riaz Mehmood
command: (conversational)
labels: [deploy, release, chat, projects, ux]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/pages/ChatPage.tsx (CORPUS_/PROJECT_EXAMPLE_QUERIES + exampleQueries(), empty-state line)
  - deploy/gpu-hold.sh, deploy/gpu-release.sh, deploy/gpu-index-auto.sh
  - docs/beta-launch/**, docs/*-analysis-2026-07.*
  - history/prompts/general/146,155,156,157
tests:
  - npx tsc --noEmit (PASS) x2
  - npm run build (PASS) x2
  - deploy 6bb5347a verified by binary mtime 21:19:18 + real vLLM generation
  - deploy e8b42941 verified by served bundle hash index-_SdwZCAW.js + health 200
---

## Prompt

we will commit next day, just for now save all things locally

(then) dont pull locally / push everything / and clear tree

(then) and in my project chat it is showing same my asking questions but they are
not related there as user will upload its own documents, so those questions must
be generic or general.

(then) dont user this type of questions summarize, inconsistent, and extracion of
dates and action items.

(then) simple questions only,

## Response snapshot

**Pushed and cleared.** Confirmed the two consequential points first — pushing to
`release` triggers the VM's ~2 min poll-deploy, and because
`src-tauri/sidecars/ingestion-sidecar/ingestion_sidecar.py` was modified it would
set `BE=1` and force a **full CUDA rebuild**, not just a bundle swap. Architect
chose `release` and "commit everything". Two commits (`6bb5347a`, `e8b42941`) on
top of `e40ca7d2`; working tree CLEAN.

⭐ **GitHub was reachable from Windows** — the standing "push from the VM over the
VPN" note is stale. Tested with `git ls-remote` rather than assuming.

Dropped `ops/corpus-2026-08-17/` (VM artifacts pulled down moments before the
"dont pull locally" instruction landed) rather than committing them.

**Project example prompts.** `ChatPage.tsx` is reused by the project route, so a
project — which searches the user's own uploads — offered three hardcoded Sindh
civil-service questions; the first click returned a refusal. Now branches on
`inProjectRoute()` (URL-derived, synchronous) rather than `inProject()` (server
answered, starts `false`, would swap the list under the reader). The empty-state
line branches too.

⭐⭐ **My first three project prompts were all broken and the architect caught
it.** I proposed *summarise* / *what's inconsistent* / *extract dates and
obligations* — the three most obvious offers, and every one unavailable:
SummarizeDoc and ActionItems sit behind `NOVIQ_MAPREDUCE` (OFF), and cross-doc
conflict/compare answers "not yet" by design. Shipped instead, per "simple
questions only": "What rules apply here?" · "Who is responsible?" · "What is the
procedure?"

## Outcome

- ✅ Impact: all pending work is on `release` and deployed; project chats no longer open with unanswerable corpus questions.
- 🧪 Tests: tsc + build clean on both changes; both deploys verified by behaviour (binary mtime, served bundle hash, health, real generation).
- 📁 Files: `src/pages/ChatPage.tsx` plus the batched ops scripts, docs and PHRs.
- 🔁 Next prompts: two-stage categoriser for the 270 residue; decide on 53 duplicate copies; `rm /opt/noviq/data/GPU_MODE_DISABLED`.
- 🧠 Reflection: I nearly shipped example prompts for three capabilities that are switched off. Suggesting an action is a claim that it works — the check is whether the feature is live, not whether the wording is good.

## Evaluation notes (flywheel)

- Failure modes observed: (a) proposed starter prompts without checking which capabilities are enabled; (b) treated a 38-day-old memory ("GitHub unreachable from Windows") as current — one `git ls-remote` disproved it; (c) acted on "save locally" by pulling VM files, immediately countermanded.
- Graders run and results (PASS/FAIL): tsc PASS ×2 · build PASS ×2 · deploy verification PASS ×2.
- Prompt variant (if applicable): project prompts v3 (v1 corpus-specific → v2 summarise/compare/extract → v3 simple retrieval lookups).
- Next experiment (smallest change to try): before offering any example prompt, grep the live env for the flag that gates the capability it exercises.
