"""CLI for the Sindh corpus harvest.

    python -m scripts.corpus_scrape.harvest enumerate [--site sindhlaws|rtw|both]
    python -m scripts.corpus_scrape.harvest download  [--site ...] [--limit N]
    python -m scripts.corpus_scrape.harvest summary

Enumerate first. It is cheap, it is safe to re-run, and it tells us how big the
haul is before we spend bandwidth on a government server.
"""

from __future__ import annotations

import argparse
import collections
import json
from dataclasses import asdict
from pathlib import Path

from . import rtw, sindhlaws
from .common import Doc, download_all, read_catalog, write_catalog

OUT = Path(__file__).resolve().parents[2] / "corpus-harvest"
CATALOG = OUT / "catalog"
FILES = OUT / "files"
MANIFEST = OUT / "manifest.jsonl"


def cmd_enumerate(site: str) -> None:
    CATALOG.mkdir(parents=True, exist_ok=True)
    if site in ("sindhlaws", "both"):
        docs = sindhlaws.enumerate_all()
        write_catalog(CATALOG / "sindhlaws.jsonl", docs)
        print(f"[sindhlaws] catalogued {len(docs)} documents")
    if site in ("rtw", "both"):
        docs = rtw.enumerate_all()
        write_catalog(CATALOG / "rtw.jsonl", docs)
        print(f"[rtw] catalogued {len(docs)} documents")
    cmd_summary()


def _load(site: str) -> list[dict]:
    docs: list[dict] = []
    for name in ("sindhlaws", "rtw"):
        if site in (name, "both"):
            docs.extend(read_catalog(CATALOG / f"{name}.jsonl"))
    return docs


def cmd_summary() -> None:
    docs = _load("both")
    if not docs:
        print("Nothing catalogued yet — run `enumerate` first.")
        return
    print(f"\n{'='*60}\nCATALOGUE: {len(docs)} documents")
    by_site = collections.Counter(d["site"] for d in docs)
    for k, v in by_site.most_common():
        print(f"  {k:12s} {v:5d}")
    print("\nBy type:")
    for k, v in collections.Counter(d["doc_type"] or "?" for d in docs).most_common(15):
        print(f"  {k:24s} {v:5d}")
    urls = {d["source_url"] for d in docs}
    print(f"\nUnique URLs: {len(urls)} (of {len(docs)} rows — the rest are cross-listings)")
    hosts = collections.Counter(
        d["source_url"].split("/")[2] for d in docs if "//" in d["source_url"]
    )
    print("\nTop hosts:")
    for k, v in hosts.most_common(10):
        print(f"  {k:34s} {v:5d}")


def cmd_download(site: str, limit: int | None) -> None:
    docs = _load(site)
    # Dedup by URL before spending a request on the same file twice.
    seen: set[str] = set()
    uniq = [d for d in docs if not (d["source_url"] in seen or seen.add(d["source_url"]))]
    if limit:
        uniq = uniq[:limit]
    print(f"Downloading {len(uniq)} documents -> {FILES}")
    stats = download_all(uniq, FILES, MANIFEST)
    print(f"\nDone: {stats}")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("command", choices=["enumerate", "download", "summary"])
    ap.add_argument("--site", default="both", choices=["sindhlaws", "rtw", "both"])
    ap.add_argument("--limit", type=int, default=None)
    a = ap.parse_args()
    if a.command == "enumerate":
        cmd_enumerate(a.site)
    elif a.command == "download":
        cmd_download(a.site, a.limit)
    else:
        cmd_summary()


if __name__ == "__main__":
    main()
