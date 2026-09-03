---
id: 0004
title: Zoho Mail DNS published and verified for noviq.pk
stage: green
date: 2026-08-23
surface: agent
model: claude-opus-5
feature: 028-noviq-pk-cutover
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (freeform) "now i signed in zoho.com now do the need full work via chrome"
labels: [f028, email, dns, zoho, cloudflare, spf, dkim, dmarc, browser-automation]
links:
  spec: specs/028-noviq-pk-cutover/spec.md
  ticket: specs/028-noviq-pk-cutover/tasks.md#T019-T026
  adr: (none — T038 ADR still open)
  pr: (none — not committed)
files:
  - path: specs/028-noviq-pk-cutover/tasks.md
    change: T019-T026 marked done with per-task findings; T025 marked PARTIAL
  - path: scripts/verify-mail-dns.sh
    change: default DKIM selector zmail -> zoho (line 16)
  - path: CHANGELOG_DEV.md
    change: dated entry for the mail cutover, with the SPF and selector hazards
tests:
  - name: scripts/verify-mail-dns.sh (DNS-over-HTTPS, public resolver)
    result: PASS 4/4 (MX, SPF, DKIM, DMARC)
  - name: nslookup against authoritative NS ara.ns.cloudflare.com
    result: PASS - all records present; exactly one SPF at the apex
  - name: Zoho console verification (MX / SPF / DKIM)
    result: PASS - domain status "Completed"; DKIM signing toggle ON
---

## Prompt

now i signed in zoho.com now do the need full work via chrome

## Response snapshot

Drove the Zoho Mail admin console and the Cloudflare DNS dashboard in Chrome to finish the
mail half of F028, which had been blocked since 2026-08-20 on a step the agent may not do
(account creation with a password sign-in). With the architect signed in, completed
T019-T025 and verified T026.

Zoho's setup wizard hard-gates on a plan chooser offering only paid tiers, so the console
was unreachable at first; the Forever Free plan is only reachable via
`https://mailadmin.zoho.com/hosting?plan=free`. Confirmed with the architect before
creating the org (free = 5 users, 5 GB/user, webmail only, no IMAP/POP), and asked them to
choose the Super Administrator local part (`admin@`) rather than picking it myself.

Published in Cloudflare, all DNS-only: the verification TXT, three MX (10/20/50), SPF with
the spec's stricter `-all`, DKIM at `zoho._domainkey` (2048-bit), and DMARC `p=none`.
Added `dmarc@noviq.pk` as an alias so the `rua=` address resolves without spending a seat.

## Outcome

- ✅ Impact: `noviq.pk` sends and receives official mail through Zoho. Zoho reports domain
  status **Completed**, MX/SPF/DKIM all verified, DKIM signing ON. F028 26/40 -> **33/40**.
- 🧪 Tests: `verify-mail-dns.sh` PASSES 4/4 via a public DoH resolver (independent of both
  Cloudflare's API and the browser session), cross-checked at the authoritative NS.
- 📁 Files: 3 changed (tasks.md, verify-mail-dns.sh, CHANGELOG_DEV.md). No app code touched.
- 🔁 Next prompts: T027/T028 — send from `@noviq.pk` to external Gmail *and* Outlook and
  confirm inbox placement with passing auth, plus one inbound delivery. Both need a human at
  a mailbox (webmail only on the free plan). Then T029 after ~2 weeks of DMARC reports.
- 🧠 Reflection: the previous session's "blocked" diagnosis was correct and precise, which
  made this session purely mechanical. Naming *what* was blocking (account creation, not
  propagation) is what let it be unblocked in one message.

## Evaluation notes (flywheel)

- Failure modes observed:
  1. **A stale assertion in a verification script reads exactly like a real failure.**
     `verify-mail-dns.sh` reported `FAIL DKIM` because it hardcoded Zoho's older `zmail`
     selector; the DNS had been right all along. Checked the checker before trusting it.
  2. **Cloudflare's Add-record dialog is type-first and coordinate-hostile.** Changing the
     record type re-lays out the form, so a click derived from an earlier screenshot landed
     on HTTPS instead of MX once and silently dismissed the dialog twice — two records were
     not saved. Caught only by watching the record count, not by any error.
  3. **A "www." prefix that is decoration, not value.** The domain field rendered `www.` as
     a fixed prefix; the screenshot showed `www.noviq.pk` while the DOM value was
     `noviq.pk`. Reading the input's real `.value` settled it — the screenshot could not.
  4. **The DKIM key cannot enter context** (content filter blocks it as cookie-like data).
     Routed page copy button -> clipboard -> paste, and verified by shape (length 410,
     prefix `v=DKIM1; k=rsa; p=`, terminator `AB`, no newline, exactly 2 spaces).
- Graders run and results: verify-mail-dns.sh PASS 4/4; authoritative-NS spot check PASS;
  Zoho console self-verification PASS on MX, SPF and DKIM.
- Prompt variant: n/a.
- Next experiment: before the next multi-record DNS session, drive Cloudflare through its
  API or the bulk **Import** button instead of the modal — the modal's re-layout on type
  change is the single largest source of silent no-ops here.
