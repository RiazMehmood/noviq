# Tasks: noviq.pk Domain Cutover

**Feature**: `028-noviq-pk-cutover` | **Date**: 2026-08-20
**Input**: [spec.md](./spec.md) · [plan.md](./plan.md) · [research.md](./research.md) · [quickstart.md](./quickstart.md)

## Owner legend

Every task carries an owner. This is the single most important field in this file —
mislabelling it is what produces a session that stalls waiting on work nobody can do.

- **`[ARCHITECT]`** — browser-console work in accounts the agent has no session for
  (PKNIC panel, Cloudflare dashboard, Zoho admin). The agent **cannot** perform these
  and must not attempt workarounds.
- **`[AGENT]`** — shell work on the VM `<PRIVATE_IP>` over VPN.
- **`[BOTH]`** — a verification needing the architect's browser and the agent's shell.

Tests: no automated test tasks. There is no unit or integration test for a DNS
delegation; verification is behavioural per FR-012 and lives in the story phases.

---

## Phase 1: Setup

- [x] T001 [AGENT] Measure the true blocking state of the domain — PKNIC whois over port 43 plus an authoritative `nslookup -type=NS noviq.pk root-c1.pknic.pk`, with the method validated against `jazz.pk`/`daraz.pk` — and record the finding in `specs/028-noviq-pk-cutover/research.md` §R1
- [x] T002 [AGENT] Inventory the VM tunnel state (`cloudflared` version and path, live unit `ExecStart`, absence of `~/.cloudflared` and `/etc/cloudflared`) and record it in `specs/028-noviq-pk-cutover/research.md` §R2
- [x] T003 [P] [AGENT] Confirm CORS will not block a new hostname by reading the `CorsLayer` construction in `src-tauri/src/api/router.rs:278-281`, and record the result in `specs/028-noviq-pk-cutover/research.md` §R5
- [x] T004 [P] [AGENT] Identify everything that must be retired at cutover — `noviq-url-watch.service`, `/opt/noviq/url-watch.sh`, `/opt/noviq/current-url.txt` — and record in `specs/028-noviq-pk-cutover/research.md` §R5
- [x] T005 [AGENT] Stage the inert named-tunnel systemd unit at `~/cloudflared-named.service.staged` on the VM, using `EnvironmentFile` rather than an inlined token. **Pre-flight verified 2026-08-20**: `systemd-analyze verify` exits 0; `noviq-server` listens on `0.0.0.0:8080` with local `/api/health` = 200, confirming the unit's `localhost:8080` target is correct (the biggest silent cutover risk). Note the `EnvironmentFile` is `root:root 0600` while the service runs as `appadmin` — correct by design, because systemd reads it as PID 1 *before* dropping privileges, so the token stays unreadable to that user.

## Phase 2: Foundational — BLOCKING

**No task in Phase 3 or later may begin until T008 passes.** This phase is the whole
critical path; it is entirely architect-owned except the gate check.

- [x] T006 [ARCHITECT] Create the `noviq.pk` zone on Cloudflare (Free plan) and copy the two assigned `*.ns.cloudflare.com` nameservers exactly — per `quickstart.md` Step 1. **DONE 2026-08-20** — zone live on Free plan under account `Engr.riaz2010@gmail.com`; assigned nameservers **`ara.ns.cloudflare.com`** and **`keenan.ns.cloudflare.com`**; Zone ID `598cd0e9207b07a3b0cd4833c1fc7642`, Account ID `dc9e27a2aeec42eaa74b640b398ff35f`. Zone reports "Waiting for your registrar to propagate your new nameservers", which is the expected state until T007 lands.
- [x] T007 [ARCHITECT] Enter those two nameservers into the four currently-empty NS fields for `noviq.pk` in the PKNIC panel — per `quickstart.md` Step 2. **DONE 2026-08-20 ~12:05 PKT**, driven in the browser with the architect's approval. ⚠️ PKNIC models nameservers as a **reusable resource**, not free-text fields on the domain: had to create a *Nameserver Set* first (`createUserNs.PK` → **set #483374** = `ara.ns.cloudflare.com` / `keenan.ns.cloudflare.com`, both optional slots left empty), then assign that set to `noviq.pk` via *Modify Nameserver & DNS* → **Update Selected Domains**. Creating the set alone changes nothing; the assign step is what alters the delegation. Domain record now shows both nameservers.
- [x] T008 [AGENT] **GATE** — verify delegation is published: `nslookup -type=NS noviq.pk root-c1.pknic.pk` returns the Cloudflare nameservers, confirmed against `8.8.8.8` and `1.1.1.1`. On persistent NXDOMAIN past 48 h, return to T007 rather than continuing to wait — per `quickstart.md` Step 3. **PASSED 2026-08-20 ~15:10 PKT** — `root-c1.pknic.pk` and `8.8.8.8` both return `ara.ns.cloudflare.com` + `keenan.ns.cloudflare.com`. Published in **~3 hours**, far inside the 24–48 h window. US1 and US2 are now unblocked.

