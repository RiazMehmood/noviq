# Track 2 Functional Eval — Summary-Recall (ECHO_SUMMARY_RECALL)

**Method (teacher):** I wrote each question from the *current* indexed corpus, answered it
myself from the **original provision text** (my answer key, below), saved it. Then each
question is run on Noviq **twice** — once with `ECHO_SUMMARY_RECALL=on`, once `off` — corpus-wide
(all docs selectable). I grade Noviq's answer against my key and diff ON vs OFF.

**What we're actually testing:** does the summary index surface the *right provision* that plain
vector+FTS misses? So most questions are phrased in **plain-language / effect terms** whose wording
differs from the statute's wording (a semantic gap FTS keyword-match can't bridge). Two "control"
questions use statute vocabulary (FTS-easy) to confirm summary recall doesn't *hurt* the easy cases.

**Grade per item:** ✅ cites the expected provision AND states the correct substance · ⚠️ partial
(right doc/topic, wrong or missing provision) · ❌ wrong or ungrounded. Plus ON-vs-OFF delta.

---

## Answer key (ground truth = original indexed text)

| # | Question (as asked to Noviq) | Expected doc | Expected provision(s) | My answer (key) | Type |
|---|---|---|---|---|---|
| 1 | "How can a company be barred from bidding on future government contracts, and on what grounds?" | SPPRA Rules 2010 | **Rule 35** (Blacklisting of Suppliers, Contractors & Consultants) | A supplier/contractor/consultant who breaches contract or fails to perform can be **blacklisted** by the procuring agency, barring it from future procurement. | semantic-gap |
| 2 | "If a bidder disagrees with a procurement decision, what remedy do they have?" | SPPRA Rules 2010 | **Rule 32** (Appeal to Review Committee) [+ Rule 31 complaints redressal] | A dissatisfied bidder first goes to the agency's **complaints redressal committee**, then may **appeal to the Review Committee** — provided he has **not withdrawn his bid security**. | semantic-gap |
| 3 | "Can a civil servant be kept out of office while an inquiry against them is ongoing?" | RSO 2000 (or E&D 1973) | **RSO Section 4** (Suspension) [E&D also relevant] | Yes — a person facing action under §3(1) may be **placed under suspension with immediate effect** if the competent authority deems it necessary; alternatively may be sent on admissible leave (reasons recorded). | semantic-gap |
| 4 | "Someone was dismissed from service — can they challenge it, where, and within what time?" | RSO 2000 | **Section 10** (Appeal) | Yes — a person aggrieved by a **final order under §9** may, **within 30 days**, appeal to the **Sindh Service Tribunal** (under the Sindh Service Tribunals Act 1973). | semantic-gap |
| 5 | "What money guarantee must a bidder provide when submitting a bid, and does it apply to small contracts?" | SPPRA Rules 2010 | **Rule 37** (Bid Security) | Bidders must furnish **bid security** for contracts above a specified value threshold (form/amount/conditions per rule) — i.e. not required below the threshold. | semantic-gap |
| 6 | "What punishments can be imposed on a government servant found guilty of misconduct?" | E&D Rules 1973 | **Rule 4** (Penalties) [+ Rule 3 grounds] | Rule 4 lists **minor and major penalties**; Rule 3 sets the grounds (inefficiency, misconduct, corruption, etc.). | semantic-gap |
| 7 | "What is the step-by-step procedure once a civil servant is accused of inefficiency or misconduct?" | E&D Rules 1973 | **Rule 5** (procedure) [+ Rule 6 inquiry] | The authorised officer **frames a charge + statement of allegations**, informs the accused, and an Inquiry Officer/Committee conducts the inquiry per Rules 5–6. | semantic-gap |
| 8 | "How much time must bidders be given to prepare and submit their bids?" | SPPRA Rules 2010 | **Rule 18** (response time) | Rule 18 sets **minimum response times**, distinguishing **National Competitive Bidding vs International Competitive Bidding** (longer for ICB). | semantic-gap |
| 9 | "Who is the appointing authority for posts in Basic Pay Scale 16 and Basic Pay Scale 18?" | APT Rules 1974 | **Rule 6** | For BS-16 / BS-18 posts the **Secretary concerned** is the appointing authority (per Rule 6). | control (FTS-easy) |
| 10 | "What is the short title and commencement of the SPPRA Rules?" | SPPRA Rules 2010 | **Rule 1** | Rule 1 gives the **short title** ("Sindh Public Procurement Rules, 2010") and the **commencement** (in force from notification). | control (FTS-easy) |

