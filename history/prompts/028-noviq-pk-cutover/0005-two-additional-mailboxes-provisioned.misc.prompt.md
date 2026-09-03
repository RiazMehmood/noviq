---
id: 0005
title: Two additional noviq.pk mailboxes provisioned
stage: misc
date: 2026-08-24
surface: agent
model: claude-opus-5
feature: 028-noviq-pk-cutover
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (chat) create riaz.mehmood@ and younus.shaikh@ with distinct passwords
labels: [ops, zoho-mail, no-code-change]
links:
  spec: specs/028-noviq-pk-cutover/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - none (external SaaS state only; no repo files changed)
tests:
  - manual: Zoho Users list shows "Total users: 3", both new users Active
---

## Prompt

now i want to create other email addresses also. riaz.mehmood@noviq.pk and
younus.shaikh@noviq.pk but they must have different password so we can easily login.

Follow-ups in the same turn:
- "Youn@s123 for younus and Ri@z123 for riaz"
- "so 1234"
- "Noviq@Sindh77 for riaz and Noviq@Karachi88 for younus"

## Response snapshot

Created both mailboxes in the Zoho Mail admin console (Users > Add) as role **User**:

- `riaz.mehmood@noviq.pk` — Riaz Mehmood
- `younus.shaikh@noviq.pk` — Younus Shaikh

Seats now 3 of 5 (free plan). Verified by the Users list reading **Total users: 3** with
both rows Active, not by the success screen alone.

Deviations from Zoho defaults, both deliberate:
- **"Force user to change password on first log in" — UNCHECKED.** Zoho checks it by
  default; left on, it discards the architect-chosen password at first login, which
  defeats the stated goal ("so we can easily login").
- **"Send credentials via email" — unchecked.** No mail sent on the user's behalf.

Password policy discovered by rejection (Zoho enforces more than it documents in the
tooltip): breach-corpus check ("Password has previously appeared in a data breach")
rejected `Ri@z1234`; a sequential-character rule rejected `Youn@s123`. The inline
tooltip only lists 5 rules (min 8, upper, lower, number, special) and showed all five
green for a password the server then refused — the tooltip is NOT the policy.

## Outcome

- ✅ Impact: two working mailboxes on the live domain; no repo or code change.
- 🧪 Tests: manual UI verification (Total users: 3, both Active, never signed in).
- 📁 Files: none.
- 🔁 Next prompts: F028 T027/T028 external deliverability test — now possible from any
  of the three mailboxes; T029 DMARC review ~2026-09-06.
- 🧠 Reflection: the first `Noviq@Karachi88` attempt was reported as failed by a toast
  that was actually STALE (it survived a full page navigation). The user count, not the
  banner, was the only trustworthy signal; the same password succeeded on a fresh form.

## Evaluation notes (flywheel)

- Failure modes observed: (1) sticky error toast indistinguishable from a fresh
  rejection — same family as the stale-DKIM-selector finding in PHR 0004; (2) client-side
  validation tooltip showing all-green for a server-rejected value; (3) two of my own
  hypotheses (length, then repeated digits) were wrong and were only killed by testing.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): when a SaaS form reports failure, re-read the
  authoritative collection count before diagnosing the input.
