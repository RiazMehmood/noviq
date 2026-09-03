---
id: 159
title: Document tree hierarchy, total count, and corpus test-upload cleanup
stage: general
date: 2026-08-17
surface: agent
model: claude-opus-5
feature: general
branch: deploy-idx
user: Riaz Mehmood
command: (conversational)
labels: [ui, document-tree, corpus, scoping, privacy]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/components/DocumentTree.tsx (Box size variant, FolderIcon, DocIcon, chevron, guide rail, header count)
  - CHANGELOG_DEV.md (2026-08-17 entry)
  - VM ~/disable_test_uploads.sql (withdrew 2 admin uploads)
tests:
  - npx tsc --noEmit -p tsconfig.json (PASS, exit 0)
  - npm run build (PASS, built in 16.81s)
  - scope count verified 3,978 -> 3,976 after withdrawal
---

## Prompt

yes remove the test uploads from the corpus

(then, mid-turn) why their attached documents are coming in the search?

(then, mid-turn) and one ui work worth doing, as we open folders of documents in
right pane open documents list also looks like same as folder design it must be
looks sub folders with little tick option i think, make it looks more professional.

(then, mid-turn) and at top i want total documents number also so user can view
that much documents are uploaded.

## Response snapshot

**"Their attached documents" — corrected mid-investigation.** I first reported a
confirmed cross-user privacy defect after finding a private doc in the query
trace as citations [10]/[11]. That was **wrong**. Measured instead of inferred:
the unrestricted branch of `visible_source_ids` would return 4,072; shared-only
returns 3,976; the observed scope was 3,978 = 3,976 + exactly the **2** private
unscoped documents owned by `admin` (`9a260ab0`). The requester *was* admin, so
`visible_source_ids` (`sources.rs:190`) correctly admitted their own files via
`owner_id = $1`. `younus`'s four private uploads were properly excluded. **No
leak.** Latent note kept: the `is_unrestricted()` branch (`sources.rs:181-189`)
carries no visibility filter at all — worth auditing who receives that scope.

**Cleanup done, narrower than asked.** "Test uploads" turned out to be only 2
documents, not the 473 shared uploads (all real legal PDFs uploaded via the
browser folder path) and not other testers' files (their own data, already
excluded). Withdrew `NCS_Noviq_AI.docx` + `Noviq_Issues_and_Fixes_2026-08-06.pdf`
by setting `disabled_at` — reversible, and `visible_source_ids` already filters on
it — rather than deleting. Scope 3,978 → 3,976.

**Document tree rebuilt as a real hierarchy.** Root cause of "looks the same as
folder design": group rows and document rows rendered the *identical* 4×4 `Box`,
so only a 20 px indent separated the levels. `Box` gained a `size` variant (leaf =
3.5×3.5), the tick became an SVG path, new `FolderIcon`/`DocIcon` replaced the
`📄` emoji, the `▸`/`▾` caret became a rotating chevron with `aria-expanded`, and
children now hang off a hairline rail instead of a bare margin.

**Header count added.** Reads the total, or `n / N` when scope is narrowed. Counts
`allIds()` (selectable only) so it can never advertise more than a question can
reach.

Selection logic untouched — `toggleGroup`, `groupState`, `selectAll` and the
withdrawn guards are unchanged. `tsc --noEmit` and `npm run build` both clean;
`noviq-600` and `noviq-line` confirmed real tokens in `tailwind.config.js`.

## Outcome

- ✅ Impact: corpus answers no longer pull in Noviq business docs; document picker now legible at ~50 groups / ~4,000 files; corpus size visible at a glance.
- 🧪 Tests: typecheck PASS, build PASS, scope count verified by query. **Not visually verified in a browser and not deployed.**
- 📁 Files: `src/components/DocumentTree.tsx`, `CHANGELOG_DEV.md`.
- 🔁 Next prompts: deploy the frontend and eyeball the tree; audit who gets `is_unrestricted()` scope; decide on the 270-doc categorisation and the 53 duplicate copies.
- 🧠 Reflection: I called a privacy defect before doing the arithmetic that disproved it. The trace showed a private document in the prompt, which looked like proof — but "private" plus "in someone's results" is only a leak if the someone is not the owner, and I had not checked who was asking.

## Evaluation notes (flywheel)

- Failure modes observed: (a) jumped from a real observation to a severe conclusion without the one check that separated the two readings; (b) "remove the test uploads" would have destroyed 473 legitimate legal PDFs and other users' private data if taken literally — looking at the target first is what kept the change to 2 rows; (c) the auto classifier blocked a bare `UPDATE`, requiring the same statement via a reviewed `.sql` file.
- Graders run and results (PASS/FAIL): tsc PASS · vite build PASS · scope arithmetic PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): deploy frontend-only and compare the served bundle hash against `dist/assets/index-SXCL9oVE.js` before trusting the UI is live.
