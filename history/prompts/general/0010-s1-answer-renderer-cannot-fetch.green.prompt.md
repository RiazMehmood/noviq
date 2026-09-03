---
id: 0010
title: S1 — the answer renderer can no longer fetch anything
stage: green
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: general (security remediation)
branch: 027-my-projects
user: Riaz Mehmood
command: (freeform) "fix S1 now"
labels: [security, prompt-injection, exfiltration, dompurify, frontend]
links:
  spec: docs/security/red-team-findings-2026-08-05.md
  ticket: S1
  adr: n/a
  pr: none (committed d5187114, not deployed)
files:
  - src/lib/sanitizeHtml.ts
  - src/lib/sanitizeHtml.test.ts
  - src/components/AnswerBody.tsx
  - src/components/Markdown.tsx
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
  - docs/security/red-team-findings-2026-08-05.md
  - package.json
tests:
  - "vitest: 26/26 pass (was 6) — 20 new in sanitizeHtml.test.ts"
  - "tsc --noEmit: clean"
  - "npm run build: clean"
  - "sabotage check: removing \"img\" from FORBIDDEN_TAGS fails exactly the 2 image tests"
  - "inverse check: stock DOMPurify KEEPS the exfiltration <img>, proving the tests measure our policy"
---

## Prompt

fix S1 now

## Response snapshot

Closed S1 from the red-team backlog: answers can no longer cause the browser to
fetch anything without a click.

**The shape of the fix.** The vulnerability was not in the model, so the fix is not
in the prompt. Answers render `marked` → DOMPurify → `innerHTML`, and the sanitiser
used DOMPurify's **default profile, which permits `<img>` and remote `src`**. Turn
32's payload was a bare link needing a click; the same payload as a Markdown image
would have fetched with none. A renderer-level fix holds regardless of what the
model can be talked into writing — which matters because the same report shows the
prompt-level defence failing 2 times in 3.

**Three call sites, three different configs** — `AnswerBody` (`ADD_ATTR` only),
`Markdown` (bare), `ChatPage`'s copy-to-clipboard (bare). Rather than patch each,
introduced `src/lib/sanitizeHtml.ts` as a **single seam**; none of the three imports
`DOMPurify` any more. Same reasoning as `visible_source_ids` in F027: three places
enforcing one rule is how one gets forgotten. Included the clipboard path
deliberately — an answer pasted into Word must not carry an image that phones home
when the document is opened.

**⚠️ The near-miss worth recording.** My first instinct was to forbid the `style`
attribute outright, since `background:url()` is a fetch. Reading `maskEngine.ts`
first showed F020 masking wraps refilled values in
`<mark style="background:#dcfce7;…">` — a blanket ban would have **silently stopped
masked values being flagged in answers**, breaking a shipped feature with a security
fix and nobody noticing until a customer did. Instead the hook strips only
`url(...)`, and a test pins the masking markup so a future tightening cannot
regress it.

**Two properties that make the tests worth having.** An *inverse* check asserts
stock DOMPurify still *keeps* the exfiltration `<img>` — proof the suite measures
our policy rather than incidental `marked`/DOMPurify behaviour. And the policy was
**sabotage-tested**: deleting `"img"` from the forbidden list failed exactly the two
image tests and nothing else, then was restored. Same discipline as F027's negative
scoping test.

## Outcome

- ✅ Impact: the zero-click exfiltration path is closed at the only place it can be
  closed reliably. S1 done; S2–S6 still open.
- 🧪 Tests: 26/26 (was 6), `tsc` clean, `npm run build` clean.
- 📁 Files: new seam + 20 tests, three call sites rerouted, CHANGELOG and the
  security backlog updated. Committed `d5187114`. **Not deployed.**
- 🔁 Next prompts: deploy (frontend-only — the poller rebuilds the bundle and does
  not restart the backend); then S2 (output-side guard for instruction echo) and S4
  (re-run the document attacks with the tester's QA Notices stripped, which decides
  what we may claim in writing).
- 🧠 Reflection: the highest-value minute in this task was reading `maskEngine.ts`
  before writing the policy. The security fix and the shipped feature collided in a
  place neither file mentions.

## Evaluation notes (flywheel)

- Failure modes observed: one avoided rather than hit — a blanket `style` ban that
  would have broken F020 masking silently. Caught by reading the consumer before
  writing the rule.
- Graders run and results (PASS/FAIL): n/a — deterministic sanitiser, unit tested.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after deploy, re-run red-team turn 32
  against production and confirm the emitted URL renders as inert text.
