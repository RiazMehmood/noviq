---
id: "0002"
title: "clarify legal RAG spec"
stage: spec
date: 2026-06-02
surface: agent
model: claude-opus-4-7
feature: 007-legal-rag-pakistan
branch: 007-legal-rag-pakistan
user: Riaz Mehmood
command: /sp.clarify
labels: [feature-007, legal-rag, pakistan, clarify]
links:
  spec: specs/007-legal-rag-pakistan/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: specs/007-legal-rag-pakistan/spec.md
    change: modified
tests:
  - name: requirements quality checklist (unchanged from /sp.specify)
    status: PASS
---

## Prompt

`/sp.clarify` invoked on `specs/007-legal-rag-pakistan/spec.md`. Empty user-input block — the workflow drove its own question generation.

## Response snapshot

Ran the taxonomy scan against the spec. Identified three Partial/Missing categories: (1) chat answer latency target (Missing), (2) corpus size / scale assumption (Partial), (3) concurrent-user model for v1 (Partial). Asked one question per gap, presented recommended option with reasoning, accepted user replies:

- Q1 (Chat latency): accepted recommended Option B — Median ≤ 5 s, P95 ≤ 15 s on offline-mid; fast-tier intents ≤ 3 s median. Integrated as new **SC-011**.
- Q2 (Corpus size): accepted recommended Option B — up to 100 instruments / ~50k chunks per machine. Integrated as new **SC-012**.
- Q3 (Concurrency): accepted recommended Option A — single-officer desktop, one officer per machine at a time. Integrated as new **FR-033** and an additional Assumption line stating CPU-class workstation deployment.

A `## Clarifications` section with a `### Session 2026-06-02` subheading was created immediately after the header block and now contains three bullets recording the Q→A pairs.

Lower-impact gaps not asked (deferred to planning): fast-index UX progress feedback, JSON sidecar input format.

## Outcome

- ✅ Impact: spec now answers latency, scale, and concurrency. Planner has the numbers it needs to choose retrieval architecture and to size acceptance tests.
- 🧪 Tests: requirements checklist still PASS (no new `[NEEDS CLARIFICATION]` markers introduced).
- 📁 Files: `specs/007-legal-rag-pakistan/spec.md` modified (1 new section + 3 bullets + 1 new FR + 2 new SCs + 1 new Assumption).
- 🔁 Next prompts: `/sp.constitution` (apply P1–P5), `/sp.plan`, `/sp.tasks`, `/sp.analyze`, `/sp.implement`.
- 🧠 Reflection: User accepted recommendations on all three questions ("b", "b", "a") without modification — the recommended options were the obvious low-risk picks given the existing memory (speed-first feedback, single-Tauri-process today, LAN deferred per design). For future `/sp.clarify` passes, consider front-loading even more reasoning into the recommendation block so the user can disagree quickly when warranted.

## Evaluation notes (flywheel)

- Failure modes observed: none. Spec writes succeeded atomically; no contradictions introduced.
- Graders run and results (PASS/FAIL): requirements checklist PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): on the next `/sp.clarify` for this feature (if any post-plan), surface JSON-sidecar input format as a Q now that planning will care about file-format contracts.
