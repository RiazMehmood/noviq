#!/usr/bin/env python3
"""Patch the 15 articles flagged "missing in IR" by the 2026-06-04 audit.

Audit source: tools/constitution_import/audits/sync_report_2026-06-04.md
Missing list (article number → source page in tools/constitution_import/.cache/):
  71 → part3.ch2.html       (Part III, Chapter 2)
  96, 96A → part3.ch3.html  (Part III, Chapter 3)
  134, 135 → part4.ch3.html (Part IV, Chapter 3)
  152A → part5.ch3.html     (Part V, Chapter 3)
  184, 186, 186A, 191A → part7.ch2.html    (Part VII, Chapter 2)
  203I → part7.ch3A.html    (Part VII, Chapter 3A)
  212A, 212B → part7.ch4.html  (Part VII, Chapter 4)
  247 → part12.ch3.html     (Part XII, Chapter 3)
  270C → part12.ch7.html    (Part XII, Chapter 7)

Extraction strategy: pakistani.org puts each Article in one outer <tr> where
col-1 is the article number (sometimes "71", sometimes "71." with trailing
period) and col-2 begins with `<b>Marginal note.</b>` followed by the body.

We extract a *flat-text* Article — `number`, `marginal_note`, `text` populated
with the full body collapsed to one string. `clauses` left empty. That's
enough for retrieval: the chunker will emit one chunk per Article and the
text will be FTS5-searchable. Detailed clause/sub-clause structure can be
re-added in a follow-up; the goal here is to close the audit gap so retrieval
on Q (e.g. "Mediation Committee", "Tribal Areas") can return the right
Article instead of falling through to hybrid-rot.

Idempotent: skips any article number already present in the target IR.
Multi-file aware: by default updates all four canonical copies.
"""
from __future__ import annotations
import json
import re
import shutil
import sys
from pathlib import Path
from typing import Optional

from bs4 import BeautifulSoup, Tag

CACHE_DIR = Path(__file__).parent / ".cache"

# article_number, source_html, part_no, chapter_no_or_None (None ⇒ direct in part)
MISSING = [
    ("71",   "part3.ch2.html",   "III",  "2"),
    ("96",   "part3.ch3.html",   "III",  "3"),
    ("96A",  "part3.ch3.html",   "III",  "3"),
    ("134",  "part4.ch3.html",   "IV",   "3"),
    ("135",  "part4.ch3.html",   "IV",   "3"),
    ("152A", "part5.ch3.html",   "V",    "3"),
    ("184",  "part7.ch2.html",   "VII",  "2"),
    ("186",  "part7.ch2.html",   "VII",  "2"),
    ("186A", "part7.ch2.html",   "VII",  "2"),
    ("191A", "part7.ch2.html",   "VII",  "2"),
    ("203I", "part7.ch3A.html",  "VII",  "3A"),
    ("212A", "part7.ch4.html",   "VII",  "4"),
    ("212B", "part7.ch4.html",   "VII",  "4"),
    ("247",  "part12.ch3.html",  "XII",  "3"),
    ("270C", "part12.ch7.html",  "XII",  "7"),
]

DEFAULT_IR_PATHS = [
    Path("/media/riaz/New Volume/echo-AI/tools/constitution_import/constitute_IR/constitution_ir.json"),
    Path("/media/riaz/New Volume/echo-AI/tools/qwen_finetune/data/constitution_ir.json"),
    Path("/home/riaz/Desktop/constitution_ir.json"),
    Path("/home/riaz/Desktop/Sindh Govt Documents/constitution_ir.json"),
]

BACKUP_SUFFIX = ".bak-pre-missing-15"


def cleantext(node) -> str:
    """Collapse whitespace; preserve [bracketed amendment text] but drop sups."""
    if isinstance(node, str):
        s = node
    else:
        for sup in node.find_all("sup"):
            sup.decompose()
        s = node.get_text(separator=" ")
    s = re.sub(r"\s+", " ", s).strip()
    s = re.sub(r"\[\s*\]", "", s)  # empty brackets from dropped sups
    return s


def find_article_tr(soup: BeautifulSoup, art_num: str) -> Optional[Tag]:
    """Locate the outer <tr> whose first <td> equals `art_num` or `art_num.`."""
    target = art_num.rstrip(".")
    for tr in soup.find_all("tr"):
        tds = tr.find_all("td", recursive=False)
        if len(tds) < 2:
            continue
        first = cleantext(tds[0]).rstrip(".")
        if first == target:
            return tr
    return None


def extract_article(html_path: Path, art_num: str) -> Optional[dict]:
    """Return the Article shape (number, marginal_note, text) or None if
    not found in the cached HTML."""
    if not html_path.exists():
        print(f"  ⚠ HTML missing: {html_path.name} — cannot extract Article {art_num}")
        return None
    soup = BeautifulSoup(html_path.read_text(), "html.parser")
    tr = find_article_tr(soup, art_num)
    if tr is None:
        return None
    body_cell = tr.find_all("td", recursive=False)[1]
    # Marginal note = leading <b>…</b>. Fall back to first sentence if absent.
    bold = body_cell.find("b")
    if bold:
        marginal_note = cleantext(bold).rstrip(".")
        # Body = full cell minus the leading bold (we just take full text;
        # the marginal_note prefix is harmless duplication for retrieval,
        # and the chunker will still match on Article number + clause text).
    else:
        full = cleantext(body_cell)
        # First sentence as marginal_note, or first 8 words.
        m = re.match(r"([^.]{3,120})\.", full)
        marginal_note = (m.group(1) if m else " ".join(full.split()[:8])).strip()
    full_text = cleantext(body_cell)
    # Trim leading marginal-note repetition if present.
    if full_text.lower().startswith(marginal_note.lower()):
        full_text = full_text[len(marginal_note):].lstrip(". ").strip()
    return {
        "number": art_num,
        "marginal_note": marginal_note + ".",
        "text": full_text,
        "clauses": [],
        "provisos": [],
        "explanations": [],
        "footnote_refs": [],
    }


