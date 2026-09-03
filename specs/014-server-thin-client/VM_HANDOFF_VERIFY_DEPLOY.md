# VM Handoff — Verify Deploy Branch & Reconcile Scripts

> Written 2026-06-27 from the **Ubuntu/local** machine for the **Windows + VPN +
> GPU-VM** session. Read `CHANGELOG_DEV.md` first. Purpose: answer the open
> questions below **on the VM**, then update the repo so committed deploy scripts
> match reality. Bring the answers back here and we proceed.

## Context (already established on local — no need to re-verify)

- Local `014-server-thin-client` == `origin/014-server-thin-client` == `0820b020`.
  **Fully in sync. Nothing to pull.** Upstream tracking is now set.
- Working tree clean. The Windows work is already on `origin/014`.
- `origin/release` is **24 commits behind** `014` (older "GPU-VM handoff prep"
  commit, before login/upload/per-user-folders).
- Live server *(that session's quick-tunnel URL — long dead; read the current value from
  `/opt/noviq/current-url.txt`)* served title **"Noviq"** + login/upload → those commits
  exist **only on `014`**.
- The poll-deploy timer runs `git reset --hard origin/$BRANCH` every ~2 min, so
  the running code is always that branch's tip. Since `014`-only features are live
  and would be wiped within 2 min if it tracked `release`, the VM **must track
  `014`**. This handoff CONFIRMS that and fixes the stale committed scripts.

## THE CONFLICT to resolve

- **Committed scripts** (`deploy/deploy.sh`, `deploy/check-release.sh`,
  `Makefile`) all say **`BRANCH=release`**.
- **CHANGELOG (2026-06-26)** says the live timer fetches **`014-server-thin-client`**.
- These disagree. The risk: if the systemd timer executes the **in-tree**
  `deploy/deploy.sh`, then every `git reset --hard` reverts that file back to
  `BRANCH=release` → the deploy loop silently sabotages itself. If the VM runs an
  **out-of-tree** copy at `/opt/noviq/deploy.sh`, it's decoupled and safe.

---

## OPEN QUESTIONS — run these ON THE VM, paste output back

### Q1. Where does the systemd timer's ExecStart point? (in-tree vs /opt copy)
```bash
systemctl cat noviq-deploy.service | grep -E 'ExecStart|WorkingDirectory'
systemctl cat noviq-deploy.timer   | grep -E 'OnUnitActiveSec|OnBootSec'
```
**Answer needed:** the exact `ExecStart=` path.
- If it is `/opt/noviq/deploy.sh` (or any path **outside** the git checkout) →
  **DECOUPLED** (safe; stale in-tree script is cosmetic only).
- If it is `.../<repo>/deploy/deploy.sh` (**inside** the git checkout) →
  **COUPLED** (reconciling the committed script to `014` is REQUIRED).

### Q2. What BRANCH does the *operational* deploy.sh actually use?
```bash
cat /opt/noviq/deploy.sh 2>/dev/null | grep -E 'BRANCH=' || echo "no /opt copy"
# also, if ExecStart pointed in-tree, show that file's BRANCH line:
grep -E 'BRANCH=' /path/from/Q1/deploy.sh
```
**Answer needed:** the literal `BRANCH=` value(s) and which file they came from.
Expected: the operational one resolves to `014-server-thin-client`.

### Q3. Where is the git checkout the timer resets, and is it on `014`?
```bash
# use WorkingDirectory from Q1, or the repo dir referenced inside deploy.sh:
REPO=/opt/noviq/echoAI   # adjust if different
git -C "$REPO" rev-parse --abbrev-ref HEAD
git -C "$REPO" rev-parse HEAD
git -C "$REPO" log -1 --format='%h %s'
git -C "$REPO" status -sb | head -1
```
**Answer needed:** current branch, HEAD commit (should match `origin/014` tip
once we push), and whether the tree is clean.

### Q4. Is the deploy loop healthy right now?
```bash
systemctl status noviq-deploy.timer --no-pager | head -5
journalctl -u noviq-deploy --no-pager | tail -30
```
**Answer needed:** is the timer active, when did it last run, any errors?

---

## DECISION TREE — what to change based on answers

**Case A — DECOUPLED** (Q1 ExecStart is out-of-tree, e.g. `/opt/noviq/deploy.sh`):
- The in-tree scripts being stale is documentation-only.
- **Action (low risk):** edit in-tree `deploy/deploy.sh`, `deploy/check-release.sh`,
  and `Makefile` so `BRANCH` defaults to `014-server-thin-client` to match reality,
  add a CHANGELOG entry, commit + `git push origin 014-server-thin-client`.
- Do NOT touch `/opt/noviq/deploy.sh` — it's already correct.

**Case B — COUPLED** (Q1 ExecStart is the in-tree `deploy/deploy.sh`):
- REQUIRED fix: change in-tree `BRANCH` default `release` → `014-server-thin-client`
  in `deploy/deploy.sh` + `deploy/check-release.sh` (and `Makefile deploy` target).
- Commit + push. Then on the next poll the reset is a no-op on this file (it now
  matches) instead of reverting it. Verify with Q4 after the next tick.
