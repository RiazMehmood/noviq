# Spec Extension (Feature 010): Secretariat-Summary Drafting Refactor

**Extends**: Feature `010-one-ask-drafting` (extension, NOT a new feature)
**Feature Branch**: `011-postgres-pgvector-port` (continue on existing branch — no new branch)
**Created**: 2026-06-16
**Status**: Draft
**Input**: Higher-ups' brief — transform output from a "legal/research narrative" into a
"Government of Sindh Secretariat summary" fit for movement on an official file to
Secretaries / ACS / Chief Secretary. 11 named changes.

> **Why an extension of 010**: 010 owns the citation-grade drafting contract. This refactor
> reshapes the *output form* (precision, structure, style) without changing the two-lane
> verbatim guarantee. FR-/SC- IDs below are local to this document.

---

## Locked decisions (settled with the architect, 2026-06-16)

These are binding for every task below:

- **D1 — Citation style = verbatim-compressed + inline (NOT model-paraphrased).** The model
  never authors legal meaning or numbers (two-lane invariant, `mod.rs:7-9`, and the
  citation-grade contract). "Readable" is achieved by *shortening* the verbatim span to one
  operative clause and rendering it inline with its deterministic label — not by letting the
  7B paraphrase what a rule "permits." Change 3's example phrasing is the *target shape*, met
  with verbatim words, not generated ones.
- **D2 — Domain logic = generic, no hardcoded term lists.** No static SPPRA/procurement
  boost/suppress vocabulary (violates the standing "no document-specific keywords" and
  "domain-general" rules). Domain emphasis comes from (a) the doc-type profile, (b) the
  existing corpus-rarity discriminative-terms leg (Fix #3), and (c) top-N source-document
  narrowing — all derived from data/config, not literals.
- **D3 — Document type = one skeleton + type *profiles*.** Keep the single compile-time
  skeleton (`skeleton.rs`); a lightweight profile modulates per-section guidance, citation
  caps, and retrieval emphasis. Do NOT build per-type skeletons (the 009 template trap).
- **D4 — Sequencing.** Corpus-independent changes (US1–US5, US7) land first and are
  validatable now on the re-indexed prose corpus. Precision changes (US6) and the tabular
  re-extraction (US8) are validated after the clean-text/table work, so thresholds are tuned
  on real scores, not fragmented ones.

---

## Problem & Context

The 2026-06-16 post-re-index draft (prose-quote garble now fixed) still reads like a legal
research memo, not a secretariat summary:

- **~16 citations** across five sections (Legal §2 alone: 6); target ≤ 3 total.
- **Off-topic provisions** cited and force-explained (contract-register/coercion Rule 5(4)(i),
  anticipatory-approval Section 8.7, project-director 9.5/9.11, conveyance Rule 27(5)(i)),
  while the genuinely governing **Part III(4)(d) Direct Selection** rule was buried in §4.
- **Cross-section duplication** (Rule 5(4)(i) in §1+§2; Section 8.7 in §2+§4; advertisement
  rule in §1+§5).
- **Academic explanation** of rules in the abstract, **background restated** in every section,
  **letter-style prayer** ("we humbly request… we appreciate your prompt attention").
- **Tabular source garble** (Delegation of Financial Powers columns interleaved even under
  pdftotext `-layout`) and the deferred **year-mislabel** ("Rule 2017(4)").

The provisions ARE retrieved — the failure is precision, placement, volume, and style.

---

## User Scenarios & Testing *(mandatory)*

### US1 — Document-type profile shapes the draft (Change 1) — Priority P2
A lightweight classifier maps the officer's request to a document type (Procurement Note,
Administrative Approval, Financial Sanction, Transfer/Posting, Policy Approval, Cabinet
Summary, Development Scheme). The type selects a **profile** that modulates section guidance,
citation policy, and retrieval emphasis — over the single shared skeleton (D3).

**Independent test**: the Canon-scanner ask classifies as *Procurement Note*; the Legal
section's guidance and the retrieval emphasis reflect procurement, with no hardcoded SPPRA
literals in code (D2).

**Acceptance**:
1. Given a procurement request, when classified, then `doc_type = ProcurementNote` and its
   profile is applied.
2. Given an unrecognised request, when classified, then a `General` profile applies (never
   an error; never a per-case template).

### US2 — Section-specific citation policy, enforced (Change 2) — Priority P1
Hard caps per section, enforced in the verifier:

| Section | Max citations |
|---|---|
| Background and Purpose | 0 (narrative only) |
| Legal and Procedural Position | 2 (directly applicable only) |
| Financial Aspect | 1 (sanction/budget authority only) |
| Proposal and Recommendation | 0 |
| Prayer for Approval | 0 |

