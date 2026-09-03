# Windows Claude — Follow-ups (SINGLE SOURCE OF TRUTH)

> Written 2026-06-27 from the Ubuntu/local session. This **replaces** the older
> Desktop helper files (deployment_check.sh, route_check.sh,
> vm_enable_deploy_banner.txt, vm_setup_sidecars.txt) — they have been merged here
> and deleted. Work the tasks below **on the GPU VM** (Windows + VPN + SSH), and
> **record results inline** in each task's `RESULT` block so this file stays the
> record of what was done.

---

## 0. Context (read first)

- **Repo (local + VM):** branch `014-server-thin-client`. Deploy model = **`release`**:
  develop on the feature branch, run `make deploy` (pushes `HEAD:release`); the VM's
  poll-timer (`noviq-deploy.timer`, ~2 min) rebuilds only what changed and restarts.
- **VM:** `<user>@<PRIVATE_IP>` (host `yshaikh-vm`), Ubuntu 20.04, A40 GPU. Reach it
  via the Windows EasyConnect VPN (local Linux EasyConnect GUI crashes on 22.04).
  Creds in `~/Desktop/sudo`. Repo on VM: `/opt/noviq/echoAI`. Build under `/opt/noviq`.
- **Server:** systemd `noviq-server.service` (User=appadmin, WorkingDirectory
  `/opt/noviq/echoAI`, `EnvironmentFile=/opt/noviq/echoAI/.env`). Binary:
  `/opt/noviq/echoAI/src-tauri/target/release/noviq-server`. Operational deploy
  script is OUT of tree at `/opt/noviq/deploy.sh` (in-tree `deploy/deploy.sh` is the
  reference copy).
- **Live URL:** read it from `/opt/noviq/current-url.txt` — never hard-code it.
  Until the F028 cutover this is a Cloudflare **quick tunnel** whose URL rotates on
  every restart (the value once written here died long ago). After F028 it becomes the
  permanent `https://noviq.pk`; see `specs/028-noviq-pk-cutover/`.
- **Done this session (already deployed):** chat forced to LLM (migration 007), browser
  indexing progress, per-user usage panel + activity/export, model_status + chat-mode
  HTTP routes, cycling "working…" status, sentinel-based auto-reload. See
  `CHANGELOG_DEV.md` (top) for full detail.

### Quick deploy/route check (used by several tasks below)
```bash
URL=$(cat /opt/noviq/current-url.txt)   # never hard-code; the quick-tunnel URL rotates
curl -s "$URL/api/health"; echo
# 401 = route LIVE (auth required); 404/405 = OLD binary (rebuild not done); 200 = route MISSING
for p in /api/model_status /api/admin/users/x/activity /api/admin/users/x/export; do
  printf "%-34s %s\n" "$p" "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$URL$p")"
done
```

---

## TASK 1 — Provision the ingestion sidecars (pdfplumber + PaddleOCR)

**Goal:** the pipeline is already coded default-on (tables: pdfplumber → `pdftotext
-bbox`; scans: PaddleOCR → tesseract) but silently falls back because the Python
sidecars aren't built on the VM. Build them, wire them in `.env`, restart, re-index.

