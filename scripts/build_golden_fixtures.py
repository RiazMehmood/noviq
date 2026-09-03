#!/usr/bin/env python3
"""Rebuild the golden extraction fixtures from the source PDFs.

Each fixture is ONE page of one real document, saved verbatim from
`pdftotext -layout` — the same text the importer sees. The paired expectations
live in `src-tauri/src/services/heading_detector/golden.rs`.

Run this on the VM, where the PDFs are:

    python3 scripts/build_golden_fixtures.py --out src-tauri/tests/golden

Pages are located by a MARKER pattern rather than a page number on purpose. Page
numbers move when a document is re-uploaded or replaced (the Sindh Local
Government Act was swapped from an 8-page scan to the 150-page principal Act
mid-project), and a fixture silently pointing at the wrong page is worse than a
missing one.

**Regenerate whenever the extractor changes.** That is the point of the set: for
the layout-extraction work, regenerate against the new extractor and the
expectations in golden.rs tell you whether it is better or merely different.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys

# (fixture stem, document display_name prefix, regex identifying the page)
CASES: list[tuple[str, str, str]] = [
    ("constitution_footnote_343",   "Constitution",                    r"343\.\s+The joint sitting passed"),
    ("constitution_glued_142",      "Constitution",                    r"142\.\s*Subject to the Constitution"),
    ("secretariat_quoted_rule_90",  "Govt. of Sindh Manual",           r"90\.\s*[“\"]Secret"),
    ("secretariat_glued_quote_131", "Govt. of Sindh Manual",           r"131\.[“\"]PUC"),
    ("secretariat_no_dot_153",      "Govt. of Sindh Manual",           r"153\s+This category shall include"),
    ("sppra_omitted_xxx",           "SPPRA Rules",                     r"56\.\s*\[XXX\]"),
    ("ed_dot_hyphen_rule_10",       "Sindh Civil Servants Efficiency", r"\[10\.-\s+\(1\)\s*A civil servant aggrieved"),
    ("apt_appointing_table",        "Sindh Civil Servants APT",        r"S\.\s*No\.\s+Posts\s+Appointing Authority"),
    ("delegation_printing_table",   "Sindh Delegation",                r"Charges for Printing at"),
    ("dev_manual_toc_table",        "Manual for Development",          r"List of Five Year Plans"),
    ("rob_doubled_number_13",       "Sindh Rules of Business",         r"13\.\s+13\.\s+The Chief Minister"),
    # NB matched on the PROVISION TEXT, not the catchline: "75. Commercial Schemes"
    # also appears in the table of contents, and the first generation of this
    # fixture silently captured the TOC page instead of the body.
    ("slga_clean_baseline",         "Sindh Local Government Act 2013", r"Government may set up a Board, Authority"),
    ("slga_toc_page",              "Sindh Local Government Act 2013", r"74\. Transfer of functions from Councils to Government and Vice Versa\.-\s*\."),
]


def load_docs(catalog: str) -> dict[str, str]:
    """`display_name|file_path` per line, as exported from the sources table."""
    docs: dict[str, str] = {}
    with open(catalog, encoding="utf-8") as fh:
        for line in fh:
            line = line.rstrip("\n")
            if "|" in line:
                name, path = line.split("|", 1)
                docs[name.strip()] = path.strip()
    return docs


def page_text(path: str) -> list[str]:
    out = subprocess.run(
        ["pdftotext", "-layout", path, "-"],
        capture_output=True, text=True, errors="replace", check=False,
    )
    if out.returncode != 0:
        print(f"  pdftotext failed: {out.stderr.strip()[:200]}", file=sys.stderr)
        return []
    # pdftotext separates pages with a form feed.
    return out.stdout.split("\f")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--catalog", default="/tmp/docs.txt",
                    help="display_name|file_path per line (export from the sources table)")
    ap.add_argument("--out", default="src-tauri/tests/golden")
    args = ap.parse_args()

    if not os.path.exists(args.catalog):
        print(
            f"catalog {args.catalog} not found. Export it first, e.g.\n"
            "  psql ... -At -F'|' -c \"select display_name, file_path from sources "
            "where file_path like '%.pdf'\" > /tmp/docs.txt",
            file=sys.stderr,
        )
        return 2

    docs = load_docs(args.catalog)
    os.makedirs(args.out, exist_ok=True)
    cache: dict[str, list[str]] = {}
    written = failed = 0

    for fixture, prefix, marker in CASES:
        path = next((p for n, p in docs.items() if n.startswith(prefix)), None)
        if not path:
            print(f"{fixture:32} SKIP  no document matching {prefix!r}")
            failed += 1
            continue
        if path not in cache:
            cache[path] = page_text(path)
        pages = cache[path]
        hit = next((i for i, pg in enumerate(pages) if re.search(marker, pg)), None)
        if hit is None:
            # A marker that stops matching is itself a finding: the document was
            # replaced, or the extractor now renders that page differently.
            print(f"{fixture:32} SKIP  marker not found — document replaced, or "
                  f"the extractor changed this page")
            failed += 1
            continue
        dest = os.path.join(args.out, f"{fixture}.txt")
        with open(dest, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(pages[hit])
        print(f"{fixture:32} p.{hit + 1:<4} {len(pages[hit]):6} chars -> {dest}")
        written += 1

    print(f"\n{written} fixture(s) written, {failed} skipped")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
