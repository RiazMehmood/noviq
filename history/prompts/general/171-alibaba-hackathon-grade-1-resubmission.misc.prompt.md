---
id: 171
title: Alibaba Cloud AI Hackathon re-evaluation submission (Grade 2A → Grade 1)
stage: misc
date: 2026-08-28
surface: agent
model: claude-opus-5
feature: general
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (freeform request)
labels: [hackathon, business, submission, alibaba, qwen, deadline]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: C:/Users/hp/Desktop/Noviq/Alibaba Hackathon/Noviq-Re-evaluation-Submission-2026-08-28.md
    action: created
    note: Form-ready answers for all five fields, plus checklist and strategy notes
tests:
  - name: noviq.pk liveness (demo link cited in the submission)
    result: PASS — HTTP 200 in 1.58 s
---

## Prompt

beside this i our noviq project got grade 2A in ai hackathon by banoqabil and ali baba, now i m
resubmitting the details so we can get grade 1, ALIBABA CLOUD AI HACKATHON PAKISTAN 2026

[Full text of the organisers' email pasted by the architect — sets out: current position Grade 2A
under Alibaba Cloud qualification-round criteria; build phase runs to 4 September; tools available
(Qoder Free Tier at qoder.com/users/sign-up, own API keys, open-source models, any toolchain);
tooling used is NOT part of assessment; re-evaluation opportunity to reach Grade 1; project name,
focus area and core scope must stay as registered; form asks for problem statement and proposed
solution, detailed project description, technical approach and technologies, delivery plan to close
of build phase; submissions close 28 August 2026 at 21:00 PKT; Qoder Enterprise Plan awarded at
discretion based on submission outcome, acceptance of terms, and attendance at the 19 or 20 August
training; form at https://forms.gle/YQMUmBAaaDGbCWwn9; official communication only via that email
address and Discord. — Alkhidmat Foundation Pakistan, Bano Qabil Platform.]

now search the net and read the email also. and i am also going to share questionare with you so
write those answer in the file also and save that in noviq folder and create new folder for ali
baba hackathon and save the file there, here are the questions.

Problem statement and proposed solution * — The problem, who it affects, and how your project
solves it

Detailed project description * — What the product does, its core features, and the end to end user
flow

Technical approach and technologies * — Models, tooling, data and architecture, and why you chose
them

Delivery plan to the close of the build phase * — What is built, what is left, in what order, and
who is doing it

Repository or demo link — Optional. A public link to work in progress

## Response snapshot

Flagged the deadline first: the form closes 28 Aug 2026 21:00 PKT and the IBA gateway's own HTTP
`Date` header (fetched minutes earlier during unrelated VPN triage) read 14:23 GMT = **19:23 PKT**,
leaving ~1h35m. Prioritised writing over further research.

Web research (2 searches + 1 fetch of the official Alkhidmat event page) surfaced the one thing that
mattered and was not in the email: **the published judging criteria — "the impact of the problem you
are solving, the creative use of AI tools, and the practical viability of your solution."** Also
recovered the six tracks (Smart Agriculture, Financial Inclusion, Urdu & Regional Tech, Healthcare,
Education, Open Innovation). No public grade rubric exists; searches for "Grade 1 / 2A" criteria
returned nothing.

Created `C:\Users\hp\Desktop\Noviq\Alibaba Hackathon\` and wrote a single form-ready document with
each of the five fields as a copy-paste block, ordered internally to hit impact → creative AI use →
viability.

Key strategic call surfaced to the architect: **Noviq has run Qwen3-32B (Alibaba's own open-weight
family) in production since before the hackathon**, and validated Qwen3-8B on CPU this week. For an
Alibaba Cloud panel that is the strongest single card, and it is defensible on engineering grounds
(open weights are a sovereignty requirement; better Urdu coverage; fits one GPU) rather than looking
adopted for the submission.

Content drawn from measured facts only: 4,074 docs / 304,617 chunks / 49 departments; 7.7 s
end-to-end (post AWQ→Marlin fix, not the stale 90–120 s figure); 81.3 tok/s decode, 169–181 with
n-gram speculation; 8 concurrent → 596.8 tok/s aggregate, slowest 4.02 s; the VRAM-not-compute
finding and the one-24GB-card-covers-15-departments conclusion. Delivery plan ordered the oversize-
chunk repair (342 passages) first as the only correctness bug, then pre-flight runner, two-mark
capture with citation-open flag, Urdu eval, load test, demo rehearsal.

Verified `https://noviq.pk` → HTTP 200 / 1.58 s before citing it as the demo link.