```bash
# prerequisites (once)
sudo apt-get update
sudo apt-get install -y python3-venv python3-dev build-essential tesseract-ocr poppler-utils

cd /opt/noviq/echoAI/src-tauri/sidecars/ingestion-sidecar

# 1) pdfplumber TABLE sidecar (frozen binary) -> src-tauri/binaries/ingestion-sidecar-<triple>
./build.sh

# 2) PaddleOCR venv -> /home/appadmin/.cache/echo-ocr-venv-build/.ocr-venv
#    (heavy download; for GPU speed later: pip install paddlepaddle-gpu)
./setup-ocr-venv.sh

# 3) point the server at both + force default-on, then restart
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
{
  echo "ECHO_INGESTION_SIDECAR=/opt/noviq/echoAI/src-tauri/binaries/ingestion-sidecar-$TRIPLE"
  echo "ECHO_OCR_PYTHON=/home/appadmin/.cache/echo-ocr-venv-build/.ocr-venv/bin/python"
  echo "ECHO_PADDLE_OCR=1"
  echo "ECHO_TABLE_SIDECAR=1"
} | sudo tee -a /opt/noviq/echoAI/.env
sudo systemctl restart noviq-server

# 4) verify
echo '{"op":"extract_tables","pdf_path":"/opt/noviq/echoAI/<a-digital>.pdf","pages":"all"}' \
  | /opt/noviq/echoAI/src-tauri/binaries/ingestion-sidecar-$TRIPLE | head -c 200; echo
/home/appadmin/.cache/echo-ocr-venv-build/.ocr-venv/bin/python -c "import paddleocr; print('paddleocr', paddleocr.__version__)"

# 5) RE-INDEX the corpus (sidecars only run at index time) and watch which path each file used:
journalctl -u noviq-server -f | grep -E "pdfplumber|PaddleOCR|tesseract|schema_anchored|ocr_skipped"
#   "pdfplumber sidecar reflowed ..." = tables via pdfplumber (good)
#   "schema_anchored"                 = tables fell back to pdftotext -bbox (sidecar not found)
#   "PaddleOCR extracted ..."         = scan via PaddleOCR (good)
#   "tesseract" / "ocr_skipped"       = OCR fell back to tesseract
```
Notes: the frozen binary (git-ignored, in `src-tauri/binaries/`) and the venv
(`~/.cache`) both survive `git reset --hard`, so this is one-time setup.

**RESULT — DONE 2026-06-27 (Windows-Claude):**
- **Prereqs:** had to `sudo apt-get install python3.8-venv python3-dev build-essential`
  first — `python3 -m venv` couldn't bootstrap pip (ensurepip) without it.
- **Table sidecar:** `build.sh` initially failed — it pinned `pdfplumber==0.11.10`,
  which hard-requires `pdfminer.six==20260107` (no installable files on PyPI; latest
  is 20250324; 0.11.6+ are likewise unsatisfiable). **Re-pinned `pdfplumber==0.11.5`**
  (pdfminer.six==20231228) in build.sh — committed. Binary built:
  `src-tauri/binaries/ingestion-sidecar-x86_64-unknown-linux-gnu` (26 MB), smoke-tested
  on `06_Manual_for_Development_Projects_2019_PC-I.pdf` → `{"ok":true, ...tables...}`.
- **OCR venv:** built clean at `~/.cache/echo-ocr-venv-build/.ocr-venv`; verified
  `paddleocr 3.7.0` imports (Python 3.8 — paddlepaddle CPU; install paddlepaddle-gpu
  later for A40 speed).
- **`.env` wired** (`/opt/noviq/echoAI/.env`, backed up): ECHO_INGESTION_SIDECAR,
  ECHO_OCR_PYTHON, ECHO_PADDLE_OCR=1, ECHO_TABLE_SIDECAR=1. Server restarted, health ok.
- **NOTE:** the binary + venv work standalone; per-file re-index log proof was NOT
  finalized (architect asked to stop the deep-reindex test mid-run). Corpus re-index
  to actually benefit is still pending a normal (simple-tier) re-index by the operator.

---

## TASK 2 — Enable the "Noviq is updating…" banner + post-deploy auto-reload

**Goal:** the banner + 5s health poller + sentinel-driven auto-reload are all coded,
but the operational `/opt/noviq/deploy.sh` doesn't write the `DEPLOYING` sentinel that
`/api/health` reads. Sync it from the in-tree reference (which has the sentinel logic).

