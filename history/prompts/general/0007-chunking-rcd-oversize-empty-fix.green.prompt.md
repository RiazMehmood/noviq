---
id: 0007
title: Chunking RC-D oversize split + drop empty/label-only chunks
stage: green
date: 2026-06-30
surface: agent
model: claude-opus-4-8
feature: chunking-audit
branch: release
user: Riaz Mehmood
command: (chat)
labels: [chunking, structure_parser, RC-D, coalesce]
links:
  - docs/chunking-audit-2026-06-29.md
  - CHANGELOG_DEV.md (2026-06-30 entry)
files:
  - src-tauri/src/services/structure_parser.rs
tests:
  - "cargo test --lib structure_parser:: -> 29/29"
---

## Prompt
Operator deep-researched live-DB chunks and found defects believed solved still
present: 40k+ chunks not split; huge empty chunks; chunks that are only a label
name; label merged into content (rule 2 body "22"); two garbage chunks where
size/unit_label/page hold content; coalescing not merging tiny chunks. Asked
whether these are tracked in memory and to fix all feasible in one go under a
near-exhausted weekly budget, respecting the context window so TOP_K doesn't bloat.

## Response
Traced every observation to `docs/chunking-audit-2026-06-29.md`. Headline: most
defects = STALE live DB (predates 2026-06-29 RC-A/B/C/PG fixes; VM re-index clears).
Two genuinely-open code issues fixed:
1. RC-D — `split_oversized_leaves` now splits `Document|Part|Chapter` bodies above
   `oversize_hard_ceiling()` (8000c, `ECHO_OVERSIZE_CEILING`) into ≤1500c
   `… part N` pieces; short indexes preserved.
2. `is_empty_or_label_only_leaf` + filter in `coalesce_undersized_leaves` drops
   blank and pure-label-echo leaves.
Context-window: pieces ≤1500c (~375 tok); split→coalesce re-merges scraps to
hard_max 1500c. Tests 29/29. Deferred (need live rows/sidecar): RC-E/T/H/F + VM
re-index + the two malformed-row garbage chunks.

## Outcome
GREEN — code + tests + CHANGELOG + memory entry (project_noviq_chunking_audit) done.
Uncommitted on `release`.
