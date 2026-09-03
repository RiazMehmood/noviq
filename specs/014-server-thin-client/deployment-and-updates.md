# Deployment & Update Path — Server / Thin-Client (Feature 014)

**Status:** DESIGN. Companion to `spec.md`. Covers how code authored locally reaches the
running GPU server, and how the server rebuilds + restarts without manual SSH each time.

**Decisions (2026-06-25):**
- **Build host = the GPU server.** `llama-cpp-2` with CUDA (cuBLAS) must be compiled where the
  CUDA toolkit + matching NVIDIA driver live. The laptop produces *source*, never binaries.
- **Trigger = auto on push to `release`, via server-side POLL-TIMER** (primary — the VM is VPN-gated
  at a private address `~10.150.x.100`, confirmed 2026-06-25, so GitHub can't reach it inbound). The
  signed `/__deploy` webhook below is an OPTIONAL future upgrade if a public HTTPS endpoint appears.
- **Downtime = brief restart OK.** One binary serves both the HTTP API and the static frontend,
  so backend and frontend always move together — no version skew.

---

## 1. Runtime topology (post-014)

```
                         GPU server (Ubuntu, NVIDIA + CUDA)
  laptop (dev) ── git push ──► GitHub ──webhook──►  ┌────────────────────────────────┐
                                                    │ caddy/nginx  (TLS, :443)        │
  browser (tester) ───────── HTTPS ───────────────► │   └─► noviq-server (axum,:8080) │
                                                    │         • /api/* JSON           │
                                                    │         • /  static dist/       │
                                                    │         • /__deploy webhook     │
                                                    │       PostgreSQL (loopback)     │
                                                    │       models/ ir_cache/ page_…  │
                                                    └────────────────────────────────┘
```

- **One Rust binary `noviq-server`** (headless axum service): serves `/api/*` (former Tauri
  commands), serves the built SolidJS app from `dist/` as static files, and exposes one guarded
  `/__deploy` webhook endpoint.
- **systemd** supervises it (`noviq-server.service`, auto-restart on crash, starts on boot).
- **TLS** terminates at a reverse proxy (Caddy = automatic certs, simplest) in front of `:8080`.
- DB, models, and caches are server-local (per spec FR-006/FR-007).

---

## 2. The update flow (PRIMARY: poll-timer on `release`)

The VM is VPN-gated at a private address, so GitHub cannot reach it inbound — the **poll-timer is
the primary mechanism**, no open port required:

```
1. Edit on laptop  ──►  commit  ──►  push to feature branch (no deploy)
2. Promote:  make deploy   (== git push origin <current-branch>:release)
3. noviq-deploy.timer fires every ~2 min:
        git fetch origin; compare origin/release vs deployed commit
4. On change, it starts the DECOUPLED oneshot unit:
        systemctl start --no-block noviq-deploy.service
5. noviq-deploy.service runs deploy.sh OUTSIDE the server's process tree:
        fetch → checkout release → npm build (frontend) → cargo build --release (CUDA)
        → health-check the new binary → swap → systemctl restart noviq-server
        → post-restart health probe → on failure, roll back to previous binary+commit
```

**Why a `release` branch trigger (not every push):** a CUDA `cargo build --release` is minutes
long. Triggering on every feature-branch push would rebuild the GPU box constantly. Only
`origin/release` is polled, so deploys are an explicit promotion you control via `make deploy`.

**Why a decoupled deploy unit:** `noviq-server` is about to be `systemctl restart`ed. If the
deploy ran as a child of `noviq-server`, the restart would SIGKILL the deploy mid-build. The timer
only *triggers* `noviq-deploy.service`; the build/restart lives in that separate unit's process
tree and survives the server bounce.

**Optional future upgrade — push webhook:** if IBA later exposes a public HTTPS endpoint, add a
signed `POST /__deploy` (HMAC-SHA256, verify `ref==refs/heads/release`) that calls the same
`systemctl start --no-block noviq-deploy.service` for instant (non-polled) deploys. Same
`deploy.sh`, same decoupling. Not needed for the VPN-only pilot.

---

## 3. Artifacts

### 3.1 `deploy/deploy.sh` (runs as `noviq-deploy.service`)
```bash
#!/usr/bin/env bash
set -euo pipefail
REPO=/opt/noviq/echoAI
BRANCH=release
SVC=noviq-server
BIN="$REPO/src-tauri/target/release/noviq-server"
LOG=/var/log/noviq/deploy.log
exec >>"$LOG" 2>&1
echo "=== deploy $(date -Is) ==="

cd "$REPO"
PREV_COMMIT=$(git rev-parse HEAD)
[ -f "$BIN" ] && cp -f "$BIN" "$BIN.prev" || true

git fetch --prune origin
git checkout "$BRANCH"
git reset --hard "origin/$BRANCH"

# frontend → dist/ (served as static by the binary)
npm ci
npm run build

# backend (CUDA). DB migrations run on server startup via TARGET_SCHEMA_VERSION (additive).
( cd src-tauri && cargo build --release --features "server,cuda" )

# atomic-ish swap is implicit: cargo wrote the new binary; restart picks it up
sudo systemctl restart "$SVC"

# post-restart health probe; roll back on failure
for i in $(seq 1 30); do
  if curl -fsS http://127.0.0.1:8080/api/health >/dev/null; then
    echo "deploy OK -> $(git rev-parse --short HEAD)"; exit 0
  fi; sleep 2
done

echo "HEALTH CHECK FAILED — rolling back to $PREV_COMMIT"
git reset --hard "$PREV_COMMIT"
[ -f "$BIN.prev" ] && cp -f "$BIN.prev" "$BIN"
sudo systemctl restart "$SVC"
exit 1
```

### 3.2 `deploy/noviq-server.service`
```ini
[Unit]
Description=Noviq headless server (API + web client)
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
User=noviq
WorkingDirectory=/opt/noviq/echoAI
ExecStart=/opt/noviq/echoAI/src-tauri/target/release/noviq-server
EnvironmentFile=/opt/noviq/echoAI/.env        # DB url, models dir, GPU layers, deploy secret
Restart=on-failure
RestartSec=2

[Install]
WantedBy=multi-user.target
```

### 3.3 `deploy/noviq-deploy.service` (oneshot, decoupled)
```ini
[Unit]
Description=Noviq build & redeploy
[Service]
Type=oneshot
User=noviq
ExecStart=/opt/noviq/echoAI/deploy/deploy.sh
```
`noviq` may `systemctl restart noviq-server` and `systemctl start noviq-deploy.service`
without a password via a tight sudoers drop-in (`/etc/sudoers.d/noviq`):
```
noviq ALL=(root) NOPASSWD: /usr/bin/systemctl restart noviq-server, /usr/bin/systemctl start --no-block noviq-deploy.service
```

### 3.4 Poll-timer (PRIMARY trigger) — `deploy/noviq-deploy.timer` + check
```ini
# noviq-deploy.timer
[Unit]
Description=Poll origin/release and redeploy on change
[Timer]
OnBootSec=2min
OnUnitActiveSec=2min
[Install]
WantedBy=timers.target
```
The timer activates a tiny `noviq-deploy-check.service` (oneshot) running `deploy/check-release.sh`,
which compares `origin/release` against the last **attempted** sha — NOT the working-tree `HEAD`:
```bash
cd /opt/noviq/echoAI
git fetch -q origin release
REMOTE=$(git rev-parse origin/release)
# 1. circuit breaker: never auto-retry a sha whose deploy already rolled back
[ "$(cat $DATA/DEPLOY_FAILED_SHA 2>/dev/null)" = "$REMOTE" ] && exit 0
# 2. compare against the recorded attempt (falls back to HEAD on first run)
LOCAL=$(cat $DATA/LAST_DEPLOYED_SHA 2>/dev/null || git rev-parse HEAD)
[ "$LOCAL" != "$REMOTE" ] && exec sudo systemctl start --no-block noviq-deploy.service
```
⚠️ **Do not revert this to `git rev-parse HEAD` (2026-07-24).** Comparing the working-tree `HEAD`
made the VM rebuild itself every 2 minutes with **nobody pushing anything**: any persistent drift of
`HEAD` from `origin/release` is unequal forever. The usual cause was self-inflicted — deploy.sh's
rollback does `git reset --hard $PREV_COMMIT`, pinning `HEAD` behind release, so a deploy that failed
its health probe re-triggered itself indefinitely, burning a full CUDA build each cycle. Manual git
state on the VM (a checked-out branch, stash, or worktree) has the same effect. Regression cases 3-6
in `deploy/test_deploy_rollback.sh` cover this.

### 3.5 Webhook endpoint (OPTIONAL future upgrade, in `noviq-server`)
- `POST /__deploy` — verify `X-Hub-Signature-256` = HMAC-SHA256(body, `$DEPLOY_SECRET`); reject
  otherwise (401). Parse JSON; proceed only if `ref == "refs/heads/release"`. Then
  `Command::new("sudo").args(["systemctl","start","--no-block","noviq-deploy.service"])` and
  return `202 Accepted`. Never builds in-process.
- The GitHub webhook is configured once: URL `https://noviq.<host>/__deploy`, content-type
  `application/json`, secret = `$DEPLOY_SECRET`, events = *push* only.

### 3.6 `Makefile` (laptop convenience — promote without remembering refspecs)
```makefile
deploy:                 ## fast-forward release branch -> triggers server auto-deploy
	git push origin $$(git rev-parse --abbrev-ref HEAD):release
```
So a normal day is: work on `015-bureaucrat-chat`, push freely; when ready, `make deploy`.

---

## 4. Migrations, models, secrets, rollback

- **DB migrations:** the project's migrations are **additive** and run on server startup via
  `TARGET_SCHEMA_VERSION` — `deploy.sh` does nothing special; the restart applies them. New
  migrations must stay additive so rollback (older binary on a newer schema) is safe.
- **Models (`models/*.gguf`):** multi-GB, **not in git**. They are NOT part of the code deploy —
  transferred once via `rsync` and updated out-of-band. `deploy.sh` never touches them.
- **Secrets:** `$DEPLOY_SECRET`, DB url, GPU layer count live in `/opt/noviq/echoAI/.env`
  (git-ignored), loaded by systemd `EnvironmentFile`. No secrets in the repo.
- **Rollback:** automatic on failed health probe (§3.1). Manual: `git reset --hard <tag>` on the
  server + restore `noviq-server.prev`, restart. Tag each good deploy (`deploy-YYYYMMDD-HHMM`).

---

## 5. Acceptance checks

- [ ] Push to `015-…` does **not** deploy; `make deploy` (push to `release`) **does**.
- [ ] Webhook with a bad/missing HMAC signature is rejected `401`; valid push to `release` returns
      `202` and starts `noviq-deploy.service`.
- [ ] Deploy survives the `noviq-server` restart (deploy runs in its own unit, not killed).
- [ ] After deploy, both the new frontend (`dist/`) and new API are live with no version skew.
- [ ] Failed health probe rolls back to the previous binary + commit and the service comes back up.
- [ ] CUDA build link confirmed (`server logs: GPU offload N layers`).
- [ ] No secrets or `models/*.gguf` committed; `.env` git-ignored.

## 6. Risks / follow-ups (≤3)
- **Inbound reachability RESOLVED (2026-06-25)** — VM is VPN-gated at a private address, so the
  poll-timer (§3.4) is the chosen primary; webhook (§3.5) is a future-only upgrade. No open item.
- **Build duration** — first CUDA build is long; rely on cargo incremental cache between deploys,
  and keep `release` promotions deliberate (not every push).
- **Single-binary restart drops in-flight requests** — acceptable for the pilot; the seam for
  blue-green (two binaries + proxy switch) is noted but out of scope.
</content>
</invoke>
