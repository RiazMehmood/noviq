# Rule-N Misattribution in Delegation Tables — Diagnosis, Fix Attempts & Code

**Feature:** 012-doc-intelligence-production · **Wave:** D (US5 tables / US6 schedule boundaries)
**Date:** 2026-06-19 · **Status:** boundary-detection fixed; per-department schedule-table attribution still open
**Scope of this doc:** the **Rule-N misattribution** defect only.

---

## 1. The Defect

Schedule **table rows** in the Sindh Delegation document are stored in the DB with a
`unit_label` of `Rule N` (e.g. `Rule 3`, `Rule 11`, `Rule 17`) even though the row is a
**delegation-table serial entry**, not the operative text of that rule. Because citation-grade
drafting reads `unit_label` deterministically, a draft can cite the wrong provision.

Example (live DB, content head):

```
label "Rule 3"  → "3. Leases of land … — To whom delegated: (i) Administrative Department …"
label "Rule 11" → "78  12.. To divert provision for contingencies of sanctioned estimate …"
label "Rule 17" → "17. WORKS AND SERVICES DEPARTMENT … 1. Technical Sanction …"
```

The content serials (`3..`, `12..`) and department headers are schedule entries, **not** Rules 3/11/17.

---

## 2. Live-DB Evidence (post Wave-D re-index, 2026-06-19)

Corpus: **7,672 chunks / 10 sources** (PG, app-managed; port rotates).

**Which documents are affected** (provision chunks under a `Rule %` label carrying a
delegation-table signature — `To whom delegated` / `Sr. #` / `Administrative Department` / `Full Powers`):

| Document | Mislabeled schedule rows |
|---|---|
| **09_Sindh_Delegation_of_Financial_Powers_2019.pdf** | **132** (the culprit) |
| 05_Compendium_Rules_under_SLGA_2013.pdf | 8 |
| 10_SPSC_Regulations_2023.pdf | 7 |

**Pages affected (Delegation doc only).** The `chunks.page_number` column is **NULL**
(not populated at index time — a separate provenance gap). Pages below are the **printed
page numbers embedded in the row content** (page footer digit swept into the chunk,
e.g. `Rule 3  34  3.. Leases of land …` → printed page 34):

> **29, 34, 36, 37, 42, 44, 45, 47, 48, 49, 55, 60, 61, 62, 67, 73, 74, 78, 80, 81**
> (range 29–81 = the Second Schedule per-department delegation tables)

**`table_row` coverage** (chunks tagged `block_class='table_row'`): only **12** rows —
1 correct (`Part-I`), 11 still `Rule N`. Low coverage is by design (the schema-anchored engine
requires an enumerator + ≥2 serial anchor rows; sparse/wrapped tables fall back to prose).

**Schedule/Part boundaries now present as labels** (were entirely missing before the formfeed fix):
`First/Second/Third Schedule`, `Part-I … Part-VIII`.

---

## 3. Fix Attempts (chronological)

### 3a. Wave D / US6 — schedule boundary detection (prior session)
**Goal:** stop schedule/annexure content inheriting the preceding rule's label.
**Code:** `heading_detector/regex_pass.rs::scan_structural_boundaries` — ALL-CAPS standalone-line
lexicon (`(FIRST..TENTH) SCHEDULE`, `SCHEDULE/ANNEXURE/APPENDIX`, `PART-<n>`, `FORM <id>`),
emitted at `NodeLevel::Part` so `build_tree_from_headings` opens a new container and
`drop_out_of_sequence_articles` resets the numeric run.
**Result:** mixed-case inline references correctly ignored; but **top-of-page** boundaries were
still missed (see 3b) → schedule rows still inherited `Rule N`.

### 3b. Formfeed boundary fix (this session, 2026-06-19) ✅
**Symptom:** 11/12 `table_row` chunks still labeled `Rule N` after the Wave-D re-index.
**Root cause:** `FIRST/SECOND SCHEDULE` / `PART-I` headings sit at the **top of a page**, right after
a formfeed (`\x0C`), not a newline. The boundary regex uses `(?m)^`, which anchors after `\n` but
**not** after `\x0C` → boundary missed → rows inherit the preceding `Rule N`.
**Fix:** scan boundaries on `text.replace('\u{0c}', "\n")`. `\x0C` and `\n` are both single UTF-8
bytes, so every `m.start()` offset stays valid against the **original** text.
**Result (live-verified):** all Part/Schedule boundaries now detected; the `Part-I` delegation row
is correctly labeled `Part-I` (was `Rule N`). **This specific bug is fixed** — but it only rescues
rows whose nearest boundary is a Part/Schedule. The per-department serial tables (§5) remain.

---

## 4. Code Written (Rule-N fix)

`src-tauri/src/services/heading_detector/regex_pass.rs` — formfeed normalization in
`scan_structural_boundaries`:

```rust
// A FIRST/SECOND SCHEDULE / PART-I heading frequently sits at the TOP of a page,
// immediately after a formfeed (\x0C) rather than a newline. (?m)^ matches after
// \n but NOT after \x0C, so those boundaries were missed and their rows inherited
// the preceding Rule's label. Normalize formfeed → newline; both are single UTF-8
// bytes, so every m.start() offset stays valid against the ORIGINAL text.
let scan_src = text.replace('\u{0c}', "\n");
let mut out = Vec::new();
for cap in re.captures_iter(&scan_src) {
    let m = cap.name("lbl").expect("named group missing");
    ...
}
```

**Regression test:** `detects_schedule_boundary_after_formfeed` — boundary fires after `\x0C`;
asserts the offset realigns to the original (un-normalized) text.
**Gate:** `cargo test --lib` = **484 passed / 0 failed** (was 483; +1).

