---
id: 163
title: NVIDIA Inception pitch deck as a Word document
stage: misc
date: 2026-08-24
surface: agent
model: claude-opus-5
feature: general
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (freeform request)
labels: [business, application, nvidia, pitch-deck, docx]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - C:\Users\hp\Desktop\Noviq\Business\Noviq_Pitch_Deck_NVIDIA.docx
  - C:\Users\hp\Desktop\Noviq\Business\make-nvidia-deck-docx.py
tests:
  - none (document artifact; 15 slides + appendix, verified by re-opening the .docx)
---

## Prompt

write a pitch deck for nividia also and save it in docx

## Response snapshot

Wrote a 15-slide deck as a slide-by-slide Word script (headline, bullets, speaker note per
slide) plus a claim-provenance appendix, saved beside the NVIDIA blurb from PHR 162.

Slides: what Noviq is / problem / founder-is-the-user / product / how it works / traction /
market / business model / differentiators / NVIDIA today / NVIDIA planned / GPU unit
economics / roadmap / team / the ask.

Every number is sourced from recorded measurements rather than invented — AWQ→Marlin
(13.75→81.32 tok/s, 36.2s→7.7s) and speculation (→3.6s), the 84-SM vGPU finding, 4,074 docs /
304,617 chunks, 93.2% categorisation, PPRA-anchored pricing, 2× RTX 4090 at ~9% of revenue.

Guardrails carried into the deck from memory, so a later editor cannot re-introduce them:
- No accuracy-superiority claim over NotebookLM (parity + data boundary is the tested claim).
- No "quarter GPU" framing — disproved by measurement.
- Not training a foundation model; Noviq is RAG.
Known blanks (financials, ask+equity, two team members, phone/address) are marked in RED,
matching the convention already used in `Noviq_Pitch_Deck_NIC.pptx`.

Generator kept next to its output, per the `make-hardware-docx.py` precedent.

## Outcome

- ✅ Impact: NVIDIA Inception deck ready for review; regenerable in one command.
- 🧪 Tests: re-opened the .docx — 15 slides, 108 paragraphs, ~1,720 words, headings correct.
- 📁 Files: 2 new on the Desktop business folder; nothing in the repo changed.
- 🔁 Next prompts: fill the four red blanks; decide whether a .pptx version is also needed.
- 🧠 Reflection: a heredoc containing apostrophes and `%` broke the shell — writing the
  generator with the file tool instead of `cat <<EOF` was the fix, and is the better default
  for any script over ~50 lines.

## Evaluation notes (flywheel)

- Failure modes observed: bash heredoc parse failure on the first write attempt.
- Graders run and results (PASS/FAIL): PASS (document re-opened and structure verified).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): generate the .pptx from the same content dict so
  the Word script and the slides can never drift apart.
