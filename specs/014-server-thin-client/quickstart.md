# Quickstart — Stand up the Noviq server + first deploy (IBA GPU VM)

Prereq: connected to Easy Connect VPN (from Windows) and `ssh`'d into the VM (creds in
`~/Desktop/sudo`). VM = Ubuntu 20.04, private IP ~10.150.x.100.

## 0. Verify the box
```bash
nvidia-smi                 # confirm GPU + driver; if absent → CPU fallback build (FR-009)
psql --version             # bundled PG 17 expected
node -v && rustc --version # Node 18+, Rust 1.87
```

## 1. Get the code (once)
```bash
sudo mkdir -p /opt/noviq && sudo chown noviq:noviq /opt/noviq
cd /opt/noviq && git clone https://github.com/RiazMehmood/echoAI.git   # PAT for private repo
cd echoAI && git checkout release        # release branch is what the server runs
```

## 2. Secrets + models (out-of-band, never in git)
```bash
cp .env.example .env       # set DB url, GPU layer count, (optional) DEPLOY_SECRET
rsync -avz models/ noviq@<vm>:/opt/noviq/echoAI/models/   # GGUFs from laptop, multi-GB
```

## 3. First build (server is the CUDA build host)
```bash
npm ci && npm run build                                   # frontend → dist/
( cd src-tauri && cargo build --release --features "server,cuda" )   # CUDA backend
```

## 4. Install services
```bash
sudo cp deploy/noviq-server.service deploy/noviq-deploy.service \
        deploy/noviq-deploy.timer deploy/noviq-deploy-check.service /etc/systemd/system/
sudo cp deploy/sudoers.noviq /etc/sudoers.d/noviq && sudo chmod 440 /etc/sudoers.d/noviq
sudo systemctl daemon-reload
sudo systemctl enable --now noviq-server.service noviq-deploy.timer
curl -fsS http://127.0.0.1:8080/api/health      # {status:ok,gpu:true,...}
```
Front with Caddy for TLS (see research R4), or expose per IBA IT.

## 5. The update loop (day-to-day, from the laptop)
```bash
# work on 015-bureaucrat-chat, commit freely (no deploy)
make deploy            # == git push origin <current-branch>:release
# within ~2 min: noviq-deploy.timer sees origin/release changed →
#   noviq-deploy.service: build frontend + CUDA backend → restart → health probe
#   → on failure, auto-rollback to previous binary + commit
journalctl -u noviq-deploy.service -f      # watch a deploy
```

## GPU layer sizing (T027) + latency capture (T028, SC-005)
`NOVIQ_GPU_LAYERS` (in `.env`) overrides auto-sizing; `0` forces the CPU fallback (FR-009).
The active count is logged on first model load: `journalctl -u noviq-server | grep "GPU layers"`.

Capture the SC-005 before/after with the same query (fill the blanks):
```bash
# CPU baseline:  set NOVIQ_GPU_LAYERS=0 in .env, restart, then:
TOKEN=$(curl -fsS -XPOST localhost:8080/api/login -H 'content-type: application/json' \
        -d '{"username":"admin","password":"…"}' | jq -r .token)
time curl -fsS -XPOST localhost:8080/api/query -H "authorization: Bearer $TOKEN" \
        -H 'content-type: application/json' -d '{"conversation_id":"bench","text":"<rep query>"}' >/dev/null
# GPU:  set NOVIQ_GPU_LAYERS=<N for 12GB>, restart, repeat.
```
| run | NOVIQ_GPU_LAYERS | wall time |
|-----|------------------|-----------|
| CPU | 0                | 230.75 s  |
| GPU | 999 (=33/33)     | 4.30 s    |

Measured 2026-06-27 on the VM (A40-12Q, Mistral-7B-Instruct-v0.3-Q4_K_M), idle box,
fresh/uncached conversation per run → **GPU ~53× faster (SC-005 ✅)**. Typical
real-traffic GPU RAG turns from `query_log.latency_ms`: ~4.5–11 s.
**CUDA toolkit (T026):** `nvcc` release **11.4, V11.4.152** (driver 470.223.02).
Load log (T027): `load_tensors: offloaded 33/33 layers to GPU`.

## Server-side tests (run on the VM, bundled PG)
```bash
cd src-tauri
POSTGRESQL_VERSION="=17.10.0" cargo test --features server \
  --test api_auth --test api_admin -- --ignored --nocapture   # T021 + T048 (SC-003/009/010)
```

## Deploy rollback test (T035, SC-008) — no GPU, run anywhere
```bash
bash deploy/test_deploy_rollback.sh   # failed probe → rollback; healthy → live on release. Already green locally.
```

## Access: ngrok (locked this session — see CHANGELOG 2026-06-26)
Front the authed server with an ngrok free static domain (poll-timer deploy unchanged); brand
`noviq.<domain>` later. Auth must be solid before the tunnel goes public — heads-up to IBA IT.
Deviates from ADR-0017 VPN-only.

## Acceptance smoke (maps to SC-001..008) — status 2026-06-27 (Windows-Claude)
- [ ] Browser (on VPN) logs in, runs query→answer→citation, zero local install (SC-001). *(operator to eyeball)*
- [ ] 3 concurrent sessions isolated (SC-002). *(operator to eyeball)* — unauth request → 401 (SC-003) **✅ verified** (`/api/model_status`,`/api/me`,`/api/admin/users` all 401).
- [x] `psql` from a client host cannot reach PG (SC-004) **✅** — PG listens `127.0.0.1:36977` only (loopback).
- [x] `/api/health` reports `gpu:true` + lower LLM latency than CPU baseline (SC-005) **✅** — GPU 4.30 s vs CPU 230.75 s (table above).
- [x] `make deploy` goes live with no manual build; feature-branch push does not (SC-007) **✅ (mechanism)** — `noviq-deploy.timer` active, `deploy.sh BRANCH=release`. *(full go-live eyeball on next real deploy)*
- [x] A build that fails health check leaves the previous version running (SC-008) **✅** — `deploy/test_deploy_rollback.sh` 7/7 green locally.
- [ ] SC-006 index a small doc → retrievable; SC-009/010 admin disable-tester + non-admin→403. *(need a 2nd non-admin tester account)*
