---
id: 165
title: Filling the NVIDIA funding and financials section
stage: misc
date: 2026-08-24
surface: agent
model: claude-opus-5
feature: general
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (freeform request)
labels: [business, application, nvidia, funding, docx]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - C:\Users\hp\Desktop\Noviq\Business\Noviq_NVIDIA_Inception_Submission.docx
  - C:\Users\hp\Desktop\Noviq\Business\make-nvidia-submission-docx.py
tests:
  - none (document artifact; rebuilt and re-read — 1 red placeholder left by design)
---

## Prompt

TO BE FILLED before upload: capital raise amount and equity offered, and current financials -
revenue, burn and runway. how can i fill this? we still not get any capital amount? needs ur
suggestions for this.

## Response snapshot

Advised that "bootstrapped, no raise" is a valid and ordinary answer — Inception asks for funding
status, not proof of funding, and revenue is not an eligibility requirement. Derived a bottom-up
pre-seed range (~PKR 19M / USD 68k for 18 months) from the existing cost model in case a number
was wanted; the architect chose to state no raise.

⚠️ **My starting assumption was wrong and the architect corrected it.** I assumed near-zero burn
(two unpaid founders) and drafted a "runway is not our constraint" framing. Actual spend is
**PKR 0.5–1.0M/month** on an engineering team — frontend, backend, DB, AI/ML, NLP, QA, DevOps.
Two consequences, both now fixed in the document:
1. The Team section listed only the two founders while a paid engineering team exists — that
   understated the company AND sat oddly against Inception's "employs at least one developer"
   eligibility rule. A team line was added.
2. The financials paragraph now states the real self-funded burn rather than implying there is
   none. Capital efficiency is framed as the strength it genuinely is: a production system,
   4,074 docs indexed and a 10x inference optimisation, delivered with no funding round.

The capital-raise red placeholder was replaced with an explicit line that we are not asking NVIDIA
for capital — Inception takes no equity and writes no cheque, so the raise belongs in a Capital
Connect investor conversation, not in the ask to NVIDIA.

⚠️ **Eligibility blocker surfaced: incorporation is "in progress".** Inception requires an
officially incorporated company and asks for the incorporation date. Recommended waiting for the
SECP certificate before applying rather than applying against a pending registration.

## Outcome

- ✅ Impact: funding/financials section written from real figures; one red placeholder left
  (phone + registered address).
- 🧪 Tests: document rebuilt and re-read — 2,029 words, Company status section verified present.
- 📁 Files: 2 on the Desktop business folder; nothing in the repo changed.
- 🔁 Next prompts: SECP certificate → then apply; confirm the burn figure is defensible if asked;
  decide whether to state engineering headcount.
- 🧠 Reflection: I inferred "two serving officers" meant "no payroll" and built a whole
  recommendation on it. The founders being unpaid says nothing about whether anyone else is —
  ask what the spend actually is before framing an argument around its absence.

## Evaluation notes (flywheel)

- Failure modes observed: unstated assumption (zero burn) nearly shipped into a submitted document.
- Graders run and results (PASS/FAIL): PASS after correction.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): when a document needs a number I do not have, ask for
  it before drafting the paragraph that depends on it, not after.
