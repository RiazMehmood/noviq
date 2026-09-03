# Noviq — Corpus Coverage Checklist

> **Purpose.** The authoritative list of rulebooks Noviq should hold to serve a
> **Sindh government** officer facing accountability action (show-cause notices,
> audit paras, disciplinary proceedings, procurement objections). Tracks, per
> book: tier, current status, the exact edition to source, who verified it, and
> whether detection was validated after indexing. Tick boxes as books land.
>
> Owner: architect (Riaz). Created 2026-07-10. Update in place; this is the
> single source of truth for corpus acquisition.

## Why these books (and not "every Sindh rulebook")

Noviq does two jobs, and each pulls from a different shelf:

1. **Ground the instrument the document *cites*** (reactive) — driven by what
   notices / audit paras actually invoke. → Shelf **B** (financial/audit).
2. **Surface the provisions that *protect* the officer** (proactive) — inquiry,
   right of hearing, penalty scale, appeal, definition of misconduct. These live
   in the parent Act + conduct/discipline rules and matter **even when the
   document never cites them.** → Shelf **A**.

A book earns a place only if it is *cited against* an officer or *contains his
shield*. Everything else is opportunistic.

## Legend

- Status: ✅ indexed · 🟡 partial · ⬜ missing
- **Freq** = mentions across the 4 AGP gold reports in `Desktop/notices/audit_paras/`
  (empirical, procurement/financial books only).
