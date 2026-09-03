# Phase 0 Research: noviq.pk Domain Cutover

**Feature**: 028-noviq-pk-cutover | **Date**: 2026-08-20

All findings below are measured unless explicitly marked as a hypothesis. Provenance:
`[M]` measured this session · `[D]` from prior recorded work.

---

## R1 — Why does `noviq.pk` not resolve?

**Decision**: The domain has **no delegation at all**. This is not a propagation delay
and no amount of waiting will change it. The first action of the feature is a registry
change by the architect.

**Evidence** `[M]`:

```
$ whois noviq.pk          (whois.pknic.net.pk, port 43)
    Domain: noviq.pk
    Status: Domain is Registered
    Creation Date: 2026-08-14
    Expiry Date:   2028-08-14
    Name Server:                 <- all four fields empty
    Name Server:
    Name Server:
    Name Server:

$ nslookup -type=NS noviq.pk root-c1.pknic.pk
    *** can't find noviq.pk: Non-existent domain
```

Method validated against known-live `.pk` domains on the same registry server, so
NXDOMAIN is a real answer and not a broken query:

| Domain | Result from `root-c1.pknic.pk` |
|---|---|
| `jazz.pk` | `ns.liquidweb.com`, `ns1.liquidweb.com` |
| `daraz.pk` | `ns1.alibabadns.cn`, `ns2.alibabadns.com`, … |
| **`noviq.pk`** | **NXDOMAIN** |

**Rationale**: Registration and delegation are separate things at PKNIC. The domain was
bought on 2026-08-14 and is paid through 2028, but the nameserver fields were never
filled, so the `.pk` zone contains no referral for it. Public resolvers (8.8.8.8,
1.1.1.1) correctly report NXDOMAIN because the authoritative parent says so.

**Consequence for the plan**: This corrects the previously recorded status "blocked on
the nameserver change — `.pk` propagation can take 24–48 h" `[D]`, which implied the
change had been made. It had not. The plan therefore makes the delegation check an
explicit gate with a named command, so the difference between *not yet published* and
*never entered* is always visible.

**Alternatives considered**: Waiting longer (rejected — nothing to wait for); using a
different TLD already under our control (rejected — `noviq.pk` is bought, paid for a
further two years, and the `.pk` suffix is meaningful for a Pakistan government product).

---

## R2 — How is the named tunnel created without a browser?

**Decision**: **Token-first.** The architect creates the tunnel in the Cloudflare Zero
Trust dashboard and hands over the connector token; the VM runs
`cloudflared tunnel --no-autoupdate run --token ${TUNNEL_TOKEN}`.

**Rationale**: The conventional path, `cloudflared tunnel login`, opens a browser to
pick a zone and writes a `cert.pem`. The agent has shell access to the VM and no
browser session, so that path cannot be driven from here — this is a hard capability
boundary, not a preference. The token path needs no `cert.pem`, no local credentials
JSON, and no `config.yml`: routing (`noviq.pk` → `http://localhost:8080`) is configured
in the dashboard alongside the tunnel, and the DNS record is created there too.

**Current state** `[M]`: `cloudflared` 2026.6.1 is at `/opt/noviq/cloudflared`. Neither
`~/.cloudflared/` nor `/etc/cloudflared/` exists — a clean slate with no leftover
credentials to reconcile. The live unit is:

```
ExecStart=/opt/noviq/cloudflared tunnel --url http://localhost:8080 --no-autoupdate --protocol http2
```

**Token handling**: the token authorises a machine to serve traffic on the zone, so it
is a credential. It goes in `/etc/noviq/cloudflared.env` as `root:root 0600`, read via
systemd `EnvironmentFile`. It is **not** inlined into `ExecStart` — `systemctl cat` and
`ps aux` expose that to any local user — and it does not go in the repository or in the
shared plaintext ops file on the Windows desktop, which already holds a sudo password,
a GitHub PAT and an ngrok token `[D]`.

**Protocol**: the staged unit omits `--protocol http2`, which the quick tunnel forced
while trying (unsuccessfully) to fix SSE buffering. `cloudflared` defaults to QUIC and
falls back to HTTP/2 automatically if UDP egress is blocked, so omitting the flag is
safe and leaves the better transport available.

