# Implementation Plan: noviq.pk Domain Cutover

**Branch**: `028-noviq-pk-cutover` | **Date**: 2026-08-20 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/028-noviq-pk-cutover/spec.md`

## Summary

Move Noviq's public entry point from a regenerating `trycloudflare` quick tunnel to a
named Cloudflare tunnel serving the apex hostname `noviq.pk`, and put Zoho Mail on the
same zone. **This feature changes no application code.** It is entirely registry, DNS,
systemd-unit and documentation work, which is why it carries no migration, no contracts
and no data model.

The plan is shaped by one measured fact: `noviq.pk` is registered and paid through
2028-08-14, but all four nameserver fields at PKNIC are **empty**, and the `.pk`
registry returns NXDOMAIN. Nothing is propagating, because nothing was ever delegated.
Every step that follows is gated on that one architect-owned change, so the plan is
staged as **prepare → gate → cut over → verify**, not as a continuous run.

## Technical Context

**Language/Version**: N/A — no application code changes. Configuration only.
**Primary Dependencies**: `cloudflared` 2026.6.1 (already on the VM at `/opt/noviq/cloudflared`); systemd; PKNIC registry panel; Cloudflare DNS + Zero Trust Tunnels; Zoho Mail
**Storage**: N/A — no schema change, no migration. `echo.db`/PostgreSQL untouched.
**Testing**: Behavioural verification only — DNS queries against the authoritative `.pk` registry and public resolvers; a real browser sign-in and query on the new hostname; live mail delivery to external Gmail and Outlook accounts. No unit or integration test applies to a DNS delegation.
**Target Platform**: Linux VM `<PRIVATE_IP>` (`appadmin`), reachable over VPN; public ingress via Cloudflare edge
**Project Type**: Infrastructure / operations — outside the `src/` and `src-tauri/` trees
**Performance Goals**: Public address stable across restarts (zero rotations, versus one per restart today); user-visible interruption during the switch under 60 s
**Constraints**: The agent has VM shell access only. It has no browser session for Cloudflare, no PKNIC credentials, and no Zoho account, so the named tunnel must be created **token-first** (`cloudflared tunnel run --token`) rather than via the browser-interactive `cloudflared tunnel login`. The token is a credential and must never enter the repository, a process listing, or the shared plaintext ops file.
**Scale/Scope**: One domain, one zone, one tunnel, one systemd unit swapped, one watcher retired, a handful of mailboxes

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Verdict | Reasoning |
|---|---|---|
| I. Privacy-First, Offline-Always | ⚠️ **Pre-existing deviation, not widened** | Noviq already serves the public internet through a Cloudflare quick tunnel (F014/F016) and already sends corpus content to a hosted model boundary under ADR-0025. This feature gives that existing exposure a permanent name; it does not create new outbound data flow. Mail carries human correspondence only — never corpus text, queries, or inference. Recorded in Complexity Tracking. |
| II. Tiered Intelligence | ✅ Not engaged | No inference path touched. |
| III. Single Model Slot | ✅ Not engaged | No model loaded, evicted, or added. |
| IV. SQLite as Single Source of Truth | ✅ Not engaged (deviation pre-exists) | No database work at all. The Postgres deviation from F011/F012 is untouched by this feature and is not re-litigated here. |
| V. Pending Actions Queue | ✅ Compliant in spirit | The destructive-ish steps (swapping the live unit, stopping the watcher) are gated on explicit architect confirmation at cutover, and the previous unit is retained for rollback. |
| VI–VIII (personas, hardware tiers, incremental delivery) | ✅ Not engaged | No agent config, no tier detection. P1 (app on the domain) ships and is testable without P2 (mail), satisfying VIII's independent-shippability requirement. |
| IX–XIII (domain, officer UX, citations, amendments, graph) | ✅ Not engaged | No retrieval, answer, or citation behaviour changes. |
| Dev constraint: *no secrets hardcoded; use `.env`* | ✅ **Directly binding** | The tunnel token goes into a root-owned `0600` `EnvironmentFile`, referenced by the unit as `${TUNNEL_TOKEN}`. It is deliberately **not** inlined into `ExecStart`, which `systemctl cat` and `ps` would expose to any local user. |
| Dev constraint: *smallest viable diff* | ✅ Compliant | Zero application-code diff. |
| Tech Stack Invariants | ✅ Untouched | No layer in the locked table changes. |

**Gate result: PASS.** One deviation carried forward and documented, none newly introduced.

📋 **Architectural decision detected**: making the public exposure permanent and named,
and placing official correspondence with a third-party mail provider, is a durable
platform and data-boundary decision with alternatives that were genuinely weighed
(self-hosted mail was measured and rejected; apex versus subdomain was decided by the
architect). Document reasoning and tradeoffs? Run `/sp.adr noviq-pk-public-entry-point-and-mail-boundary`

## Project Structure

### Documentation (this feature)

```text
specs/028-noviq-pk-cutover/
├── plan.md              # This file
├── spec.md              # Feature specification
├── research.md          # Phase 0 output — decisions and rejected alternatives
├── quickstart.md        # Phase 1 output — the operator runbook
├── checklists/
│   └── requirements.md  # Spec quality validation
└── tasks.md             # Phase 2 output (/sp.tasks — NOT created by /sp.plan)
```

`data-model.md` and `contracts/` are **deliberately absent**. The feature introduces no
persisted entity and no API surface; the spec's "Key Entities" are external records
(a registry delegation, a DNS zone, a tunnel, a mailbox domain) that live in third-party
systems and have no representation in our storage. Generating empty scaffolds for them
would be noise.

### Source Code (repository root)

No files under `src/` or `src-tauri/` are modified. The artifacts this feature touches:

```text
# On the VM (<PRIVATE_IP>) — not in version control
/etc/noviq/cloudflared.env             # NEW  root:root 0600 — holds TUNNEL_TOKEN
/etc/systemd/system/cloudflared.service # REPLACED — named tunnel, token from EnvironmentFile
/etc/systemd/system/cloudflared-quick.service.bak # NEW — retained rollback copy
/opt/noviq/current-url.txt             # UPDATED — permanent hostname
~/cloudflared-named.service.staged     # STAGED (already written, inactive)
noviq-url-watch.service                # STOPPED + DISABLED — can no longer match

