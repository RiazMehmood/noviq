---
id: 143
title: Re-index exposed the marker and verb defects
stage: green
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: "push it to release / gpu re index in process via admin ui / reindex complete"
labels: [tables, parsers, ingestion, chrome-strip, heading-detector, re-index]
links:
  spec: specs/023-structured-table-capture/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/services/parsers/chrome.rs
  - src-tauri/src/services/parsers/table_sidecar.rs
  - src-tauri/src/services/structured_importer.rs
  - src-tauri/src/services/heading_detector/regex_pass.rs
  - /opt/noviq/gpu-reindex.sh (VM-only, out of tree)
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib --features server (VM): 923 passed / 2 failed (both fail on baseline c367a71a)"
  - "regex_pass.rs: a_wrapped_provision_reaches_its_verb (Articles 280/61/266 verbatim)"
  - "structured_importer.rs: chrome_strip_never_eats_a_table_marker"
  - "structured_importer.rs: an_unpaired_marker_deletes_nothing_but_itself"
  - "live: 0 table rows stored as provisions corpus-wide (was 105+7)"
  - "live: RoB Schedule-I one unit pp.26-34, 53 rows, serials 1-39"
  - "live: Manual for Development Projects 0 -> 49 tables / 419 rows"
---

## Prompt

push it to release
gpu re index in process via admin ui.
i re indexed but it just run and finishes in flashes why?
reindex complete
save progress upto now we will start working from here in next turn.

## Response snapshot

Deployed the table work, then the re-index exposed two further defects that no
unit test or reflow-level diagnostic could have caught.

1. **Deployed `8b382983`**, re-indexed. Two of three claims landed (Manual 0→49
   tables, RoB Schedule-I one unit) but a corpus query still found 105 table
   rows stored as `clause` provisions in Delegation. Cause: `chrome.rs`
   normalises digits away before testing cross-page repetition, so every
   `⟦TBL:n⟧` collapsed to one recurring key and was deleted as a masthead —
   after which the table strip had nothing to lift. Fixed; also made the strip
   fail safe (only a MATCHED pair drops anything) and closed the diagnostic gap
   that had reported 0 while production had 105.
2. **Deployed `af278b17`.** The next re-index aborted in 14 s with `CANARY
   FAIL`. The canary greps a fixed 300-second journal window that reached back
   past its own restart into the CPU-mode process my deploy had started three
   minutes earlier. Patched `gpu-reindex.sh` on the VM to bound the window with
   `RUN_START_MS`.
3. **Clean re-index (5 min).** All table claims verified live. But the
   Constitution came back **267 Articles, max 279** (was 280/280). Cause was NOT
   the table work: `5cc66670`'s circulation-list rule ran for the first time and
   tests a physical LINE, while `-layout` wraps a provision past its verb.
   Reading the whole entry recovers 12 of 13.

## Outcome

- ✅ Impact: table→provision contamination eliminated corpus-wide and verified
  in production; 12 of 13 lost Articles recovered in a local commit.
- 🧪 Tests: 923 passed / 2 failed (both pre-existing on baseline).
- 📁 Files: 4 source files + a VM-only script + changelog.
- 🔁 Next prompts: length cap for Article 119 → push → re-index → Build
  Summaries → ask for the department count.
- 🧠 Reflection: every defect this session was found by the re-index and a DB
  query, never by a test. The tests were green at each of the three points where
  production was wrong.

## Evaluation notes (flywheel)

- Failure modes observed: (a) a diagnostic that stopped before the stage that
  writes to the DB reported 0 while prod had 105; (b) a deploy immediately
  before a re-index poisoned the GPU canary; (c) a parse fix committed a day
  earlier deleted 13 Articles the first time it actually ran.
- Graders run and results (PASS/FAIL): PASS — 0 table rows as provisions; PASS —
  RoB Schedule-I single unit, serials 1–39; PASS — Manual 49 tables; **FAIL —
  Constitution completeness 279/280 (was 267 before the fix, target 280)**.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): cap a circulation entry at ~140
  chars — the longest real one in the corpus is 68 — which saves Article 119
  regardless of where its verb sits.