**Alternatives considered**: Cloudflare API token plus `cloudflared tunnel create`
(rejected — needs a separate API credential with broader scope than the connector token,
and still needs dashboard work for the public hostname); keeping the quick tunnel and
publishing a redirect (rejected — the redirect target still rotates, so it solves
nothing).

---

## R3 — Apex or subdomain for the application?

**Decision**: **Apex — `noviq.pk`**, with `www.noviq.pk` reaching the same application.
Decided by the architect during specification.

**Rationale**: A tester told "open noviq.pk" needs nothing else — no link, no path, no
subdomain to mistype. It is also the form that reads correctly on the NIC and SEIC
applications, which currently list no website at all `[D]`.

**The apex/MX concern is not real here**: a tunnel's DNS target is
`<uuid>.cfargotunnel.com`, which resolves only as a proxied Cloudflare record. Cloudflare
CNAME-flattens at the apex, publishing A records rather than a literal apex CNAME, so
the delegation stays valid and MX records on the same name are unaffected. Mail and app
coexist on `noviq.pk`.

**Alternatives considered**: `app.noviq.pk`, keeping the apex free for a future
marketing site (rejected by the architect — one more thing to explain to testers, and no
landing page is planned or in scope).

---

## R4 — Will the named tunnel fix web token streaming?

**Decision**: **Unknown. Treated as a measurement at cutover, not a promised outcome.**
Recorded as FR-013 so the result is written down either way.

**Background** `[D]`: a reproducible finding from 2026-07-05 — through the quick tunnel,
`/api/index/events` and `/api/chat/events` deliver response headers and **zero body
bytes, ever**, while streaming correctly on localhost. Forcing `--protocol http2` did
not help. Two consequences persist today: indexing status never live-updates in the
browser (worked around by a polling fallback in `SettingsPage.tsx` and `ChatPage.tsx`),
and **web chat has never streamed tokens** — answers appear only when the POST completes.

**Why it might improve**: a named tunnel on a real zone is a different ingress path from
an anonymous quick tunnel, and `text/event-stream` normally passes the Cloudflare edge
unbuffered.

**Why it might not**: if the buffering is Cloudflare *edge* behaviour rather than
quick-tunnel behaviour, it follows us to the named tunnel unchanged. The original
investigation could not separate the two.

**Consequence for the plan**: no task claims to fix streaming, and the polling fallback
stays in place regardless. One verification task measures the behaviour on the new path
and records the result. Removing the fallback, if it turns out to be unnecessary, is
separate work and is listed Out of Scope in the spec.

---

## R5 — What else breaks or needs retiring at cutover?

**Decision**: Two things need handling; a third was checked and is a non-issue.

**Needs handling — the URL watcher goes stale silently** `[M]`.
`noviq-url-watch.service` is active and runs `/opt/noviq/url-watch.sh`, which greps the
`cloudflared` journal for `https://[a-z0-9-]+\.trycloudflare\.com` and writes matches to
`/opt/noviq/current-url.txt` (currently
`https://treasurer-hometown-lung-strengths.trycloudflare.com`). After cutover that
pattern can never match again, so the watcher neither updates nor errors — the file just
keeps holding a dead address indefinitely. Stale is worse than empty, because a stale
value gets trusted. Task: stop and disable the unit, write the permanent hostname into
the file.

**Needs handling — stale hostnames in the repository** `[M]`.
`specs/014-server-thin-client/WINDOWS_CLAUDE_FOLLOWUPS.md` (3 occurrences) and
`specs/014-server-thin-client/VM_HANDOFF_VERIFY_DEPLOY.md` (1 occurrence) hard-code
`ceramic-conceptual-inventory-regular.trycloudflare.com`, which is already dead — it
predates the current URL. FR-011 covers correcting these.

**Checked, not an issue — CORS** `[M]`. `src-tauri/src/api/router.rs:278-281` builds its
`CorsLayer` with `AllowOrigin::mirror_request()`, so any hostname is accepted, and the
frontend is served same-origin anyway. A new hostname needs **no code change**. This was
worth confirming rather than assuming, since an origin allowlist would have turned a
config change into a rebuild-and-redeploy.

**Related risk, out of scope**: any change under `src-tauri/**` — even a sidecar `.py` —
triggers a full CUDA rebuild and server restart on deploy, ~6 minutes with chat down
`[D]`. Because this feature changes no application code, that cost is avoided entirely.
