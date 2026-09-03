---
id: 078
title: Notice Assist implementation plan
stage: plan
date: 2026-07-10
surface: agent
model: claude-opus-4-8
feature: 022-notice-assist
branch: 022-notice-assist
user: Riaz Mehmood
command: sp.plan
labels: [notice-assist, plan, qwen3-8b, slot-filling, postgres-migration, drafting]
links:
  spec: specs/022-notice-assist/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/022-notice-assist/plan.md
  - specs/022-notice-assist/research.md
  - specs/022-notice-assist/data-model.md
  - specs/022-notice-assist/contracts/notice-assist-commands.md
  - specs/022-notice-assist/contracts/extraction-json.md
  - specs/022-notice-assist/quickstart.md
tests:
  - "n/a (plan stage) — test entry points enumerated in quickstart.md"
---

## Prompt

proceed to /sp.plan

## Response snapshot

Ran the plan workflow on a new `022-notice-assist` branch. Produced plan.md, research.md,
data-model.md, contracts/ (notice-assist-commands.md, extraction-json.md), quickstart.md; ran
update-agent-context.sh (CLAUDE.md Active Technologies updated).

Key design decisions:
- NoticeAssist is a **capability** intercepted in `run_turn` (like transform/additive); NO new
  HTTP endpoint — flows through existing `send_message`/`/api/query`. `/help` token or an open
  session routes the turn.
- **Local Qwen3-8B** (single model slot via the inference/vLLM seam, `NOVIQ_NO_THINK=1`,
  free-text JSON, tolerant parse, no llguidance) does Layer-2 extraction; NOT the Mistral worker.
  Pinned to local via `NOVIQ_NOTICE_EXTRACT_ENGINE=local` so the sensitive notice never routes to
  a remote chat engine.
- Universal `Frame` (allegations/rule/authority/relief/forum/deadline + model-derived
  per-allegation questions) is the only predefined structure; slot-filling loop asks only gaps;
  resumable via additive `notice_assist_session(conversation_id PK, state_json, updated_at)` (one-ask
  seam). Grounding uses the corpus (`seeks_legal_authority`, R7) with verbatim citations; templates
  for show-cause and audit-para; export via `export_draft`; F020 masking.
- Frontend: `/` command palette in `ConfidentialComposer` mirroring the shipped `@` typeahead.

Constitution Check: PASS. Divergences logged in Complexity Tracking are all pre-accepted
(§IV SQLite→Postgres F011/F012; §II/§III worker-model → reuse resident Qwen3-8B which BETTER
honors single-slot; stack `llama-cpp-2`→vLLM F016). Corpus rulebook coverage (E&D/audit/RoB/CS
Act) flagged as a GA gate, not a build blocker.

## Outcome

- ✅ Impact: full implementation plan + Phase-0/1 artifacts; ready for `/sp.tasks`.
- 🧪 Tests: none (plan); entry points listed (extraction tolerant-parse, slot-loop, template,
  grounding gap-flag, `is_notice_assist_command`).
- 📁 Files: plan.md, research.md, data-model.md, contracts/×2, quickstart.md (+ CLAUDE.md context).
- 🔁 Next prompts: `/sp.tasks`; confirm command naming + corpus coverage; consider ADR.
- 🧠 Reflection: reusing the resident Qwen3-8B (not a demand-loaded worker) turned a potential
  Principle-III violation into a compliance win; the capability-intercept keeps the wire unchanged.

## Evaluation notes (flywheel)

- Failure modes observed: extraction reliability without llguidance → tolerant parse + retry-once
  + single-question fallback.
- Graders run and results (PASS/FAIL): n/a (plan).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): prototype the Qwen extraction prompt on 3–5 real
  notices/paras; measure JSON-parse success + allegation recall before UI work.
