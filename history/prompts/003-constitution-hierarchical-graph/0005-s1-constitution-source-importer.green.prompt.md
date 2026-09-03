---
id: 0005
title: S1 constitution source importer
stage: green
date: 2026-05-21
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: (implementation — tasks T002, T003, T006, T007, T008, T009 — stage S1)
labels: [legal-rag, constitution, scraper, importer, ir, s1]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: N/A
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: N/A
files:
  - tools/constitution_import/Cargo.toml
  - tools/constitution_import/src/ir.rs
  - tools/constitution_import/src/main.rs
  - tools/constitution_import/constitution_ir.json
  - specs/003-constitution-hierarchical-graph/tasks.md
  - CHANGELOG_DEV.md
tests:
  - T009 acceptance (automated counts) — PASS: 12 Parts / 312 Articles / all have marginal notes
  - T009 manual 10-article spot-check vs na.gov.pk PDF — PENDING (user gate before S2)
---

## Prompt

`continue to work now where we left.` — resume feature 003 at stage S1 (the
source importer). Mid-task addendum: `after extraction complete constitution
also save that at Desktop in a suitable file so we can use that clean file for
next time also.`

## Response snapshot

Read `CHANGELOG_DEV.md` (mandatory), `tasks.md`, `data-model.md`, `research.md`,
then inspected real pakistani.org markup before coding.

Built `tools/constitution_import/` — a **standalone** dev-only Rust crate
(scraper 0.27, ureq 3, serde, anyhow):
- `ir.rs` — `ConstitutionIR` serde types (Part → Chapter → Article → Clause →
  Sub-clause; provisos/explanations attached in-leaf; amendment refs).
- `main.rs` — fetch index + Part/Chapter pages into a local `.cache/`, parse the
  HTML into the IR, emit JSON, run a structural cross-check.

Deviations / corrections forced by reality:
1. **No Cargo workspace** in the repo — made the importer a standalone crate,
   not a workspace member (plan T002 wording), to avoid risking the Tauri NTFS
   build. Flagged in CHANGELOG.
2. The **index page is a plain `<a href>` list**, not the `<select>` nav on
   article pages — rewrote `build_plan` to parse links.
3. Each Chapter's articles are **split across many sibling `<table>` blocks** —
   first attempt grabbed only the first table (got 124 articles); fixed to
   collect every table in an `<h2>` section (→ 312).
4. `<h2>` text included `<sup>` amendment-footnote numbers — stripped so titles
   stay clean.

Result `constitution_ir.json` (466 KB): 12 Parts, 312 Articles, 571 clauses,
208 sub-clauses, 62 provisos, 12 explanations. Copied to `~/Desktop/` per the
user's addendum.

## Outcome

- ✅ Impact: S1 done — a clean structured IR of the constitution exists,
  replacing the 93%-mislabeled PDF extraction. S2 (structure-aware parser)
  unblocked once the user clears the manual spot-check gate.
- 🧪 Tests: automated T009 cross-check PASS (12 Parts / 312 Articles / all
  marginal notes); spot-checked Article 10 (9 clauses, Explanations I/II on
  cl.4, provisos on cl.5/7) and flat Article 7. Manual 10-article check vs the
  official PDF still pending.
- 📁 Files: 3 new crate files + emitted IR; tasks.md T002/T003/T006/T007/T008/
  T009 checked; CHANGELOG entry.
- 🔁 Next prompts: user does the manual 10-article spot-check, then S2 (T010–
  T013 — `constitution` doc-type profile + structure-aware tree parser).
- 🧠 Reflection: inspecting the real HTML before and during coding caught two
  structural surprises (link-list index, multi-table chapters) that the plan's
  description glossed over. The `.cache/` makes re-runs offline + deterministic.

## Evaluation notes (flywheel)

- Failure modes observed: (a) assumed the index carried the `<select>` nav —
  it does not; (b) assumed one table per Chapter — articles span many tables.
  Both fixed by reading cached HTML, not guessing.
- Graders run and results (PASS/FAIL): T009 automated counts PASS.
- Prompt variant (if applicable): N/A.
- Next experiment (smallest change to try): S2 — run `parse_tree` over the IR
  and assert the L0–L5 tree depth + provisos attached in-leaf.