---

## Run protocol (after your approval)
1. Flag is currently **ON** in prod `.env` (server restarted, vLLM ready). Run all 10 → save answers+citations+trace_id.
2. Revert flag to **off**, restart, run all 10 again → save.
3. Grade both passes vs the key; produce ON-vs-OFF diff table (esp. items where ON surfaces the expected provision that OFF misses).
4. Decide: keep ON in prod if ON ≥ OFF with no regressions on controls; else revert.

## Notes / caveats
- Runs are **corpus-wide** (all docs), matching real usage — expected doc is where the answer *should* come from, not a scope restriction.
- Summary index has known text noise (leaked model scratch reasoning in some gists) — flagged separately; affects embedding quality, not this recall test's validity.
- Grading substance is judged against the original text quoted above, not against the summaries (avoids circularity).

---

## RESULTS — 2026-07-12 (run after full corpus re-index + build_summaries regeneration)

Both passes run corpus-wide via `POST /api/query`; graded against the answer key above.

| Q | Expected | ON | OFF | Winner |
|---|---|---|---|---|
| 1 | Rule 35 (blacklisting) | ✅ | ✅ | tie |
| 2 | Rule 31/32 (complaint → appeal) | ✅ (leads with complaint-redressal committee) | ⚠️ (leads with debriefing) | **ON** |
| 3 | Suspension (E&D 5(1)/RSO §4) | ✅ | ✅ | tie |
| 4 | RSO §10 (Service Tribunal, 30d) | ✅ (reaches tribunal appeal) | ⚠️ (stops at Governor representation) | **ON** |
| 5 | Rule 37 (bid security) | ✅ | ✅ | tie |
| 6 | Rule 4 (penalties) | ⚠️ (definition/grounds, no penalty list) | ✅ (states major & minor penalties) | **OFF** |
| 7 | Rule 5/6 (procedure) | ⚠️ (grounds + suspension) | ✅ (statement of allegations + Rule 5/6) | **OFF** |
| 8 | Rule 18 (response time) | ✅ | ✅ | tie |
| 9 | Rule 6 APT (control) | ✅ | ✅ | tie |
| 10 | Rule 1 SPPRA (control) | ✅ | ✅ | tie |

**Tally: ON +2 (Q2, Q4), OFF +2 (Q6, Q7), 6 ties. Controls both pass — no regression.**

**Decision: net-neutral → REVERT TO OFF** (per the pre-agreed rule "keep ON only if ON ≥ OFF with no
control regression"; ON is not ≥ OFF). Mechanism observed:
- ON **helps** appeal-chain questions (Q2/Q4) — the summary index surfaces a *downstream* provision
  (tribunal appeal, complaint→appeal path) that plain vector/FTS ranks below the cutoff.
- ON **hurts** Q6/Q7 — summary-recalled provisions **crowd the capped top-K context**, displacing the
  precise chunk (penalty list; statement-of-allegations step) that OFF surfaces directly.

`ECHO_SUMMARY_RECALL=off` set in prod `.env` 2026-07-12 (server restarted). The pre-baked whole-document
summaries (`document_full` serving path) are unaffected — only the summary-*recall* Q&A arm is retired.
Raw pass data: VM `/tmp/eval_on.jsonl`, `/tmp/eval_off.jsonl`.
