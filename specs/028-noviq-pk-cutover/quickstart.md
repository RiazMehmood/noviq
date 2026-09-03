# Quickstart: noviq.pk Cutover Runbook

**Feature**: 028-noviq-pk-cutover | **Date**: 2026-08-20

Ordered operator runbook. Every step carries an **owner** — `ARCHITECT` steps are
browser-console work the agent physically cannot perform; `AGENT` steps are VM shell
work. The wrong owner on a step is what produces a stalled session.

Current live address until cutover: `https://treasurer-hometown-lung-strengths.trycloudflare.com`

---

## Step 1 — Create the Cloudflare zone · `ARCHITECT`

Add `noviq.pk` as a site on the **Free** plan at dash.cloudflare.com. Cloudflare assigns
two nameservers of the form `<name>.ns.cloudflare.com`. Copy them exactly — they are
specific to the account.

Do this *before* touching PKNIC even though it feels backwards: the nameservers needed in
step 2 only exist once the zone does.

## Step 2 — Set the delegation at PKNIC · `ARCHITECT`

Log in to the PKNIC panel and enter the two Cloudflare nameservers into the currently
empty NS fields for `noviq.pk`.

**This is the single change that unblocks the entire feature.** Nothing can break: the
domain resolves to nothing today, so there is no live service to disturb. This is the
cheapest moment this change will ever be — it will not be true once the domain is in use.

> ⏳ **Gate: registry publication, up to 24–48 h.**

## Step 3 — GATE: confirm the delegation is live · `AGENT`

No later step may start until this passes.

```bash
# Authoritative — ask the .pk registry directly
nslookup -type=NS noviq.pk root-c1.pknic.pk

# Then the public resolvers
nslookup -type=NS noviq.pk 8.8.8.8
nslookup -type=NS noviq.pk 1.1.1.1
```

- **PASS** — returns the two `*.ns.cloudflare.com` names.
- **STILL WAITING** — registry says NXDOMAIN and step 2 was confirmed done: wait, recheck.
- **FAIL** — registry returns *different* nameservers, or NXDOMAIN persists beyond 48 h:
  the entry did not take. Return to step 2 rather than waiting longer.

Sanity-check the method itself if anything looks odd — `nslookup -type=NS jazz.pk
root-c1.pknic.pk` must return nameservers. If it does not, the query is broken, not the
domain.

## Step 4 — Register the domain with Zoho Mail · `ARCHITECT`

Add `noviq.pk` in the Zoho admin console. It issues three things that **cannot be written
in advance**, because all are generated per account:

- a domain-verification record (TXT, or a `zb*` CNAME),
- a DKIM selector and public key,
- the MX hostnames **for your data centre**.

⚠️ Zoho runs regional data centres (`.com` / `.eu` / `.in` / `.com.au` / `.sa`) and the MX
hostnames differ per region. Their public help pages do not flag this. Take every value
from your own console, not from documentation — including the table below.

## Step 5 — Enter the DNS records in Cloudflare · `ARCHITECT`

Shape below; authoritative values come from step 4. **Mail records must be DNS only
(grey cloud)** — proxying them breaks delivery.

| Type | Name | Value | Prio | Proxy |
|---|---|---|---|---|
| CNAME | `@` | `<uuid>.cfargotunnel.com` — created for you in step 6 | — | Proxied |
| CNAME | `www` | `<uuid>.cfargotunnel.com` | — | Proxied |
| MX | `@` | `mx.zoho.com` | 10 | DNS only |
| MX | `@` | `mx2.zoho.com` | 20 | DNS only |
| MX | `@` | `mx3.zoho.com` | 50 | DNS only |
| TXT | `@` | `v=spf1 include:zohomail.com -all` | — | DNS only |
| TXT | `@` | `zoho-verification=…` *(from console)* | — | DNS only |
| TXT | `<selector>._domainkey` | `v=DKIM1; k=rsa; p=…` *(from console)* | — | DNS only |
| TXT | `_dmarc` | `v=DMARC1; p=none; rua=mailto:dmarc@noviq.pk` | — | DNS only |

## Step 6 — Create the named tunnel · `ARCHITECT`