**Independent test**: any draft has ≤ 3 citations total, all in Legal/Financial; BG/Proposal/
Prayer contain zero citations.

**Acceptance**:
1. Given >2 citation markers in Legal, when assembled, then only the 2 highest-priority render;
   the rest are dropped (not flagged — over-cap is a style cut, not a verification failure).
2. Given any citation marker in BG/Proposal/Prayer, when assembled, then it is suppressed and
   the prose stands alone.

### US3 — One provision, one section, one quote (Change 4) — Priority P1
`MAX_SECTIONS_PER_CARD = 1`; each card is assigned to its **best-fit** section (argmax of
section scores), not first-come-first-served. No provision is quoted in two sections. Legal is
the normal home for provisions; Financial may hold the single sanction-authority card.

**Independent test**: no chunk id appears as a rendered citation in more than one section.

**Acceptance**:
1. Given a card scoring highest for Legal, when assigned, then it serves Legal only.
2. Given the prior cross-section duplicates, when re-run, then each appears once.

### US4 — Verbatim-compressed, inline citations (Change 3, per D1) — Priority P1
The quoted span is compressed to one operative clause and rendered inline with its label, so
the sentence reads as application, not block-quote — while remaining a verbatim char-slice
that passes the gate. The evidence layer keeps the fuller excerpt for verification.

**Independent test**: a Legal citation renders as a single clause + label (≤ ~20 words quoted),
not a multi-sentence dump; `quote_verifies` still passes.

**Acceptance**:
1. Given a long excerpt, when cited, then the rendered quote is one operative clause, verbatim.
2. Given any rendered quote, when checked, then it is a normalised substring of the card
   excerpt (invariant 1 holds).

