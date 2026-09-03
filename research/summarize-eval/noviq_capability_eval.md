# Noviq Legal-AI — Full Capability Eval (teacher-graded)

**Goal:** stress every capability a world-class legal AI for government officers must have —
retrieval, summarization, cross-doc comparison, conflict finding, amendments, deterministic
counting, note drafting, obligation extraction, and officer notice-defense — against the
**currently indexed corpus**, with a written answer key produced first, then Noviq graded against it.

**Corpus under test (15 library docs):** Constitution; Sindh Rules of Business 1986; SPPRA Rules
2010; Compendium under SLGA 2013; SLGA 2013; Delegation of Financial Powers 2019; Manual of
Secretariat Instructions 2010; Manual for Development Projects 2019; Sindh Public Procurement Act
2009; APT Rules 1974; Removal from Service (Special Powers) Ordinance 2000 (RSO); E&D Rules 1973.

**Capabilities exercised (all implemented):** `guidance` · `count` · `summarize_doc` · `compare` ·
`conflict` · `amendment_chain` · `action_items` · `draft` · `/help` notice-assist.

> **IMPORTANT — map-reduce is DISABLED in prod (`NOVIQ_MAPREDUCE=off`).** `summarize_doc`,
> `compare`, `conflict`, and `action_items` are therefore **rerouted through the single-pass
> Guidance path** — one bounded top-K retrieval, NOT a full-provision sweep. So (a) they are
> **retrieval-based and A/B-sensitive**, and (b) their expected answer covers the **key** provisions
> retrieved + cited, **not** exhaustive whole-document coverage. Effective live capability set =
> `guidance`, `count`, `amendment_chain`, `draft` (+ the four map-reduce phrasings collapsing to
> guidance). This is by design (kill-switch avoids minutes-long sweeps on the weak GPU).

**Grade:** ✅ correct provision/number + substance · ⚠️ right topic, wrong/missing provision ·
❌ wrong/ungrounded · 🕳️ **expected data/feature gap** (we test it to confirm graceful behaviour).

**A/B (summary-recall):** items marked **[AB]** are retrieval-based → run twice, `ECHO_SUMMARY_RECALL`
ON vs OFF, and diff. Deterministic items (count, amendment_chain) are **not** A/B-sensitive.

---

## §1 — Retrieval / Guidance (semantic recall)  [all AB]

| # | Question | Expect → provision | Answer key |
|---|---|---|---|
| G1 | How can a company be barred from bidding on future government contracts, and on what grounds? | SPPRA **Rule 35** | Blacklisting for contract breach / non-performance bars future procurement |
| G2 | If a bidder disagrees with a procurement decision, what remedy do they have? | SPPRA **Rule 32** (+31) | Complaints redressal → **appeal to Review Committee**; only if bid security not withdrawn |
| G3 | Can a civil servant be kept out of office while an inquiry against them is ongoing? | RSO **§4** (/E&D) | Yes — **suspension** with immediate effect, or admissible leave (reasons recorded) |
| G4 | Someone was dismissed from service — can they challenge it, where, and within what time? | RSO **§10** | **30 days**, appeal to **Sindh Service Tribunal** |
| G5 | What money guarantee must a bidder provide, and does it apply to small contracts? | SPPRA **Rule 37** | **Bid security** above a value threshold; not below |
| G6 | What punishments can be imposed on a government servant guilty of misconduct? | E&D **Rule 4** (+3) | **Minor & major penalties**; grounds in Rule 3 |
| G7 | Step-by-step procedure once a civil servant is accused of misconduct? | E&D **Rule 5** (+6) | Charge + statement of allegations → Inquiry Officer/Committee |
| G8 | How much time must bidders be given to prepare and submit bids? | SPPRA **Rule 18** | Minimum response times, **NCB vs ICB** |

## §2 — Deterministic counting (`count`, SQL, no LLM — NOT AB)

| # | Question | Expect | Answer key (ground truth from DB) |
|---|---|---|---|
| C1 | How many Acts are in the library? | corpus | **4** acts |
| C2 | How many sets of Rules do we have? | corpus | **6** rules |
| C3 | How many rules are in the SPPRA Rules 2010? | within-doc | **90** rules |
| C4 | How many rules are in the Sindh Rules of Business 1986? | within-doc | **59** rules |
| C5 | How many articles are in the Constitution? | within-doc | **283** (≈280 real) |
| C6 | How many sections are in the Removal from Service Ordinance 2000? | within-doc | **16–17** (§§1–16 + preamble) |
| C7 🕳️ | How many departments are listed in the Rules of Business 1986? | **GAP** | No department count-unit (Schedule-I trapped in tables) → expect graceful fallback (structural breakdown or "can't count"), **NOT** a hallucinated number |
| C8 🕳️ | What is the total number of penalties defined in the E&D Rules? | **GAP** | "Penalty" not a structural level → expect fallback, not a made-up total |

## §3 — Summarization (`summarize_doc`, map-reduce — SLOW, minutes)  [AB]

| # | Question | Expect | Answer key |
|---|---|---|---|
| S1 | Summarize the Sindh Removal from Service (Special Powers) Ordinance 2000. | full-doc | Must cover: grounds for dismissal/removal (§3), suspension (§4), inquiry (§5–8), representation/review (§9), appeal to Service Tribunal (§10), overriding effect (§11) |
| S2 | Give a broad overview of the SPPRA Rules 2010. | broad | Procurement principles, bidding methods (Rule 15), response times (18), pre-qualification (27), bid/performance security (37/39), blacklisting (35), appeals (31/32), consultants selection (66–80) |

