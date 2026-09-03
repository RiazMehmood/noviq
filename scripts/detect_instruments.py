#!/usr/bin/env python3
"""Does a PDF contain ONE legal instrument, or several — and where does each begin?

Deterministic, no model, `pdftotext` only — about a second on 30 pages, which is
the point: this runs at UPLOAD, before indexing, and a check the user waits two
minutes for is a check the user skips.

Why this exists: `Compendium Rules under SLGA 2013` is a dozen separate rule-sets
in one file, each with its own Rule 1. Indexed as a single source it makes
"Rule 4" ambiguous across instruments, yields one meaningless document summary,
and cites answers to a book instead of to a law — which is why `query_router`
carries a hand-written demotion for that one document. The pipeline assumes
file == instrument; this reports where that assumption is false.

TWO INDEPENDENT SIGNALS, AND NEITHER IS COMPLETE
  * SHORT-TITLE CLAUSE — "These rules may be called the X Rules, 2015". The
    instrument naming itself, so it is an excellent LOCATOR. It is a poor LABEL:
    read from `-layout` text it picks up the marginal catchline, giving
    "Sindh Local Councils Short title and (Election) Rules, 2015".
  * TABLE OF CONTENTS — dotted-leader rows, often WRAPPED across two lines.
    Clean titles and printed page numbers, so it is an excellent LABEL.

Measured on the Compendium, 2026-07-30: the clause pass finds `Sindh Councils
(Budget) Rules, 2017` at p.150, which the contents list omits entirely; the
contents list carries four instruments the clause pass misses. So they are
UNIONED, never ranked — trusting either alone loses real instruments.

The contents list prints BOOK page numbers, not PDF indices. The offset is
derived (not assumed) by matching clause hits to contents rows by title and
taking the median difference — measured +1 on the Compendium.

Nothing here decides anything. Every range is proposed to a human, and the cases
where the two signals disagree are reported as disagreements rather than quietly
reconciled, because that is exactly where a human's attention is worth spending.

The result is deliberately NOT cached. A re-uploaded edition is re-detected from
scratch: remembering last year's split would carry a stale boundary silently past
a compendium that has gained an instrument.

usage: detect_instruments.py <pdf> [<pdf> ...]
       detect_instruments.py --dir <directory>
       detect_instruments.py --verbose <pdf>
"""
import os
import re
import subprocess
import sys

SHORT_TITLE = re.compile(
    r"may\s+be\s+(?:called|cited\s+as)\s+(?:the\s+)?(?P<name>[^.;]{6,140}?)"
    r"\s*(?:,\s*)?(?P<year>1[89]\d\d|20\d\d)\s*[.,;]",
    re.IGNORECASE | re.DOTALL)

ANCHOR = re.compile(r"(?m)^[ \t]*\[?[ \t]*(\d{1,3})[A-Z]?\.")

# A contents row OPENS with its ordinal and CLOSES with dots + a page number.
# Between those it may wrap across any number of lines.
TOC_OPEN = re.compile(r"^\s*(?P<n>\d{1,2})\.\s+(?P<head>\S.*)$")
TOC_CLOSE = re.compile(r"^(?P<tail>.*?)\.{3,}\s*(?P<page>\d{1,4})\s*$")

# A title that names a legal instrument, as opposed to front/back matter
# ("Introduction", "Functions to be Performed by Corporations…").
INSTRUMENT_TITLE = re.compile(
    r"\b(Rules|Act|Ordinance|Regulations|Order|Policy)\b[^)]*?,?\s*(1[89]\d\d|20\d\d)\s*$",
    re.IGNORECASE)

CORROBORATION_WINDOW = 2   # pages, clause <-> numbering restart
MERGE_WINDOW = 3           # pages, clause <-> contents row are the same start
MIN_INSTRUMENT_PAGES = 2

# Catchline fragments that `-layout` splices into a short-title clause.
CATCHLINE_NOISE = re.compile(
    r"\b(Short\s+title(\s+and)?(\s+commencement)?|commencement|Definitions?)\b[.,]?",
    re.IGNORECASE)


def pages_of(pdf):
    try:
        out = subprocess.run(["pdftotext", "-layout", pdf, "-"],
                             capture_output=True, timeout=300)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if out.returncode != 0:
        return None
    pages = out.stdout.decode("utf-8", "replace").split("\f")
    # `pdftotext` TERMINATES every page with a form feed, so the split leaves a
    # trailing empty element that is not a page. Counted, it reports one page
    # too many and pushes the last range past the end of the document. Drop
    # exactly ONE: `Sindh Local Government Act 2013` genuinely ends on a blank
    # page, and dropping every empty element loses it (149 vs 150).
    if pages and not pages[-1].strip():
        pages.pop()
    return pages