```bash
# review what would change, then replace with a backup
diff /opt/noviq/deploy.sh /opt/noviq/echoAI/deploy/deploy.sh
sudo cp -f /opt/noviq/deploy.sh /opt/noviq/deploy.sh.bak.$(date +%s)
sudo cp -f /opt/noviq/echoAI/deploy/deploy.sh /opt/noviq/deploy.sh
sudo chmod +x /opt/noviq/deploy.sh

# confirm data dir matches between deploy script + service, and branch is release
grep -E '^NOVIQ_DATA_DIR' /opt/noviq/echoAI/.env || echo "(unset -> /opt/noviq/echoAI/.noviq-data)"
systemctl show noviq-server -p Environment --no-pager
grep -E '^BRANCH=' /opt/noviq/deploy.sh
```
**Test:** trigger a deploy (`make deploy` from the dev box, or push to `origin/release`),
then on the VM `watch -n2 'curl -s http://127.0.0.1:8080/api/health'` — expect
`"status":"deploying"` during the build, `"status":"ok"` after restart; the browser
shows the amber bar then auto-refreshes.

**RESULT — DONE 2026-06-27 (Windows-Claude) — did NOT copy wholesale:**
- **Diff:** the in-tree `deploy/deploy.sh` writes the sentinel BUT rebuilds FE+BE and
  `systemctl restart`s **unconditionally every poll** — it has no change-detection /
  early-exit. The operational `/opt/noviq/deploy.sh` DOES (fetch → `OLD==NEW` exit →
  FE/BE grep). Copying the reference over it (as this task literally said) would
  restart the live server every ~2 min. **So I surgically MERGED the sentinel into the
  operational script instead** (write after `OLD!=NEW`; `trap cleanup EXIT`; clear
  before each restart). Backup at `/opt/noviq/deploy.sh.bak.<ts>`; `bash -n` clean.
- **NOVIQ_DATA_DIR matched:** yes — sentinel resolves from `.env` →
  `/opt/noviq/data/DEPLOYING`, which is what `/api/health` reads.
- **Health flip observed:** YES — during the Task-3 route-fix deploy, `/api/health`
  went `ok → {"status":"deploying"} → ok` (sentinel written by the merged script).
- **Browser banner/auto-refresh:** logic confirmed live (status:"deploying" served);
  visual confirmation in a browser is for the operator to eyeball on the next deploy.

---

## TASK 3 — Verify the latest backend actually deployed (admin activity panel)

**Goal:** the admin **Activity** panel was erroring (HTML instead of JSON) because the
backend hadn't caught up to the commit with `/api/admin/users/:id/activity`. Confirm
it's live now.

```bash
# run the quick route check from §0. Expect:
#   /api/model_status            -> 401
#   /api/admin/users/x/activity  -> 401
#   /api/admin/users/x/export    -> 401
```
- If any are **200/405**, the backend is still behind → check the deploy loop:
  `journalctl -u noviq-deploy -n 80 --no-pager` (look for a failed `cargo build` that
  rolled back). Paste any build error back to the dev session to fix forward.
- Once all **401**, log into the web UI as admin → User Management → click **Activity**
  on a user who has run queries; confirm rows show + JSON/CSV download works.

**RESULT — DONE 2026-06-27 (Windows-Claude) — found + fixed a REAL bug:**
- The "405 = old binary" heuristic in §0 is **wrong**: 405 here was the static SPA
  fallback rejecting POST, not a registered route. Real root cause: `api/router.rs`
  registered the four `/admin/users/{id}/{disable,enable,activity,export}` routes with
  **axum-0.8 brace syntax `{id}`** while the crate is on **axum 0.7.9**, where `{id}`
  is a *literal* segment → none matched a real id → all fell through to the SPA
  fallback. This broke the **entire** admin per-user surface (enable/disable too), not
  just activity/export.
- **Fix:** `{id}` → `:id` (axum-0.7 syntax). Committed `f4d02e12`, pushed to `release`,
  deployed (observed ok→deploying→ok). **Verified live with an admin token:**
  `GET …/activity` → 200 application/json (real rows); `GET …/export?format=csv` →
  200 text/csv; unauthenticated → 401. Controls (`/api/me`, `/api/admin/users`) → 401.
- Deploy loop healthy; incremental CUDA rebuild ~OK, server back to `status:"ok"`.
- TODO when axum is upgraded to 0.8 (Task 4 era): revert `:id` → `{id}`.

---

## TASK 4 — Rename EchoAI → Noviq (internal identifiers)