---

## 5. Root Cause of the REMAINING Problem (not yet fixed)

The Delegation **Second Schedule** is laid out as **per-department tables, each restarting serial
numbering at `1, 2, 3 …`**. The generic `rules` profile sees every `"3.\tLeases of land"` /
`"17. WORKS AND SERVICES DEPARTMENT"` as a `Rule N` heading. `drop_out_of_sequence_articles`
**cannot** reject them because within each department table the serials are genuinely in sequence
(1→2→3), and the document **also** contains real Rules 1–37. So dozens of unrelated department
entries collapse under shared `Rule 3` / `Rule 11` / `Rule 17` labels.

Evidence — everything tagged `Rule 3` in the Delegation doc:

```
Rule 3  3. Leases of land (i) Administrative Department …
Rule 3  3. To sanction expenditure Principal Agriculture Training …
Rule 3  3. Abolition of Posts …
Rule 3  3. SCHOOL EDUCATION DEPARTMENT
Rule 3  3. Chairman, Planning and Development Board …
Rule 3  The Principal Accounting Officer is responsible …
```

**Why a regex tweak can't fix it:** a schedule serial item (`3. Leases of land`) is textually
identical to a real rule heading (`3. Title`). No generic lexical signal separates them, and the
document interleaves both its real rule body and these delegation tables.

---

## 6. Resolution — Phase 1: Schedule-Aware Heading Suppression (IMPLEMENTED 2026-06-19)

Chosen over the two options below because it directly enforces the success criterion
("no false Rule-N labels") with the lowest regression risk, and does NOT touch global reconstruction
thresholds.

- **Option A (deferred):** relax the schema-anchored reconstruction anchor (`body_column_lefts`
  **≥2 → ≥1**). Increases coverage but is corpus-wide and risks false tables / promoted prose. Now a
  *Phase 2*, gated on evidence (§8).
- **Option B (rejected as primary):** manual curation. Does not generalize; high maintenance.
- **Option C (rejected):** accept mislabeling — violates the citation-grade contract.

### Design
`scan_with_profile` → after dedup, before the sequence filter → `suppress_schedule_table_serials`:
1. Split the document into spans between **structural boundaries** (levels shallower than Article —
   Part/Chapter/Schedule), plus document start.
2. For each span, test the text for a **delegation-table signature**
   (`is_delegation_table_context`): `To whom delegated` | `Nature of power` | `Sr. #` | a TIGHT
   `(1)(2)(3)(4)` column header (only spaces/tabs between, so spread-out sub-clauses don't match).
3. Within a span that has the signature, **drop Article-level bare-number headings**. Boundaries and
   deeper Clause/SubClause markers are kept. The serial rows then attach to the nearest container
   (`Part-II` / `Second Schedule`) — the desired fallback.

**Key decision:** the discriminator is the **table signature**, NOT a schedule ancestor — the live DB
proved `Part-II`/`Part-I` are TOP-LEVEL containers (201/162 chunks), not nested under a `SCHEDULE`
node, so a schedule anchor would have missed most affected rows.

### Code (`src-tauri/src/services/heading_detector/regex_pass.rs`)
- `suppress_schedule_table_serials(text, &mut hits)` — span splitting + Article-level drop.
- `is_delegation_table_context(region)` — the signature regex.
- Call site added in `scan_with_profile` between `dedup_by_key` and `drop_out_of_sequence_articles`.

### Tests added (`regex_pass::tests`)
- `delegation_table_serials_not_promoted_to_rules` — `3.`/`17.` under `Part-II` + table signature → no `Rule 3`/`Rule 17`; `Second Schedule`/`Part-II` kept.
- `real_bare_number_rules_outside_table_still_detected` — bare-number rules with no table signature still detected.
- `rules_then_delegation_schedule_suppresses_only_table` — main-body rules kept; only the schedule-table serials dropped.
- `schedule_without_table_context_keeps_numbered_headings` — a schedule WITHOUT a table signature keeps its numbered headings (proves narrowness).

**Gate:** `cargo test --lib` = **489 passed / 0 failed** (was 484).

### Regression-risk assessment
- Main-body rules: never in a delegation-table span → untouched.
- Docs without delegation tables: no signature anywhere → zero effect (early return).
- Schedules that are forms/lists (no signature): not suppressed.
- Tight `(1)(2)(3)(4)` avoids matching ordinary sequential sub-clauses.
- **Index-time change** → needs one re-index; current DB still shows old `Rule N` until re-indexed.

---

## 7. Phase 2 (conditional — evaluate AFTER a Phase-1 re-index)

Only if, after re-index, delegation rows fall under `Part-II` correctly but **table-row recovery is
too low**, consider a **delegation-specific** reconstruction profile that allows
`body_column_lefts >= 1` **only when** a strong table schema is present (`Sr. #` / `Nature of Power` /
`To whom delegated` / `Extent`). Do NOT globally relax `>=2 -> >=1`. Gate on the page-8 golden + a
re-index spot-check showing no garble and no false tables. **Phase 2 may be unnecessary** — Phase 1
already satisfies the success criterion (no false Rule-N); Phase 2 only improves recall, which is
explicitly NOT the success metric.

---

## 8. Open Follow-ups (Rule-N related)

1. **Re-index** to activate Phase 1, then re-run the §2 scan → target 0 `Rule N` on delegation-table rows.
2. Decide whether Phase 2 (recall) is needed, based on post-re-index `table_row` coverage.
3. Populate `chunks.page_number` at index time (currently NULL) — needed for curation/audit.
4. Commit the Wave D + formfeed + Phase-1 tree (currently uncommitted on branch `012-doc-intelligence-production`).