def clean(s):
    return re.sub(r"\s+", " ", s).strip(" ,;:-")


def tokens(title):
    """Distinctive lowercase words, for matching a clause to a contents row."""
    stop = {"the", "of", "and", "rules", "act", "sindh", "a", "to", "for", "by",
            "ordinance", "regulations", "order", "local", "councils", "council"}
    return {w for w in re.findall(r"[a-z]+", (title or "").lower())
            if len(w) > 2 and w not in stop}


def clause_starts(pages):
    """[(pdf_page, raw_name)] where a document names itself."""
    hits, seen = [], set()
    for i, text in enumerate(pages, 1):
        for m in SHORT_TITLE.finditer(text):
            name = clean(CATCHLINE_NOISE.sub(" ", m.group("name")))
            name = clean(re.sub(r"^(?:the\s+)?", "", name, flags=re.I))
            if 6 <= len(name) <= 140 and i not in seen:
                seen.add(i)
                hits.append((i, "%s, %s" % (name, m.group("year"))))
    return hits


def restart_pages(pages):
    out, high = [], 0
    for i, text in enumerate(pages, 1):
        nums = [int(n) for n in ANCHOR.findall(text)]
        if not nums:
            continue
        if high >= 5 and min(nums) == 1:
            out.append(i)
            high = max(nums)
        else:
            high = max(high, max(nums))
    return out