def part_chapter_articles(ir: dict, part_no: str, chapter_no: Optional[str]) -> Optional[list]:
    """Return the articles list inside the requested Part/Chapter, or None."""
    for part in ir.get("parts", []):
        if str(part.get("number")) != part_no:
            continue
        if chapter_no is None:
            return part.setdefault("articles", [])
        for ch in part.get("chapters", []):
            if str(ch.get("number")) == chapter_no:
                return ch.setdefault("articles", [])
    return None


def article_sort_key(num_str: str) -> tuple:
    """Sort 8, 8A, 9, 9A, 10 sensibly."""
    m = re.match(r"(\d+)([A-Za-z]*)", str(num_str))
    if m:
        return (int(m.group(1)), m.group(2))
    return (10**9, str(num_str))


def insert_sorted(arts: list, new_art: dict) -> int:
    """Insert preserving numeric order. Skip "IDX-…" entries (chapter index
    is always at slot 0). Returns insertion index."""
    new_key = article_sort_key(new_art["number"])
    insert_at = len(arts)
    for i, a in enumerate(arts):
        num = str(a.get("number", ""))
        if num.startswith("IDX-"):
            continue
        if article_sort_key(num) > new_key:
            insert_at = i
            break
    arts.insert(insert_at, new_art)
    return insert_at


def article_present(arts: list, art_num: str) -> bool:
    return any(str(a.get("number")) == art_num for a in arts)


def _process_one(ir_path: Path, extracted: dict) -> tuple[int, int, int]:
    """Apply the extracted articles dict to one IR file. Returns
    (n_inserted, n_skipped_already_present, n_missing_extract)."""
    if not ir_path.exists():
        print(f"⚠ Skipping (missing): {ir_path}")
        return (0, 0, 0)
    print(f"\n▶ {ir_path}")
    ir = json.loads(ir_path.read_text())
    backup = ir_path.with_suffix(ir_path.suffix + BACKUP_SUFFIX)
    if not backup.exists():
        shutil.copy2(ir_path, backup)
        print(f"  ✓ Wrote backup → {backup.name}")
    n_inserted = 0
    n_already = 0
    n_missing = 0
    for art_num, _, part_no, chapter_no in MISSING:
        if art_num not in extracted:
            n_missing += 1
            continue
        arts = part_chapter_articles(ir, part_no, chapter_no)
        if arts is None:
            label = f"Part {part_no}" + (f", Chapter {chapter_no}" if chapter_no else "")
            print(f"  ⚠ {label} not found in IR — cannot place Article {art_num}")
            continue
        if article_present(arts, art_num):
            n_already += 1
            continue
        pos = insert_sorted(arts, extracted[art_num])
        n_inserted += 1
        label = f"Part {part_no}" + (f", Chapter {chapter_no}" if chapter_no else "")
        print(f"    + Article {art_num} → {label} at index {pos}")
    ir_path.write_text(json.dumps(ir, indent=2, ensure_ascii=False))
    print(
        f"  ✓ Inserted {n_inserted}.  Skipped {n_already} already-present.  "
        f"Missing extract: {n_missing}."
    )
    return (n_inserted, n_already, n_missing)


def main() -> int:
    print("── Phase 1: extract from cached pakistani.org HTML ─────────────")
    extracted: dict[str, dict] = {}
    for art_num, html_name, _, _ in MISSING:
        art = extract_article(CACHE_DIR / html_name, art_num)
        if art is None:
            print(f"  ✗ Article {art_num} not found in {html_name}")
            continue
        extracted[art_num] = art
        print(f"  ✓ Article {art_num} — {art['marginal_note'][:60]}  "
              f"({len(art['text'])} chars)")
    print(f"\nExtracted {len(extracted)} / {len(MISSING)} articles.")

    if not extracted:
        print("✗ Nothing extracted; aborting.")
        return 1

    print("\n── Phase 2: patch all IR files ─────────────────────────────────")
    paths = [Path(p) for p in sys.argv[1:]] if len(sys.argv) > 1 else DEFAULT_IR_PATHS
    totals = [0, 0, 0]
    for p in paths:
        ins, alr, mis = _process_one(p, extracted)
        totals[0] += ins
        totals[1] += alr
        totals[2] += mis
    print(
        f"\n══ Aggregate: {totals[0]} inserted, {totals[1]} already-present, "
        f"{totals[2]} extraction-misses (per-file)."
    )
    print(
        "\nFollow-up: clause/sub-clause structure is intentionally flat in "
        "this pass (full text is in `Article.text`). A v2 pass can parse "
        "the per-clause <table> rows on the same HTML to populate "
        "`clauses[]` properly. Retrieval works either way; the flat text "
        "is FTS5- and vector-searchable as-is."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