- "Validated" = after indexing, a real notice/para that cites this book was run
  and the correct provision was quoted (not a neighbouring statute's Rule N).

## Definition of done — per book

1. [ ] Source the **current consolidated Sindh edition** (not a superseded/other-province PDF).
2. [ ] Record edition + provenance + who verified it in the table.
3. [ ] Index **shared** (owner NULL / global), the same path as E&D 1973 / RSO 2000.
4. [ ] Validate detection on one real citing document; confirm verbatim quote + citation.
5. [ ] Tick the row.

---

## Tier 1 — do first (highest leverage)

Unlocks audit paras (B) and strengthens *every* notice reply (A).

| ✔ | Book (official title) | Shelf | Status | Freq | Edition to source | Verified by | Notes |
|---|---|---|---|---|---|---|---|
| ☑ | Sindh Civil Servants (Efficiency & Discipline) Rules, 1973 | A | ✅ | — | — | — | 172 chunks. Primary notice statute (SC01–SC10). |
| ☑ | Removal from Service (Special Powers) Sindh Ordinance, 2000 | A | ✅ | — | — | — | 80 chunks. SC12 cites §3 / §5(4). |
| ☑ | Sindh Public Procurement Rules 2010 + Act 2009 (SPPRA) | B | ✅ | ~70 | — | — | Biggest single audit-para citation source. |
| ⬜ | **General Financial Rules (GFR), Volume I** | B | ⬜ | ~44 | [S1] finance.gov.pk, Updated Ed. (1979 base, 2019 update) | | Federal, still applied provincially. Cited as **"Rule N"** *and* **"Para N of GFR"** — see tokenizer follow-up. |
| ⬜ | **Sindh Financial Rules, Volume I** | B | ⬜ | ~25 | [S2] pprasindh.gov.pk (also finance.gos.pk) | | Cited "Rule 88 / Rule-23 of Sindh Financial Rules Vol-I". Confirm it is **Vol-I** consolidated. |
| ⬜ | **Sindh Civil Servants Act, 1973** (parent Act) | A | ⬜ | — | [S3] sindhlaws.gov.pk (SindhCode); prefer "updated till 2013" | | Source of hearing/appeal/terms; APs cite "Rule 26 of Sindh Civil Servant Act 1973". |
| ⬜ | **Sindh Civil Servants (Conduct) Rules, 2008** | A | ⬜ | — | [S4] sindhhighcourt.gov.pk | | Defines *misconduct* — the thing most show-cause notices allege. Title/year now confirmed (2008). |

## Tier 2 — high value, situational

| ✔ | Book | Shelf | Status | Freq | Edition to source | Verified by | Notes |
|---|---|---|---|---|---|---|---|
| ⬜ | Sindh Delegation of Financial Powers & Financial Control Rules, 2019 | B | ⬜ | 2 | [S5] sindhforests.gov.pk (also finance.gos.pk) | | The "who was competent to sanction" defense. |
| ⬜ | Sindh Civil Servants (Appointment, Promotion & Transfer / APT) Rules, 1974 | A | ⬜ | — | [S6] sindhhighcourt.gov.pk | | Service matters; already the APT-1974 vs E&D-1973 disambiguation case. |
| ⬜ | Fundamental Rules & Supplementary Rules (FR & SR) Vol I & II | C | ⬜ | — | [S7] finance.gov.pk, Updated Ed. 2018 | | Pay/leave/TA-DA basis; TA-DA overpayment is a classic para. Federal — check Sindh adoption. |
| ⬜ | Travelling Allowance (TA/DA) Rules | C | ⬜ | — | (within FR&SR / Sindh TA rules) | | |
| ⬜ | Sindh Treasury Rules / Central Treasury Rules | B | ⬜ | 3 | [S8] finance.gos.pk/RulesRegulations/TreasuryRules | | Minor; "Rule-303 of Central Treasury Rules" seen once. |

## Tier 3 — as demand appears

| ✔ | Book | Shelf | Status | Freq | Notes |
|---|---|---|---|---|---|
| 🟡 | Sindh Local Government Act, 2013 (SLGA) | D | 🟡 | ~11 | Only 118 chunks — extraction under-indexed; re-investigate. AP02/AP04 are council audits. Re-source [S9] lgdsindh.gov.pk. |
| ⬜ | Local Councils (Accounts / Budget) Rules | D | ⬜ | — | Pairs with SLGA. [S10] "Compendium of Rules framed under SLGA 2013" (lgdsindh.gov.pk) bundles these. |
| ⬜ | Sindh Civil Servants (Pension) Rules | C | ⬜ | — | Pension/gratuity overpayment paras. |
| ⬜ | Account Code / Audit Code | B | ⬜ | — | Accounting-standard citations. |
| ⬜ | NAB Ordinance 1999 | E | ⬜ | — | When a para escalates to a reference. |
| ⬜ | Sindh Enquiries & Anti-Corruption Act + rules | E | ⬜ | — | ACE proceedings. |
| ⬜ | Prevention of Corruption Act, 1947 | E | ⬜ | — | |

## Tier 4 — opportunistic only (do NOT chase)

Appear once each in the golds; index only if a real document needs them:
Grant-in-Aid Rules 2021 · Sindh Rental Premises Ordinance 1979 · Sindh Hospital
Waste Management Rules 2014 · PDMA (Provincial Disaster Management Authority)
rules.

---

## Guardrails

1. **Index deliberately, one tier at a time.** Every added book widens retrieval
   and gives `instrument_scope()` more "Rule 4"s to disambiguate. Re-validate
   detection after each book; don't bulk-dump.
2. **Edition is the real risk.** Noviq quotes *verbatim with a citation.* GFR,
   Conduct Rules, FR&SR are old federal instruments adopted+amended by Sindh —
   source the **current consolidated Sindh edition**. A confidently-cited *wrong*
   edition is worse than an honest ⚠ miss.
3. **Shared scope, not per-user.** Rulebooks are indexed global (owner NULL) like
   E&D/RSO so every officer sees them; they are corpus law, never attachments.

## Code follow-up unlocked by this list

- **`provision_tokens()` — add a `Para N` variant.** GFR is cited both as
  "Rule N" and "Para 213(5) of GFR" / "Para 149 of GFR Vol-I". The current
  tokenizer handles `Rule N(x)`; without `Para N`, GFR paragraph-citations won't
  verify even after indexing. Do this when GFR goes in.

## Official source candidates (located 2026-07-10 — VERIFY EDITION BEFORE INDEXING)

> ⚠️ These are the *best located* government-hosted copies, not yet edition-verified.
> Noviq quotes **verbatim with a citation**, so before indexing each book: confirm
> it is the **current consolidated version** in force in Sindh (amendments folded
> in), prefer a `.gov.pk` host over Scribd/3rd-party, and record who verified it in
> the table's "Verified by" column. A confidently-cited superseded edition is worse
> than an honest ⚠ miss.

| Ref | Book | Official-ish URL |
|---|---|---|
| S1 | GFR Vol-I | https://www.finance.gov.pk/publications/GFR_Vol_I.pdf |
| S2 | Sindh Financial Rules | http://www.pprasindh.gov.pk/downloads/FinancialRules141117.pdf · https://finance.gos.pk/RulesRegulations/Index |
| S3 | Sindh Civil Servants Act 1973 | http://sindhlaws.gov.pk/setup/publications_SindhCode/PUB-15-000293.pdf · https://sstsindh.gov.pk/admin/upload/rules/THE%20SINDH%20CIVIL%20SERVANTS%20ACT,%201973.pdf |
| S4 | Sindh Civil Servants (Conduct) Rules 2008 | https://sindhhighcourt.gov.pk/laws/SINDH%20CIVIL%20SERVANTS%20(CONDUCT)%20RULES,%202008.pdf |
| S5 | Sindh Delegation of Financial Powers & Financial Control Rules 2019 | https://sindhforests.gov.pk/uploads/files/THE_SINDH_DELEGATION_OF_FINANCIAL_POWERS_AND_FINANCIAL_CONTROL_RULES_2019.pdf |
| S6 | APT Rules 1974 | https://sindhhighcourt.gov.pk/laws/SINDH%20CIVIL%20SERVANTS%20(APPOINTMENT,%20PROMOTION%20AND%20TRANSFER)%20RULES,%201974.pdf |
| S7 | FR & SR Vol I & II (2018) | https://www.finance.gov.pk/publications/compilation_of_fundamental_rules_2018_vol_I.pdf · https://www.finance.gov.pk/publications/compilation_of_fundamental_rules_2018_vol_II.pdf |
| S8 | Sindh Treasury Rules | https://finance.gos.pk/RulesRegulations/TreasuryRules |
| S9 | Sindh Local Government Act 2013 | https://lgdsindh.gov.pk/wp/wp-content/uploads/2023/09/Act-2013.pdf · http://www.pas.gov.pk/uploads/acts/Sindh%20Act%20No.XLII%20of%202013.pdf |
| S10 | Compendium of Rules framed under SLGA 2013 | https://lgdsindh.gov.pk/wp/wp-content/uploads/2023/08/CompendiumOfRulesFramedUnderSindhLocalGovernmentAct-2013.pdf |

## Open scoping question

**Primary user — Secretariat/provincial officers, or local-government (council)
officers?** AP02/AP04 golds are council audits; if councils are a primary
audience, SLGA + Local Councils rules move up into Tier 1.
