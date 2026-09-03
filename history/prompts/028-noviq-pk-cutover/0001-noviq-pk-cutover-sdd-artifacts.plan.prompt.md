---
id: 0001
title: noviq pk cutover sdd artifacts
stage: plan
date: 2026-08-20
surface: agent
model: claude-opus-5
feature: 028-noviq-pk-cutover
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: /sp.specify → /sp.plan → /sp.tasks
labels: [f028, infra, dns, cloudflare-tunnel, zoho-mail, ops, pknic]
links:
  spec: specs/028-noviq-pk-cutover/spec.md
  ticket: n/a
  adr: proposed — noviq-pk-public-entry-point-and-mail-boundary (not yet created)
  pr: n/a
files:
  - specs/028-noviq-pk-cutover/spec.md
  - specs/028-noviq-pk-cutover/plan.md
  - specs/028-noviq-pk-cutover/research.md
  - specs/028-noviq-pk-cutover/quickstart.md
  - specs/028-noviq-pk-cutover/tasks.md
  - specs/028-noviq-pk-cutover/checklists/requirements.md
  - "VM: ~/cloudflared-named.service.staged (staged, inert)"
tests:
  - "n/a — no automated test applies to a DNS delegation; verification is behavioural per FR-012"
---

## Prompt

Session opened with: `recall memory what is still pending?`

Then, after the pending list was reported:

```text
noviq.pk work start that
```

Then, mid-turn, redirecting the approach:

```text
use the sdd to apply this properly
```

Feature description passed to `/sp.specify`:

```text
Cut Noviq over from the rotating trycloudflare quick tunnel to the permanent noviq.pk
domain: named Cloudflare tunnel serving the app on the apex hostname, and Zoho Mail on
the same zone for official correspondence. Blocked on PKNIC nameserver delegation
(measured: registered and paid, all four NS fields empty, .pk registry returns NXDOMAIN).
Split of work: architect owns PKNIC panel, Cloudflare zone, Zoho signup and tunnel token;
agent owns VM-side tunnel install, systemd unit swap, URL-watcher retirement, and
verification by behaviour.
```

## Response snapshot

**The finding that reshaped the feature.** The recorded status was "blocked on the
nameserver change — `.pk` propagation can take 24–48 h", which implied the change had
been made six days earlier. It had not. PKNIC whois over port 43 returns `Status: Domain
is Registered`, created 2026-08-14, expiring 2028-08-14, with **all four Name Server
fields empty**; the authoritative `.pk` registry (`root-c1.pknic.pk`) returns NXDOMAIN.
The query method was validated against `jazz.pk` and `daraz.pk`, both of which return
nameservers from the same server, so NXDOMAIN is a real answer rather than a broken
query. Nothing was propagating because nothing had ever been delegated.

**Also measured this session:** `cloudflared` 2026.6.1 at `/opt/noviq/cloudflared` with
a quick-tunnel unit and no `~/.cloudflared` or `/etc/cloudflared` (clean slate);
`noviq-url-watch.service` active, greps only `trycloudflare.com` URLs, so it goes stale
silently after cutover; `CorsLayer` at `router.rs:278-281` uses
`AllowOrigin::mirror_request()`, so a new hostname needs no code change; 4 stale
quick-tunnel hostnames hard-coded across two `specs/014-server-thin-client/` documents.

**Artifacts produced** (full SDD chain on branch `028-noviq-pk-cutover`): spec with 3
prioritised user stories, 13 FRs and 7 measurable success criteria; quality checklist
recording three failed validation items and their fixes; plan with a constitution check;
research resolving 5 questions; quickstart operator runbook; 39 tasks.

**Constitution check**: PASS with one deviation carried forward, not newly introduced.
Principle I (Privacy-First, Offline-Always) is already deviated from — the product has
served the public internet through a quick tunnel since F014/F016 and crosses a data
boundary under ADR-0025. This feature names that existing exposure rather than widening
it, and mail carries correspondence only, never corpus content. Recorded in Complexity
Tracking with the measured justification for rejecting self-hosted mail.

**Key design decisions**: token-first tunnel (`cloudflared tunnel login` is
browser-interactive and the agent has no browser — a capability boundary, not a
preference); token in a root-owned `0600` `EnvironmentFile`, never inlined into
`ExecStart` where `systemctl cat` and `ps` expose it; the delegation check is an explicit
**gate** (T008) rather than a step, precisely because the invisible version of that wait
cost six days; rollback copy taken *before* the unit is replaced.

**Deliberately left unresolved**: whether the named tunnel fixes web token streaming.
The 2026-07-05 finding (SSE delivers headers and zero body bytes through the quick
tunnel; web chat has never streamed) could not separate quick-tunnel behaviour from
Cloudflare edge behaviour. FR-013 requires the result be measured and recorded at
cutover; no task claims a fix and the polling fallback stays regardless.

## Outcome

- ✅ Impact: Feature 028 fully specified, planned and broken into 39 tasks. The real
  blocker was identified and corrected — an empty delegation, not a propagation wait —
  turning an indefinite wait into a three-task critical path (T006 → T007 → T008), all
  architect-owned. VM side staged and inert, so cutover is minutes once the gate opens.
- 🧪 Tests: none — no automated test applies to a DNS delegation. All verification is
  behavioural (a page loaded and signed into, a message delivered to an inbox, a query
  answered with citations), per FR-012 and standing project practice.
- 📁 Files: 6 new artifacts under `specs/028-noviq-pk-cutover/`; 1 inert unit staged on
  the VM. **Zero application-code changes**, so no CUDA rebuild is triggered.
- 🔁 Next prompts: architect executes T006/T007 (Cloudflare zone, PKNIC nameservers);
  then `/sp.implement` from T008. `/sp.adr noviq-pk-public-entry-point-and-mail-boundary`
  suggested and awaiting consent. T032–T034 (repo hostname cleanup) need no gate and
  could run today.
- 🧠 Reflection: The memory note read "blocked on the nameserver change", which is
  ambiguous between *waiting for it to propagate* and *waiting for someone to make it*.
  Six days were lost to the first reading when the second was true. Checking the
  authoritative registry took one command. **When a task is "blocked on propagation",
  verify the change was actually entered before waiting on it** — and write the
  distinction into the note, not just the blocker.

## Evaluation notes (flywheel)

- Failure modes observed: a status note phrased ambiguously ("blocked on the nameserver
  change") was carried forward across sessions as fact without re-measurement. The
  ambiguity, not the blocker, was the defect.
- Graders run and results (PASS/FAIL): spec quality checklist — 3 items FAILED on
  iteration 1 (implementation details in requirements; technology-specific success
  criteria; one untestable verification requirement), all fixed; iteration 2 PASS on all
  16 items. Constitution check PASS with 1 documented deviation.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): when a memory note records a blocker, make
  it state who owns the next action and what command proves it is done. "Blocked on X"
  should never be storable without an owner and a check.
