---
id: 146
title: Proxima AI reports read, #4 and #7 root-caused on prod
stage: explainer
date: 2026-08-06
surface: agent
model: claude-opus-5
feature: general
branch: 027-my-projects
user: Riaz Mehmood
command: (conversational)
labels: [security, retrieval, red-team, diagnosis, no-code-change]
links:
  spec: n/a
  ticket: docs/security/red-team-findings-2026-08-05.md
  adr: n/a
  pr: n/a
files:
  - path: (none changed — investigation only)
    change: read-only diagnosis
tests:
  - name: live prod re-run of Proxima #4 and #7
    result: both reproduced
---

## Prompt

Sequence of requests in this session:

1. "recall memory what we have done yet?"
2. "reports also shared by proxima ai, i saved those inside noviq folder where we
   already placed details there i create another folder named reports where you
   can find a pdf and xlsx file also check those also"
3. "first tell me in details about #4 and #7 critical and high, what are the
   findings? and what our app is doing and why? in simple and plain english only"
4. "yes, run that check against prod so we can compare their findings and our
   current production details, if one time it fails to answer why pass in at
   number 6 while it failed on 4? now run those quetion in production vm is
   connected."
5. "first tell me the problems currently we have and how will you fix those in
   plain english first tell me this, then we move forward."
6. "save this report on pdf on desktop so i can read this, and also save progress
   upto now, i want this last response from you in plain english in pdf so i can
   understand the issues easily."

## Response snapshot

**Read Proxima's own deliverables** at
`Desktop\Noviq\proxima_ai_testing\reports\` — `Noviq_QA_Report _.pdf` and
`Noviq_Prompt_Testing.xlsx`. The PDF claims 50 prompts / 43 pass / 86 %; the
spreadsheet holds **110 planned rows, only 64 executed** (55 pass / 9 fail),
rows 65–110 never run. Quote the sheet, not the PDF.

⭐ **5 of the 9 failures are retrieval quality, not security** — the angle our
own 98-turn `query_log` analysis missed by scoring only attacks.

**Corpus check (prod PG, port 39779 resolved by data dir):** both "missing"
texts are present and cleanly chunked — SPPRA board = `Sindh Public Procurement
Act 2009` **Section 6(2)** (members (i)–(xviii)), eligibility = APT Rules 1974
**Rules 12–17**. ⚠️ The tester's "Section 4/5" expectation is wrong. So neither
failure is an ingestion defect.

**Live re-run on prod** (fresh conversations, `/tmp/askq.sh`; traces read from
`logs/ab_rerank/queries/*.log`) — both reproduced, with **two different causes**:

- **#7 (High)** — retrieval *succeeded* (STEP 12 top-10 = 9× Procurement Act),
  then **STEP 14 "table completion" pulled 64 table chunks into a 36-slot cap
  and evicted the Act entirely**. The model got Rules-of-Business department
  rows and correctly refused. The refusal is grounding working; the bug is the
  unbounded table pull.
- **#4 (Critical)** — plain ranking failure. Top match was the Local Government
  Act; only **3 of 33** context chunks came from the APT Rules, and **2 of those
  3 are the title page** (one a junk one-column-table duplicate). Exactly one
  real provision (Rule 13) reached the model. Cause: "Government … Sindh" is the
  definitions clause of all 18 documents.

**#4 vs #6 answered:** same prompt asked twice (turns 2 and 3), identical
"Actual Response Summary" text copied into both rows, graded Fail/Critical and
Pass/Medium. A grading inconsistency in their sheet, not app behaviour.

**Also found:** turn 1 of the same conversation ("retirement age for a Grade 17
civil servant") got the same 82-char refusal in 2.27 s and was never logged —
so narrow numeric questions failed ≥3×.

**Confirmed in code (their #36):** the chat-attachment upload path
(`api/uploads.rs:713`) passes the **raw file extension** into
`sources::insert_scoped`, unlike the documents path (`:318`) which validates via
`FileFormat::from_path`. Everything not spelled identically in the migration-001
CHECK crashes: `.html .markdown .xlsm .wav .ogg .m4a .mkv .avi .mov` and **all
code files**.

**Deliverable:** full plain-English problem inventory and fix plan rendered to
`C:\Users\hp\Desktop\Noviq_Issues_and_Fixes_2026-08-06.pdf` (10 pages, Chrome
headless from a scratchpad HTML), with every item tagged CONFIRMED / REPORTED /
KNOWN.

## Outcome

- ✅ Impact: two headline defects root-caused with reproducible evidence; one new
  code defect (B2) confirmed by reading; a fix plan agreed in priority order.
- 🧪 Tests: no automated tests written. Both failures reproduced live on prod.
- 📁 Files: no source changed. PDF written to the Desktop; memories updated.
- 🔁 Next prompts: implement first pass — deploy S1+S2, fix table-completion
  eviction, fix attachment file types, run Build Summaries.
- 🧠 Reflection: ⭐ **The `ab_rerank` query trace is the best diagnostic in this
  system** — it prints all 16 steps including the exact chunks fed to the LLM,
  and it turned two "the model is bad" complaints into two precise, different
  mechanisms. ⚠️ I misread the response field `audit.kept_law: 0` as "zero
  chunks retrieved"; it is the F015 conversation-memory counter. The journal
  line `llm_start chunks=36` was the real measurement — **two places measuring
  one thing, again**.

## Evaluation notes (flywheel)

- Failure modes observed: table-completion eviction (unbounded pull vs fixed
  cap); boilerplate/definitions dominating broad-query retrieval; duplicate
  title-page chunks consuming context slots; raw extension bypassing format
  validation on one of two upload paths.
- Graders run and results (PASS/FAIL): n/a — no grader involved.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): cap the STEP 14 table pull and make
  it additive rather than displacing, then re-ask the SPPRA question and check
  the Procurement Act survives into STEP 16.