### US5 — Secretariat drafting style (Changes 5, 8, 9, 10) — Priority P1
A "Government of Sindh Secretariat Summary" style profile replaces the current writer rules:
concise, formal, decision-oriented; **no advocacy/persuasion, no academic commentary, no
greetings, no "we request"/"kindly approve"/"we await."** §1 establishes the case; §2–§5
assume the background is known and do not restate it. Prayer = 1–2 sentences, no citation.
Financial fallback (no figures) uses a structured line ("Financial implications shall be met
from the approved budget allocation; detailed estimates shall be placed on record before
issuance of the purchase order.") instead of "costs will be determined later."

**Independent test**: the draft contains none of the banned phrases; the prayer is ≤ 2
sentences; background facts appear once.

**Acceptance**:
1. Given §2–§5, when composed, then no banned filler phrase appears and background is not
   restated.
2. Given no officer figure, when the Financial section composes, then the structured fallback
   is used (no "to be determined later").

### US6 — Relevance floor + generic doc-set narrowing (Changes 6, 7, per D2) — Priority P2 *(post clean-text)*
Add an absolute eligibility threshold so marginal cards are dropped; prefer a thin/uncited
section over a wrong one. Narrow retrieval to the top-N source documents by aggregate relevance
to the request (generic; no keyword lists), suppressing cross-statute noise.

**Independent test** (after US8): the Canon-scanner draft cites no contract-register,
secretariat-return, project-director, or conveyance provision.

**Acceptance**:
1. Given no card above `MIN_RELEVANCE`, when a section composes, then it is uncited narrative
   (no forced card).
2. Given a corpus of mixed statutes, when retrieving for a procurement ask, then off-domain
   documents fall below the doc-set cut.

### US7 — Retrieval audit logging (Change 11) — Priority P3
Per selected card: section assigned, vector score, lexical score, fusion score, reason
selected. Per rejected card: reason (below floor / duplicate / out-of-doc-set / over-cap).
Drives precision evaluation.

**Acceptance**: a single ask emits a structured per-card accept/reject log line set.

### US8 — Ingestion structural quality (expanded) — Priority P2 *(extraction, one re-index)*
Re-scoped per the 2026-06-17 post-meeting roadmap (`docs/post-meeting-roadmap-2026-06-17.md`).
US8 absorbs the former US11 index-time data-hygiene scope so a single re-index covers all of
it. Two parts:

**R1 — Robust ingestion (the real work).**
- *Tabular module* (`parsers/table.rs`): multi-column tables (Delegation of Financial Powers)
  interleave columns under pdftotext `-layout`, scrambling sanction-authority text. Detect
  tabular regions from `pdftotext -bbox` word geometry, cluster words into a grid, and
  linearize each row into ONE self-describing unit (column headers folded in) so each row is a
  coherent, citable provision. Simple grids only in v1; prose regions are left untouched.
- *Year-rule (folds in, was a separate "year-guard"):* the structure parser must treat a bare
  1800–2099 4-digit in a title position as a statute **year**, not a provision ordinal
  (`looks_like_year`). This is parser *correctness*, not a bolt-on guard.

**R2 — Generic boilerplate de-prioritisation (D1), measured AFTER R1.** Much of D1 is
extraction-artifact junk that R1 removes for free; the residual genuine boilerplate ("Contact
Us", form pages) is generic/structural (low prose-content ratio + no provision identity),
NEVER a per-doc keyword blocklist. **Safety rules (mandatory — dropping a real provision is
silent + unrecoverable):** (1) **flag, don't delete** — set a boilerplate flag; retrieval
excludes flagged chunks from ranking; the chunk stays, auditable + reversible; (2)
**provision-label-keep invariant** — R2 may only touch chunks that are simultaneously
label-less, marker-less, AND low-prose; (3) **conservative threshold** — keep-bias; (4)
**dry-run-review gate** — emit the exact would-flag list for human review against the labeled
set (101 known junk) and require zero false positives before any re-index.

**Independent test**: (a) the DG/Ombudsman financial-powers rows extract as discrete, in-order
authority statements and the Financial section quote is not column-scrambled; (b) 0 year-as-rule
labels after re-index; (c) the R2 dry-run shows zero false positives on the labeled set.

### US9 — Legal-body analytical moves + urgency + auto-extracted stakeholders (per-move caps) — Priority P2
The Legal section is upgraded from a flat "here are some provisions" into a fixed **ordered
analytical sequence** that reads like a real secretariat legal opinion, and the Background
opens with factual urgency. Five rhetorical *moves*:

1. **Urgency** *(Background, not Legal)* — surface the factual, time-bound reason the matter is
   pressing. **Detected-and-surfaced from the officer's own words** (a deadline / "within one
   month" / financial-year close / figure-at-risk span), never generated. **Omitted entirely
   when the officer states no such fact** — we never manufacture urgency, and never use
   advocacy phrasing (FR-008 still holds).
2. **Legal basis** *(Legal, move 1)* — the governing provision(s) authorizing/governing the
   matter. If none is retrieved, that gap *is* the finding (flag "no clear legal basis").
3. **Stakeholders** *(Legal, move 2)* — name the parties involved. **Auto-extracted**, no extra
   officer question (D-US9-c).
4. **Stakeholder legal status** *(Legal, move 3)* — each named party's standing/powers in *this*
   matter, asserted **only where a cited provision supports it**; otherwise the party is
   named-only with no powers claimed.
5. **Normative procedure** *(Legal, move 4)* — the prescribed steps, from cited provisions.

**Decisions** (architect, this session):
- **D-US9-a** Moves 2/4/5 sit inside the **Legal** section as an ordered internal sequence
  (basis → stakeholders → status → procedure); move 1 (urgency) opens **Background**. Financial /
  Proposal / Prayer are unchanged and still wrap around. One skeleton retained (D3).
- **D-US9-b** Urgency is verbatim-safe: surface the officer's own time/consequence span; omit if
  absent. Never model-generated (extends FR-007).
- **D-US9-c** Stakeholders auto-extracted from **(officer-sentence entities) ∪ (role-holders
  named in retrieved Legal cards)**, deterministically (regex + a *generic* role-noun lexicon,
  Tier-1 discipline like `intent.rs` — no model NER in v1, no per-document terms; D2 intact).
- **D-US9-d** **Per-move** citation caps replace the flat Legal section cap **and supersede the
  ≤3 global cap**: Basis ≤2, Status ≤2 (flat in v1; per-stakeholder is v2), Procedure ≤2,
  stakeholder-naming = 0 (narrative). Background / Proposal / Prayer = 0, Financial = 1.
- **D-US9-e** One LLM call for Legal (ordered-move guidance + move-bucketed cards), **not** N
  sub-calls (the 7B writer is minutes-long; N calls would be intolerable). Per-move caps are
  enforced in the verifier via a **move tag on each Legal card**.
- **D-US9-f** Per-move grounding (FR-018): moves 4/5 assert only cited-provision-supported
  content; ungrounded assertions are dropped/flagged. Verbatim/refuse-or-flag guarantee intact.

**Independent test**: (a) an ask with "within one month" surfaces a factual urgency line in
Background; an ask with no time fact surfaces none. (b) The Legal section renders in move order
with stakeholders named and only provision-supported powers asserted. (c) Per-move caps hold:
≤2 basis + ≤2 status + ≤2 procedure citations; no fabricated stakeholder power; the two-lane
verbatim gate still passes on every rendered quote.

---

### US10 — Move-per-paragraph skeleton + 2-sentence cap + advocacy strip (precision) — Priority P1
The live US9 draft proved the analytical moves work but read as a long run-on (3 moves crammed in
one Legal paragraph) and the 7B padded every paragraph with advocacy. US10 makes the summary
**one move per short paragraph** and enforces brevity **deterministically**.

**New skeleton (output order), each paragraph ≤2 sentences:**
1. **Urgency** (cap 0) — time-bound fact + what is sought.
2. **Legal validation** (cap 2) — the governing provision(s); or "no clear basis found".
3. **Stakeholders** (cap 0) — names only (extracted list).
4. **Stakeholder status** (cap 2) — powers only where a cited card supports; name-only otherwise.
5. **Procedure** (cap 2) — the prescribed steps, cited.
6. **Financial** (cap 1) — **conditional: rendered only when the officer states a figure**, else omitted.
7. **Prayer** (cap 0) — one sentence.

Background and Proposal are dropped (what-is-sought folds into ¶1, recommendation into the prayer).

**Decisions** (architect, this session):
- **D-US10-a** Each move is its own section → US9's per-move `LegalMove` tag / `classify_legal_move`
  / per-move verify counting are **removed**; standard per-section caps apply. Simpler, same effect.
- **D-US10-b** **No hard truncation** (revised after architect review): a blind sentence cap could
  cut a sentence carrying the governing rule + its citation — a citation-grade violation. Brevity
  comes from (i) one move per paragraph and (ii) the advocacy strip; a paragraph stays as long as it
  needs to state its move + citation, minus padding. *(Optional, deferred: a citation-SAFE trim that
  drops only trailing sentences containing no citation and no figure — never the governing rule.)*
- **D-US10-c** Deterministic **advocacy-phrase strip** over a *generic* lexicon (no per-doc terms; D2):
  remove pleading/embellishment ("it is crucial", "imperative", "promptly", "significantly
  contribute", "we request", "kindly", etc.) so padding dies regardless of the model.
- **D-US10-d** Financial section is **conditional** on `intent.figures` being non-empty.
- **D-US10-e** Compose-time only — **no re-index**. Two-lane verbatim guarantee unchanged (D1/FR-007).

**Independent test**: (a) the skeleton renders 5 move-paragraphs + (financial iff figures) + a
one-sentence prayer, in order; (b) advocacy phrases are absent from the rendered prose while every
fact/rule/citation survives; (c) a figure-less ask omits the Financial paragraph; (d) the verbatim
gate still passes on every rendered quote — no sentence carrying a citation is ever cut.

## Functional Requirements

- **FR-001** Classify the request into a document type via a generic, config/keyword-light
  classifier; fall back to `General`; never error (US1, D2/D3).
- **FR-002** A doc-type **profile** supplies per-section guidance, citation caps, and retrieval
  emphasis over the single skeleton (US1, D3).
- **FR-003** Enforce per-section citation caps and the ≤3 global cap in the verifier; over-cap
  citations are dropped as a style cut, never rendered (US2).
- **FR-004** `MAX_SECTIONS_PER_CARD = 1`; assign each card to its argmax section (US3).
- **FR-005** No provision renders as a citation in more than one section (US3).
- **FR-006** Compress the rendered citation to one operative verbatim clause + label; the
  verbatim-substring gate must still pass (US4, D1).
- **FR-007** The model never authors a rule number, quote, or legal meaning (unchanged
  invariant; D1) — re-affirmed because Change 3 must not erode it.
- **FR-008** Apply the Secretariat style profile; ban advocacy/greeting/"we request" phrasing;
  §2–§5 do not restate background (US5).
- **FR-009** Prayer ≤ 2 sentences, no citation (US5).
- **FR-010** Structured financial fallback when no officer figure exists (US5).
- **FR-011** Absolute relevance floor; thin-over-wrong; generic top-N doc-set narrowing (US6,
  D2). *Validated post-US8.*
- **FR-012** Structured per-card accept/reject audit log (US7).
- **FR-013** Column-aware extraction for tabular sources: detect tabular regions from
  `pdftotext -bbox` geometry, reconstruct the grid, and emit each row as one self-describing
  unit with headers folded in; prose regions unchanged (US8/R1).
- **FR-013a** The structure parser treats a bare 1800–2099 4-digit in a title position as a
  statute year, never a provision ordinal (`looks_like_year`) — parser correctness, index-time
  (US8/R1).
- **FR-013b** Generic boilerplate de-prioritisation (US8/R2): (i) **flag, not delete** —
  boilerplate is flagged and excluded from retrieval ranking, never removed from the corpus;
  (ii) **provision-label-keep invariant** — only a chunk that is simultaneously label-less,
  marker-less, and low-prose may be flagged; (iii) **conservative keep-bias threshold**;
  (iv) **dry-run-review gate** — the would-flag list is reviewed against the labeled junk set
  and must show zero false positives before any re-index. No per-doc keyword lists.
- **FR-014** The Legal section is composed as an ordered move sequence — (a) legal basis,
  (b) named stakeholders, (c) per-stakeholder legal status, (d) normative procedure — in one
  writer call over move-bucketed cards (US9, D-US9-a/e).
- **FR-015** Urgency is surfaced in Background only from an officer-supplied time/consequence
  span (verbatim), omitted when absent, never model-generated (US9, D-US9-b).
- **FR-016** Stakeholders are auto-extracted from officer-sentence entities ∪ role-holders named
  in retrieved Legal cards, deterministically; a stakeholder with no empowering provision is
  named-only with no powers asserted (US9, D-US9-c/f).
- **FR-017** Per-move citation caps (Basis 2 / Status 2 / Procedure 2 / naming 0) enforced in the
  verifier via a per-card move tag; this **supersedes** the flat Legal cap and the ≤3 global cap
  (US9, D-US9-d; amends FR-003 and SC-004).
- **FR-018** Per-move grounding: moves (c)/(d) assert only cited-provision-supported content;
  ungrounded assertions are dropped/flagged; FR-007 verbatim/refuse-or-flag intact (US9, D-US9-f).
- **FR-019** The skeleton is 7 single-move sections (urgency, legal-validation, stakeholders,
  stakeholder-status, procedure, financial, prayer); each move is its own ≤2-sentence paragraph;
  Background/Proposal removed; US9's per-move `LegalMove` machinery removed (US10, D-US10-a).
- **FR-020** Brevity is achieved WITHOUT truncation: one move per paragraph + advocacy strip; no
  sentence carrying a citation/governing rule may ever be cut (US10, D-US10-b). Optional citation-safe
  trailing-filler trim is deferred.
- **FR-021** A generic advocacy/pleading phrase lexicon is stripped from rendered prose
  deterministically (US10, D-US10-c).
- **FR-022** The Financial section renders only when the officer states a figure; otherwise it is
  omitted (US10, D-US10-d).

## Success Criteria (the higher-ups' gate)

On *"Draft note to Chief Secretary for allowing procurement through direct contracting with the
sole distributor of Canon scanners for 10 departments to be used in e-Daftar, as e-Daftar is
launching within one month."* the resulting draft:

- **SC-001** contains **no irrelevant rules** (no contract-register, secretariat-return,
  project-director, conveyance citations).
- **SC-002** contains **no duplicated provision** across sections.
- **SC-003** contains **no repeated background narrative**.
- **SC-004** *(amended by US9/FR-017)* citations are bounded by **per-move caps**: Legal ≤6
  (basis 2 + status 2 + procedure 2), Financial ≤1, Background/Proposal/Prayer = 0. (Was: ≤3
  total — superseded now that the Legal body carries the staged legal analysis.)
- **SC-005** Legal centers on the **Direct Selection / single-source** provision (Part
  III(4)(d)) as its lead authority.
- **SC-006** Prayer is one decision sentence; no banned filler anywhere.
- **SC-007** Reads as a movable Government of Sindh file summary without major editing.
- **SC-008** The two-lane verbatim guarantee is intact (no model-authored numbers/quotes/
  meaning; every rendered quote passes verbatim verification).

## Non-Goals

- No per-case uploaded templates / per-type skeletons (D3).
- No hardcoded domain term lists (D2).
- No change to the verbatim/refuse-or-flag guarantee (D1/FR-007).
- No retrieval-ranking rewrite beyond floor + doc-set narrowing.

## Sequencing

1. **Slice 1 (now, corpus-independent)**: US2, US3, US4, US5, US7 — caps, single-section
   ownership, compressed citations, style/prayer/financial-fallback, audit logging.
2. **Slice 2 (now)**: US1 doc-type profiles wiring the above per type.
3. **Slice 3 (extraction)**: US8 tabular re-extraction → re-index.
4. **Slice 4 (post clean-text)**: US6 relevance floor + doc-set narrowing, tuned on real
   scores; final success-criteria gate run.