- Safer alternative: copy the script out to `/opt/noviq/deploy.sh`, repoint
  `ExecStart` there (converts to Case A), then the in-tree edit is cosmetic.

**Either case — also confirm:**
- `git -C <repo> rev-parse HEAD` (Q3) == `origin/014-server-thin-client` tip.
- After any push, watch one timer tick (~2 min) + reload the live URL to confirm
  the change landed (`journalctl -u noviq-deploy -f`).

---

## What to report back to the local session
1. Q1 ExecStart path → **Case A or Case B**.
2. Q2 operational `BRANCH=` value.
3. Q3 repo branch + HEAD commit + clean?
4. Q4 timer healthy? last run / errors.
5. Whether you applied the Case A/B edit on the VM, or want local to do it.

Once I have these, we update the committed scripts + CHANGELOG from local, push,
and the local→push→server loop is fully clean and documented.

---

## ✅ VERIFIED ON VM — 2026-06-27 (Windows + VPN + GPU-VM session)

Connected: `<user>@<PRIVATE_IP>` (host `yshaikh-vm`), VPN up (~5ms RTT).
**Result: Case A — DECOUPLED, and the operational script is already correct.**
No VM changes required. Discovered units: `noviq-deploy.{service,timer}`,
`noviq-server.service`, `noviq-url-watch.service`.

### Q1 — ExecStart → **Case A (DECOUPLED)**
```
ExecStart=/opt/noviq/deploy.sh        # OUT of the git checkout — safe
OnBootSec=2min / OnUnitActiveSec=2min  # ~2-min poll confirmed
```
The operational script lives outside `REPO`, so `git reset --hard` cannot revert
it. The stale in-tree `BRANCH=release` scripts are **cosmetic / documentation
only** — they are never executed by the timer.

### Q2 — operational BRANCH → **already correct**
```
/opt/noviq/deploy.sh:  BRANCH=014-server-thin-client   # REPO=/opt/noviq/echoAI
```
deploy.sh logic confirmed: fetch → compare → `reset --hard origin/$BRANCH` →
rebuild only changed (`src/...`→frontend `npm run build`; `src-tauri/...`→backend
`cargo build --release --no-default-features --features llm,server,cuda --bin
noviq-server` with `CMAKE_CUDA_ARCHITECTURES=86`) → restart `noviq-server`.

### Q3 — repo state → **on 014, clean, at current tip**
```
branch: 014-server-thin-client
HEAD:   b18735ad8ca34e757b572f4a25afe002aad5276c   == origin/014 tip
log:    b18735a docs(014): VM handoff — verify deploy branch + reconcile stale deploy scripts
status: ## 014-server-thin-client...origin/014-server-thin-client   (clean)
```

### Q4 — timer → **healthy**
- `noviq-deploy.timer` active (waiting), firing every 2 min, no errors.
- Journal shows the loop pulled `0820b02 -> b18735a` at 09:39 PKT, then steady
  "up to date (b18735a)" on each subsequent tick (frontend=0 backend=0 — docs-only
  commit, no rebuild needed). Loop is functioning exactly as designed.

### Verification result: Case A (DECOUPLED), VM was deploying `014` directly.

---

## 🟢 FINAL RESOLUTION — 2026-06-27 — restored the `release` model

After verifying the loop was safe, the architect chose to **restore the `release`
deploy model** rather than keep tracking `014` directly. Rationale: with `release`
as a stable deploy ref, you develop on feature branches (`014`, `015`, …) and
`make deploy` promotes the current branch → `origin/release`; the **VM never needs
reconfiguring** when a new feature branch starts. Direct-tracking would have forced
a manual `/opt/noviq/deploy.sh` edit on every new branch.

**Done (Windows/VM session):**
1. **Promoted `release`:** `git push origin 014-server-thin-client:release` —
   fast-forward `b57b6436 → 477364a0` (+26 commits: login/upload/per-user-folders).
   `release` now has every live feature.
2. **VM repointed:** `/opt/noviq/deploy.sh` `BRANCH=014-server-thin-client →
   release` (backup `deploy.sh.bak.<ts>` kept); VM repo `git checkout -B release
   origin/release`. Both refs already at `477364a0` → next poll a clean no-op
   (`up to date (477364a)`, no rebuild/restart, site unaffected). Loop healthy.
3. **In-tree scripts:** back on `release` (now truthful) but **env-parameterised**
   (`BRANCH=` / `DEPLOY_BRANCH=` overridable) with comments documenting the
   out-of-tree live copy + `make deploy` promotion flow. Commit pushed to `014`
   then promoted to `release`.

**New deploy workflow (for the Ubuntu/local session too):**
- Work + push freely on your feature branch.
- `make deploy` when tester-ready → pushes `HEAD:release`; VM rebuilds changed
  parts within ~2 min. Do NOT hand-edit `/opt/noviq/deploy.sh` anymore.

**State to pull on local:** `origin/release == origin/014-server-thin-client ==`
(this commit). `git fetch` then `git log --oneline origin/release -3` to confirm.
