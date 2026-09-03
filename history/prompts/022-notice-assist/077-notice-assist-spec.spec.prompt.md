---
id: 077
title: Notice Assist — /help officer-defense capability spec
stage: spec
date: 2026-07-10
surface: agent
model: claude-opus-4-8
feature: 022-notice-assist
branch: release
user: Riaz Mehmood
command: spec
labels: [notice-assist, officer-defense, slash-command, qwen3-8b, slot-filling, drafting]
links:
  spec: specs/022-notice-assist/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/022-notice-assist/spec.md
tests:
  - "n/a (spec stage) — acceptance checklist embedded in spec"
---

## Prompt

(spanning several turns) A simple mechanism: a `/` command (the slash twin of the `@`
document mention) like `/help` that intelligently understands the officer wants help
responding to a show-cause notice / warning / audit para, and gathers info from the user
intelligently. Concern raised: we cannot predefine fields because we don't know the document
type or structure — the intelligence must derive the fields. Decision: safeguard honest
officers who acted under political pressure (non-illegal intent) with accurate, cited,
procedurally-correct replies. Use the LOCAL model for extraction — specifically **Qwen3-8B**
(NOT Mistral 7B). "yes spec it out, use the local worker model … we need to use qwen 3 8b
local model."

## Response snapshot

Wrote `specs/022-notice-assist/spec.md`. Core design resolves the "can't predefine fields"
problem with TWO LAYERS: (1) a tiny universal *argument frame* every accountability document
shares (allegations, invoked rule/authority, relief sought, forum, deadline, officer facts,
cited rule text) — the only predefined structure; (2) the LOCAL **Qwen3-8B** model
(single model slot via the inference/vLLM seam, `NOVIQ_NO_THINK=1`, free-text JSON, no
llguidance — NOT the Mistral IR worker) reads the attached document and DERIVES the specific
allegations + the per-allegation questions to ask the officer. Intake is slot-filling
(ask only gaps; resumable via a new `notice_assist_session` state_json seam). Grounding uses
corpus retrieval (notice = premise) with verbatim cited rules; reply follows fixed templates
(show-cause / audit-para) + a documents-to-attach checklist; gaps flagged, never fabricated
(integrity leash + written-orders doctrine for the political-pressure case). 5 user stories,
12 FRs, reuse map onto @-mention/doc-scoping/one-ask/drafting/export/F020/F021.

## Outcome

- ✅ Impact: a headline officer-defense capability specced end-to-end, reusing existing
  machinery; resolves the dynamic-fields concern without predefining domain fields.
- 🧪 Tests: none (spec); acceptance checklist + per-story independent tests embedded.
- 📁 Files: specs/022-notice-assist/spec.md.
- 🔁 Next prompts: /sp.plan for the design (composer `/` palette, Qwen extraction prompt +
  tolerant JSON, session migration, templates); confirm corpus rulebook coverage.
- 🧠 Reflection: the key move was separating a fixed *argument frame* from LLM-*derived*
  content — intelligence where documents vary, structure where the argument is universal.

## Evaluation notes (flywheel)

- Failure modes observed: predefined-field intake can't handle unknown document types →
  solved via model-derived fields over a universal frame.
- Graders run and results (PASS/FAIL): n/a (spec).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): prototype the Qwen3-8B extraction prompt on 3–5
  real sample notices/paras; measure JSON-parse success + allegation-recall before building UI.
