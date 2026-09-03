#!/usr/bin/env bash
# T026 (F028) — verify what the public internet actually publishes for noviq.pk mail.
#
# Why this exists: the F028 delegation was once believed to be "propagating" for six
# days when in fact nobody had entered it. The console claiming a record exists is not
# evidence; a public resolver answering with it is. Run this AFTER T019–T025, from a
# machine OUTSIDE the Cloudflare account, and read the FAIL lines literally.
#
# Uses DNS-over-HTTPS against Google's public resolver so the result does not depend on
# whatever resolver the local network or VPN injects.
#
# Usage:  bash scripts/verify-mail-dns.sh [domain] [dkim-selector]
set -uo pipefail

DOMAIN="${1:-noviq.pk}"
SELECTOR="${2:-zoho}"   # The selector actually created in the Zoho console (T022, 2026-08-23). Override as arg 2 if it is ever rotated.
fails=0

q() { # q <name> <type> -> prints one record per line, empty if none
  curl -sS --max-time 15 -H 'accept: application/dns-json' \
    "https://dns.google/resolve?name=$1&type=$2" |
    python -c 'import sys,json;d=json.load(sys.stdin);[print(a.get("data","")) for a in d.get("Answer",[])]'
}

check() { # check <label> <ok|fail> <detail>
  if [ "$2" = ok ]; then printf '  PASS  %-22s %s\n' "$1" "$3"
  else printf '  FAIL  %-22s %s\n' "$1" "$3"; fails=$((fails+1)); fi
}

echo "Verifying mail DNS for $DOMAIN (public resolver, DoH)"

mx=$(q "$DOMAIN" MX)
[ -n "$mx" ] && check MX ok "$(echo "$mx" | tr '\n' ' ')" \
             || check MX fail "no MX records published — T020 not done, mail cannot be delivered"

txt=$(q "$DOMAIN" TXT)
spf=$(echo "$txt" | grep -i 'v=spf1' || true)
spf_count=$(printf '%s' "$spf" | grep -c . || true)
if [ "$spf_count" -eq 1 ]; then
  check SPF ok "$spf"
  echo "$spf" | grep -qi 'include:zohomail.com' || check "SPF include" fail "does not include zohomail.com"
  echo "$spf" | grep -qi -- '-all' || check "SPF policy" fail "not a hard fail (-all)"
elif [ "$spf_count" -eq 0 ]; then
  check SPF fail "no v=spf1 record — T023 not done"
else
  # The documented trap: a second SPF record invalidates BOTH, and with -all that
  # hard-fails every message the domain sends.
  check SPF fail "$spf_count SPF records published — MUST be exactly one; delete the extras"
fi

dmarc=$(q "_dmarc.$DOMAIN" TXT | grep -i 'v=DMARC1' || true)
[ -n "$dmarc" ] && check DMARC ok "$dmarc" \
                || check DMARC fail "no _dmarc TXT — T024 not done"

dkim=$(q "${SELECTOR}._domainkey.$DOMAIN" TXT | grep -i 'v=DKIM1\|p=' || true)
[ -n "$dkim" ] && check "DKIM ($SELECTOR)" ok "${dkim:0:72}…" \
               || check "DKIM ($SELECTOR)" fail "nothing at ${SELECTOR}._domainkey — T022 not done, or the selector differs"

echo
if [ "$fails" -eq 0 ]; then
  echo "T026 PASSES — the internet publishes all four. Proceed to T027/T028 (send + receive)."
else
  echo "T026 FAILS — $fails of 4 missing or wrong. These are DNS entries someone has to"
  echo "type; nothing here resolves itself by waiting. Fix the named task, then re-run."
fi
exit $((fails > 0))