**Gate criteria**: registry returns Cloudflare nameservers. Until then, US1 and US2 are
both blocked and no VM change should be made.

---

## Phase 3: User Story 1 — Stable public address (Priority: P1) 🎯 MVP

**Goal**: A tester types `noviq.pk` and reaches Noviq, at an address that survives every
restart.

**Independent test**: Open `https://noviq.pk`, sign in, run a corpus query, get an
answer with citations. Restart the tunnel three times; the address never changes.
Delivers the stable entry point with no mail configured at all.

- [x] T008a [US1] [ARCHITECT] **Activate Cloudflare Zero Trust on the account** — an unlisted prerequisite discovered 2026-08-21. `quickstart.md` Step 6 and T009 both assume Networks → Tunnels is directly reachable; on a fresh account it redirects to `/one/onboarding` → "Choose a plan", and selecting even the **Zero Trust Free** tier ($0/seat, 50-seat cap) routes through `/zero-trust/checkout/payment`, which demands a card or PayPal before it will activate. ⚠⚠ **"Free" still requires a payment method on file** against a metered product ("additional usage beyond included allowance billed monthly"). The architect completed this step personally — the agent stopped at the payment form and did not enter card, PayPal, or the emailed Stripe Link verification code.
- [x] T009 [US1] [ARCHITECT] Create the named tunnel `noviq-prod` in Cloudflare Zero Trust → Networks → Tunnels and copy **only** the connector token — per `quickstart.md` Step 6. **DONE 2026-08-21**, driven in the browser with the architect's approval. Tunnel `noviq-prod`, type `cloudflared`, **Tunnel ID `56e33258-c0c8-4281-ada5-fc718d0880d1`**. ⚠️ **UNLISTED PREREQUISITE — see T008a**: Networks → Tunnels was unreachable because Zero Trust had never been activated on the account. ⭐ The agent deliberately did **not** read the connector-token screen (navigated past it by element ref, never screenshotted) so the token never entered a chat log, per T011. Status is `Inactive` until the VM connector runs — expected, not a fault.
- [x] T010 [US1] [ARCHITECT] Add public hostnames on the tunnel: `noviq.pk` → `http://localhost:8080` and `www.noviq.pk` → `http://localhost:8080`, both Proxied. **DONE 2026-08-21** — both published application routes created with path `*` (match-all) and service type **HTTP** (not HTTPS: T005 pre-flight measured `noviq-server` answering plain HTTP on `0.0.0.0:8080`). Cloudflare auto-created both DNS records. ⭐ **Proxied confirmed by measurement, not by the console**: both names resolve to Cloudflare anycast (`<REDACTED_IP>`, `<REDACTED_IP>`) from `1.1.1.1`; a DNS-only record would have exposed the `.cfargotunnel.com` target instead. Catch-all rule left at the default `http_status:404`.
- [x] T011 [US1] [ARCHITECT] Transmit the connector token to the agent privately — never into the repository, a chat log, or `C:\Users\hp\Desktop\Noviq\Ops\ssh file.txt` **DONE 2026-08-21 — satisfied WITHOUT the token ever being read.** ⭐⭐⭐ The architect copied the dashboard command to the Windows clipboard; the agent piped it `Get-Clipboard` → `ssh` stdin straight into the VM, so the token passed through the shell and never entered the transcript. Integrity proven by comparing `sha256` on both ends (`b816e4ea…`), not by eyeballing the value.
- [x] T012 [US1] [AGENT] Copy the working unit aside for rollback: `/etc/systemd/system/cloudflared.service` → `/etc/systemd/system/cloudflared-quick.service.bak` — **before** any replacement (FR-010) **DONE 2026-08-21** — `root:root 644`, 333 bytes, taken before T014 touched anything.
- [x] T013 [US1] [AGENT] Write the token to `/etc/noviq/cloudflared.env` as `root:root 0600`, never inlined into `ExecStart` where `systemctl cat` and `ps` would expose it (FR-009) **DONE 2026-08-21** — `root:root 0600`, 198 bytes, 184-char token, sha `b816e4ea…`. ⚠️⚠️ **Three silent corruptions were caught only by measuring; each would have produced a working-looking file that failed at connect time.** (1) PowerShell 5.1 prepends a **UTF-8 BOM** when piping to a native command — fix by setting BOTH `$OutputEncoding` and `[Console]::OutputEncoding` to `New-Object System.Text.UTF8Encoding $false`. (2) PowerShell **eats backslashes** in native-command arguments, so `printf` with a newline escape emitted a literal `n` onto the END of the token, and a `tr -d` CR/LF filter stripped the LETTERS r and n instead — use `echo` for the newline and `tr -dc '[:print:]'` for the filter. (3) `cut -d` and `awk` lose their quoting across the same boundary — stage a script on the VM instead. ⭐⭐⭐ **Verify a secret by hash on both ends; a byte count alone would have passed case (1).**
- [x] T014 [US1] [AGENT] Install `~/cloudflared-named.service.staged` as `/etc/systemd/system/cloudflared.service`, `daemon-reload`, and restart `cloudflared` — per `quickstart.md` Step 7c **DONE 2026-08-21 ~10:49 UTC** — `ExecStart` is now the named-tunnel form with `EnvironmentFile=/etc/noviq/cloudflared.env`; `systemd-analyze verify` ok; service `active`; cloudflared precheck all PASS, protocol `quic`.
- [x] T015 [US1] [BOTH] Verify by behaviour: `https://noviq.pk` loads the sign-in page over valid TLS with no warning, a signed-in user submits a corpus query, and a real answer with citations comes back (FR-001, FR-004, FR-012) **DONE 2026-08-21** — verified end-to-end through `https://noviq.pk` over valid TLS with no warning (cert `CN=noviq.pk`, Google Trust Services). Signed in as admin, submitted “What is the procedure for disciplinary action against a government servant?” over the full 3,931-document corpus, and got a substantive answer citing the West Pakistan Government Servants (Conduct) Rules, 1966 with inline citations [1]–[4] in ~11 s. ⭐ The architect signed in; the agent does not enter credentials.
- [x] T016 [P] [US1] [BOTH] Verify `https://www.noviq.pk` reaches the same application (FR-003) **DONE 2026-08-21** — HTTP 200, identical 749-byte payload to the apex.
- [x] T017 [US1] [AGENT] Verify address stability: `systemctl restart cloudflared` three times and reboot the host once; confirm the public address is unchanged and the app still answers (FR-002, SC-002) **DONE 2026-08-21** — three `systemctl restart cloudflared` cycles plus a full host reboot (boot id `b98c931e…` → `e1880eac…`, SSH back in ~45 s). Address unchanged throughout: `https://noviq.pk`, `https://www.noviq.pk` and `/api/health` all returned **200** after the reboot, with no DNS or config change — which is the whole point of the named tunnel versus the quick one. ⭐⭐ **Postgres survives because `noviq-server` owns it**: the embedded postmaster sits inside `noviq-server.service`'s cgroup (`postgresql_embedded`), so systemd brings it back indirectly — there is **no postgres systemd unit, no crontab entry**, and its parent shows as `init`, which makes it look unmanaged. Verified before rebooting rather than after. ⚠️ Its port **drifted 37855 → 33493 across the reboot**, as expected — never hardcode the PG port. ⚠️ `cloudflared` settled at **3 of 4 edge connections**; `connIndex=2` flapped with `control stream encountered a failure` against edge ip `<REDACTED_IP>` for ~2 minutes after boot, then went quiet (0 warnings in the following 60 s). Serving is unaffected but redundancy is reduced — worth re-checking on the next restart. ⭐ A stray test postgres from 2026-08-07 (`/home/appadmin/test-pg-install`, `/tmp` data dir, port 33761) had been running for two weeks and was cleared by the reboot.
- [ ] T018 [P] [US1] [AGENT] Measure and record whether web token streaming works on the named-tunnel path — improved, unchanged, or worse — in `CHANGELOG_DEV.md`. Do **not** claim a fix; the polling fallback stays in place regardless (FR-013, research.md §R4) ⏳ **PARTIALLY MEASURED 2026-08-21** — on the named-tunnel path a signed-in corpus query rendered **progressively**: at ~5 s the heading and first section were on screen with the stop button still active; the full cited answer completed at ~11 s. So streaming does work through the named tunnel. ⚠️ **“improved / unchanged / worse” cannot be answered** — that needs a quick-tunnel baseline measured under the same query, and the quick tunnel was torn down by T014 before this was captured. Recording the observation, NOT a comparison. The polling fallback stays in place regardless (FR-013).

