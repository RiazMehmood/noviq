---
id: 173
title: SSWMB MD presentation, masking default off, citation evidence
stage: misc
date: 2026-08-30
surface: agent
model: claude-opus-5
feature: general
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (chat) modernise the MD briefing deck; take Noviq screenshots via Chrome
labels: [presentation, sswmb, f020-masking, citations, retrieval]
links:
  spec: specs/020-confidential-masking/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/components/ConfidentialComposer.tsx
  - CHANGELOG_DEV.md
tests:
  - "npx tsc --noEmit: PASS (no output)"
---

## Prompt

we are going to create a presentation for MD of Sindh Solid waste management, you can find its details inside noviq folder inside solid waste management folder, now you need to update the presentation and make it modernized, innovative and attractive, you also extracted multiple information and created md_meeting.docx which is also placed there, now i want to ask md that we both officers wants to create five advancements in the existing board, our main moto is to create a Data & digital monitoring cell, which we both will lead and this will perform 5 tasks, number one centerilized knowledge base, 2nd AI based decision making, and these two tasks will be performed by noviq and its my project perspective, and we want to initiate 3 other things also, live reporting dashboards via mobile or desktop so from whole karachi on daily bases we can collect all reports, then we also want to centerilized the complaint management system also, by using voice & chat bots which speaks multilanguages and talk to customers and facilitate them, the final thing which we want is to create a Data Analytics & prediction system, which guide us on high impact areas, and different other aspect so we can accelelarte the system and enhance its abilitites, now first discuss with me this then update the ppt, in ppt at first we need to provide into about app and us both, then we want to show screen shots of noviq, then we run 3 critical question which u also showed in md_meeting.docx and its screen shots, we will talk about them that these are your cases or etc, we show them that we just run random questions and got these answer and it will automatically clicks their mind to their recent works.

(follow-ups, mid-turn)
- keep intro first, take screen shots of the app via chrome tool
- and as you can see this masking is by default on, i want it by default off. so if anyone wants this feature so he can on it.
- and there are no citation inside the answer why?

## Response snapshot

**Deck audit.** `md briefing.pptx` is a 9-slide skeleton with empty content
placeholders and 3 diagrams. `image2.png` names the REPEALED 2014 Board Act as
governing statute #1 — must be rebuilt against the 2021 Act. No Noviq screenshots
exist anywhere on disk.

**Live demo run (noviq.pk, logged in, 3,932 files / 51 departments).**
- Q1 (fines for illegal dumping) — FAILED the demo bar. One citation, and it was
  the **repealed 2014 Ordinance**, S.8(3), plus "the exact maximum fine is not
  specified in the retrieved documents".
- Q2 (does the 2021 Act override SLGA 2013 sanitation functions) — good prose,
  3 sources all from the 2021 Act. Screenshot saved.

**Root causes, evidenced from the admin query trace (Trace 8962bc98):**
1. STEP 19 Grounding validation — `Citations found: 0`, `Uncited factual
   sentences: 6`, `Score 0.00/1.00`, `Fallback triggered: true`.
   STEP 21 — `Model cited: NO — top-3 by rank, supports no specific claim`,
   `Citations shipped: 3`. So the "Sources" block is the fallback top-3 by rank,
   NOT model citations. The UI still badges it `CITED 3 TIMES · CONFIDENCE: HIGH`
   while the server's own quality notice says the answer "could not be verified
   against a specific source".
2. The indexed corpus copy of the 2021 Act is **badly OCR'd**: trace shows
   `NO.PASfLEGIS~B~43/1021·The 5ndhSolidWaste Management SII, 202_`, `The Soard
   shall take over solio waste ... ocherbocies`. The Desktop copy of the same Act
   `pdftotext`s cleanly. Garbled text is why the model cannot emit `[N]` markers.
3. Legal finding: the 2021 Act contains **no penalty schedule**. The only fining
   power is S.7(1)(v) — "impose fine ... **as may be prescribed by rules**". The
   Rs 50,000–500,000 scale is not in the Act, so demo Q1 as scripted in
   `md_meeting.docx` is unanswerable from it.

**Shipped: masking OFF by default (F020).** `ConfidentialComposer.tsx` —
`maskingOn` signal defaulting false, persisted at `localStorage["noviq.masking.on"]`;
single gate in `activeSpans` (returns `[]` when off) so overlay, chips and
`onRanges` all follow; tray label became the `aria-pressed` switch; `markSelection`
turns masking on rather than being swallowed. Triggered because `detectNames`
flagged "Solid Waste Management" and "Municipal Corporations" as NAMEs — sending
the question would have redacted the statute out of it.

## Outcome

- ✅ Impact: masking is opt-in; deck blockers identified with trace evidence; the
  demo script in `md_meeting.docx` needs re-writing around a real legal gap.
- 🧪 Tests: `npx tsc --noEmit` clean. Not yet verified in a browser (not deployed).
- 📁 Files: `src/components/ConfidentialComposer.tsx`, `CHANGELOG_DEV.md`.
- 🔁 Next prompts: create the SSWMB project from the 16 clean Desktop PDFs and
  re-run all three questions inside it; then build the deck.
- 🧠 Reflection: the first demo question failing was worth more than it cost — it
  surfaced a corpus-quality defect and a UI honesty defect that would both have
  fired in front of the MD.

## Evaluation notes (flywheel)

- Failure modes observed: high-recall NER masking statute names; fallback
  citations presented as real ones with "CONFIDENCE: HIGH"; OCR'd duplicate of a
  clean source document outranking nothing because the clean copy is absent.
- Graders run and results (PASS/FAIL): typecheck PASS; live Q1 FAIL, Q2 PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): load the 16 clean PDFs into a scoped
  project and re-run Q1 — isolates OCR quality from retrieval ranking.
