---
id: "0001"
title: "specify legal RAG pakistan"
stage: spec
date: 2026-06-02
surface: agent
model: claude-opus-4-7
feature: 007-legal-rag-pakistan
branch: 007-legal-rag-pakistan
user: Riaz Mehmood
command: /sp.specify
labels: [feature-007, legal-rag, pakistan, spec]
links:
  spec: specs/007-legal-rag-pakistan/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: specs/007-legal-rag-pakistan/spec.md
    change: created
  - path: specs/007-legal-rag-pakistan/checklists/requirements.md
    change: created
tests:
  - name: requirements quality checklist
    status: PASS
---

## Prompt

`/sp.specify` invoked with the following feature description:

> Pakistan government legal RAG product. Officers drop PDFs of Acts, Ordinances, Rules, Regulations, Policies, and the Constitution. Default indexing is fast (existing 004/005 pipeline, no LLM). On demand, officer triggers "Deep re-index with Mistral" which runs the legal-IR pipeline, replaces existing chunks, and populates an amendment chain + cross-reference graph + procedure flows. Sticky tier once deep. Single chat window with intent-routed answers across eight intents: Lookup, ListChildren, Summarize, Difference (amendment + instrument), CrossCompare, ActionItems, Timeline (amendment + thematic), Procedure (officer workflow flow). Every answer carries an officer-grade legal citation. Default LLM tier is offline (Qwen 0.5B/3B, Mistral 7B); Groq/Cerebras cloud is eval-only. Primary user: government officer / policy analyst. Out of scope v1: case law, point-in-time queries, foreign jurisdictions, Urdu, audit logs, saved queries. Reference design doc: docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md.

Upstream context: this `/sp.specify` was preceded by a brainstorming session that produced `docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md` (architecture, data model, slice plan) and a Slice 0 plan at `docs/superpowers/plans/2026-06-02-legal-rag-slice-0-foundation.md`. User then corrected the workflow back to SDD (`sp.*`) per CLAUDE.md.

## Response snapshot

Created branch `007-legal-rag-pakistan` off the current HEAD (which carries the brainstorming-era design + Slice 0 plan). Wrote `specs/007-legal-rag-pakistan/spec.md` containing:

- Three prioritized user stories (P1 fast-path MVP, P2 deep upgrade, P3 cross-doc graph + procedure flows), each with 4–5 acceptance scenarios and an independent-test description.
- Seven edge cases.
- 32 functional requirements grouped under: ingestion & indexing tiers (FR-001–008), legal structure (FR-009–012), chat: intent-routed answers (FR-013–020), citations (FR-021–024), scope picker (FR-025–026), LLM tiering & offline-first (FR-027–029), out-of-scope behaviors (FR-030–032).
- 9 key entities (Instrument, Section/hierarchical node, Amendment event, Cross-reference edge, Procedure, Procedure step, Officer, Citation, Chat query).
- An Assumptions block (fast-path reuse, local LLM stack capability, no internet, marker conventions).
- 10 measurable success criteria, all numeric, technology-agnostic, user-observable.

Wrote `specs/007-legal-rag-pakistan/checklists/requirements.md`; validation passed on first iteration with no `[NEEDS CLARIFICATION]` markers.

## Outcome

- ✅ Impact: spec ready for `/sp.clarify` (optional — no markers present) and `/sp.plan`.
- 🧪 Tests: requirements quality checklist PASS on first iteration.
- 📁 Files: `specs/007-legal-rag-pakistan/spec.md` (created); `specs/007-legal-rag-pakistan/checklists/requirements.md` (created).
- 🔁 Next prompts: `/sp.constitution` to apply P1–P5 amendment; `/sp.plan` to generate the implementation plan; `/sp.tasks`; `/sp.analyze`; `/sp.implement`.
- 🧠 Reflection: brainstorming output mapped cleanly to the SDD spec template — three user stories with priorities surfaced directly from the slice structure (Slice A → US1 fast-path, Slice B → US2 deep upgrade kept partial, Slices C+D → US3 cross-doc + procedure). The two-tier indexing (fast default + deep on demand) added late in brainstorming reads cleanly as the central user-facing distinction in the spec.

## Evaluation notes (flywheel)

- Failure modes observed: initial attempt to write the spec failed because Write requires Read first on existing files (the script had pre-created the file with template content); resolved by reading-then-overwriting.
- Graders run and results (PASS/FAIL): requirements.md checklist — PASS (all items).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): on `/sp.plan`, propose three independent slice-plan files (one per user story) rather than one mega-plan, mirroring the project's per-feature branch hygiene.