**Goal:** product is **Noviq**; user-facing strings were already rebranded (commit
fa0d0084). Remaining = internal identifiers. This is a **large, staged, build-breaking**
rename (do each phase fully, then `cargo build`/`tsc`/tests green before the next).
Best done on the VM where a full CUDA rebuild + tests can verify. **Do NOT do it
piecemeal across a deploy** — land it in one branch, test, then `make deploy`.

Occurrence counts (local, excludes build dirs): `echo_ai_lib` 133 · `echoai` 56 ·
`echo-ai` 47 · `echoAI` 56 · `com.echoai` 6 · `echo.db` 6 · `ECHO_` 303.

**Recommended phasing (low → high risk):**
1. **Cosmetic/build metadata (low):** `package.json` name `echo-ai`→`noviq`;
   `tauri.conf.json` `productName` `EchoAI`→`Noviq`. (Identifier handled in phase 3.)
2. **Crate lib rename (medium, mechanical, atomic):** `src-tauri/Cargo.toml`
   `[lib] name = "echo_ai_lib"` → `noviq_lib`; then update every `use echo_ai_lib::`
   in `tests/`, `bin/`, examples (133 refs). One commit; `cargo build --features
   llm,server` + `cargo test --lib` must pass. (Bin `echo-ai` / `noviq-server` names:
   optionally rename `echo-ai`→`noviq-desktop`; **`noviq-server` stays** — the deploy
   script + systemd unit reference it.)
3. **Identity + data paths (HIGH risk — touches where data lives):**
   - `tauri.conf.json` identifier `com.echoai.app` → `com.noviq.app` changes the
     desktop app-data dir → **orphans existing desktop DB/caches**. Only do with a
     data-migration plan, or accept a fresh local store. (Server uses `NOVIQ_DATA_DIR`,
     not the identifier, so the live VM data is unaffected.)
   - `echo.db` (6 refs): the SQLite path string. Server is on **PostgreSQL** now, so
     `echo.db` is desktop/legacy only — verify it's not the live server path before
     touching. Keep or rename consistently; don't change the live PG.
4. **`ECHO_` → `NOVIQ_` env prefix (HIGH risk, 303 refs):** these are tuning knobs
   (`ECHO_RERANK`, `ECHO_PADDLE_OCR`, `ECHO_TABLE_SIDECAR`, `ECHO_INGESTION_SIDECAR`,
   `ECHO_OCR_PYTHON`, `ECHO_N_CTX`, …) referenced in code **and** the live VM `.env`
   + operational scripts. **Recommended:** read BOTH prefixes during a transition
   (`std::env::var("NOVIQ_X").or_else(|_| var("ECHO_X"))`) so nothing breaks, migrate
   the `.env`, then drop the old prefix later. Update Task 1's `.env` keys to the new
   names at the same time.
5. **Repo dir `echoAI` / `echo-ai`:** rename the directory + the VM clone path
   `/opt/noviq/echoAI` last (touches systemd unit `WorkingDirectory`/`ExecStart`,
   `/opt/noviq/deploy.sh` `REPO=`, and `CARGO_MANIFEST_DIR`-derived sidecar paths).
   Update all three together.

**Acceptance per phase:** `cargo build --no-default-features --features llm,server` →
0; `tsc --noEmit` → 0; `cargo test --lib` green; for phases 3–5 also confirm the live
server still starts + serves `/api/health` after deploy.

**RESULT — DEFERRED 2026-06-27 (architect): "deferred task 4 for now, we will work
on that [later]."** No rename work done. Pick up here when scheduled.

---

## TASK 5 — Admin-indexed docs are global + Settings cleanup (architect, mid-session) — DONE

Architect requests during the 2026-06-27 Windows session, all shipped in commit
`cff27e4f` (pushed to `release`):

