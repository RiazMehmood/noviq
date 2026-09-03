---
id: 0006
title: Sindh 15-department pilot plan and the scoping tick
stage: plan
date: 2026-08-26
surface: agent
model: claude-opus-5
feature: 028-noviq-pk-cutover
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (conversational)
labels: [ui, pilot, evaluation, gpu-costing, research, sindh]
links:
  spec: specs/028-noviq-pk-cutover/
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: src/components/DocumentTree.tsx
    change: scoping tick reduced a size step and pulled tight to the expand arrow
  - path: CHANGELOG_DEV.md
    change: dated entry for the tick, the stale latency register, the VRAM finding and the vGPU lease correction
tests:
  - name: npx tsc --noEmit
    result: PASS
  - name: npx vitest run src/components
    result: N/A — no component test files exist for this path
---

## Prompt

> recall memory what is still pending

> complete the ui remaining work of arrow, and discuss this we are going to test the application in 15 departments as per chief secretary summary, it is in pipeline and soon will be approved, so we want that as we create users so we can collect as much data as possible, and we are supposed to give 2 days for each department, and there are too much hurdals in govt sectors in sindh, some time secretary is busy, some time Section officer is busy, so we want a full fool proof plan to execute this properly, and after two days suppose we select 3 persons or secretary assign task to 3 persons so we get 1.5 to 2 hours for each person and explain the app then ask him to execute queries, we discuss about creating categories for queries into four categories, procurement, audit, transfer and posting, unique/departmental, so on first day we give them at least 10 queries for each category first 3 categories so they execute them then on second day we ask him to use its own queries which falls under unique/departmental queiries, then after this we will print a certificate where it shows name of the user its designation, and below that queries it runs and after those queires some marking like good, bad, better type tick option and below that at the end comment section so officer write his comments on it after that he sign and stamp the certificate so we get evidence of testing report, and we also want a tab for feedback of officer in the app also so we ask user to fill that also or we can use that tab info and print that in hard copy then get stamped and signed by officer, and we also want cloud gpu costing and physical gpu costing for number of concurrent users complete calculation also so discuss this first complete then generate plan accordingly

> and i dont want you to write the plan first write the document

> and you are saying 90 to 120 s per query but we after vllm_marlin changing we reduce the time for query also / which was i think below 10 s / check that also before writing the document

> the vgpu license for our vm or the core host will expire?

> you mixed both documents technical which we will work on and the process which we will carry during testing, so split them, and dont relay on your only knowledge, search the web deeply read the blogs for proper operations and procedurs, review the testing scenrios in sindh govt all details then get on a point.

> I want 3 officers from each department like 1st officer came for 1.5-2 hours, and it goes from the prcess, then after its complete work we ask him to come tomorrow for 30 min to 1 hour only and execute your own queries currently you are working on or facing difficuilty in it, then we call another officer which also goes in the same manner then third one so this is the procedure and again on day 2 these 3 officers will come and ask unique queries. so update accordingly

> save progress upto now.

## Response snapshot

**Code.** Closed the outstanding tick request from 2026-08-25 (a prior attempt had
failed its assertion and shipped nothing). `Box` sizes down one step — group
14px → 12px, document 12px → 10px, check mark and dash scaled with them — with the
ratio between levels deliberately preserved, because shrinking only the leaf would
collapse the size cue distinguishing a document from a department. The group tick
button gained `-ml-1` so it sits ~2px from the expand arrow and reads as one
cluster. Typecheck clean.

**Four corrections, three of them prompted by the architect.**

1. **Latency register is stale.** "90–120 s" predates the 2026-08-12 AWQ→Marlin
   fix; measured now 7.7 s production-shaped, SC-002 p95≤10 s passes. Two errors
   had already been built on it — a query bank cut from 30 to 22, and a five-GPU
   sizing. Both reverted.
2. **The 13.0 tok/s @ 8 streams figure was the broken-kernel measurement**, quoted
   originally to disprove saturation. Post-fix: 596.8 tok/s aggregate, slowest of
   eight requests 4.02 s.
3. **The vGPU "expiry" is a rolling lease.** Checked live: 2026-09-10 → 2026-09-24,
   +14 days in 13. No September cliff, nothing to renew. The host-side entitlement
   is invisible from the guest, so the signal is whether the date keeps moving.
4. **The serving ceiling is VRAM, not compute.** KV cache 13,968 tokens against an
   8,192 window = 1.7 concurrent. One 24 GB card covers all fifteen departments at
   peak; the current A40 slice does not. Buy VRAM, not FLOPs.

**Research (web, not recall), at the architect's instruction.** Sindh Government
Rules of Business 1986 (extracted from PDF via pypdf after WebFetch returned
compressed streams); OECD *Generative AI Experimentation in Government* 2026;
UK Cabinet Office *The People Factor*; Australian DTA Copilot trial; SPPRA Rules
2010 thresholds; standard UAT entry/exit criteria; think-aloud/usability practice;
Pakistan data-protection status.

Findings that changed the plan: **Rule 21(f)** (CS may call for any case or
information) is the authority; **Rule 23(i)** puts the circular with S&GAD;
**Rule 22(g)** is the instrument for naming a Focal Person; and **Rule 24(ii)**
makes a *Section Officer's* signature valid authentication — which removes the
Secretary from the critical path entirely. The OECD names the trap the proposed
certificate was heading for (evaluation that "relies on basic indicators such as
usage or user satisfaction"). The DTA trial gives a candid-result benchmark
(7.21/10, a third daily use). Five participants surface 77–85% of usability
problems, so three officers per department is acceptance evidence, not usability
findings.

**Deliverables.** Two published documents, split after the architect noted one was
doing two jobs — Volume 1 *Sindh Pilot Field Manual* (process) and Volume 2
*Noviq Pilot Readiness* (engineering), then both updated for the architect's
two-sitting session structure (Day 1: 1.5–2 h × 3 officers; Day 2: 30–60 min × 3
officers with their own live work).

## Outcome

- ✅ Impact: the outstanding tick work is shipped; four measurement errors caught and
  corrected before they reached a plan anyone acted on; the 15-department pilot has a
  process manual and an engineering readiness document, both grounded in cited sources.
- 🧪 Tests: `npx tsc --noEmit` PASS. No component tests exist for `DocumentTree.tsx`.
- 📁 Files: `src/components/DocumentTree.tsx`, `CHANGELOG_DEV.md`.
- 🔁 Next prompts: run `POST /api/admin/resplit_oversize?dry_run=false` (G2, the only
  real blocker); build B1 pre-flight coverage runner; deploy the tick.
- 🧠 Reflection: the single most valuable move in this session was the architect
  challenging a number I had quoted from my own memory index. The stale figure was
  internally consistent, cited, and wrong — and it had already produced two downstream
  decisions. A recorded measurement is only as good as its date.

## Evaluation notes (flywheel)

- Failure modes observed: (1) quoting a stale figure from the memory index without
  checking its date against a later fix in the same index; (2) carrying a "before"
  benchmark row forward as if it were the "after"; (3) writing a blocking gate from an
  inventory value without re-reading it live — the value had moved and was never a
  deadline; (4) merging process and engineering content into one document, which the
  architect had to ask to be undone.
- Graders run and results: `tsc --noEmit` PASS; HTML tag-balance check on both
  documents PASS.
- Prompt variant: n/a.
- Next experiment: when a plan depends on a measured number, re-read the measurement at
  source before the number reaches a table — the three corrections here each cost one
  command and would each have survived into a document otherwise.
