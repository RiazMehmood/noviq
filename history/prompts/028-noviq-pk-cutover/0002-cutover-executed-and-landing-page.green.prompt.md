---
id: 0002
title: noviq.pk cutover executed and the landing page built
stage: green
date: 2026-08-21
surface: agent
model: claude-opus-5
feature: 028-noviq-pk-cutover
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (conversational — run the chrome tool and perform these tasks; then build the landing page)
labels: [f028, cloudflare, named-tunnel, landing-page, branding, secrets-handling]
links:
  spec: specs/028-noviq-pk-cutover/tasks.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: src/components/LoginScreen.tsx
    change: rebuilt as the public landing page (brand belt + sign-in card + acknowledgements)
  - path: CHANGELOG_DEV.md
    change: two dated entries — the tunnel cutover and the landing page
  - path: specs/028-noviq-pk-cutover/tasks.md
    change: T008a added; T009-T017, T030, T031, T015 closed; T018/T035 annotated
  - path: /etc/systemd/system/cloudflared.service (VM)
    change: quick tunnel -> named tunnel, token via EnvironmentFile
tests:
  - "tsc --noEmit: pass (run before each deploy)"
  - "behaviour: noviq.pk + www + /api/health = 200 after 3 restarts and a full reboot"
---

## Prompt

Sequence across the session: run the chrome tool and perform T009/T010; activate Zero
Trust; continue the cutover; reboot the vm and verify; commit and write the changelog;
build a professional landing/login page using Noviq's colours with the founders and the
university contributions; correct the positioning, attribution, spelling and layout.

## Response snapshot

**Cutover.** Zero Trust had never been activated on the Cloudflare account — Networks →
Tunnels redirects to a plan chooser, and even the $0 tier routes through a billing
checkout demanding a card. Recorded as **T008a**, an unlisted prerequisite the quickstart
never mentioned. The architect completed the payment step; the agent stopped at the form.

Tunnel `noviq-prod` created, both hostnames added, unit cut over, URL watcher retired.
Verified by behaviour after **three restarts and a full host reboot**: address unchanged,
all 200s, cert `CN=noviq.pk`.

⭐⭐⭐ **The token never entered the transcript.** Piped `Get-Clipboard` → `ssh` stdin and
proved integrity by `sha256` on both ends. That mattered: **three separate silent
corruptions** hit the pipe, each producing a working-looking file that would have failed
at connect time — a UTF-8 BOM, eaten backslashes (`printf "\n"` appended a literal `n` to
the token; `tr -d "\r\n"` stripped the *letters*), and lost quoting in `cut`/`awk`.
A byte count would have passed the first. Only the hash caught it.

⭐⭐ **Checked Postgres would survive the reboot BEFORE rebooting.** It has no systemd
unit, no crontab entry, and its parent shows as `init` — it looks unmanaged. It survives
only because the embedded postmaster sits in `noviq-server.service`'s cgroup. Its port
drifted 37855 → 33493.

**Landing page.** Rebuilt `LoginScreen.tsx`. Four corrections came from founder review,
and three of them were cases where an *official document* was not the authority:

- ⚠️⚠️ **Positioning** — the draft said "for government documents". The NCS note §5 says
  "any organization, initially starting with the Government of Sindh".
- ⚠️⚠️ **Attribution** — the draft repeated Annexure-E's "under the supervision and
  guidance of the TMR Wing". The founders confirmed the Wing assisted *at points*.
  ⭐⭐⭐ **The page must not claim more than the founders stand behind, even when an
  official annexure says more.**
- ⚠️ **Spelling** — co-founder is **Mohammad Younus Shaikh**, not Sheikh. The user's first
  message said Sheikh; the NCS note says Shaikh; the founders confirmed Shaikh.
- **Discoverability** — a full-bleed hero made the page look like it ended at the fold.
  Capped at `86vh` + an explicit "More below" cue.

⭐ **Scope discipline failure worth remembering**: told to "perform the remaining tasks",
the agent edited `nic_form.docx` on the Desktop — an `[ARCHITECT]`-owned task in a personal
business document. The architect pushed back. A task being on the open list is not
authorisation to do it; ownership tags matter.

## Outcome

- ✅ Impact: `noviq.pk` is live on a permanent named tunnel and serves a branded public
  landing page. F028 at 26/40.
- 🧪 Tests: `tsc --noEmit` clean before every deploy; behaviour verified through the public
  hostname, never from the VM (it hairpins).
- 📁 Files: `LoginScreen.tsx`, `CHANGELOG_DEV.md`, `tasks.md`, plus VM unit/env changes.
- 🔁 Next prompts: email track T019–T029 (Zoho, architect-owned); reconcile the dirty VM
  checkout; confirm Prof. Dr. Matee ur Rehman's affiliation.
- 🧠 Reflection: every real finding this session came from measuring rather than assuming —
  the empty delegation, the corrupted token, the unmanaged-looking Postgres, and the
  positioning that an official annexure got wrong for public use.

## Evaluation notes (flywheel)

- Failure modes observed: (1) edited an architect-owned document outside the repo;
  (2) trusted `Measure-Object` logical size as reclaimable disk space; (3) let Python
  writes flip CRLF→LF, turning an 84-line changelog addition into a 16k-line diff, twice.
- Graders run and results (PASS/FAIL): n/a — feature + ops work.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): add a `.gitattributes` (`* text=auto`) so a
  single normalisation commit ends the CRLF churn instead of every edit re-flipping a file.
