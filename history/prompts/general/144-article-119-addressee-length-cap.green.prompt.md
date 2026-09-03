---
id: 144
title: Article 119 — an addressee entry is short, whatever its verbs do
stage: green
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: "yes, fix Article 119"
labels: [parser, heading-detector, constitution, circulation-list, regression]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/services/heading_detector/regex_pass.rs
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib --no-default-features --features server, NOVIQ_TEST_PG_INSTALL_DIR set (VM, isolated worktree): 925 passed / 2 failed — the same two that fail on baseline"
  - "regex_pass::tests::a_provision_whose_verb_is_on_the_next_page_survives — NEW, passes"
  - "regex_pass::tests::real_addressee_entries_stay_under_the_length_cap — NEW, passes"
  - "regex_pass::tests::is_circulation_line_direct + circulation_list_is_suppressed_but_provisions_survive — still pass (no regression in suppression)"
---

## Prompt

yes, fix Article 119

## Response snapshot

Article 119 was the 13th and last Constitution Article lost to the
circulation-list rule (`5cc66670`). The third pass (`2ffb01d5`) rescued 12 by
widening `entry_at` to the whole entry so a wrapped provision reaches its verb.
119 could not be rescued that way, and extracting the live `Constitution.pdf`
with `pdftotext -layout` showed why: `shall be regulated` is on the **next
page**, past a form feed and a page header, and a footnote rule (`–––––`)
overruns the 400-char window before then. No window size reaches it.

Fix: a second, independent safeguard in `is_circulation_line`. An addressee
entry NAMES one office and stops — the longest real one in the corpus is 68
chars — so an entry past **140 collapsed chars** is not a circulation entry
whatever its verbs do. Measured through a new `collapse_whitespace` helper so
`-layout` column padding and the marginal catchline cannot inflate the count.

Verified before committing by porting `entry_at` + `is_circulation_line` to a
throwaway script and sweeping every numbered line of the real extraction:
Article 119's entry is **285 collapsed chars with no reachable verb**;
suppressions go **64 → 58**; the six rescued are 119 plus five long
Schedule/Legislative-List entries (fail-safe direction); all six verbatim
circulation entries stay suppressed; the three short lines still suppressed are
table-of-contents rows whose Articles survive in the body.

Then compiled and ran the real suite on the VM under the safe recipe (bundle →
temp ref → detached worktree → separate `CARGO_TARGET_DIR`), verifying prod HEAD
and `noviq-server` were untouched during and after, and cleaning up.

## Outcome

- ✅ Impact: closes the last of the 13 lost Constitution Articles in code. `84ac73a0` on `release-deploy`; **not pushed, not deployed, not re-indexed** — the fix is inert until a re-index.
- 🧪 Tests: 925 passed / 2 failed (was 923/2 — the delta is exactly the two new tests; the same two pre-existing failures remain).
- 📁 Files: `regex_pass.rs` (+65: the cap, `collapse_whitespace`, two tests), `CHANGELOG_DEV.md` (fourth-pass entry).
- 🔁 Next prompts: push → deploy → **re-index** → confirm Constitution **280 distinct / max 280** via `scripts/corpus_gap_check.sql`; then **Build Summaries** (`document_full` still 0 on all 18 docs — check `/api/health` `engine.ready:true` first, it deletes before regenerating).
- 🧠 Reflection: ⭐ **When a discriminator depends on evidence arriving, add a second discriminator that cannot be starved.** The verb test was never wrong — it was defeated by page geometry. Widening the window is chasing the same failure mode; an orthogonal signal (length) ends it.

## Evaluation notes (flywheel)

- Failure modes observed: two wasted VM build cycles from missing build/test env — `--no-default-features` (default features pull Tauri → `glib-2.0` absent on the headless VM) and `NOVIQ_TEST_PG_INSTALL_DIR` (its absence yields a bogus 71 DB failures, the recurring "~70 failing" ghost). Both are already documented in memory; I ran before reading.
- Graders run and results (PASS/FAIL): PASS — simulation over the real `-layout` extraction, plus the Rust suite with a matched pre-existing-failure control.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after the re-index, run the completeness check immediately rather than at session end — the re-index is the only oracle for any parse fix, and both defects this week were found by it and nothing else.