**Checkpoint**: US1 complete and independently shippable. Mail may never be configured
and the product still has its permanent address.

---

## Phase 4: User Story 2 — Official mail on the domain (Priority: P2)

**Goal**: Staff send and receive at `@noviq.pk`, and the mail actually arrives.

**Independent test**: Send from a `@noviq.pk` mailbox to external Gmail and Outlook
accounts; both land in the inbox, not spam. Reply; the reply arrives. Testable the
moment mail is configured, independent of US1.

- [x] T019 [US2] [ARCHITECT] Register `noviq.pk` in the Zoho admin console and collect the three per-account values: domain-verification record, DKIM selector + public key, and the MX hostnames **for your data centre** — per `quickstart.md` Step 4 **DONE 2026-08-23 (agent, in-browser)** — org `Noviq` created on the Zoho **Forever Free** plan (1 domain, 5 users, 5 GB/user, **no IMAP/POP/ActiveSync**). ⚠ The console hard-gates on a plan chooser offering only paid tiers; the free plan is reachable only via `https://mailadmin.zoho.com/hosting?plan=free`. Data centre = **zoho.com (US)** → MX `mx/mx2/mx3.zoho.com`.
- [x] T020 [US2] [ARCHITECT] Add the MX records in Cloudflare DNS from the console values, set **DNS only** (grey cloud) — proxying breaks mail delivery (FR-008) **DONE 2026-08-23** — all three added at `@`, **DNS only** (TXT/MX cannot be proxied). Zoho console reports “Your domain's MX Records are pointed to Zoho”, all three green.
- [x] T021 [P] [US2] [ARCHITECT] Add the domain-verification TXT (or `zb*` CNAME) record and complete verification in the Zoho console **DONE 2026-08-23** — TXT `@` = `zoho-verification=zb38175890.zmverify.zoho.com`; Zoho reports “You have now verified your domain ownership”.
- [x] T022 [P] [US2] [ARCHITECT] Add the DKIM TXT record at `<selector>._domainkey` using the console's public key, and enable signing in Zoho **DONE 2026-08-23** — selector **`zoho`** (NOT Zoho's older `zmail` default that `verify-mail-dns.sh` assumed; the script default is now corrected). **2048-bit** key at `zoho._domainkey`; Cloudflare chunks the 410-char value automatically. Zoho reports “DKIM selector is successfully verified” and the **signing toggle is ON**.
- [x] T023 [US2] [ARCHITECT] Add the single SPF record `v=spf1 include:zohomail.com -all` — exactly one such record may exist on the domain; a second invalidates both and `-all` hard-fails all mail **DONE 2026-08-23** — `v=spf1 include:zohomail.com -all` at `@`. ⚠ Zoho's console **recommends `~all`**; the spec's stricter `-all` was used and Zoho accepted it (“SPF Records have been pointed successfully”). Confirmed **exactly one** SPF record at the apex. Revisit if anything other than Zoho ever sends as `@noviq.pk`.
- [x] T024 [P] [US2] [ARCHITECT] Add the DMARC record `v=DMARC1; p=none; rua=mailto:dmarc@noviq.pk` in report-only mode (FR-007) **DONE 2026-08-23** — TXT `_dmarc` = `v=DMARC1; p=none; rua=mailto:dmarc@noviq.pk`, report-only. Note Zoho's DMARC tab is only a *generator*; the live record is the one in Cloudflare, which is what receivers read.
- [x] T025 [US2] [ARCHITECT] Create the working mailboxes and the `dmarc@noviq.pk` reporting address **PARTIAL 2026-08-23** — `admin@noviq.pk` created as **Super Administrator** (architect chose the local part), and `dmarc@noviq.pk` added as an **alias** on it so DMARC reports have a real destination without burning one of the 5 free seats. ▶ Remaining: any further per-person mailboxes the founders want.
- [x] T026 [US2] [AGENT] Verify the published records from the public internet — MX, SPF, DKIM and DMARC each resolve to the intended values — **PASSES 2026-08-23, 4 of 4.** `bash scripts/verify-mail-dns.sh` (DNS-over-HTTPS against a public resolver, so independent of Cloudflare's own API and of the browser session) reports MX `10 mx / 20 mx2 / 50 mx3.zoho.com`, SPF `v=spf1 include:zohomail.com -all`, DMARC `v=DMARC1; p=none; rua=mailto:dmarc@noviq.pk`, DKIM `v=DKIM1; k=rsa; p=MIIBIjAN…`. Cross-checked directly at the authoritative NS `ara.ns.cloudflare.com`. ⚠ The script's **first run FAILED on DKIM against a stale hardcoded `zmail` selector** — the DNS was correct and the *checker* was wrong; default now `zoho`. This supersedes the 2026-08-21 “4 of 4 missing” finding, whose diagnosis (never entered, not propagation) was correct.
- [ ] T027 [US2] [BOTH] Verify by behaviour: send from `@noviq.pk` to an external Gmail **and** an external Outlook account; both must land in the **inbox** with passing sender authentication, 0 classified as spam (FR-006, SC-004)
- [ ] T028 [P] [US2] [BOTH] Verify inbound: an external sender's message arrives in the `@noviq.pk` mailbox (FR-005)
- [ ] T029 [US2] [ARCHITECT] After ~2 weeks of DMARC aggregate reports, review whether any legitimate mail is failing, then decide on tightening to `p=quarantine` — do not tighten before reading reports

**Checkpoint**: US2 complete. Domain sends and receives credible official mail.

---

## Phase 5: User Story 3 — Nothing points at a dead address (Priority: P3)

**Goal**: No script, file, or document in the system advertises an address that no
longer resolves.

**Independent test**: Read the recorded current-URL file and grep the repository for the
old hostnames; every result is either the permanent address or explicitly historical.

- [x] T030 [US3] [AGENT] Stop and disable `noviq-url-watch.service` — its grep only matches `trycloudflare.com` URLs, so after cutover it can never match and would hold a dead address indefinitely without erroring (FR-011, research.md §R5) **DONE 2026-08-21** — was `active`/`enabled`, now `inactive`/`disabled`.
- [x] T031 [US3] [AGENT] Write `https://noviq.pk` into `/opt/noviq/current-url.txt` **DONE 2026-08-21** — replaced the dead `https://treasurer-hometown-lung-strengths.trycloudflare.com`.
- [x] T032 [P] [US3] [AGENT] Update the 3 stale quick-tunnel hostnames in `specs/014-server-thin-client/WINDOWS_CLAUDE_FOLLOWUPS.md` (lines ~25, ~35, ~278) to the permanent address, or mark them historical. **DONE 2026-08-20** — the "Live URL" line now points at `/opt/noviq/current-url.txt` and names F028 as the eventual permanent address; the two `URL=…` bash snippets became `URL=$(cat /opt/noviq/current-url.txt)`, which cannot go stale on the next rotation
- [x] T033 [P] [US3] [AGENT] Update the stale quick-tunnel hostname in `specs/014-server-thin-client/VM_HANDOFF_VERIFY_DEPLOY.md` (line ~15). **DONE 2026-08-20** — marked historical rather than rewritten; the sentence is a dated observation about what that session's server served, so replacing the URL would falsify the record
- [x] T034 [US3] [AGENT] Grep the repository for `trycloudflare` and confirm every remaining occurrence is a deliberate historical reference in `CHANGELOG_DEV.md` or a PHR, not live guidance (SC-006). **DONE 2026-08-20** — 7 occurrences remain, all legitimate: 2 PHRs and `CHANGELOG_DEV.md` (historical), `tasks.md`/`research.md` (describing the watcher's grep pattern and the findings), `quickstart.md` (the address that *is* live until cutover), and `specs/014-server-thin-client/connectivity-check.md:189` — **deliberately left alone**, being the recorded HTTP 200 from the original F014 tunnel-feasibility test. ⭐ A dead URL inside a dated measurement is evidence, not rot; rewriting it would destroy the record

**Checkpoint**: US3 complete. No stale address survives anywhere.

---

## Phase 6: Polish & Cross-Cutting

- [ ] T035 [AGENT] Append a dated entry to `CHANGELOG_DEV.md`: the empty-delegation finding and how it was measured, the token-first tunnel decision, the URL-watcher retirement, and the measured streaming result from T018 ⏳ **ENTRY WRITTEN 2026-08-21** — `CHANGELOG_DEV.md` now carries the empty-delegation finding, the token-first tunnel decision, the URL-watcher retirement, the Zero Trust billing prerequisite (T008a), and the three PowerShell pipe corruptions. **Deliberately left unchecked**: the task also requires the measured streaming result from T018, which needs an authenticated session and is not yet done. Amend the entry once T018 is measured, then tick this.
- [x] T036 [P] [AGENT] Update the memory note `project_noviq-pk-domain-and-email.md` — the recorded status "blocked on the nameserver change" implied the change had been made; correct it to the measured empty-delegation finding and its resolution. **DONE 2026-08-20** — corrected in place, plus a new session note `project_f028-noviq-pk-cutover-2026-08-20.md` indexed at the top of `MEMORY.md` carrying the gate command, the PKNIC reusable-set trap, and the whois-vs-published-zone distinction
- [ ] T037 [P] [ARCHITECT] Add `noviq.pk` as the website on the NIC and SEIC applications, which currently list none
- [ ] T038 [ARCHITECT] Consider `/sp.adr noviq-pk-public-entry-point-and-mail-boundary` to record the data-boundary decision flagged in `plan.md` — permanent public exposure through a third-party edge, and correspondence held by a third-party mail provider
- [x] T039 [P] [AGENT] Document the rollback procedure from `quickstart.md` in the ops notes so restoring the quick tunnel does not require reconstructing it from memory. **DONE 2026-08-20** — written to **`~/ROLLBACK-cloudflared.md` on the VM**, deliberately not only in the repo: an operator already SSH'd into a box with the app down cannot read a repo file. Lists the four restore commands, the files involved, and the reminder to verify by loading the URL rather than by reading logs

---

## Dependencies

```text
Phase 1 (Setup, T001–T005) ─── COMPLETE
        │
        ▼
Phase 2 (Foundational)  T006 → T007 → T008 ★ GATE ★
        │
        ├──────────────────────┬─────────────────────┐
        ▼                      ▼                     │
Phase 3 US1 (P1)         Phase 4 US2 (P2)            │
T009→T010→T011→T012      T019→T020,T021,T022,        │
→T013→T014→T015,T016     T023,T024→T025→T026         │
→T017,T018               →T027,T028→T029            │
        │                      │                     │
        └──────────┬───────────┘                     │
                   ▼                                 │
        Phase 5 US3 (P3)  ◄──────────────────────────┘
        T030→T031, T032,T033→T034
        (T030/T031 need US1 cut over; T032–T034 need neither)
                   │
                   ▼
        Phase 6 Polish  T035–T039
```

**Story independence**: US1 and US2 share only the T008 gate and can run in parallel
once it passes — different systems entirely (tunnel versus mail). US3's repository
cleanup (T032–T034) needs nothing at all and could be done today; only T030–T031 wait
on US1.

---

## Parallel execution examples

**After the T008 gate — two tracks at once:**

```text
Track A (US1, app):   T009 → T010 → T011 → T012 → T013 → T014 → T015
Track B (US2, mail):  T019 → T020 ∥ T021 ∥ T022 → T023 → T024 → T025
```

**Within US2** — once T019 hands over the console values, T020/T021/T022/T024 are
separate DNS records with no ordering between them. T023 (SPF) is deliberately *not*
marked `[P]`: it is the one record where a concurrent second entry silently breaks all
outbound mail.

**Available immediately, no gate:** T032, T033, T034 — repository text only.

---

## Implementation strategy

**MVP = User Story 1 alone.** A permanent address that testers can be handed is the
entire justification for buying the domain; mail is credibility and cleanup is hygiene.
If only one phase ships, ship Phase 3.

**Critical path is three tasks long and none of them are code**: T006 → T007 → T008.
Everything else in this file is fast once the gate opens — the VM side is staged and
takes minutes. The realistic schedule is one architect console session, a wait of up to
48 hours, then a single working session to finish (SC-007).

**Do not start VM changes before T008 passes.** Swapping the unit against a domain with
no delegation takes the product off its only working address and replaces it with one
that cannot resolve.

**Total**: 39 tasks — 5 complete (Phase 1), 34 remaining.
Owners: 15 `[ARCHITECT]`, 20 `[AGENT]`, 4 `[BOTH]`.
