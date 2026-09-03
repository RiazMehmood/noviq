---
id: 0001
title: ingestion hardening spec
stage: spec
date: 2026-06-19
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: Riaz Mehmood
command: /sp.specify
labels: [ingestion, citation-grade, legal-docs, deterministic-parser, knowledge-graph]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/013-ingestion-hardening/spec.md
  - specs/013-ingestion-hardening/checklists/requirements.md
tests:
  - none (spec stage)
---

## Prompt

Use SDD to formalize the Fast-Tier ingestion hardening work. Feature: refactor the deterministic
Rust ingestion pipeline so structure detection, citation labeling, normalization, and persistence
produce correct, verbatim, correctly-structured chunks. Driven by the Iteration-1 audit of
fundamental_rights.pdf (Constitution of Pakistan, Part II, Articles 7-40) which surfaced 12 defects
(D1-D12). Two-pass design (structure first with NO citation word; vocabulary resolved later with
confidence layering -> unresolved on conflict). Locked phase order P0..P4. Guardrails: verbatim
substring, flag-don't-fabricate, deterministic fast tier, one re-index per phase, frozen 177-chunk
regression fixture. Non-goals: OCR/PaddleOCR (scan-only, deferred), table reconstruction,
retrieval/LLM stages. Mid-spec, stakeholder added: "we have graph nodes for amendments and
references we did not use those still, i think we need to use those also."

## Response snapshot

Created branch `013-ingestion-hardening` (next number after 012). Wrote spec.md with 5 user stories:
US1 trustworthy provision identity (P1), US2 uncontaminated verbatim text (P1), US3 page-accurate
citations (P2), US4 clause/amendment structure (P2), and — added per stakeholder — US5 amendment &
cross-reference relationships captured into the existing knowledge-graph store (P3, first use of the
graph substrate; capture-only, graph querying remains a later stage). 15 functional requirements
(FR-001..FR-015), 9 measurable success criteria (SC-001..SC-009), Key Entities (Provision, Footnote,
Document profile, Amendment relationship, Cross-reference relationship), edge cases (25/25A source
disorder, no-catchline provisions, multi-provision pages, bare integers, hybrid docs), Assumptions
(footnotes retained/retrievable, catchline=title metadata, vocab-conflict→unresolved, geometry-based
detection), and Non-Goals. No [NEEDS CLARIFICATION] markers — three candidate ambiguities resolved
via documented assumptions using refuse-or-flag defaults. Quality checklist all green.

## Outcome

- ✅ Impact: Formal spec for the citation-grade ingestion refactor on a clean SDD branch; abstracts a
  highly implementation-heavy input (defect IDs, file/function names) into WHAT/WHY outcomes while
  preserving the verbatim/flag-don't-fabricate guardrails. Adds graph capture (US5) as the first use
  of the existing unused graph_nodes/graph_edges substrate.
- 🧪 Tests: none (spec stage). Regression fixture defined as a requirement (FR-012) + success criteria.
- 📁 Files: spec.md, checklists/requirements.md.
- 🔁 Next prompts: /sp.plan (or optional /sp.clarify). Plan must map US→phases (locked order
  P0 D9+D12 → P1 zoning D5/D6 → P2 tree D2/D3/D4/D10 → P3 vocabulary D1 → P4 boundaries D7/D8) and
  add US5 graph capture; consider an ADR for the two-pass structure/vocabulary split.
- 🧠 Reflection: The codebase already separates NodeLevel (structure) from profile citation_fmt
  (vocabulary) and has label_status='unresolved' — the "two-pass" architecture is an evolution, not a
  rewrite. Source artifact `~/Desktop/ingestion_hardening_plan.md` + the judge audit hold the code map.

## Evaluation notes (flywheel)

- Failure modes observed: input was HOW-heavy (file:line, defect IDs); risk of leaking implementation
  into spec — mitigated by abstracting to outcomes and parking the code map for the plan.
- Graders run and results (PASS/FAIL): spec-quality checklist — all items PASS (re-validated after US5).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run /sp.plan and confirm the two-pass split + graph
  capture survive into design without an LLM creeping into the fast tier.
