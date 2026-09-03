"""Shared plumbing for the Sindh corpus harvesters.

Two phases, deliberately separate:

  enumerate -> catalog.jsonl   (metadata only, cheap, safe to re-run)
  download  -> files/ + manifest.jsonl

Keeping them apart means we can size the haul and eyeball the titles before
spending bandwidth, and a failed download run never loses the catalogue.
"""

from __future__ import annotations

import hashlib
import html
import json
import os
import re
import time
from dataclasses import dataclass, field, asdict
from pathlib import Path

import requests

UA = "Mozilla/5.0 (compatible; NoviqCorpusHarvester/1.0; +legal corpus currency refresh)"

# Politeness. These are provincial government servers on modest hosting; a
# harvester that hammers them is both rude and likely to get blocked.
DELAY_S = 1.0
TIMEOUT_S = 90
RETRIES = 3


@dataclass
class Doc:
    """One catalogued document, before download."""

    site: str
    doc_id: str
    title: str
    source_url: str
    doc_type: str = ""
    department: str = ""
    # `as_of` is the instrument's own stated date. Audit §C.3 item 1 asks for
    # this per source so citations can show currency instead of hiding it.
    as_of: str = ""
    lang: str = "english"
    extra: dict = field(default_factory=dict)


def session() -> requests.Session:
    s = requests.Session()
    s.headers.update({"User-Agent": UA, "Accept-Language": "en-US,en;q=0.9"})
    return s


def get(s: requests.Session, url: str, **kw) -> requests.Response:
    """GET with retry/backoff. Raises on final failure."""
    last = None
    for attempt in range(RETRIES):
        try:
            r = s.get(url, timeout=TIMEOUT_S, **kw)
            r.raise_for_status()
            time.sleep(DELAY_S)
            return r
        except Exception as e:  # noqa: BLE001 - network layer, retry anything
            last = e
            time.sleep(DELAY_S * (2**attempt))
    raise RuntimeError(f"GET failed after {RETRIES} tries: {url}: {last}")


def clean(s: str) -> str:
    """Strip tags, unescape entities, collapse whitespace."""
    s = re.sub(r"<[^>]+>", " ", s or "")
    return re.sub(r"\s+", " ", html.unescape(s)).strip()


def safe_name(title: str, doc_id: str, ext: str = ".pdf") -> str:
    stem = re.sub(r"[^A-Za-z0-9]+", "_", title).strip("_")[:110] or "untitled"
    return f"{stem}__{doc_id}{ext}"


# ── catalogue / manifest io ───────────────────────────────────────────────────


def write_catalog(path: Path, docs: list[Doc]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        for d in docs:
            f.write(json.dumps(asdict(d), ensure_ascii=False) + "\n")


def read_catalog(path: Path) -> list[dict]:
    if not path.exists():
        return []
    with path.open(encoding="utf-8") as f:
        return [json.loads(line) for line in f if line.strip()]


def read_manifest(path: Path) -> dict[str, dict]:
    """Keyed by source_url so a re-run resumes instead of re-downloading."""
    out: dict[str, dict] = {}
    if path.exists():
        with path.open(encoding="utf-8") as f:
            for line in f:
                if line.strip():
                    rec = json.loads(line)
                    out[rec["source_url"]] = rec
    return out


def append_manifest(path: Path, rec: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as f:
        f.write(json.dumps(rec, ensure_ascii=False) + "\n")


# ── download ──────────────────────────────────────────────────────────────────


def download_all(docs: list[dict], out_dir: Path, manifest_path: Path) -> dict:
    """Download every catalogued doc, skipping ones already fetched.

    Dedups on sha256: the same instrument is frequently listed under several
    departments (RTW) and mirrored between the two sites, so identical bytes
    arriving under two titles is the normal case, not an anomaly.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    done = read_manifest(manifest_path)
    seen_sha = {r["sha256"]: r for r in done.values() if r.get("sha256")}
    s = session()

    stats = {"ok": 0, "skipped": 0, "duplicate": 0, "failed": 0, "bytes": 0}

    for i, d in enumerate(docs, 1):
        url = d["source_url"]
        if url in done:
            stats["skipped"] += 1
            continue

        try:
            r = get(s, url, stream=True)
            body = r.content
        except Exception as e:  # noqa: BLE001
            append_manifest(
                manifest_path,
                {**d, "ok": False, "error": str(e)[:300], "sha256": None},
            )
            stats["failed"] += 1
            print(f"  [{i}/{len(docs)}] FAIL {d['title'][:60]}: {str(e)[:90]}")
            continue

        sha = hashlib.sha256(body).hexdigest()
        ctype = r.headers.get("Content-Type", "").split(";")[0].strip()

        # A 200 that is really an HTML error page is the classic government-site
        # failure mode. Record it as failed rather than indexing an error page.
        looks_pdf = body[:5] == b"%PDF-"
        ext = ".pdf" if looks_pdf else (".html" if b"<html" in body[:2000].lower() else ".bin")

        if sha in seen_sha:
            append_manifest(
                manifest_path,
                {
                    **d,
                    "ok": True,
                    "sha256": sha,
                    "duplicate_of": seen_sha[sha]["source_url"],
                    "local_path": seen_sha[sha].get("local_path"),
                    "bytes": len(body),
                    "content_type": ctype,
                },
            )
            stats["duplicate"] += 1
            continue

        name = safe_name(d["title"], d["doc_id"], ext)
        dest = out_dir / d["site"] / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(body)

        rec = {
            **d,
            "ok": looks_pdf,
            "sha256": sha,
            "local_path": str(dest.relative_to(out_dir.parent)),
            "bytes": len(body),
            "content_type": ctype,
            "is_pdf": looks_pdf,
        }
        append_manifest(manifest_path, rec)
        seen_sha[sha] = rec
        stats["ok" if looks_pdf else "failed"] += 1
        stats["bytes"] += len(body)

        if i % 25 == 0:
            print(f"  [{i}/{len(docs)}] {stats}")

    return stats
