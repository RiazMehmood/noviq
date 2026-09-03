---
id: 0007
title: Pilot manual split, query annexures and the Word deliverable
stage: plan
date: 2026-08-26
surface: agent
model: claude-opus-5
feature: 028-noviq-pk-cutover
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (conversational)
labels: [pilot, sindh, documentation, docx, research, evaluation]
links:
  spec: specs/028-noviq-pk-cutover/
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: (no repo code changed)
    change: deliverables are two published artifacts plus a Word document on the Desktop
  - path: C:/Users/hp/Desktop/Noviq/CS testing plan/Noviq-Sindh-Pilot-Field-Manual.docx
    change: new — the field manual as a Word deliverable
  - path: C:/Users/hp/Desktop/Noviq/CS testing plan/make-field-manual-docx.py
    change: new — generator, kept beside its output so the .docx can always be rebuilt
tests:
  - name: HTML tag-balance + anchor-resolution check on both volumes
    result: PASS
  - name: pyspellchecker pass over the field manual's visible text
    result: PASS — 40 flags, all hyphenated compounds, abbreviations or British spellings
  - name: python-docx readback (sections, footers, headings, specimen content)
    result: PASS
---

## Prompt

> use the chrome tool and see the error in ur written document. <artifact url>

> check the spellings also

> write a word document for this manual

> page numbers are not correct in word document and you also did not show the certificate which we will issue to the officers. show that sample also

> save progress upto now we will work tomorrow

(Earlier in the same session, covered by PHR 0006 up to the first save: the scoping
tick, four corrected measurements, and the first draft of the pilot plan. This record
covers everything after it — the split into two volumes, the two-sitting session
structure, the pre-loaded-query finding, the cleaned manual with annexures, the Chrome
review and the Word deliverable.)

## Response snapshot

**Split into two volumes** after the architect observed one document was doing two
jobs — Volume 1 *Sindh Pilot Field Manual* (process) and Volume 2 *Noviq Pilot
Readiness* (engineering) — and, at his instruction, grounded in web research rather
than recall: Sindh Government Rules of Business 1986 and the Civil Servants (APT)
Rules 1974 (both extracted from PDF with `pypdf` after WebFetch returned compressed
streams), OECD *Generative AI Experimentation in Government* 2026, the UK *People
Factor* study, the Australian DTA Copilot trial, SPPRA Rules 2010, and standard UAT
and usability practice.

**Two structural corrections came from the architect's questions, not from me.**

1. *"If we want the officer to run pre-designed queries, why do we need to write
   those?"* — exposed that making an officer retype a pre-written query is the worst
   of both worlds: he does the typing and we lose the fixed question. The 30 are now
   pre-loaded (new build item B7). It also exposed a **contradiction in my own
   document**: the T−3 pre-flight said "acquire the document, or swap the query" while
   the risk table said never edit the bank. Swapping per department turns the pilot
   into an exam we set, sit and mark ourselves. Removed from both volumes.
2. The two-sitting session shape (Day 1 long, Day 2 a 30–60 min return with the
   officer's own live work) — which also solves the freeze problem I had flagged,
   because the overnight gap gives him time to notice what he is stuck on. Added a
   scripted **homework brief** at 1:58 on Day 1.

**Cleaned Volume 1 on request**: research section cut, runbook rewritten as a
plain-language countdown with tick-boxes, and the query bank turned into **three
annexures carrying the actual 30 questions with expected answers and rule numbers**.
Annexure C is quoted directly from the APT Rules 1974; Annexure B (audit) has five
rows tagged `[VERIFY]` and says on its face that it needs review by an officer who has
sat in a DAC. No answers were invented to fill gaps — a wrong expected answer would
make us mark a correct output as a failure.

**Chrome review found two real defects**: an empty black band under the header (the
masthead's bottom border and the first section's top border, both 2px, 64px apart) and
a Day-1 clock that did not add up (two 35-minute blocks for 30 queries = 2.3 min each
against a stated 2.5–3 min cycle). Both fixed, the second with an explicit pacing rule.
A spell check was clean.

**Word deliverable** generated with `python-docx`, generator kept beside its output.
Two page-number bugs fixed after the architect reported them, and two specimens added —
the User Acceptance Test Record and a separate Certificate of Participation — because
I had only described the layout, which is not the same as showing it.

## Outcome

- ✅ Impact: the pilot now has a process manual (web + Word), an engineering readiness
  document, and a test bank of 30 sourced questions ready for domain review.
- 🧪 Tests: HTML structure and anchors PASS; spell check clean; docx readback confirms
  3 sections, correct per-section footers, 29 headings and both specimens present.
- 📁 Files: no repo code changed. Deliverables are two artifacts and the Desktop docx.
- 🔁 Next prompts: run the G2 chunk repair; get Annexure B reviewed; add the
  participation certificate to the web Volume 1; trim Volume 2.
- 🧠 Reflection: three of the session's most valuable changes came from the architect
  asking a short sceptical question about something I had written confidently. The
  pre-loaded-query question in particular found a contradiction I had authored and
  validated past.

## Evaluation notes (flywheel)

- Failure modes observed: (1) authored a document containing a direct self-contradiction
  (pre-flight "swap the query" vs a risk row forbidding it) and did not catch it in my
  own validation, which only checked HTML structure; (2) described an artefact instead
  of showing it, and only produced the certificate specimen when asked twice;
  (3) spent ~8 browser calls diagnosing a scroll bug that did not exist because I
  trusted stale screenshots over the possibility that my tooling was wrong.
- Graders run and results: tag-balance PASS · anchor resolution PASS · spell check PASS
  · docx readback PASS.
- Prompt variant: n/a.
- Next experiment: add a consistency grader alongside the structural one — extract every
  numeric claim and every "never/always" rule from a document and check them against
  each other before publishing. Both content defects this session were of that shape.