# In version control
specs/028-noviq-pk-cutover/*                          # this feature's artifacts
specs/014-server-thin-client/WINDOWS_CLAUDE_FOLLOWUPS.md  # stale quick-tunnel URLs
specs/014-server-thin-client/VM_HANDOFF_VERIFY_DEPLOY.md  # stale quick-tunnel URL
CHANGELOG_DEV.md                                      # dated entry on completion
```

**Structure Decision**: Operations feature, no source tree. Changes are confined to VM
system configuration plus the two connectivity documents under
`specs/014-server-thin-client/` that hard-code the old rotating hostname, which FR-011
requires be corrected so nothing in the repository advertises a dead address.

## Phase 0: Research

See [research.md](./research.md). Four questions were open; all four are resolved, none
required a `NEEDS CLARIFICATION` marker to survive into the plan:

1. **Why does the domain not resolve?** — Measured. Empty delegation at PKNIC, not a
   propagation delay. Determines that step 1 is a registry change, not a wait.
2. **Token-first or certificate-first tunnel?** — Token-first, because
   `cloudflared tunnel login` is browser-interactive and the agent has no browser.
3. **Apex or subdomain?** — Apex, decided by the architect. Cloudflare CNAME-flattening
   makes an apex tunnel record coexist with Zoho MX on the same name.
4. **Will the named tunnel fix SSE streaming?** — **Unknown, and deliberately left
   unknown.** Treated as a measurement at cutover, not a promised outcome.

## Phase 1: Design

See [quickstart.md](./quickstart.md) for the operator runbook — the ordered console
steps, the DNS record shapes, the cutover commands, the rollback, and the verification
checks.

**Design decisions carried into tasks:**

- **The delegation check is a hard gate, not a step.** Tasks after it must not begin
  until `nslookup -type=NS noviq.pk root-c1.pknic.pk` returns the Cloudflare
  nameservers. The gate exists specifically because the last six days were spent
  waiting on propagation for a delegation that had never been entered — an explicit
  check turns an invisible wait into a visible pass/fail.
- **Ownership is encoded per task.** Architect-owned tasks are console work the agent
  physically cannot perform; agent-owned tasks are VM shell work. Mislabelling these
  is what produces a blocked session, so every task carries an owner.
- **Rollback before cutover.** The current working unit is copied aside *before* it is
  replaced, so recovery is a `cp` and a restart rather than a reconstruction.
- **Verification is behavioural.** Per FR-012 and standing project practice, each
  verification task names an observation — a page loaded and signed into, a message
  delivered to an inbox, a query answered — never "the logs look clean".

**Agent context**: not updated. `update-agent-context.sh` records new *technologies*
introduced by a feature; this feature introduces none, and adding "Cloudflare tunnel"
to the active-stack list would misrepresent an ops change as a stack change.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Principle I — public internet exposure through a third-party edge network | The product is being demonstrated to government testers who are not on the VPN. A permanent, shareable address is a precondition for the beta and for the NIC/SEIC applications, which currently list no website. | VPN-only access was the status quo and blocks every external tester. Direct public IP exposure is impossible — the VM is `<PRIVATE_IP>`, private, with no inbound public address (measured). The tunnel is the only available ingress. |
| Principle I — official correspondence held by a third-party mail provider | Mail must be *deliverable*; a mailbox that lands in spam is worse than none for government correspondence. | Self-hosting was built out and rejected **on measurement**: outbound port 25 is open and egress is `<VPN_GATEWAY_IP>`, but that is the NAT egress, not an inbound address, and we have no PTR/reverse-DNS control over that block. Without PTR control, mail is filtered as spam regardless of correct SPF/DKIM/DMARC. Correspondence never carries corpus content, so the data boundary crossed is human email, not the legal corpus. |