def toc_entries(pages):
    """[(ordinal, title, printed_page)] including rows wrapped over two+ lines."""
    rows, pending = [], None
    for text in pages[:max(4, len(pages) // 8)]:
        for line in text.splitlines():
            if not line.strip():
                continue
            opened = TOC_OPEN.match(line)
            if opened:
                pending = (int(opened.group("n")), opened.group("head"))
            elif pending:
                # a continuation line of the row we are already building
                pending = (pending[0], pending[1] + " " + line.strip())
            if pending is None:
                continue
            closed = TOC_CLOSE.match(pending[1])
            if closed:
                title = clean(closed.group("tail"))
                if title:
                    rows.append((pending[0], title, int(closed.group("page"))))
                pending = None
    # a contents list is ordered and starts at 1; anything else is a false read
    return rows if len(rows) >= 2 and rows[0][0] == 1 else []


def derive_offset(toc, clauses):
    """PDF page minus printed page, from rows both signals agree on."""
    diffs = []
    for _, title, printed in toc:
        tt = tokens(title)
        if not tt:
            continue
        for pdf_page, name in clauses:
            overlap = tt & tokens(name)
            if len(overlap) >= 2 and abs(pdf_page - printed) < 40:
                diffs.append(pdf_page - printed)
                break
    if not diffs:
        return 0, False
    diffs.sort()
    return diffs[len(diffs) // 2], True


def reconcile(pages, toc, clauses, restarts):
    """Union both signals into ordered segments over the whole document."""
    offset, offset_known = derive_offset(toc, clauses)
    npages = len(pages)

    segments = []   # (pdf_page, title, source, is_instrument)
    for _, title, printed in toc:
        segments.append([printed + offset, title, "toc", bool(INSTRUMENT_TITLE.search(title))])

    for pdf_page, name in clauses:
        # Merge on TITLE, not mere proximity. Two contents rows can sit a page
        # apart ("Introduction" p.5, the Election Rules p.6), and merging on
        # distance alone attaches the clause to whichever came first — which
        # promoted "Introduction" to an instrument and inverted both ranges.
        nt = tokens(name)
        best, best_overlap = None, 0
        for s in segments:
            if abs(s[0] - pdf_page) > MERGE_WINDOW:
                continue
            overlap = len(nt & tokens(s[1]))
            if overlap > best_overlap:
                best, best_overlap = s, overlap
        if best is not None and best_overlap >= 1:
            best[2] = "toc+clause"          # contents keeps the clean title
            best[0] = pdf_page              # clause keeps the exact page
            best[3] = True
        else:
            segments.append([pdf_page, name, "clause", True])

    segments.sort(key=lambda s: s[0])
    out = []
    for i, (start, title, source, is_inst) in enumerate(segments):
        end = segments[i + 1][0] - 1 if i + 1 < len(segments) else npages
        if end - start + 1 < MIN_INSTRUMENT_PAGES and source == "clause":
            continue                        # a quotation, not an instrument
        corroborated = any(abs(start - r) <= CORROBORATION_WINDOW for r in restarts)
        out.append({"start": start, "end": end, "title": title, "source": source,
                    "instrument": is_inst, "restart": corroborated})
    return out, offset, offset_known


def analyse(pdf):
    pages = pages_of(pdf)
    if pages is None:
        return {"file": os.path.basename(pdf), "verdict": "UNREADABLE",
                "segments": [], "notes": [], "pages": 0}

    clauses = clause_starts(pages)
    restarts = restart_pages(pages)
    toc = toc_entries(pages)
    notes = []

    segments, offset, offset_known = reconcile(pages, toc, clauses, restarts)
    instruments = [s for s in segments if s["instrument"]]

    if len(instruments) >= 2:
        verdict = "CONTAINER (%d instruments)" % len(instruments)
    else:
        verdict = "single"

    if toc:
        notes.append("contents list: %d rows; page offset %+d %s"
                     % (len(toc), offset,
                        "(derived)" if offset_known else "(NOT derived - assumed 0)"))
    only_toc = [s for s in instruments if s["source"] == "toc"] if toc else []
    # "absent from the contents list" says nothing when there is no such list.
    only_clause = [s for s in instruments if s["source"] == "clause"] if toc else []
    if only_toc:
        notes.append("%d instrument(s) from the contents list with no short-title "
                     "clause found - pages are the contents' own, verify them"
                     % len(only_toc))
    if only_clause:
        notes.append("%d instrument(s) found in the text but ABSENT from the "
                     "contents list - the book's own index is incomplete"
                     % len(only_clause))
    if not toc and len(instruments) >= 2:
        notes.append("no contents list; names come from short-title clauses and "
                     "may carry marginal catchline text")
    # Every page must end up somewhere. Unassigned pages are how content
    # disappears silently, which is the whole failure class this guards against.
    # Only meaningful on a container: a single instrument is never cut, so its
    # front matter is carried along rather than dropped.
    if segments and len(instruments) >= 2:
        lead = segments[0]["start"] - 1
        if lead > 0:
            notes.append("pp.1-%d assigned to nothing (cover / contents / front "
                         "matter) - confirm they are meant to be dropped" % lead)
        holes = [(segments[i]["end"] + 1, segments[i + 1]["start"] - 1)
                 for i in range(len(segments) - 1)
                 if segments[i + 1]["start"] > segments[i]["end"] + 1]
        for a, b in holes:
            notes.append("pp.%d-%d fall between two instruments - UNASSIGNED" % (a, b))
    return {"file": os.path.basename(pdf), "verdict": verdict, "pages": len(pages),
            "segments": segments, "notes": notes}


MARK = {"toc+clause": "ok", "clause": "??", "toc": "? "}


def report(r, verbose):
    flag = "  <== CONTAINER" if r["verdict"].startswith("CONTAINER") else ""
    print("%-56s %-28s%s" % (r["file"][:54], r["verdict"], flag))
    if r["verdict"].startswith("CONTAINER") or verbose:
        for s in r["segments"]:
            kind = "" if s["instrument"] else "   [not an instrument]"
            print("      %s pp.%-4s-%-4s %-62s %s%s"
                  % (MARK[s["source"]], s["start"], s["end"], s["title"][:62],
                     "restart" if s["restart"] else "", kind))
    for n in r["notes"]:
        print("      - %s" % n)
    print()


def main(argv):
    verbose = "--verbose" in argv
    as_plan = "--plan" in argv
    argv = [a for a in argv if a not in ("--verbose", "--plan")]
    if as_plan:
        # Machine-readable split plan for one document: instruments only, in the
        # shape POST /api/admin/sources/:id/split expects. Front matter and
        # closing schedules are excluded, which is what leaves pages unassigned.
        import json
        r = analyse(argv[0])
        json.dump(
            {"pieces": [{"start": s["start"], "end": s["end"], "title": s["title"]}
                        for s in r["segments"] if s["instrument"]]},
            sys.stdout, indent=1)
        print()
        return
    if argv and argv[0] == "--dir":
        files = sorted(os.path.join(argv[1], f) for f in os.listdir(argv[1])
                       if f.lower().endswith(".pdf"))
    else:
        files = argv

    containers = 0
    for pdf in files:
        r = analyse(pdf)
        containers += r["verdict"].startswith("CONTAINER")
        report(r, verbose)
    print("%d of %d documents are containers." % (containers, len(files)))
    print("Legend: ok = contents list and short-title clause agree | "
          "?? = clause only, not in contents | ?  = contents only, no clause found")


if __name__ == "__main__":
    main(sys.argv[1:])
