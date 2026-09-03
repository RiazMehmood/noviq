---
id: 0003
title: Contributor identity on the landing page, then the mail-DNS baseline
stage: green
date: 2026-08-21
surface: agent
model: claude-opus-5
feature: 028-noviq-pk-cutover
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (conversational)
labels: [landing-page, attribution, dns, email, f028]
links:
  spec: specs/028-noviq-pk-cutover/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/components/LoginScreen.tsx
  - src/assets/institutions/{govt-of-sindh,iba-karachi,sukkur-iba,maju,fast-nuces,isra,proxima-ai}.png
  - src/assets/people/{shakeel-khoja,sajjad-haider,imran-khan,tariq-mahmood,abdul-rehman-soomrani,ghulam-mujtaba-shaikh}.jpg
  - scripts/verify-mail-dns.sh
  - specs/028-noviq-pk-cutover/tasks.md
  - CHANGELOG_DEV.md
tests:
  - "tsc --noEmit: PASS"
  - "vite build: PASS"
  - "visual check of the built page at vite preview: PASS"
  - "scripts/verify-mail-dns.sh: FAILS 4/4 by design — records not yet published"
---

## Prompt

> complete in this order, 4 is confirmed, and i also want their linkedin profiles if
> exist by clicking on their names it sends to their linked in profile, and universities
> logos also needed. first complete this then come to 1

Followed mid-turn by:

> i want their pics also with the names side.

> and if you can get their current status in the university like head of department etc
> so write that also

> and write riaz mehmood as developer and younus shaikh as co-developer

> also add on top Director S&IT department Government of Sindh Saif ul Malook also

(Item "4" = the open question of Prof. Dr. Matee ur Rehman's affiliation, Annexure-E's
"Isra University" versus the NCS note's MAJU grouping. Item "1" = the Zoho email track,
T019–T029.)

## Response snapshot

Landing page: institution logos fetched from each institution's own site (MAJU publishes
only a white-on-transparent mark, so a dark variant was derived by inverting RGB while
preserving alpha). Contributor headshots taken from the same official faculty pages — IBA
embeds them as base64 data URIs, Sukkur IBA serves them from `facultyimages.iba-suk.edu.pk`
keyed by the `INS-*` id, whose page title confirmed each identity before use. Current
designations were read from those faculty pages rather than from search summaries. LinkedIn
links were set only where the profile's own headline named the matching institution — six of
twelve contributors have none and render as plain text.

Then the email track: `scripts/verify-mail-dns.sh` queries a public resolver over DNS-over-
HTTPS and reports 4 of 4 records missing.

## Outcome

- ✅ Impact: the acknowledgements section now carries logos, photographs, current titles
  and LinkedIn links; founders re-labelled Developer/Co-Developer; a new first card credits
  the Government of Sindh. T026's baseline is recorded as *not entered*, not *propagating*.
- 🧪 Tests: tsc + vite build clean; page verified visually in a browser, not from logs.
- 📁 Files: 1 component, 13 new assets, 1 script, tasks.md, CHANGELOG_DEV.md.
- 🔁 Next prompts: architect completes T019–T025 in the Zoho console, then re-run the
  verifier; supply founder LinkedIn URLs; confirm the three name spellings.
- 🧠 Reflection: three of the twelve names in Annexure-E disagree with the institutions'
  own records. Rendering both ("listed by the institution as …") keeps the page accurate
  without an agent silently overwriting a document the founders signed.

## Evaluation notes (flywheel)

- Failure modes observed: a `\b` inside a regex written through a `python <<'PY'` heredoc
  was collapsed by the shell and parsed as a **backspace**, leaving a literal `\x08` in the
  shipped regex that TypeScript compiled without complaint. Caught only by printing
  `repr()` of the emitted line. Same family as the F028 tunnel-token corruptions.
- Graders run and results (PASS/FAIL): n/a — no automated grader for presentation code.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): replace the sourced headshots with images the
  contributors supply themselves, which removes the consent question entirely.