## §4 — Cross-document comparison (`compare`, map-reduce)  [AB]

| # | Question | Expect | Answer key |
|---|---|---|---|
| X1 | Compare the appeal mechanism under the Removal from Service Ordinance 2000 with the E&D Rules 1973. | compare | RSO: appeal to **Sindh Service Tribunal within 30 days** (§10) after §9 review. E&D: representation/appeal to the **Chief Minister** (Rule 7). Should contrast forum + timeline |

## §5 — Conflict / overriding provisions (`conflict`, map-reduce)  [AB]

| # | Question | Expect | Answer key |
|---|---|---|---|
| K1 | Are there provisions that override or take precedence over other laws in our procurement and service rules? | non_obstante | SPPRA **Rule 90** (these rules prevail over other govt rules); RSO **§11** (ordinance overrides conflicting law). 17 non-obstante clauses exist in-graph — expect it surfaces the overriding clauses, not invents conflicts |

## §6 — Amendments (`amendment_chain`, deterministic — NOT AB)

Reads the `amendment_events` table (108 events: 83 substituted + 25 inserted). Coverage is
**Constitution-dominant** (98 events; APT 8, SPPA 1, E&D 1). Other consolidated docs legitimately
have none (no amendment footnotes) — that's expected, not a bug.

| # | Question | Expect → provision | Answer key |
|---|---|---|---|
| A1 | How has Article 105 of the Constitution been amended over time? | Constitution **Art 105** | 3 events: **substituted** by *P.O. 14 of 1985*; **substituted** by the *Constitution (8th Amdt.) Act 1985*; **inserted** by *Act 10 of 2010* (18th Amdt). Expect a real, cited timeline — not a fabrication |
| A2 | When was Article 10A inserted into the Constitution, and by what? | Constitution **Art 10A** | **Inserted** by a Constitution Amendment Act (per parsed footnote). Expect the insertion event + instrument, not prose invention |
| A3 🕳️ | How has Rule 4 of the SPPRA Rules been amended? | **GAP (correct)** | SPPRA is consolidated text with no amendment footnotes → expect "no amendment history recorded," **NOT** a fabricated timeline. Confirms graceful behaviour on a doc with no events |

## §7 — Note drafting (`draft`)  [AB]

| # | Question | Expect | Answer key |
|---|---|---|---|
| D1 | Draft a note for the Chief Secretary on the procedure for disciplinary action against a civil servant under the E&D Rules 1973. | draft | A structured note grounded in E&D **Rule 5** (charge/allegations), **Rule 6** (inquiry), **Rule 4** (penalties), with `[n]` citations — must not invent procedure |

## §8 — Obligation / action-item extraction (`action_items`, map-reduce)  [AB]

| # | Question | Expect | Answer key |
|---|---|---|---|
| O1 | What are the obligations / action items for a procuring agency under the SPPRA Rules? | checklist | Should list duties: obtain prior approval (Rule 14), maintain procurement records (Rule 9), give minimum response time (18), pre-qualify fairly (27), handle complaints/appeals (31–32) — each cited |

## §9 — Officer notice-defense (`/help` notice-assist)  [manual, not scriptable]

| # | Test | Expect | Answer key |
|---|---|---|---|
| N1 | `/help` + attach a show-cause notice citing E&D Rule-5 (gold: `SC12` / any E&D notice) | support brief | Charge stated once; invoked provision verbatim + `[n]`; **provisions that PROTECT the officer** (right to inquiry Rule 5/6, hearing, penalty scale Rule 4, appeal Rule 7) verbatim from the **same** statute; then CoWrite critique. Must refuse to write the reply. Local-engine-pinned. *(Run in browser — needs file attach; local golds at `C:\Users\hp\Desktop\notices\`.)* |

---

## Run protocol
1. **A/B passes** for [AB] items: run all with `ECHO_SUMMARY_RECALL=on` (current), save answer+citations+trace_id; then flip **off**, restart, re-run, save.
2. **Deterministic items** (C*, A1): run once (flag can't change them) — verify vs ground-truth numbers.
3. **Map-reduce items** (S1,S2,X1,K1,O1): slow (minutes each on the A40) — run sequentially, longer timeouts. If time-boxed, prioritise S1, X1, K1.
4. **N1**: manual browser test (attachment flow), do last.
5. Grade every item vs the key; produce a per-capability scorecard + the ON-vs-OFF delta table for [AB] items.
6. Decision: keep summary-recall ON in prod if [AB] items improve with no control regressions; else revert.

## Known caveats baked into expectations
- Summary index has **text noise** (leaked model scratch reasoning in some gists) — separate fix; doesn't invalidate recall grading.
- Constitution article count is 283 distinct vs ~280 real (duplicate-node data bug, absorbed by DISTINCT).
- `amended_by` and department/penalty counts are **known gaps** — items A1/C7/C8 exist to prove Noviq degrades gracefully instead of hallucinating.
- Map-reduce capabilities were deliberately kept for **named-doc / broad** scopes only; topical summaries route to fast guidance (by design).