## Outcome

- ✅ Impact: A complete, form-ready Grade-1 re-evaluation submission delivered inside a ~1.5 h window,
  written against the organisers' actual published judging criteria rather than the email alone.
- 🧪 Tests: noviq.pk liveness PASS (HTTP 200, 1.58 s) — the demo link is real before it is submitted.
- 📁 Files: 1 created (submission doc) + new folder `Desktop/Noviq/Alibaba Hackathon/`.
- 🔁 Next prompts: repair the 342 oversize chunks (item 1 of the delivery plan we just committed to
  in writing); build the pre-flight coverage runner; Urdu evaluation pass.
- 🧠 Reflection: ⭐⭐ **The judging criteria were on the public event page and not in the email.**
  Three sentences of published rubric reshaped every answer. Reading only what the user pastes is
  reading half the brief. ⭐⭐ **A deadline is a fact to establish, not to assume** — the PKT
  conversion came free from an HTTP `Date` header captured during an unrelated task minutes earlier.
  ⚠️ Deliberately did NOT claim accuracy superiority over NotebookLM or any named product: the
  comparison we ran does not support it, and an unsupported claim in front of a technical panel
  costs more than the sentence gains. ⚠️ Also stated the out-of-scope items (amendment graph,
  conflict/compare, map-reduce) explicitly — a delivery plan that admits what will not land reads as
  credible; one that promises everything reads as a wish.

## Evaluation notes (flywheel)

- Failure modes observed: none in execution. One tooling block: the auto-mode classifier denied a
  script that XOR-deobfuscated the IBA portal's JS during the earlier VPN triage; stopped and
  reported rather than working around it, and the diagnosis did not depend on it.
- Graders run and results (PASS/FAIL): noviq.pk liveness PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after the outcome is known, record which of the three
  criteria the panel actually cited — it would tell us whether the Qwen alignment or the measured
  performance numbers carried the grade.

## Amendment — positioning correction (same session)

**Architect's correction:** *"you write only for government but as u see the original page of
noviq.pk it is for all industries which works on written rules, policies, documentations, etc."*

They were right. I had written sections 1 and 2 from the Sindh pilot in my memory index rather than
from the product's actual positioning, narrowing a five-audience product to one vertical.

⭐⭐⭐ **The landing copy was NOT in the repo and NOT fetchable from the URL** — `noviq.pk` serves an
SPA shell, so WebFetch returned only the brand name, and `src/pages/` has no landing component (a
repo-wide grep timed out at 120 s and had to be backgrounded). **The working recipe: download the
served bundle and grep the string literals out of it.**
`curl -s https://noviq.pk/assets/index-C9ZyoNEs.js -o bundle.js`, then
`grep -oE '\{id:"[a-z]+",title:"[^"]+",fear:"[^"]+"' bundle.js` — which recovered the audience
cards intact, structure and all. This is the same served-bundle technique used on 2026-08-26 to
verify a deploy; it works just as well for **recovering product copy that exists only in the
built artifact**.

**The five audiences the live site actually addresses** (id/title/fear, verbatim):
- `government` — "You are answerable for a rule you never found."
- `academia` — "The regulation changed and your reading list did not."
- `industry` — "An auditor asks which clause you relied on."
- `developers` — "The spec is right there and nobody can find it."
- `policy` — "Two instruments say different things and nobody noticed."

Core promise lines recovered: *"Your material indexed: acts, rules, notifications, contracts,
manuals, records."* · *"Deployed on your own servers — documents and questions never leave them."*
· *"Every answer cites the document, the provision and the page it came from."*

