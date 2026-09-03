---
id: 073
title: Senior-bureaucrat persona integration
stage: green
date: 2026-07-08
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: Riaz Mehmood
command: integrate this persona and also generate 5 complex queries
labels: [persona, prompt, grounding, 8B]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - src-tauri/config/personas/bureaucrat.toml
  - CHANGELOG_DEV.md
  - C:/Users/hp/Desktop/complex_queries.md
tests:
  - "VM cargo test --lib -- assistant::persona: 5 passed / 0 failed (TOML parses + guard assertions hold)"
---

## Prompt

integrate this persona and also generate 5 complex queries and save to desktop in complex_queries.md file
(persona = Desktop/persona.txt: senior, highly seasoned bureaucrat with decades of experience,
grounded in Constitution/Acts/rules/regulations/policies, aware of legislature/Cabinet/executive/judiciary.)

## Response snapshot

Integrated the user's senior-bureaucrat persona into the live config (`bureaucrat.toml`
`[persona].full` + `.compact`), grafting a grounding-leash paragraph so the richer "decades of
institutional knowledge" voice cannot leak parametric law: authority from retrieved context only,
cite every rule/number/authority verbatim, flag gaps rather than fill them, never cite an
out-of-context rule/judgment/precedent, and every analytical sentence must rest on a cited
provision (closes the citation-gate blind spot for citation-less analysis). `GROUNDING_SYSTEM_PROMPT`
v5.4 + capability fragments unchanged. Confirmed for the current 8B: use the full profile; the
leash matters more at 8B (more law to leak). Verified on VM (persona tests 5/5). Delivered
`Desktop/complex_queries.md` — 5 executive-grade multi-hop queries (procurement threshold+authority,
missing Finance concurrence audit, manual-vs-RoB conflict, BS-17 leave entitlement, re-appropriation
delegation ladder) + a scoring rubric.

## Outcome

- ✅ Impact: assistant voice is now a senior advisor to executives, still evidence-first; TOML integrated + VM-verified.
- 🧪 Tests: assistant::persona 5/5 on VM (config valid + assertions hold). Not yet run against the live 8B.
- 📁 Files: 1 config edit + changelog + PHR + Desktop complex_queries.md.
- 🔁 Next prompts: run the 5 queries on the live 8B; check citation density ~1.5/answer and Analysis tethering; optionally add the executive answer-structure capability fragment; decide replace-vs-new-lawyer.toml if A/B wanted.
- 🧠 Reflection: the crucial move was NOT the persona text (user supplied it) but the grounding leash + analysis-tethering line — without them a richer, more capable-model persona degrades the anti-hallucination guarantees rather than improving answers.

## Evaluation notes (flywheel)

- Failure modes observed: none in unit test; live behavioural risk = advisor tone lowering citation density or emitting ungrounded analysis (mitigated by the analysis-tethering sentence; to be measured).
- Graders run and results (PASS/FAIL): persona unit tests PASS (5/5). No live eval yet.
- Prompt variant (if applicable): full + compact persona variants.
- Next experiment (smallest change to try): run Q1–Q5 from complex_queries.md on the 8B; if Analysis drifts ungrounded, tighten the leash further or add the structured fragment.
