#!/usr/bin/env bash
# F014 (T032) — poll origin/release; trigger a deploy on change.
#
# Run by noviq-deploy-check.service, which is activated every ~2 min by
# noviq-deploy.timer. This is the PRIMARY deploy trigger: the VM is VPN-gated at
# a private address, so GitHub can't reach it inbound (no webhook). The VM dials
# OUT to GitHub instead (egress-only) — confirmed viable by connectivity-check.md.
#
# 2026-07-24 — compares against the last ATTEMPTED sha, not the working-tree HEAD.
# The old `git rev-parse HEAD` version rebuilt with nobody pushing anything: any
# persistent drift of HEAD from origin/release made the two unequal forever, so a
# full CUDA build fired every 2 minutes. The drift was usually self-inflicted —
# deploy.sh's own rollback does `git reset --hard $PREV_COMMIT`, pinning HEAD behind
# release, so a failed deploy re-triggered itself indefinitely. Manual git state on
# the VM (a checked-out branch, a stash, a worktree) did the same. Two guards now:
#   1. LAST_DEPLOYED_SHA — what we last tried, immune to HEAD moving under us.
#   2. DEPLOY_FAILED_SHA — a circuit breaker; never auto-retry a sha that rolled back.
set -euo pipefail

REPO="${REPO:-/opt/noviq/echoAI}"
cd "$REPO"

BRANCH="${BRANCH:-release}"

# Same resolution as deploy.sh, so both agree on where the state files live.
NOVIQ_DATA_DIR="$( . ./.env >/dev/null 2>&1; echo "${NOVIQ_DATA_DIR:-$REPO/.noviq-data}" )"
FAILED_SHA_FILE="${FAILED_SHA_FILE:-$NOVIQ_DATA_DIR/DEPLOY_FAILED_SHA}"
LAST_SHA_FILE="${LAST_SHA_FILE:-$NOVIQ_DATA_DIR/LAST_DEPLOYED_SHA}"

git fetch -q origin "$BRANCH"
REMOTE=$(git rev-parse "origin/$BRANCH")

# Circuit breaker: this exact sha already failed its health probe and was rolled back.
# Rebuilding it would fail identically. A new push moves REMOTE and clears the block;
# to force a retry of the same sha, delete the file by hand.
if [ -f "$FAILED_SHA_FILE" ] && [ "$(cat "$FAILED_SHA_FILE")" = "$REMOTE" ]; then
  echo "skip: $REMOTE previously failed its health probe (breaker at $FAILED_SHA_FILE)"
  exit 0
fi

# First run after this change has no marker; fall back to HEAD so an already-deployed
# box doesn't rebuild once for no reason.
if [ -f "$LAST_SHA_FILE" ]; then
  LOCAL=$(cat "$LAST_SHA_FILE")
else
  LOCAL=$(git rev-parse HEAD)
fi

if [ "$LOCAL" != "$REMOTE" ]; then
  echo "$BRANCH changed ($LOCAL -> $REMOTE) — starting deploy"
  exec sudo systemctl start --no-block noviq-deploy.service
fi
