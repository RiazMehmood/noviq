---
id: 0009
title: Re-chunk & re-index spec extension (010)
stage: spec
date: 2026-06-15
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: /sp.specify (re-chunk & re-index corpus)
labels: [spec, rechunk, reindex, fragmentation, citation-grade, 010-extension]
links:
  spec: specs/010-one-ask-drafting/spec-rechunk-reindex-extension.md
  ticket:
  adr:
  pr:
files:
  - specs/010-one-ask-drafting/spec-rechunk-reindex-extension.md (new — spec)
  - specs/010-one-ask-drafting/checklists/rechunk-requirements.md (new — quality checklist)
tests:
  - "n/a (specification artifact)"
---

## Prompt

commit fix #1 then spec the re-chunk work

(/sp.specify input) Re-chunk and re-index the indexed corpus to fix chunk fragmentation that
blocks citation-grade retrieval. Problem (empirically confirmed via live DB): 20,817 chunks,
median 41 chars, 55% under 50 chars; provisions shredded into 3–6-word scraps (SPPRA "Direct
Contracting" exists only as "(1) Direct Contracting" (22c) + fragments). Root cause:
structure_parser.rs emits one chunk per node, a node per marker (1)/(i)/(a), dirty OCR/pdftotext
over-segmented, and split_oversized_leaves splits big leaves with NO inverse to merge small
leaves. Scope: (1) coalesce small sibling leaves into ~300–800c provision units; (2) repair
OCR/newline artifacts; (3) fix hierarchy/label capture; (4) re-index + re-embed. Non-goals:
PaddleOCR swap, Note-drafting, Postgres US2/US4. Success: median length rises, Direct-Contracting
rule becomes citable for the Canon-scanner query, Constitution not regressed. Constraints:
offline/local-first, bundled Postgres+pgvector, re-embed via fastembed, verbatim contract preserved.

(follow-up corrections during execution) "no dont create new branch make it extension of 010"

## Response snapshot

Committed Fix #1 first (`a0be4fcf` — lexical leg = officer subject-nouns only, role-hints dropped;
7/7 tests; changelog entry capturing the fragmentation finding). Then drafted the re-chunk spec.
Per architect direction, scoped it as an EXTENSION of Feature 010 (not a new feature/branch):
wrote `spec-rechunk-reindex-extension.md` under 010, removed the briefly-created `012` dir, stayed
on branch 011. Spec has 4 user stories (US1 governing-provision retrievable=P1, US2 clean quotes=P2,
US3 correct labels=P3, US4 safe re-index=P1), 12 FRs, 6 measurable SCs, edge cases (over-merge,
boundaries, interruption, verbatim), assumptions, and explicit out-of-scope. Quality checklist all-pass.

## Outcome

- ✅ Impact: corpus-fragmentation fix specified as the citation-grade prerequisite; ready for /sp.plan.
- 🧪 Tests: n/a (spec). Fix #1 commit verified 7/7 + tsc clean separately.
- 📁 Files: spec + checklist under specs/010-one-ask-drafting/.
- 🔁 Next prompts: /sp.clarify (re-index trigger UX) or /sp.plan (coalescing + repair + label + re-index design).
- 🧠 Reflection: live-DB evidence (median 41c, the 22c "(1) Direct Contracting") turned a theory into a proven root cause; retrieval tuning was correctly abandoned as the primary lever.

## Evaluation notes (flywheel)

- Failure modes observed: split-only chunking (no small-leaf merge) + dirty OCR over-segmentation; one-node-one-chunk granularity.
- Graders run and results (PASS/FAIL): spec quality checklist PASS (all items).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): plan the coalescing pass + validate threshold against SC-001/SC-005 on a sample re-index.