1. **Admin uploads must be visible to ALL users.** They were scoped to the admin's
   personal `/uploads/admin` folder. Cause: `add_indexed_folder`/`upload_documents`
   set `owner_id = Some(uploader)`, and `folders::list_for_owner` returned only
   `owner_id = $1`. Fix: admin → `owner_id = NULL` (global); `list_for_owner` now
   `WHERE owner_id = $1 OR owner_id IS NULL`. Regular users' uploads stay private;
   they can VIEW but not delete shared docs (`can_manage_folder` unchanged). Chat
   retrieval was never owner-scoped, so answers already used these docs.
2. **Removed** the admin "server index folder path" input + (disabled) "Index server
   path" button from `SettingsPage.tsx` (+ dead `webFolderPath`/`handleAddFolderWeb`).
3. **Deep reindex** stays a secondary/opt-in path — normal indexing uses the simple
   tier first; do NOT force `deep_index_folder_source`.

**Live-DB data fixes already applied (persist across deploys):**
- `UPDATE indexed_folders SET owner_id=NULL WHERE owner_id='<admin uuid>'` → the 11
  existing admin docs are now global (`UPDATE 1`).
- Deleted the `testcorpus` folder (f033f3ba, 2 files) via the `revoke_indexed_folder`
  admin API (proper cascade). Only `/opt/noviq/data/uploads/admin` remains.
- PG access for reference: `psql -h 127.0.0.1 -p 35417 -U postgres -d echo`
  (password in `/opt/noviq/data/pg/.pgpass`; APP_DB_NAME = `echo`).

Verified pre-deploy on the VM: `cargo check --features llm,server` + `tsc --noEmit`
= 0 errors. **Operator to eyeball:** log in as a regular user (younus/ayaz) after the
`cff27e4f` deploy lands and confirm the admin folder now appears in their Settings.

---

## TASK 6 — Close out the last 5 Feature-014 tasks ON THE VM (architect, 2026-06-27)

