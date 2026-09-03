---
id: 0001
title: specify one-ask drafting redesign
stage: spec
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 010-one-ask-drafting
branch: 010-one-ask-drafting
user: Riaz Mehmood
command: /sp.specify
labels: [spec, drafting, redesign, citation-grade, supersedes-009]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: history/adr/0012-model-assisted-template-ingestion-with-deterministic-verbatim-verify-gate.md (superseded path)
  pr: null
files:
  - specs/010-one-ask-drafting/spec.md
  - specs/010-one-ask-drafting/checklists/requirements.md
tests:
  - none (spec stage — acceptance scenarios defined in spec)
---

## Prompt

One-ask drafting: officer types a single natural-language sentence stating what they want (e.g. "I want to get 4 billion to develop schools/IT institutes for youngsters") and the app generates the complete MIDDLE CONTENT of a government summary — numbered paragraphs ready to copy-paste into official stationery. No letterhead, reference number, addressee, signature block, or CC section in the output (officers' official documents already contain those). This REPLACES the Feature 009 template/slot system entirely: no template upload, no slot schemas, no intent Q&A, no document-set curation ceremony. Gold summaries are retained only as style/format examples for the writer, not slot schemas. Pipeline: (1) parse the one-sentence intent (subject, amount/figures, action sought) deterministically or with the local model; (2) research pass — retrieve across the whole indexed corpus the governing provisions (e.g. PC-I process from Manual for Development Projects, sanctioning powers from Delegation of Financial Powers, routing from Rules of Business) as verbatim provision cards with their true numbers; (3) compose using a built-in generic Sindh summary skeleton (Background/Purpose → Legal & procedural position → Financial aspect → Proposal/Recommendation → Prayer for approval) — skeleton is hardcoded domain knowledge, not learned per case; (4) local Mistral-7B (already on disk, models/ir_builder/) writes connective prose ONLY, emitting [CITE:n] markers that reference provision cards — the model is never allowed to produce a rule number, quote, or figure itself; user-stated figures (the 4 billion) are injected verbatim from the parsed intent; (5) deterministic renderer splices verbatim provision text/numbers at markers; existing verbatim-verify gate checks every quote against source chunks; anything unverifiable is flagged inline, never invented — preserving the citation-grade 100%-accuracy contract; (6) officer can send follow-up turns to revise ("make para 3 shorter", "add the procurement angle") which re-run targeted composition, and then copies the final text. Generation time of minutes (local 7B on CPU) is acceptable — it replaces weeks of manual cross-document research. Non-goals: letterhead/boilerplate rendering, template ingestion UI, slot-level gap panels. Constraints: fully offline/local (no cloud), reuse existing retrieval (fastembed + sqlite-vec), citation resolution (decompose::resolve_provision_label), and chat persistence (chat_message table); fix the whole-corpus retrieval to use vec0 KNN MATCH/k form instead of the current 281-second brute-force scan.

(Origin: live officer test 2026-06-11 — ADP-funding intent rendered through the SPSC age-relaxation slot template produced 7/8 "Needs a source" cited slots + gibberish prose; user verdict: officers want ChatGPT-style one-ask → copy-paste middle content. User decisions via AskUserQuestion: prose engine = local Mistral-7B; template/slot system = replace entirely.)

## Response snapshot

Committed the green Phase D+E tree on 009 first (`14763d68`) so the pivot branch starts clean. Created branch `010-one-ask-drafting` (number 10 verified highest+1 across remote/local/specs). Wrote spec.md: 4 user stories (P1 one-sentence→full summary body, P1 refuse-or-flag, P2 conversational revision + restart persistence, P3 progress visibility/cancel), 16 FRs preserving the citation-grade contract (FR-005 writer restricted to connective narrative; FR-006 deterministic verbatim verify gate; FR-007 refuse-or-flag; FR-015 replaces template/slot officer path), 7 measurable SCs (≤15 min generation, ≤15 s research stage, 0 unflagged fabricated citations, 80% no structural rework), edge cases (vague ask, model emits numbers anyway, empty corpus, restart mid-generation), assumptions + out-of-scope. Technology bindings (Mistral-7B, vec0 KNN form) deliberately kept out of the spec for plan stage. Quality checklist created and all items pass; zero [NEEDS CLARIFICATION] markers (the three scope forks were already user-decided).

## Outcome

- ✅ Impact: Feature 010 specified — supersedes 009's slot-template architecture with one-ask drafting while carrying forward the 100%-accuracy citation contract.
- 🧪 Tests: none at spec stage; acceptance scenarios + SC-001..007 defined for downstream gates.
- 📁 Files: specs/010-one-ask-drafting/spec.md, specs/010-one-ask-drafting/checklists/requirements.md.
- 🔁 Next prompts: /sp.clarify (optional) → /sp.plan (bind Mistral-7B writer, [CITE:n] marker protocol, vec0 KNN research pass, skeleton composer, style examples) → /sp.tasks → /sp.implement.
- 🧠 Reflection: The live-test failure (template mismatch) was the decisive evidence; spec keeps all deterministic-verification machinery from 009 and discards only the structure-from-template idea.

## Evaluation notes (flywheel)

- Failure modes observed: previous architecture (per-case slot templates) failed on first off-template subject — structure must be generic, sourced from domain knowledge, not per-case ingestion.
- Graders run and results (PASS/FAIL): requirements.md checklist — PASS (all items).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): plan-stage spike — can Mistral-7B reliably emit [CITE:n] markers referencing provision cards in free-text mode (llguidance is dead per F006/spike crashes)?
