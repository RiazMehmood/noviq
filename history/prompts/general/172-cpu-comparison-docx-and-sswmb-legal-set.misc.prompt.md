---
id: 172
title: IBA vs Rapid Compute CPU comparison (Word) and the SSWMB legal document set
stage: misc
date: 2026-08-29
surface: agent
model: claude-opus-5
feature: general
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (chat) CPU configs of IBA/Rapid VMs -> one-page Word comparison; then gather the SSWMB legal framework into a folder
labels: [hardware, docs, corpus, sswmb, deliverable]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - "C:/Users/hp/Desktop/Noviq/Hardware/Noviq-CPU-Comparison-IBA-vs-RapidCompute.docx"
  - "C:/Users/hp/Desktop/Noviq/Hardware/make-cpu-comparison-docx.py"
  - "C:/Users/hp/Desktop/Noviq/Solid Waste Management/ (16 PDFs + README + collect_swm.py)"
tests:
  - "content verification: pdftotext -l 3 on every candidate before copying — 16/16 verified"
---

## Prompt

1. "what are the cpu configuration of iba vm and rapid vm?"
2. "write it into a word file so i can share the details, and make it specific only, and if
   available share tok/s of iba cpu also. only a single page comparision required. in the
   form of table if possible"
3. (after an SSWMB legal-framework research brief) "if we have already these documents, then
   create a folder inside noviq folder named solid waste managment and place all these
   required files inside that folder so i can creaet a project and upload those documents"

## Response snapshot

**CPU comparison.** One-page Word table at `Desktop/Noviq/Hardware/`, generator kept beside
it per the convention set on 2026-08-13. IBA VM: 12 vCPU, KVM-masked as "Core2 Duo T7700",
**SSE4.2 only — no AVX**, 62 GiB RAM, A40-12Q GPU. Rapid Compute: 32 vCPU Ice Lake,
**avx512_bf16**, 62 GB RAM, no GPU.

⚠️ The requested **IBA CPU tok/s does not exist** — every figure held for that host
(81.3 decode / 596.8 aggregate / 54.3 sustained) is GPU. Attempted to measure it and could
not: `ssh <user>@<PRIVATE_IP>` timed out, VPN gateway still down. The document states this
explicitly rather than leaving a blank cell.

**SSWMB set.** 16 instruments located in `D:\Sindh-Corpus` and copied to
`Desktop/Noviq/Solid Waste Management/`, each **verified by reading the PDF's own text**
before copying. All 16 taken from the `sindhlaws` source.

⚠️⚠️ **Gap: the Sindh Public Procurement Rules 2010 are absent from the corpus** — base
rules and all five amendment notifications are `ok:false` with no local file. The
procurement Act 2009 is present; the Rules are not.

## Outcome

- ✅ Impact: two shareable deliverables; one corpus gap identified that would otherwise have
  surfaced during the MD meeting.
- 🧪 Tests: every copied PDF passed a content check (`pdftotext -l 3` + expected phrase).
- 📁 Files: as listed above; both generators kept beside their output.
- 🔁 Next prompts: fetch SPPRA Rules 2010; build the "SSWMB Legal & Regulatory Map" after
  the documents are indexed so each row cites a real section.
- 🧠 Reflection: content verification was not ceremony — the `rtw` harvest has rows whose
  title and file disagree (a row titled "SINDH PUBLIC PROCUREMENT ACT, 2009" points at a
  COMMECS Institute PDF). Trusting manifest titles would have put wrong documents in front
  of a Managing Director.

## Evaluation notes (flywheel)

- Failure modes observed: (a) `ok:false` manifest rows read as corpus coverage when they are
  records of failed downloads; (b) exact-title matching silently failed on trailing full
  stops, reporting two present documents as missing; (c) a measurement that does not exist
  is easy to substitute with an adjacent one — GPU tok/s was NOT offered as CPU tok/s.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): normalise titles (strip trailing punctuation,
  collapse whitespace) in any future corpus-selection script.