**Rewrote** sections 1 and 2 around all five, plus ~15 targeted edits through sections 3–5
replacing officer/department/government-only wording. ⭐⭐ **The reframing that keeps the Sindh
evidence working instead of discarding it:** government is presented as the *proving ground*, not
the market — largest document estate, strictest data-residency constraint, least tolerance for an
unsourced answer — so **"a system that satisfies a government audit satisfies a corporate one."**
The 4,074-document corpus becomes the hardest-case proof rather than a scope limit.

⚠️ Flagged to the architect: the organisers require **name, focus area and core scope to stay as
registered**. If the registered focus area is government-specific, the registered fields must not
be changed — the broader framing lives in the description only.

🧠 Reflection: ⚠️⚠️ **I described the product from memory when the product itself was one curl
away.** MEMORY.md is dense with the Sindh pilot because that is where the work has been, and I let
recency of *my own notes* stand in for positioning the architect had actually shipped. When writing
externally-facing copy about a live product, **read the live product first** — the memory index is
a record of what I worked on, not of what the product is.

## Amendment 2 — three factual corrections, then the short version (same session)

The architect corrected me three times, each time on something I had asserted from my own notes.

1. **Positioning** (see Amendment 1) — five audiences, not government only.
2. ⚠️⚠️ **Model: ours is Qwen3-8B-AWQ, NOT Qwen3-32B.** *"we are not using qwen 3 32B model why
   are you writting this again n again."* Proof: `deploy/vllm/run-vllm.sh:15`. Full root cause in
   [[reference_production-model-is-qwen3-8b]]. All perf numbers were always 8B numbers; only the
   label was wrong. ⭐⭐ Rewrote it as an *argument* rather than a correction: in a
   retrieval-grounded design the model reads retrieved passages rather than recalling the rule, so
   the freed memory buys concurrency — buy VRAM, not FLOPs.
3. ⭐⭐ **Remaining work is Workflow + Meeting Assistant ONLY** — *"chat, global search, my project
   are working properly, tested multiple times."* I had filled Q4 with my engineering backlog (342
   chunks, pre-flight runner, two-mark capture, Urdu eval, load test). **My open-bug list is not
   the product roadmap.** Then: **no dates** on those two — still researching, needs specialist
   input and GPU capacity, follows the pilot launch.

⚠️ **The form rejected the long answers.** Cut all five to Q1 211 w / 1,321 ch · Q2 237 / 1,536 ·
Q3 326 / 2,019 · Q4 279 / 1,802 — a ~65–70% reduction — into
`Noviq-Hackathon-Answers-SHORT.docx`. ▶ Budget **~1,300–2,000 characters per field** next time.
Demo link went out then back in; the architect's final paste gives `noviq.pk`.

## ⚠️⚠️ Incident — I truncated MEMORY.md

Updating the index with a Python script, `io.open(p, "w")` **opened and truncated the file, then
the write raised `UnicodeEncodeError`** (lone surrogates from `\ud83d\udd0c` escapes — Python 3
rejects unpaired surrogates in UTF-8). The exception looked like "nothing happened"; in fact the
index was already **0 bytes**. Not a git repo, so no version history to restore from. Rebuilt it
from the session-start context.

⭐⭐⭐ **`open(path,"w")` destroys the file before your content is validated.** Never open the
destination for writing until the full output is encoded — build the string, `.encode('utf-8')`
it, then write; or write a temp file and rename. The failure mode is silent and total.
⭐⭐ **A traceback is not proof the target is unharmed** — check the target after any failed write.
⭐⭐ Rebuilding surfaced a second problem: the index was **already at its 24.4KB read limit**, so
trailing entries were being silently dropped on load. Recompressed to **17,079 bytes / 71 entries,
one line each**, all links verified to resolve.

## Files this session

- `Desktop/Noviq/Alibaba Hackathon/Noviq-Hackathon-Answers-SHORT.docx` ← **submitted**
- `Desktop/Noviq/Alibaba Hackathon/make-submission-docx.py` (generator, kept beside it)
- `Desktop/Noviq/Alibaba Hackathon/Noviq-Re-evaluation-Submission-2026-08-28.md` (long draft)
- memory: `project_alibaba-hackathon-and-vpn-2026-08-28.md`,
  `reference_production-model-is-qwen3-8b.md`, MEMORY.md rebuilt
