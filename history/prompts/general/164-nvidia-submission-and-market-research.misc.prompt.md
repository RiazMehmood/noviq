---
id: 164
title: NVIDIA Inception submission document plus market research
stage: misc
date: 2026-08-24
surface: agent
model: claude-opus-5
feature: general
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (freeform request)
labels: [business, application, nvidia, market-research, docx]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - C:\Users\hp\Desktop\Noviq\Business\Noviq_NVIDIA_Inception_Submission.docx
  - C:\Users\hp\Desktop\Noviq\Business\make-nvidia-submission-docx.py
  - C:\Users\hp\Desktop\Noviq\Business\Noviq_Market_Research_NVIDIA.docx
  - C:\Users\hp\Desktop\Noviq\Business\make-nvidia-market-research-docx.py
tests:
  - none (document artifacts; structure verified by re-opening both .docx files)
---

## Prompt

i changed some lines in the pitch deck, and dont make it for presentation i will upload this to
nividia website for starups funding and gpu support, so write accordingly and dont write slide 1
etc and speaking notes either.

(mid-turn) and make some market research so we can pitch correctly and get full support from the
nividia for our startup properly if in need.

## Response snapshot

Rewrote the deck as a written company overview for upload — prose sections, no slide numbering,
no speaker notes, no internal guardrail appendix. Founder edits were read off the .docx first and
carried over: contact → `riaz.mehmood@noviq.pk`, the "remove this one" bullet deleted, co-founder
Younus Shaikh added (typo "Muncipla" → "Municipal"). One founder line —
"Can be used from academia and corporate supports from summary for chief secretary" — was too
ambiguous to render, so it was left out and flagged rather than guessed at.

Then ran web research (12 sources, retrieved 2026-08-24) and wrote a separate INTERNAL brief.

⭐⭐⭐ The finding that changes the application: **consulting and outsourced-development firms are
explicitly INELIGIBLE for NVIDIA Inception**, and our own deck said Noviq "installs on-site and
trains the department's IT team". That is our anti-consulting delivery model, but it reads like a
services business to a reviewer skimming eligibility. Reworded to licence + annual support, with
onboarding as one-time and fixed-scope.

⭐⭐ Inception takes no equity and writes no cheque — "funding" means partner credits (DGX Cloud
~$100k, AWS Activate ~$100k, Nebius ~$150k), preferred hardware pricing as a rebate, free NIM
endpoints, and VC introductions via Capital Connect. The ask was rewritten to match what the
programme actually has, instead of asking for money it does not give.

⭐⭐ Pakistan is already an NVIDIA public-sector market: NTC signed a 3-year GPUaaS/MaaS deal with
Data Vault (July 2026) deploying H200/A100/B200 in government data centres, already hosting
Qwen3-32B — our model class. Noviq is the application layer for compute NVIDIA has already placed.
This is now named in the submission.

Competitive read: Harvey (~$11B, $190M ARR, 100k lawyers) and Legora ($5.55B, $150M ARR) validate
the category, but every funded player sells cloud SaaS to law firms in high-income markets. None
serve an on-premises government secretariat in an emerging market. Positioned as the category they
cannot serve, not as a competitor.

Four research-driven edits were applied back to the submission doc: sovereign-AI opening, the
NTC/Data Vault paragraph, the consulting-safe business model, and the reframed ask (plus the GPU
demand we create — one permanent inference workload per institution across ~370 institutions).

## Outcome

- ✅ Impact: upload-ready submission (16 sections, ~1,860 words) plus an internal research brief
  (7 sections, 3 tables, sourced).
- 🧪 Tests: both .docx re-opened and verified — 0 slide/speaker-note references, consulting
  wording confirmed replaced, 2 red placeholders remaining by design.
- 📁 Files: 4 on the Desktop business folder; nothing in the repo changed.
- 🔁 Next prompts: confirm incorporation status/date (Inception asks for it), fill financials +
  ask/equity + phone/address, and resolve the ambiguous academia/chief-secretary line.
- 🧠 Reflection: a `python - <<EOF` patch script wrote literal `\n` into Python source and broke
  the file twice; string-surgery on a generator is worse than editing the generator directly.

## Evaluation notes (flywheel)

- Failure modes observed: two failed in-place patches (unterminated string literal; a whitespace
  mismatch that silently did nothing and was only caught by re-reading the built .docx).
- Graders run and results (PASS/FAIL): PASS — verified by reading the generated documents back,
  not by trusting the patch script's exit code.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): assert on the built .docx inside the generator (e.g.
  fail if "anti-consulting" or "Slide " appears) so a bad edit cannot ship silently.