Zero Trust → Networks → Tunnels → **Create a tunnel** → Cloudflared → name it
`noviq-prod`. On the install screen, copy **only the token** — the long string after
`--token`. Ignore the rest of the command; the VM already has `cloudflared` installed.

Then add public hostnames on that tunnel:

- `noviq.pk` → HTTP → `localhost:8080`
- `www.noviq.pk` → HTTP → `localhost:8080`

Send the token **privately**. It authorises a machine to serve traffic on the zone. It
must not go into the repository, a chat log, or the plaintext ops file on the Desktop.

## Step 7 — Swap the VM off the quick tunnel · `AGENT`

The replacement unit is already staged at `~/cloudflared-named.service.staged` on the VM
and is inert until installed.

```bash
# 7a. Keep a rollback copy of the working unit FIRST
sudo cp /etc/systemd/system/cloudflared.service \
        /etc/systemd/system/cloudflared-quick.service.bak

# 7b. Token into a root-only env file (never into ExecStart)
sudo install -d -m 755 /etc/noviq
printf 'TUNNEL_TOKEN=%s\n' "$TOKEN" | sudo tee /etc/noviq/cloudflared.env >/dev/null
sudo chmod 600 /etc/noviq/cloudflared.env
sudo chown root:root /etc/noviq/cloudflared.env

# 7c. Install the named unit and restart
sudo cp ~/cloudflared-named.service.staged /etc/systemd/system/cloudflared.service
sudo systemctl daemon-reload
sudo systemctl restart cloudflared

# 7d. Retire the URL watcher — it can never match a trycloudflare URL again
sudo systemctl disable --now noviq-url-watch.service
echo "https://noviq.pk" | sudo tee /opt/noviq/current-url.txt
```

Expect chat to drop for a few seconds during 7c. No rebuild is triggered — nothing under
`src-tauri/**` changes.

## Step 8 — Verify by behaviour · `AGENT` + `ARCHITECT`

Per FR-012, none of these are satisfied by clean logs.

| Check | Passes when |
|---|---|
| App reachable | `https://noviq.pk` loads the sign-in page, valid TLS, no warning |
| App functional | Signed in, a corpus query returns a real answer **with citations** |
| `www` | `https://www.noviq.pk` reaches the same application |
| Stability | Address unchanged after `systemctl restart cloudflared` ×3 and one reboot |
| Mail out | Message from `@noviq.pk` lands in an external **Gmail inbox** and an **Outlook inbox**, not spam |
| Mail in | External sender's message arrives in the `@noviq.pk` mailbox |
| No stale address | `/opt/noviq/current-url.txt` reads `https://noviq.pk`; no repo file cites a dead hostname |
| Streaming (measure only) | Record whether tokens stream on the new path — improved, unchanged, or worse. Do **not** claim a fix. |

## Rollback

If the named tunnel fails to connect or the app is unreachable:

```bash
sudo cp /etc/systemd/system/cloudflared-quick.service.bak \
        /etc/systemd/system/cloudflared.service
sudo systemctl daemon-reload
sudo systemctl restart cloudflared
sudo systemctl enable --now noviq-url-watch.service
cat /opt/noviq/current-url.txt   # watcher repopulates the new rotating URL
```

Back on the quick tunnel within a minute. The DNS and mail records are harmless if left
in place — the tunnel CNAME simply points at a tunnel that is not running.

---

## Watch-outs

- **A domain may publish only one SPF record.** If the app ever sends mail itself, merge
  its sender into the existing line. A second TXT record does not add to the first — it
  invalidates both, and `-all` is a hard fail, so all mail from the domain starts
  bouncing.
- **Start DMARC at `p=none` with `rua=`.** Read the reports for a couple of weeks,
  confirm legitimate mail passes, then tighten. Tightening early silently drops real
  correspondence — the worst failure mode for a government-facing product.
- **Self-hosted mail stays ruled out**, on measurement not preference: no inbound public
  address and no PTR control over the egress block, so mail would be spam-filtered
  regardless of correct SPF/DKIM/DMARC.
- **CORS is not in the way.** `router.rs:278-281` mirrors the request origin and the
  frontend is same-origin, so the new hostname needs no code change.
- **The VPN path is unaffected.** Direct access to `<PRIVATE_IP>` for deploys and admin
  continues to work exactly as before; this feature only changes public ingress.
