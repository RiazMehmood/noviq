#!/usr/bin/env python3
"""Inject synthetic "chapter index" articles into constitution_ir.json.

Why
---
Questions like "Which Articles fall under Chapter 1 of Part II?" and
"What are the Principles of Policy listed in Chapter 2 of Part II?" fail on
the fast tier because no single chunk enumerates every article in a chapter.
The Part-level summary chunk exists ("Part II … comprises Chapter 1 — F…")
but it doesn't list the chapter's children. Hybrid retrieval then surfaces
unrelated Part XII chapter-3 / Election-Commission noise.

Fix
---
For every Chapter (and for each Part that has direct articles instead of
chapters), prepend one synthetic Article whose `text` is the full enumeration
of that chapter/part. The constitution indexer will emit it as a chunk like
any other article. We give the synthetic article a distinctive `number`
(`IDX-P<part>-C<chapter>`) and a clear `marginal_note` so the trace is
self-explanatory.

Run once on the canonical IR; the script is idempotent — it skips chapters
whose first article already has an `IDX-` number.
"""
from __future__ import annotations
import json
import shutil
import sys
from pathlib import Path

DEFAULT_IR_PATHS = [
    Path(__file__).parent / "constitute_IR" / "constitution_ir.json",
    Path("/media/riaz/New Volume/echo-AI/tools/qwen_finetune/data/constitution_ir.json"),
    Path("/home/riaz/Desktop/constitution_ir.json"),
    Path("/home/riaz/Desktop/Sindh Govt Documents/constitution_ir.json"),
]
BACKUP_SUFFIX = ".bak-pre-chapter-index"


def article_label(a: dict) -> str:
    n = a.get("number", "?")
    mn = (a.get("marginal_note") or "").strip().rstrip(".:")
    return f"Article {n} — {mn}" if mn else f"Article {n}"


def synth_chapter_index(part_no: str, chapter_no: str, chapter_title: str, articles: list) -> dict:
    enum = "; ".join(article_label(a) for a in articles)
    text = (
        f"Part {part_no}, Chapter {chapter_no} — {chapter_title} contains the "
        f"following articles: {enum}."
    )
    return {
        "number": f"IDX-P{part_no}-C{chapter_no}",
        "marginal_note": f"Index of Chapter {chapter_no} of Part {part_no} ({chapter_title})",
        "text": text,
        "clauses": [],
        "provisos": [],
        "explanations": [],
        "footnote_refs": [],
    }


def synth_part_index(part_no: str, part_title: str, articles: list) -> dict:
    enum = "; ".join(article_label(a) for a in articles)
    text = (
        f"Part {part_no} — {part_title} contains the following articles: "
        f"{enum}."
    )
    return {
        "number": f"IDX-P{part_no}",
        "marginal_note": f"Index of Part {part_no} ({part_title})",
        "text": text,
        "clauses": [],
        "provisos": [],
        "explanations": [],
        "footnote_refs": [],
    }


def has_index_marker(articles: list) -> bool:
    return bool(articles) and str(articles[0].get("number", "")).startswith("IDX-")


def strip_index_markers(articles: list) -> int:
    """Remove all IDX-* articles in place. Returns count removed."""
    n = 0
    i = 0
    while i < len(articles):
        if str(articles[i].get("number", "")).startswith("IDX-"):
            articles.pop(i)
            n += 1
        else:
            i += 1
    return n


def _process_one(ir_path: Path, refresh: bool = False) -> tuple[int, int, int]:
    if not ir_path.exists():
        print(f"⚠ Skipping (missing): {ir_path}")
        return (0, 0, 0)
    print(f"\n▶ {ir_path}{' (refresh)' if refresh else ''}")
    ir = json.loads(ir_path.read_text())
    if refresh:
        n_wiped = 0
        for part in ir.get("parts", []):
            for ch in part.get("chapters") or []:
                n_wiped += strip_index_markers(ch.get("articles") or [])
            n_wiped += strip_index_markers(part.get("articles") or [])
        if n_wiped:
            print(f"  ↺ Removed {n_wiped} existing IDX-* entries before re-inject")
    backup = ir_path.with_suffix(ir_path.suffix + BACKUP_SUFFIX)
    if not backup.exists():
        shutil.copy2(ir_path, backup)
        print(f"  ✓ Wrote backup → {backup.name}")
    else:
        print(f"  • Backup exists → {backup.name} (not overwriting)")

    n_chapters_injected = 0
    n_parts_injected = 0
    n_chapters_skipped = 0
    n_parts_skipped = 0

    for part in ir.get("parts", []):
        part_no = str(part.get("number", "?"))
        part_title = part.get("title", "")
        chapters = part.get("chapters") or []
        direct_articles = part.get("articles") or []

        if chapters:
            for ch in chapters:
                ch_no = str(ch.get("number", "?"))
                ch_title = ch.get("title", "")
                ch_arts = ch.get("articles") or []
                if not ch_arts:
                    continue
                if has_index_marker(ch_arts):
                    n_chapters_skipped += 1
                    continue
                idx_art = synth_chapter_index(part_no, ch_no, ch_title, ch_arts)
                ch_arts.insert(0, idx_art)
                n_chapters_injected += 1
                print(f"    + Part {part_no}, Chapter {ch_no}: indexed "
                      f"{len(ch_arts) - 1} articles → {idx_art['number']}")
        elif direct_articles:
            if has_index_marker(direct_articles):
                n_parts_skipped += 1
                continue
            idx_art = synth_part_index(part_no, part_title, direct_articles)
            direct_articles.insert(0, idx_art)
            n_parts_injected += 1
            print(f"    + Part {part_no}: indexed {len(direct_articles) - 1} "
                  f"articles → {idx_art['number']}")

    ir_path.write_text(json.dumps(ir, indent=2, ensure_ascii=False))
    print(
        f"  ✓ Injected {n_chapters_injected} chapter-index + "
        f"{n_parts_injected} part-index.  Skipped "
        f"{n_chapters_skipped + n_parts_skipped}."
    )
    return (n_chapters_injected, n_parts_injected,
            n_chapters_skipped + n_parts_skipped)


def main() -> int:
    args = sys.argv[1:]
    refresh = False
    if "--refresh" in args:
        refresh = True
        args = [a for a in args if a != "--refresh"]
    paths = [Path(p) for p in args] if args else DEFAULT_IR_PATHS
    totals = [0, 0, 0]
    for p in paths:
        ch, pt, sk = _process_one(p, refresh=refresh)
        totals[0] += ch
        totals[1] += pt
        totals[2] += sk
    print(
        f"\n══ Aggregate: {totals[0]} chapter-index + {totals[1]} part-index "
        f"across {len(paths)} files.  {totals[2]} skipped (already present)."
    )
    print(
        "\nNext step: re-import the constitution into echo.db on the host app "
        "(drop the source or trigger re-index). The new chunks retrieve under "
        "unit_label 'Article IDX-P<part>-C<chapter>' and rank naturally on "
        "\"Chapter N of Part X contains\" / \"Principles of Policy listed in\" "
        "queries."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
