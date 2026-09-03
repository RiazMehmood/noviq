#!/usr/bin/env bash
# F014 / T035 (SC-008) — integration test for deploy.sh rollback.
#
# Proves, without a GPU or real systemd, that:
#   (b) a deploy whose post-restart health probe FAILS rolls back to the
#       previous commit AND restores the previous binary (SC-008);
#   - the success path goes live on the `release` commit (the `release`-only
#       trigger itself is check-release.sh's job; this shows deploy operates on
#       BRANCH=release).
#
# It runs the REAL deploy.sh against a throwaway git repo with the heavy/dangerous
# tools stubbed on PATH (npm, cargo, sudo, systemctl, curl, sleep). Run anywhere:
#   bash deploy/test_deploy_rollback.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
DEPLOY="$HERE/deploy.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

pass=0; fail=0
ok()  { echo "  PASS: $1"; pass=$((pass+1)); }
bad() { echo "  FAIL: $1"; fail=$((fail+1)); }

# ── Stub external tools on PATH ───────────────────────────────────────────────
STUB="$TMP/bin"; mkdir -p "$STUB"
cat >"$STUB/npm" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
# `cargo build` simulates producing a NEW binary (v2) at $BIN.
cat >"$STUB/cargo" <<'EOF'
#!/usr/bin/env bash
[ "${1:-}" = "build" ] && echo v2 > "$BIN"
exit 0
EOF
# Records its args when $SUDO_LOG is set, so the check-release cases can assert
# whether a deploy WOULD have been triggered.
cat >"$STUB/sudo" <<'EOF'
#!/usr/bin/env bash
[ -n "${SUDO_LOG:-}" ] && echo "$*" >> "$SUDO_LOG"
exit 0
EOF
cat >"$STUB/systemctl" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
cat >"$STUB/sleep" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$STUB"/*

# ── Throwaway origin + work repo with master + release branches ───────────────
ORIGIN="$TMP/origin.git"; WORK="$TMP/work"
git init -q --bare "$ORIGIN"
git init -q "$WORK"
( cd "$WORK"
  git config user.email t@t && git config user.name t
  mkdir -p src-tauri && touch src-tauri/.keep                       # deploy.sh: cd src-tauri
  echo base > file.txt && git add -A && git commit -qm base       # master
  git branch -M master
  git checkout -q -b release
  echo rel > file.txt && git add -A && git commit -qm release      # release ahead
  git push -q "$ORIGIN" master release )

run_deploy() { # $1 = curl-result (0 ok / 1 fail)
  local repo="$TMP/repo.$2"
  git clone -q "$ORIGIN" "$repo"
  ( cd "$repo" && git checkout -q master )
  echo "NOVIQ_DATA_DIR=$repo/.noviq-data" > "$repo/.env"
  mkdir -p "$repo/bin"; echo v1 > "$repo/bin/noviq-server"          # previous binary
  cat >"$STUB/curl" <<EOF
#!/usr/bin/env bash
exit $1
EOF
  chmod +x "$STUB/curl"
  PATH="$STUB:$PATH" \
  REPO="$repo" BRANCH=release SVC=test BIN="$repo/bin/noviq-server" \
  LOG="$TMP/deploy.$2.log" HEALTH_URL="http://127.0.0.1:9/api/health" \
    bash "$DEPLOY"; echo $? > "$TMP/rc.$2"
  echo "$repo"
}

# ── Case 1: health FAILS → rollback ───────────────────────────────────────────
echo "Case 1: failing health probe -> rollback (SC-008)"
PREV_REL="$(cd "$WORK" && git rev-parse master)"
set +e; REPO_FAIL="$(run_deploy 1 fail)"; set -e
rc="$(cat "$TMP/rc.fail")"
[ "$rc" = "1" ] && ok "deploy exits non-zero on failed probe" || bad "expected exit 1, got $rc"
head="$(cd "$REPO_FAIL" && git rev-parse HEAD)"
[ "$head" = "$PREV_REL" ] && ok "HEAD rolled back to previous commit" || bad "HEAD=$head not rolled back to $PREV_REL"
[ "$(cat "$REPO_FAIL/bin/noviq-server")" = "v1" ] && ok "previous binary restored (v1)" || bad "binary not restored"
grep -q "rolling back" "$TMP/deploy.fail.log" && ok "log records rollback" || bad "no rollback log line"

