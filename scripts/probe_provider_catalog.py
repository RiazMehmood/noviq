#!/usr/bin/env python3
"""Probe every model in `catalog.json` against its live provider.

The catalogue shipped with the server is *researched*, not verified: model ids
were taken from mid-2026 aggregator sources and some will have been renamed or
retired. Across 41 ids, a few 404s are a near-certainty rather than a worry.
This script turns the catalogue from researched into verified.

It talks to the providers directly rather than through
`POST /api/admin/inference/validate`, for three reasons: no admin session to
obtain, no Noviq server needs to be running, and the probe still exercises the
exact URL construction the server uses (`{base_url}{prefix}/chat/completions`).
The request is the same 1-token non-streaming call.

Keys are read from the environment or a key file; they are never echoed, never
logged, and never written to the output. Only providers you hold a key for are
probed -- everything else is reported as SKIPPED, not as a failure.

Usage
-----
  # keys from env (one per provider you want to probe)
  export NOVIQ_PROBE_KEY_GROQ=...
  export NOVIQ_PROBE_KEY_GOOGLE=...
  python3 scripts/probe_provider_catalog.py

  # or from a file of `provider=key` lines (keep it out of the repo)
  python3 scripts/probe_provider_catalog.py --keys-file ~/.noviq-probe-keys

  # one provider only, then write a pruned catalogue
  python3 scripts/probe_provider_catalog.py --provider groq
  python3 scripts/probe_provider_catalog.py --prune out.json

Exit status: 0 when nothing failed, 1 when at least one model was rejected,
2 on a usage error. Rate-limited and skipped models never fail the run.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

DEFAULT_CATALOG = Path(__file__).resolve().parents[1] / "src-tauri/src/services/inference/catalog.json"

# Outcome classes. Only NOT_FOUND is evidence that a model id is wrong -- see
# `prune_catalog` for why the others must never remove anything.
OK = "OK"
RATE_LIMITED = "RATE_LIMITED"
UNAUTHORIZED = "UNAUTHORIZED"
NOT_FOUND = "NOT_FOUND"
BAD_REQUEST = "BAD_REQUEST"
UPSTREAM = "UPSTREAM_ERROR"
UNREACHABLE = "UNREACHABLE"
SKIPPED = "SKIPPED"

# Statuses that mean "this key/model pair is usable". A 429 proves auth *and*
# model entitlement both passed and only the rate limit was hit -- which is a
# pass for our purposes, exactly as the server's validate endpoint treats it.
GOOD = {OK, RATE_LIMITED}

# ASCII only: this runs on the VM over SSH and on a Windows console whose default
# code page (cp1252) cannot encode box glyphs -- a UnicodeEncodeError while
# printing progress would abort a sweep that was otherwise working.
GLYPH = {
    OK: "[ok]",
    RATE_LIMITED: "[rl]",
    SKIPPED: "[--]",
}
FAIL_GLYPH = "[XX]"


def load_keys(keys_file: str | None) -> dict[str, str]:
    """Collect provider keys from `NOVIQ_PROBE_KEY_<PROVIDER>` and/or a key file.

    File format is one `provider=key` per line; `#` comments and blanks ignored.
    File entries win over the environment so a scratch file can override a shell
    that still has a stale export in it.
    """
    keys: dict[str, str] = {}
    prefix = "NOVIQ_PROBE_KEY_"
    for name, val in os.environ.items():
        if name.startswith(prefix) and val.strip():
            keys[name[len(prefix):].strip().lower()] = val.strip()
    if keys_file:
        p = Path(keys_file).expanduser()
        if not p.is_file():
            sys.exit(f"error: keys file not found: {p}")
        for raw in p.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, _, v = line.partition("=")
            if v.strip():
                keys[k.strip().lower()] = v.strip()
    return keys


def probe(base_url: str, prefix: str, model: str, key: str, timeout: float) -> tuple[str, int, str]:
    """One 1-token chat completion. Returns (outcome, http_status, detail)."""
    url = f"{base_url.rstrip('/')}{prefix}/chat/completions"
    body = json.dumps(
        {
            "model": model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1,
            "stream": False,
        }
    ).encode()
    req = urllib.request.Request(url, data=body, method="POST")
    req.add_header("Content-Type", "application/json")
    if key:
        req.add_header("Authorization", f"Bearer {key}")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            # `.status` on the urlopen result is only guaranteed from 3.9; the VM
            # runs 3.8, where `getcode()` is the portable accessor.
            status = getattr(resp, "status", None) or resp.getcode()
            return OK, status, ""
    except urllib.error.HTTPError as e:
        status = e.code
        # Read the body for diagnostics, but keep it short: some providers return
        # a full HTML error page and it would swamp the report.
        try:
            detail = e.read().decode("utf-8", "replace").strip().replace("\n", " ")[:200]
        except Exception:
            detail = ""
        if status == 429:
            return RATE_LIMITED, status, detail
        if status in (401, 403):
            return UNAUTHORIZED, status, detail
        if status == 404:
            return NOT_FOUND, status, detail
        if status == 400:
            return BAD_REQUEST, status, detail
        return UPSTREAM, status, detail
    except Exception as e:  # DNS, TLS, timeout, no egress
        return UNREACHABLE, 0, str(e)[:200]


def main() -> int:
    ap = argparse.ArgumentParser(description="Probe every catalogue model against its live provider.")
    ap.add_argument("--catalog", default=str(DEFAULT_CATALOG), help="path to catalog.json")
    ap.add_argument("--keys-file", help="file of `provider=key` lines")
    ap.add_argument("--provider", action="append", help="probe only this provider id (repeatable)")
    ap.add_argument("--delay", type=float, default=1.5,
                    help="seconds between calls; free tiers are tight (default 1.5)")
    ap.add_argument("--timeout", type=float, default=20.0, help="per-request timeout (default 20)")
    ap.add_argument("--json", dest="as_json", action="store_true", help="emit JSON results instead of a table")
    ap.add_argument("--prune", metavar="OUT",
                    help="write a copy of the catalogue with NOT_FOUND models removed")
    ap.add_argument("--list", action="store_true", help="list what would be probed, then exit")
    args = ap.parse_args()

    cat_path = Path(args.catalog)
    if not cat_path.is_file():
        sys.exit(f"error: catalogue not found: {cat_path}")
    catalog = json.loads(cat_path.read_text(encoding="utf-8"))

    keys = load_keys(args.keys_file)
    only = {p.lower() for p in args.provider} if args.provider else None

    providers = [
        p for p in catalog.get("providers", [])
        if p.get("models") and (only is None or p["id"].lower() in only)
    ]
    if only:
        missing = only - {p["id"].lower() for p in catalog.get("providers", [])}
        if missing:
            sys.exit(f"error: unknown provider(s): {', '.join(sorted(missing))}")

    if args.list:
        for p in providers:
            has = "key" if p["id"] in keys else "NO KEY"
            print(f"{p['id']:<12} {len(p['models']):>3} models  [{has}]")
        return 0

    if not keys:
        print("No keys found. Set NOVIQ_PROBE_KEY_<PROVIDER> or pass --keys-file.", file=sys.stderr)
        print("Nothing to probe -- every provider would be SKIPPED.", file=sys.stderr)
        return 2

    results: list[dict] = []
    total = sum(len(p["models"]) for p in providers)
    done = 0

    for p in providers:
        pid = p["id"]
        key = keys.get(pid, "")
        for m in p["models"]:
            done += 1
            if not key:
                results.append({"provider": pid, "model": m["id"], "tier": m.get("tier"),
                                "outcome": SKIPPED, "status": 0, "detail": "no key for this provider"})
                continue
            outcome, status, detail = probe(p["base_url"], p.get("prefix", ""), m["id"], key, args.timeout)
            results.append({"provider": pid, "model": m["id"], "tier": m.get("tier"),
                            "outcome": outcome, "status": status, "detail": detail})
            if not args.as_json:
                mark = GLYPH.get(outcome, FAIL_GLYPH)
                print(f"[{done:>3}/{total}] {mark} {pid}/{m['id']} -> {outcome}"
                      + (f" ({status})" if status else ""), flush=True)
            # Be a good citizen: free tiers are exactly what we are testing, and
            # hammering them would turn real passes into spurious 429s.
            if args.delay:
                time.sleep(args.delay)

    if args.as_json:
        print(json.dumps({"catalog_version": catalog.get("version"), "results": results}, indent=2))
    else:
        report(results)

    if args.prune:
        prune_catalog(catalog, results, Path(args.prune))

    failed = [r for r in results if r["outcome"] not in GOOD and r["outcome"] != SKIPPED]
    return 1 if failed else 0


def report(results: list[dict]) -> None:
    print("\n" + "=" * 78)
    print("SUMMARY")
    print("=" * 78)
    by_outcome: dict[str, list[dict]] = {}
    for r in results:
        by_outcome.setdefault(r["outcome"], []).append(r)

    for outcome in (OK, RATE_LIMITED, NOT_FOUND, UNAUTHORIZED, BAD_REQUEST, UPSTREAM, UNREACHABLE, SKIPPED):
        rows = by_outcome.get(outcome)
        if not rows:
            continue
        print(f"\n{outcome} ({len(rows)})")
        for r in rows:
            line = f"  {r['provider']}/{r['model']}"
            if r["detail"] and outcome not in (OK, RATE_LIMITED, SKIPPED):
                line += f"\n      {r['detail']}"
            print(line)

    usable = sum(len(by_outcome.get(o, [])) for o in GOOD)
    probed = len(results) - len(by_outcome.get(SKIPPED, []))
    print("\n" + "-" * 78)
    print(f"usable {usable}/{probed} probed  ({len(by_outcome.get(SKIPPED, []))} skipped for want of a key)")
    if by_outcome.get(NOT_FOUND):
        print(f"\n{len(by_outcome[NOT_FOUND])} model id(s) returned 404 -- these are wrong in the")
        print("catalogue and should be corrected or removed. Re-run with --prune to")
        print("write a cleaned copy.")
    if by_outcome.get(UNAUTHORIZED):
        print(f"\n{len(by_outcome[UNAUTHORIZED])} returned 401/403. Check the key first: a bad key")
        print("looks identical to a model you are not entitled to.")


def prune_catalog(catalog: dict, results: list[dict], out: Path) -> None:
    """Write a copy with 404 models removed.

    Deliberately narrow: only NOT_FOUND is removed. UNAUTHORIZED usually means a
    wrong or unentitled key rather than a wrong id, UNREACHABLE means the network
    failed, and SKIPPED means we never asked -- pruning on any of those would
    silently delete perfectly good models on the strength of a local problem.
    """
    dead = {(r["provider"], r["model"]) for r in results if r["outcome"] == NOT_FOUND}
    if not dead:
        print(f"\nnothing to prune -- no 404s. {out} not written.")
        return
    removed = 0
    for p in catalog.get("providers", []):
        before = len(p.get("models", []))
        p["models"] = [m for m in p.get("models", []) if (p["id"], m["id"]) not in dead]
        removed += before - len(p["models"])
    out.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"\npruned {removed} model(s) -> {out}")
    print("Review the diff before copying over catalog.json.")


if __name__ == "__main__":
    sys.exit(main())