These are the **only remaining open tasks** in `tasks.md`: T026, T028, T051, T052
(T036 is marked **WON'T-DO** — see end). None need new code; they are
verify-and-record steps that can only be done on the live GPU VM. Work them in this
order and paste the outputs into the `RESULT` blocks. The `deploy.sh` already builds
`--features server,cuda`, so the running server should **already be the CUDA build** —
T026 is mostly "confirm + record the toolkit version," not a from-scratch rebuild.

### 6a — T026: confirm GPU build + record CUDA toolkit version
```bash
nvidia-smi | head -20            # driver 470 / A40 present
nvcc --version                   # <-- record this exact CUDA toolkit version
URL=$(cat /opt/noviq/current-url.txt)   # source of truth; the URL rotates on restart
curl -s "$URL/api/health"; echo  # expect {"status":"ok","gpu":true,"layers":N,...}
sudo journalctl -u noviq-server | grep -iE "offload|gpu layers|n_gpu" | tail -5
```
PASS when `gpu:true` + `layers>0` and the log shows the offload count (T027).
If `gpu:false`/`layers:0`: check `NOVIQ_GPU_LAYERS` in `/opt/noviq/echoAI/.env`,
set it (see 6b), `sudo systemctl restart noviq-server`, recheck.
**Record:** the `nvcc --version` string → goes into `quickstart.md` §3 (T051).

### 6b — T028: CPU-vs-GPU latency (SC-005 evidence)
Pick ONE representative query and time it twice. Get a token first (log in via the
browser devtools → copy the session cookie/bearer, or use the test login flow).
```bash
Q='<your standard eval question>'
# GPU (current): time it
time curl -s -X POST "$URL/api/chat" -H "Authorization: Bearer $TOKEN" \
  -d "{\"question\":\"$Q\"}" >/dev/null

# CPU baseline: edit /opt/noviq/echoAI/.env -> NOVIQ_GPU_LAYERS=0
sudo systemctl restart noviq-server
time curl -s -X POST "$URL/api/chat" -H "Authorization: Bearer $TOKEN" \
  -d "{\"question\":\"$Q\"}" >/dev/null

# RESTORE: set NOVIQ_GPU_LAYERS back to the working N, restart
sudo systemctl restart noviq-server
```
**Record:** fill the `| run | NOVIQ_GPU_LAYERS | wall time |` table in
`quickstart.md` §"GPU layer sizing (T027) + latency capture (T028, SC-005)".

### 6c — T052: acceptance gate SC-001..010
Walk the checklist in `quickstart.md` (the "Acceptance smoke" section) and tick each:
- **SC-001** browser on VPN: log in → ask a question → get answer + citation, zero local install.
- **SC-002** open 3 concurrent browser sessions (diff users/incognito) → answers isolated.
- **SC-003** `curl -s -o /dev/null -w '%{http_code}' "$URL/api/model_status"` with NO token → **401**.
- **SC-004** from a *client* host (not the VM): `psql -h <vm-ip> -p 35417 ...` → **refused** (loopback-only).
- **SC-005** the GPU<CPU latency win from 6b.
- **SC-006** index a small doc via the UI → ask about it → it's retrievable.
- **SC-007** from the dev box `make deploy` (pushes `HEAD:release`) → goes live within ~1 poll cycle; a push to a *feature* branch does NOT deploy.
- **SC-008** already GREEN locally (`deploy/test_deploy_rollback.sh`, 7/7) — note as passed.
- **SC-009/010** admin creates a tester → tester logs in; disable the tester → login rejected; non-admin hits an `/api/admin/*` route → **403**. (Or run the ignored tests on the VM: `cargo test --features server --test api_admin -- --ignored --nocapture`.)

### 6d — T051: finalize the runbook
As you do 6a–6c, edit `quickstart.md`: real `/opt` paths, the `nvcc` version, the
Caddy/Cloudflare-tunnel front, the latency table, and tick the SC-001..010 boxes.
Then in `tasks.md` check off **T026, T028, T051, T052**.

**RESULT — TASK 6 (Windows-Claude, 2026-06-27, in progress):**
- [x] **6a T026 — PASS.** `nvcc --version` = **CUDA 11.4, V11.4.152** (Built Oct 11 2021,
  `cuda_11.4.r11.4`). `nvidia-smi` = driver **470.223.02**, **A40-12Q** (12107 MiB).
  `/api/health` (127.0.0.1:8080) = `{"status":"ok","gpu":true,"layers":999,"version":"0.1.0"}`.
  Offload log (T027): `load_tensors: offloaded 33/33 layers to GPU`, CUDA0 model buffer
  4097.52 MiB, all 32 KV layers on `CUDA0`, "CUDA graph warmup complete". Active model =
  `models/Mistral-7B-Instruct-v0.3-Q4_K_M.gguf`.
- [x] **6b T028 — DONE. GPU ~4.3s vs CPU ~231s → ~53× speedup (SC-005 ✅).**
  Same prompt shape, idle box, fresh (uncached) conversation each run:
  - **GPU** (`NOVIQ_GPU_LAYERS=999`): wall **4.30s**, `query_log.latency_ms=4300`, gpu_util 95%, gpu_mem 7075 MB.
  - **CPU** (`NOVIQ_GPU_LAYERS=0`): wall **230.75s** (~3.8 min), http 200. (PG port rotated on
    restart so couldn't re-read latency_ms, but wall time is end-to-end authoritative.)
  - Restored `NOVIQ_GPU_LAYERS=999` + restarted; health back to `gpu:true,layers:999`; `.env` verified intact.
  - Corroborating real-traffic GPU latencies from `query_log` (queries that didn't queue):
    4550 ms, 5311 ms, 5316 ms, 8098 ms, 11138 ms → typical RAG turn ~**4.5–11s on GPU**.
  - **Load-investigation result (architect asked):** the earlier 12-min 95% saturation was
    **self-inflicted, NOT a runaway/storm** — every slow row in `query_log` was the *admin*
    user (me) and was my own duplicated "Summarize the role of the Planning Commission" test
    query firing ~3× from SSH-timed-out batches; serialized by `gen_gate` they each waited
    ~7 min (445,566 / 445,682 / 446,347 ms) behind one another. No external users, no storm.
    Takeaway: don't fire repeat timing queries without waiting — they pile up in the gen gate.
- [~] **6c T052 — partial (non-disruptive checks done):**
  - **SC-003 ✅** no-token → 401 on `/api/model_status`, `/api/me`, `/api/admin/users`.
  - **SC-004 ✅** PG listens **127.0.0.1:36977 only** (loopback; ephemeral port, pw in
    `/opt/noviq/data/pg/.pgpass`). Not bound to any public iface.
  - **SC-007 ✅ (mechanism)** `noviq-deploy.timer` active, `/opt/noviq/deploy.sh` `BRANCH=release`;
    full end-to-end still wants a real `make deploy` to observe go-live.
  - **SC-005 ✅** GPU 4.30s vs CPU 230.75s (~53×) — see 6b.
  - **SC-008 ✅** already green locally (`deploy/test_deploy_rollback.sh` 7/7) — noted passed.
  - Server bind confirmed `0.0.0.0:8080`.
  - **PENDING (need operator browser / 2nd non-admin user):** SC-001 (browser full flow),
    SC-002 (3 concurrent isolated), SC-006 (index small doc → retrievable), SC-009/010
    (admin creates tester → login; disable → rejected; non-admin `/api/admin/*` → 403).
    Or run the ignored server tests on the VM: `cd src-tauri && POSTGRESQL_VERSION="=17.10.0"
    cargo test --features server --test api_admin -- --ignored --nocapture`.
- [x] **6d T051 — quickstart.md filled** (nvcc 11.4, latency table CPU 230.75 / GPU 4.30,
  SC-001..008 box statuses). **tasks.md: T026 + T028 checked off.** T051/T052 left OPEN until
  the remaining SCs above are eyeballed by the operator (no code work left, just verification).

> **T036 — WON'T-DO (architect decision 2026-06-27).** The optional `POST /__deploy`
> HMAC webhook is unnecessary: the deploy trigger is the `noviq-deploy.timer`
> poll (no inbound port). Closed out as intentionally skipped; revisit only if a
> public deploy endpoint is ever exposed.

---

## TASK 7 — Qwen3-8B + no-think is now the production chat model (architect, 2026-06-27) — DONE

**Request:** Qwen3-8B lookups were ~14 s (vs Mistral ~4–6 s) because Qwen3 runs
thinking mode **on by default** — it generates a `<think>…</think>` chain every
turn (the `llm.rs` filter discards it but the tokens are still spent). Force
Qwen3's no-think mode, then re-test and **switch the product to Qwen3** (no
restore to Mistral).

**Shipped (commit `f0f8a6fe`, deployed via `release`):** new
`query_router::no_think_assistant_prefix()` pre-closes an empty
`<think>\n\n</think>\n\n` after the assistant tag (== Qwen3 `enable_thinking=false`)
when env `NOVIQ_NO_THINK` ∈ {1,true,on,yes}; wired into `build_rag_prompt`
(main/retry/both map-reduce passes) + `build_judge_prompt`. Default OFF.

**On the VM:** `use-model.sh Qwen3-8B-Q4_K_M.gguf` (37/37 layers on GPU);
`NOVIQ_NO_THINK=1` added to `/opt/noviq/echoAI/.env` (permanent). Mistral parked
in `/opt/noviq/model-library/`.

**Measured (A40-12Q, fresh/uncached, `ECHO_QUERY_CACHE=0` during timing then reverted):**
Qwen3-8B no-think lookups **3.8 / 4.3 / 5.1 / 5.6 s (avg ~4.7 s)** vs ~14 s
thinking-on → **~3× faster**, on par with Mistral, answers clean + `[n]`-cited.

**Knob audit (requested):** reranker OFF (`ECHO_RERANK=1` to enable), reviewer/judge
OFF (`ECHO_REVIEWER=1`), NER Pass 2 OFF (index-time only), **query cache ON in
release** (`ECHO_QUERY_CACHE=0` to force off — important: use uncached queries for
any latency capture).

---

## How to hand results back
Keep this file updated with the `RESULT` blocks, and append any new follow-ups at the
bottom. When back on the Ubuntu box, paste the filled-in results so `CHANGELOG_DEV.md`
+ MEMORY can be updated.