# ── Case 2: health OK → goes live on release ──────────────────────────────────
echo "Case 2: passing health probe -> live on release"
REL_COMMIT="$(cd "$WORK" && git rev-parse release)"
set +e; REPO_OK="$(run_deploy 0 ok)"; set -e
rc="$(cat "$TMP/rc.ok")"
[ "$rc" = "0" ] && ok "deploy exits 0 on healthy probe" || bad "expected exit 0, got $rc"
head="$(cd "$REPO_OK" && git rev-parse HEAD)"
[ "$head" = "$REL_COMMIT" ] && ok "live on the release commit" || bad "HEAD=$head != release $REL_COMMIT"
[ "$(cat "$REPO_OK/bin/noviq-server")" = "v2" ] && ok "new binary kept (v2)" || bad "new binary not kept"

# ── check-release.sh: the auto-rebuild-with-no-push regression (2026-07-24) ────
# The old trigger compared the working-tree HEAD to origin/release. After Case 1 the
# rolled-back repo has HEAD *behind* release, so it re-triggered every 2 min forever,
# rebuilding the same broken commit. These cases pin the new behaviour.
CHECK="$HERE/check-release.sh"

run_check() { # $1 = repo, $2 = tag  → echoes the sudo-log path
  local repo="$1" tag="$2"
  : > "$TMP/sudo.$tag"
  PATH="$STUB:$PATH" SUDO_LOG="$TMP/sudo.$tag" \
  REPO="$repo" BRANCH=release \
    bash "$CHECK" > "$TMP/check.$tag.log" 2>&1 || true
  echo "$TMP/sudo.$tag"
}

echo "Case 3: after a rolled-back deploy, the poll must NOT redeploy the same sha"
run_check "$REPO_FAIL" breaker >/dev/null
[ ! -s "$TMP/sudo.breaker" ] && ok "no deploy triggered (circuit breaker held)" \
  || bad "deploy re-triggered: $(cat "$TMP/sudo.breaker")"
grep -q "^skip:" "$TMP/check.breaker.log" && ok "breaker logged its reason" \
  || bad "no skip line: $(cat "$TMP/check.breaker.log")"

echo "Case 4: a drifted HEAD alone must NOT trigger a deploy"
# Same repo with the breaker cleared: HEAD is still rolled back (drifted), but the
# recorded attempt equals origin/release, so there is genuinely nothing new to build.
rm -f "$REPO_FAIL/.noviq-data/DEPLOY_FAILED_SHA"
run_check "$REPO_FAIL" drift >/dev/null
[ ! -s "$TMP/sudo.drift" ] && ok "no deploy triggered on HEAD drift alone" \
  || bad "HEAD drift re-armed the trigger: $(cat "$TMP/sudo.drift")"

echo "Case 5: a real new commit on release MUST still trigger a deploy"
( cd "$WORK" && git checkout -q release \
  && echo next > file.txt && git commit -qam "new release commit" \
  && git push -q "$ORIGIN" release )
run_check "$REPO_FAIL" newpush >/dev/null
grep -q "systemctl start --no-block noviq-deploy.service" "$TMP/sudo.newpush" \
  && ok "new commit triggers a deploy" \
  || bad "real push did NOT trigger: $(cat "$TMP/check.newpush.log")"

echo "Case 6: a successful deploy clears a parked breaker"
echo deadbeef > "$REPO_OK/.noviq-data/DEPLOY_FAILED_SHA"
[ -f "$REPO_OK/.noviq-data/LAST_DEPLOYED_SHA" ] && ok "successful deploy recorded LAST_DEPLOYED_SHA" \
  || bad "LAST_DEPLOYED_SHA not written on success"
set +e; run_deploy 0 ok2 >/dev/null; set -e
[ ! -f "$TMP/repo.ok2/.noviq-data/DEPLOY_FAILED_SHA" ] && ok "breaker cleared on a healthy deploy" \
  || bad "breaker survived a successful deploy"

echo "---"
echo "PASS=$pass FAIL=$fail"
[ "$fail" = "0" ]
