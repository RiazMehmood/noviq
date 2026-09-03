# Heading-mislabel defect — corpus diagnosis (2026-06-16)

## Symptom (citation-grade violation)
Live draft cited `Rule 54` for text that reads `"55. Entry into Force…"`, `Part II(3)`
for `"50. Publication of the Award…"`, and `Rule 3(i)` for `"2. Sanction of Director
General…"`. Both the label and the verbatim snippet are deterministic (metadata-driven,
the model never generates them) — so the stored chunk's **unit_label number disagrees
with the provision number that begins its own content**. A director copy-pasting the
draft would cite the wrong rule. This is the exact failure the zero-tolerance citation
contract forbids.

## Corpus impact (conservative, anchored to "content starts with `N. Title`")
| Source | Profile | Chunks | Mislabeled | % |
|---|---|---|---|---|
| 05_Compendium_Rules_under_SLGA_2013 | rules | 2136 | 229 | 10.7 |
| 09_Sindh_Delegation_of_Financial_Powers_2019 | rules | 697 | 154 | 22.1 |
| 10_SPSC_Regulations_2023 | rules | 753 | 116 | 15.4 |
| 02_SPPRA_Rules_2010_Revised_2013 | rules | 571 | 30 | 5.3 |
| 11_Sindh_Civil_Servants_APT_Rules_1974 | rules | 121 | 1 | 0.8 |
| 01_Sindh_Rules_of_Business_1986 | manual | 963 | 0 | 0 |
| 03_Sindh_Public_Procurement_Act_2009 | manual | 148 | 0 | 0 |
| 04_Sindh_Local_Government_Act_2013 | act | 79 | 0 | 0 |
| 06_Manual_for_Development_Projects_2019_PC-I | manual | 1728 | 0 | 0 |
| Govt. of Sindh Manual of Secretariat instruction 2010 | manual | 628 | 0 | 0 |

**~530 mislabeled provision chunks, every one on the `rules` profile; act/manual = 0%.**
(The looser inline-heading scan counts even more — 83 SPPRA, 388 Compendium — but the
anchored count above is the defensible floor.)

## Root cause (proven by profile→defect mapping)
The mislabeling is a **heading-detection miss**, not a labeling bug downstream. When a
top-level rule heading is not detected as a unit boundary, its heading+body is swept into
the **preceding** detected unit (usually a `(N)` sub-clause marker) and inherits that
unit's label.

The `rules` profile (`src-tauri/src/services/doc_profiles/rules.rs:22-78`) recognizes a
bare-numbered `N. Title` rule heading **only** through four narrow special-cases, each
added reactively for one earlier doc:
1. literal `Rule N.` prefix (`:29`) — misses docs that number `N. Title` with no "Rule" word.
2. `N. (sub)` — number immediately followed by `(1)` (`:36`).
3. `N. Title. ----` — title ending `.`/`:` then **two-or-more** dashes (`:49`, "Fix #8" for E&D).
4. opener-word safelist `The|Notwithstanding|On|If|Where|…` (`:64`, forensic-audit fix for RoB).

The dominant Sindh procurement-rules style — **`N. Title - body`** (single dash) and
**`N. Title`** (plain title not starting with a safelist word, e.g. `6. Language`,
`4. Powers of Higher authority`) — matches **none** of the four. So those rule headings
are invisible; only the `(N)` sub-clause pattern (`:69`) fires, and every parent rule is
mislabeled with a sub-clause marker.

The `act` and `manual` profiles do **not** have this defect because each carries a single
**general** bare-number heading pattern:
- `act.rs:46`  — `^\s*(?P<num>\d+[A-Z]?)\.\s+(?P<title>[\[A-Z][^\n]{0,120})`
- `manual.rs:36` — `^\s*(?P<num>\d+)\.\s+[A-Z]`

`rules.rs` is the only one of the three missing that general pattern. Hence the exact
profile→defect mapping above.

### Why it kept landing (recurrence cause)
1. **Accretion, not generalization.** Each broken doc added another hyper-specific regex
   tuned to that doc's punctuation instead of one general rule. Several special-cases
   target docs that have since been re-routed (RoB's safelist lives in `rules.rs` but RoB
   now resolves to the `manual` profile — the guard is dead weight tuned for a doc that no
   longer uses it).
2. **No invariant.** Nothing asserts that a chunk's `unit_label` provision number equals
   the provision number beginning its content, so off-by-one mislabeling is silent until a
   human notices a wrong citation in a draft.
3. **Tests only assert positives.** `detection_matches_rules_corpus` checks that literal
   `Rule N` headings are found; it never checks the bare-number `N. Title - body` forms or
   that no real heading is absorbed into a neighbor.

## Mitigation (generic, no per-doc keywords — recurrence-proof)
**Fix 1 — detection (the cure, index-time).** Give `rules.rs` a single general
bare-number provision-heading pattern covering `N. Title`, `N. Title - body`, and
`N[A-Z]. Title`, capturing the title up to end-of-line or the ` - `/`--` body delimiter
(model on the proven `act.rs:46` form). Keep a **generic** footnote discriminator (require
a substantial following body and/or an ascending-number run) rather than a vocabulary
safelist, then drop the now-redundant narrow patterns 2–4. Strip the inherited
`<label> ` prefix that pollutes stored content (`"Rule 54 55. Entry…"`).

**Fix 2 — invariant guard (recurrence-proof).** Add a post-build / index-time check: a
unit's label provision number must equal the leading provision number of its own text, and
no chunk content may *start* with an undetected `N. Title` heading. Wire it as a unit-test
assertion over a real SPPRA/Delegation fixture and (cheaply) as an index-time warning, so
any future doc that regresses is caught mechanically.

**Fix 3 — batched with US8 tabular re-extraction.** §3 Delegation-of-Financial-Powers
column-interleaved garble (`pdftotext -table`/TSV for multi-column pages) ships in the same
slice so both index-time fixes land in **one re-index**.

## Verification recipe
Find live PG port: `ps -eo args | grep '[p]ostgres -D /home/riaz/.local/share/com.echoai.app'`.
After re-index, re-run this scan — target is **0** mismatch on every `rules`-profile doc:
```sql
with x as (
  select s.file_name fn, c.unit_label lbl,
    trim(regexp_replace(regexp_replace(c.content,E'[\n\r]+',' ','g'),
      '^(Rule|Section|Part|Chapter)\s+[0-9IVXLC]+[A-Z]?\s*','','i')) txt
  from chunks c join sources s on s.id=c.source_id)
select fn, count(*) filter (where txt ~ '^[0-9]{1,3}\. [A-Z]'
   and position((regexp_match(txt,'^([0-9]{1,3})\.'))[1] in lbl)=0) as mismatch
from x group by fn order by 2 desc;
```
