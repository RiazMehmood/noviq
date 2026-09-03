#!/usr/bin/env python3
"""
Migrate a Feature 006 generic IR JSON sidecar into the Feature 007 legal IR schema.

Usage:
    python3 tools/migrate_generic_to_legal_ir.py INPUT.json OUTPUT.json \\
        --instrument-title "The Constitution of the Islamic Republic of Pakistan, 1973" \\
        --instrument-id "" \\
        --year 1973

Mapping rules (best-effort; unrecognized shapes are flagged, not silently dropped):

  Generic NodeKind            → Legal NodeKind
  ----------------            -------------
  "Section"                   → "Section" (or "Article" if instrument is Constitution)
  "Clause" (numbered (1),(2)) → "SubSection"
  "Clause" (lettered (a),(b)) → "Clause"
  "Clause" (roman (i),(ii))   → "SubClause"
  "Paragraph"                 → kept as-is; flagged for review
  unknown                     → kept verbatim; printed to stderr

Output adds: instrument_id, legal_path (best-effort path inference),
in_force=True, source_page (preserved if present).

The script is intentionally simple and non-destructive — it does NOT
mutate the input file. Review the output before pointing the indexer at it.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

NUMBER_RE = re.compile(r"^\((\d+)\)$")
LETTER_RE = re.compile(r"^\(([a-z]+)\)$")
ROMAN_RE = re.compile(r"^\(([ivxlcdm]+)\)$", re.IGNORECASE)


def reclassify_kind(node: dict, is_constitution: bool) -> str:
    kind = node.get("kind") or node.get("node_kind") or "Paragraph"
    number = (node.get("number") or "").strip()

    if kind == "Section":
        return "Article" if is_constitution else "Section"
    if kind == "Clause":
        if NUMBER_RE.match(number):
            return "SubSection"
        if LETTER_RE.match(number):
            return "Clause"
        if ROMAN_RE.match(number):
            return "SubClause"
        return "Clause"
    if kind in {
        "Section",
        "Clause",
        "Paragraph",
        "Schedule",
        "Definition",
        "Chapter",
        "Part",
    }:
        return kind
    print(
        f"  ! unknown kind {kind!r} on node {node.get('id')}; kept verbatim",
        file=sys.stderr,
    )
    return kind


def build_legal_path(nodes: list[dict], by_id: dict[str, dict]) -> None:
    """Walk parent_id chains and set legal_path on every node."""
    for node in nodes:
        segments: list[str] = []
        cur = node
        guard = 0
        while cur is not None and guard < 50:
            guard += 1
            kind = cur.get("legal_kind") or cur.get("kind")
            num = (cur.get("number") or "").strip().lstrip("(").rstrip(")")
            if kind == "Chapter" and num:
                segments.append(f"Ch-{num}")
            elif kind == "Part" and num:
                segments.append(f"Pt-{num}")
            elif kind in {"Section", "Article"} and num:
                segments.append(f"S-{num}")
            elif kind == "SubSection" and num:
                segments.append(f"({num})")
            elif kind == "Clause" and num:
                segments.append(f"({num})")
            elif kind == "SubClause" and num:
                segments.append(f"({num})")
            parent_id = cur.get("parent_id")
            cur = by_id.get(parent_id) if parent_id else None
        node["legal_path"] = "/".join(reversed(segments))


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("input", type=Path)
    p.add_argument("output", type=Path)
    p.add_argument("--instrument-title", required=True)
    p.add_argument("--instrument-id", default="")
    p.add_argument("--year", type=int, default=None)
    args = p.parse_args()

    data = json.loads(args.input.read_text(encoding="utf-8"))
    nodes = data.get("nodes") or data.get("graph_nodes") or []
    if not isinstance(nodes, list):
        print(
            "error: input has no 'nodes' or 'graph_nodes' array",
            file=sys.stderr,
        )
        return 2

    is_constitution = "constitution" in args.instrument_title.lower()
    instrument_id_str = args.instrument_id or args.instrument_title

    for n in nodes:
        n["legal_kind"] = reclassify_kind(n, is_constitution)
        n["instrument_id"] = instrument_id_str
        n["in_force"] = True

    by_id = {n["id"]: n for n in nodes if "id" in n}
    build_legal_path(nodes, by_id)

    out = {
        "schema_version": 16,
        "instrument_title": args.instrument_title,
        "instrument_id": instrument_id_str,
        "year": args.year,
        "nodes": nodes,
    }
    args.output.write_text(
        json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(f"wrote {args.output} ({len(nodes)} nodes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
