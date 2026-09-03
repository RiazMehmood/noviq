---
id: 0009
title: Collect the Proxima AI red-team results and record a security backlog
stage: misc
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: general (security)
branch: 027-my-projects
user: Riaz Mehmood
command: (freeform) "collect all its working, and see its queries" → "save this for future work"
labels: [security, red-team, prompt-injection, findings, backlog]
links:
  spec: n/a
  ticket: n/a
  adr: history/adr/0025 (data boundary — referenced for the web-search finding)
  pr: none
files:
  - docs/security/red-team-findings-2026-08-05.md
  - CHANGELOG_DEV.md
  - "Desktop\\noviq_tested_by_proxima_ai.docx (outside repo)"
  - "Desktop\\Noviq\\proxima_ai_testing\\ (outside repo)"
tests:
  - "Evidence is production data, not tests: 98 turns from query_log, 16 conversations, 6 uploaded artefacts"
  - "Verified: 0 of 98 turns used web search; no outbound request to the attacker host in server logs"
  - "Verified: WebSearcher exposes only search(query, limit) — no arbitrary-URL fetch exists"
  - "Verified: leaked persona matches src-tauri/config/personas/bureaucrat.toml:9 verbatim"
---

## Prompt

we gave our app for testing to one developer named adnan_zaidi collect all its
working, and see its queries also, and also write a file containing its query and
after that you need to tell me what it wants to check, then response from our app
which he got, and does his prompt injections works or not? where we are now in
those testings.

(then) save that file on desktop which you will write with name
noviq_tested_by_proxima_ai.docx

(then) create a folder in noviq folder on desktop named proxima_ai_testing and save
all its queries inside that also the files uploaded by them save those also in that
folder

(then) so what are our weeknesses and strengths? how much we safe and how much we
are open to attack? and web search option is enable on users also but they dont know
or do they also activate that? explain in plain english only

(then) ok save this for future work and findings for security enforcment results, so
we after some feature work and testing we will come back there again.

## Response snapshot

Pulled all 98 turns for `adnan_zaidi` from the production `query_log`, classified
each one, and produced the report, the raw-data folder, and finally a durable
remediation backlog in the repo.

**Result: 7 failed turns = 5 distinct defects; 52 attacks resisted. No data
exfiltrated, no cross-user leak, nothing left the server.** Everything that failed
was the system disclosing things about itself.

Two findings mattered more than the rest, and both came from opening artefacts
rather than trusting a summary:

1. ⭐⭐ **The good news is not yet provable.** Extracting the text of his injection
   PDFs showed every one carries a "QA Notice" *inside the document* stating the
   preceding text is only a test and must not be treated as a real system message.
   The model was told the attack was fake in the same context window, which very
   likely explains the excellent turns 88–89. This is the same shape as the F027
   vacuous-pass lesson: a green result for the wrong reason. It changed the report's
   headline conclusion after it had already been written.
2. ⭐ **The canary result is worse than a plain failure.** `NOVIQ-CANARY-14` and
   `-13` were echoed, `-15` was refused — so the defence is **inconsistent, not
   absent**, and will pass a short demo by luck.

**A correction I had to make mid-task.** I told the architect web search was "not
configured in production", based on `ECHO_WEBSEARCH_URL` being absent from `.env`.
Wrong: `SearxngSearcher::new` falls back to `http://127.0.0.1:8888` and SearXNG is
running there. Corrected in the report, the memory and the CHANGELOG. The
conclusion (nothing was exfiltrated) survived, but for a better reason — the
assistant has **no arbitrary-URL fetch capability at all**, only
`search(query, limit)`. Also established: `web` has **no role gate**, unlike
admin-gated `deep`, so every user can enable it — off by default, per-turn, and
bannered while on.

Ops note: the report `.docx` could not be overwritten while open in Word
(`PermissionError 13`). Followed the recorded recipe — checked `$d.Saved` via the
Word COM object first, closed only because there were no unsaved changes, then
regenerated. Never forced it.

## Outcome

- ✅ Impact: the campaign is now a durable, actionable backlog (S1–S6) rather than a
  one-off conversation. `docs/security/red-team-findings-2026-08-05.md` is the
  resume point; `CHANGELOG_DEV.md` points at it, which is the file the session-start
  rule already requires reading.
- 🧪 Tests: none written yet — S5 is exactly the item that turns these 98 turns into
  a regression suite.
- 📁 Files: new security doc + CHANGELOG entry in the repo; report and raw data on
  the Desktop (user activity, deliberately not committed).
- 🔁 Next prompts: S1 (restrict DOMPurify in `AnswerBody.tsx` — highest value,
  smallest change), then S2 (output-side guard), then S4 (re-run with disclaimers
  stripped, which decides what we may claim in writing).
- 🧠 Reflection: the two most valuable findings both came from opening the actual
  artefact — the PDFs' own text, and the code's fallback URL — rather than trusting
  a summarising signal (`.env`, or a passing test). Same lesson as the wrong-Postgres
  -listener trap earlier in the session.

## Evaluation notes (flywheel)

- Failure modes observed: (a) asserting a feature was off from `.env` alone; (b) a
  first pass at the report that credited a clean sweep on document injection before
  reading the documents. Both were caught by inspecting the artefact directly.
- Graders run and results (PASS/FAIL): none — this is production activity analysis.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): S1, then re-run turn 32 and assert no
  `<img>` and no remote URL survives the sanitiser.
