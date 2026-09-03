---
id: 049
title: Oversize chunks — recursive split_long_text + size-cap front/back-matter persist
stage: green
date: 2026-07-01
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: noviq.ai.pk@gmail.com
command: (freeform) "start working on chunk issue still remaining"
labels: [chunking, ingestion, structure_parser, oversize]
links:
  spec: docs/chunking-audit-2026-06-29.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/structure_parser.rs
  - src-tauri/src/services/structured_importer.rs
  - CHANGELOG_DEV.md
tests:
  - split_long_text_enforces_threshold_on_giant_paragraph
  - split_block_for_persist_splits_above_ceiling_keeps_small_whole
---

## Prompt

start working on chunk issue still remaining.

## Response snapshot

Diagnosed (from prior live-VM work, memory `oversize-chunks-rootcause`) two leaks
RC-D did not fix, and applied both in code:

- **B — `split_long_text` didn't enforce its threshold on pieces.** The
  clause-marker / paragraph / sentence branches returned pieces AS-IS; only the
  char-cut honoured the limit, so a 35k paragraph with no internal break shipped
  whole (Manual `Section_30/p1`=35,604c). Added a nested recursive `enforce()`
  that re-splits any piece still over threshold; terminates at the char-cut.
- **A — front/back-matter persist paths bypass the tree splitter.** Added
  `pub(crate) split_block_for_persist` (≤ 8000c whole, else split to ≤1500c) and
  routed `persist_front_matter_chunks` / `persist_back_matter_chunks` through it
  with a running `chunk_index`. Constitution TOC (42k) / schedules (17k) now
  split-and-kept, not one blob.

Table-row 27k and RC-D container splits inherit Fix B automatically (both call
`split_long_text`). No local Rust on Windows → compile + re-index pending on the
GPU VM.

## Outcome

- ✅ Impact: eliminates the two remaining sources of 35k–42k-char chunks in the
  legal corpus; keeps all content retrievable and citable at context size.
- 🧪 Tests: 2 new unit tests added; NOT yet run (no local cargo — VM compile owed).
- 📁 Files: structure_parser.rs (recursive split + new helper), structured_importer.rs
  (two persist fns expand blocks), CHANGELOG_DEV.md.
- 🔁 Next prompts: compile on VM (`cargo test --lib structure_parser::`), commit,
  deploy to `release`, re-index all 11 docs, verify no unintended chunk > ~1500c.
- 🧠 Reflection: Fix B's recursion terminates because every semantic split yields
  ≥2 strictly-smaller non-empty pieces and bottoms out at the char-cut.

## Evaluation notes (flywheel)

- Failure modes observed: prior RC-D only touched the provision tree; the two
  bypass/leak paths were invisible to it.
- Graders run and results (PASS/FAIL): pending VM compile.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after re-index, if TOC still noisy,
  evaluate marking constitution TOC boilerplate/exclude (operator's open call).
