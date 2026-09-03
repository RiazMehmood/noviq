#!/usr/bin/env python3
"""Echo-AI ingestion sidecar — indexing-time, deterministic, NO LLM.

Phase 9a capability: ``extract_tables`` via **pdfplumber** (MIT). On digital
PDFs with real ruled/grid tables pdfplumber returns correct *cells*, where
``pdftotext -layout`` returns column-interleaved wrapped text the Rust
projection engine has to reverse-engineer (and sometimes scrambles). The spike
(``~/Desktop/table_ingestion_test/spike_results/``) proved pdfplumber == PyMuPDF
in quality with a clean MIT license.

Process model (v1): **one-shot CLI**. Read a single JSON request on stdin,
write a single JSON response on stdout, exit. The Rust pipeline owns gating,
discovery, timeouts and fallback (a missing/failing sidecar simply falls back to
the projection engine — never fails the index).

Protocol
--------
Request  (stdin, one JSON object):
    {"op": "extract_tables", "pdf_path": "/abs/path.pdf", "pages": "all" | [1,2,...]}

Response (stdout, one JSON object):
    {"ok": true, "pages": {
        "8": {
            "above": "heading / prose above the table region (verbatim text)",
            "below": "prose below the table region",
            "tables": [
                {"bbox": [x0, top, x1, bottom],
                 "header": ["Sr.#", "Nature", "To whom", "Extent"],
                 "rows":   [["1.", "Creation of Post", "(i) ...", "Full Powers"], ...]}
            ]
        }
    }}
    {"ok": false, "error": "..."}

Invariants
----------
* **Verbatim cells (G1):** every cell string is pdfplumber's own text for that
  cell (only inter-cell joining downstream is synthetic). Internal whitespace in
  a wrapped cell is collapsed to single spaces; no characters are invented.
* Pages are 1-based, matching the ``pdftotext -layout`` form-feed page split so
  the Rust side can splice page-for-page.
* ``above`` / ``below`` are the page's text *outside* the table band, so the
  Rust side can rebuild ``prose-above + linearized-rows + prose-below``.
"""

import collections
import json
import os
import re
import statistics
import sys


# ── superscript amendment-footnote markers ───────────────────────────────────
# A statute reprint marks amended words with a superscript digit:
#
#     ¹54. (1) The President may, from time to time, summon either House…
#
# `extract_text(layout=True)` places characters by position, so that marker lands
# flush against the provision number and the page emits `154.` — Article 54 read
# as Article 154. Downstream every clause of the real Articles 54-63 inherited
# that label, which is how Article 63(1)(e) came to be stored as Article 154(e)
# (trace a72f1539, 2026-07-27).
#
# The markers are separable in the character stream. Measured on the Constitution
# (amended to the 27th), page 51:
#
#     '1'  size= 5.50  top=238.57  bottom=244.07   <- marker
#     '('  size=11.00  top=240.33  bottom=251.33   <- body
#
# Half the body size, baseline ~7pt higher. Three conditions, all required:
#
#   1. the character is a DIGIT. Without this the rule also eats the marginal
#      catchlines ("Summoning and prorogation…"), which are set smaller and
#      slightly off-baseline — trading one defect for a worse one.
#   2. it is <= SUPERSCRIPT_SIZE_RATIO of the dominant size ON ITS OWN LINE.
#      Per-line matters: the page-foot footnote block is uniformly ~8pt, so
#      judged against its own line nothing there is an outlier and it survives
#      intact (verified: 0 of 709 characters dropped).
#   3. its baseline is raised at least SUPERSCRIPT_RAISE_MIN above that line's.
#
# Across all 252 pages this removes 1,483 characters — 0.04% of the document.
SUPERSCRIPT_SIZE_RATIO = 0.80
SUPERSCRIPT_RAISE_MIN = 1.0
# Escape hatch: ECHO_KEEP_SUPERSCRIPTS=1 restores the pre-fix extraction.
_KEEP_SUPERSCRIPTS = os.environ.get("ECHO_KEEP_SUPERSCRIPTS") == "1"


def _superscript_keys(page):
    """Identity keys of superscript footnote digits on ``page``.

    Keyed by (x0, top, text) rather than object identity because ``page.filter``
    re-materialises objects. Returns an empty set on any failure — losing the
    fix is acceptable, failing the extraction is not.
    """
    if _KEEP_SUPERSCRIPTS:
        return set()
    keys = set()
    try:
        lines = page.extract_text_lines(return_chars=True)
    except Exception:
        return keys  # older pdfplumber, or a page it cannot line-group
    for line in lines:
        chars = line.get("chars") or []
        if len(chars) < 2:
            continue
        modal_size = collections.Counter(
            round(c["size"], 1) for c in chars).most_common(1)[0][0]
        modal_bottom = collections.Counter(
            round(c["bottom"], 1) for c in chars).most_common(1)[0][0]
        if not modal_size:
            continue
        for c in chars:
            if not c["text"].isdigit():
                continue
            if (c["size"] <= modal_size * SUPERSCRIPT_SIZE_RATIO
                    and c["bottom"] < modal_bottom - SUPERSCRIPT_RAISE_MIN):
                keys.add((round(c["x0"], 2), round(c["top"], 2), c["text"]))
    return keys


def _page_without_superscripts(page):
    """``page`` with superscript footnote digits filtered out, or ``page``
    unchanged when there are none (or filtering is unavailable)."""
    keys = _superscript_keys(page)
    if not keys:
        return page
    try:
        return page.filter(
            lambda o: o.get("object_type") != "char"
            or (round(o["x0"], 2), round(o["top"], 2), o["text"]) not in keys
        )
    except Exception:
        return page


# ── marginal catchline column ────────────────────────────────────────────────
# A statute reprint prints a marginal catchline (the provision's short title) in
# the OUTER margin, beside the provision it names. `extract_text(layout=True)`
# preserves x-position, so it flattens the two columns onto one line. On a RECTO
# page the catchline lands to the right and is harmless:
#
#     54. (1) The President may, from time to time, summon either  Summoning
#
# but on a VERSO page it lands to the LEFT and displaces the provision number off
# the line start, where no heading regex can anchor it:
#
#     Qualifications [62. (1) A person shall not be qualified to be elected
#
# That single mechanism hid 10 of the 31 Articles in Constitution Part III alone.
# Measured on the Constitution amended to the 27th: 19/31 Articles 50-80 anchored
# before, 28/31 after (the remaining 3 were a separate heading-pattern gap).
#
# The columns are separated by a real whitespace gutter, so they can be split on
# geometry. The DECISION is document-level, never per-page: page 61 of that same
# document has no detectable gutter at all, and a wrong crop silently deletes
# text — the worst possible failure for a legal corpus.
MIN_GUTTER_PT = 8.0          # narrower than this is inter-word space, not a gutter
GUTTER_OCC_TOLERANCE = 1     # a stray word may bridge a real gutter (page 61)
MAX_MARGIN_FRAC = 0.30       # wider than this is a genuine second text column
GUTTER_EDGE_FRAC = 0.05      # the blank outer page margins are not the gutter
MIN_PAGE_SUPPORT = 0.50      # below this, treat the document as single-column
COLUMN_SAMPLE_PAGES = 40     # consensus needs a sample, not every page
# Above this share of sampled pages losing a provision number to the crop, the
# "margin" is a hanging indent (the number column itself) and must never be cut.
# Measured 2026-07-30: Constitution 0%, SLGA Compendium 7%, Delegation 70%.
MAX_ANCHOR_LOSS_FRAC = 0.25
# Escape hatch: ECHO_KEEP_MARGIN_COLUMN=1 restores the pre-fix extraction.
_KEEP_MARGIN_COLUMN = os.environ.get("ECHO_KEEP_MARGIN_COLUMN") == "1"


def _inner_gutters(page):
    """Whitespace runs strictly inside ``page``'s text area, widest first."""
    width = int(page.width)
    if width <= 0:
        return []
    occ = [0] * (width + 1)
    try:
        words = page.extract_words()
    except Exception:
        return []
    for w in words:
        for x in range(max(int(w["x0"]), 0), min(int(w["x1"]) + 1, width)):
            occ[x] += 1
    runs, start = [], None
    for x in range(width):
        if occ[x] <= GUTTER_OCC_TOLERANCE:
            if start is None:
                start = x
        elif start is not None:
            runs.append((start, x))
            start = None
    if start is not None:
        runs.append((start, width))
    lo, hi = page.width * GUTTER_EDGE_FRAC, page.width * (1 - GUTTER_EDGE_FRAC)
    runs = [r for r in runs
            if r[0] > lo and r[1] < hi and (r[1] - r[0]) >= MIN_GUTTER_PT]
    return sorted(runs, key=lambda r: r[1] - r[0], reverse=True)


def _page_split(page):
    """``(side, gutter)`` for ``page``, or None. ``side`` is where the MARGIN sits.

    The narrower side of the widest qualifying gutter is the margin column; it
    must also be narrow in absolute terms, so a genuine two-column layout (two
    columns of similar width) is never mistaken for a catchline margin.
    """
    for g in _inner_gutters(page):
        left_w, right_w = g[0], page.width - g[1]
        if left_w < right_w and left_w <= page.width * MAX_MARGIN_FRAC:
            return "left", g
        if right_w < left_w and right_w <= page.width * MAX_MARGIN_FRAC:
            return "right", g
    return None


def _column_consensus(pages):
    """``(geometry, may_crop)`` for the document's catchline margin.

    ``geometry`` is ``{side: (gutter_x0, gutter_x1)}`` built from the MEDIAN
    gutter per side over an evenly-spaced sample, or None when a majority of
    sampled pages do not agree a margin column exists — absent that, the document
    is treated as single-column and extraction is left exactly as it is today.

    ``may_crop`` says whether the BODY may be narrowed to exclude that margin.
    The two are separate because they carry different risk: cropping deletes
    text and can decapitate provisions, whereas pairing a catchline to its
    provision only reads a column that is already there.
    """
    if _KEEP_MARGIN_COLUMN or not pages:
        return None, False
    step = max(1, len(pages) // COLUMN_SAMPLE_PAGES)
    sample = pages[::step]
    seen = collections.defaultdict(list)
    for p in sample:
        try:
            split = _page_split(p)
        except Exception:
            continue
        if split:
            seen[split[0]].append(split[1])
    support = sum(len(v) for v in seen.values()) / len(sample)
    if support < MIN_PAGE_SUPPORT:
        return None, False
    consensus = {
        side: (statistics.median(g[0] for g in gs),
               statistics.median(g[1] for g in gs))
        for side, gs in seen.items()
    }
    # How OFTEN cropping would cost a provision number tells us WHICH layout this
    # is, and the two layouts are far apart — measured 2026-07-30: Constitution 0%
    # of sampled pages, SLGA Compendium 7%, Delegation of Financial Powers 70%.
    # A genuine catchline margin fails on almost no page; a hanging indent, where
    # the "margin" is the provision-number column itself, fails on most.
    loss = _anchor_loss_rate(sample, consensus)
    if loss > MAX_ANCHOR_LOSS_FRAC:
        sys.stderr.write(
            "catchline margin rejected: cropping loses provision numbers on %.0f%% "
            "of sampled pages - treating as single column\n" % (100 * loss))
        return None, False
    # Below the threshold the margin is real, but individual pages can still
    # differ (the Compendium's table of contents is set full-width). Verify each
    # page before cropping it — but only when the sample saw any loss at all, so
    # a uniformly clean document pays nothing.
    return consensus, loss > 0.0


# A provision heading at the start of a line — the thing the whole crop exists to
# protect. Deliberately looser than the Rust heading patterns: this is a safety
# check, so it should over-count rather than miss.
_ANCHOR_RE = re.compile(r"(?m)^[ \t]*\[?[ \t]*\d{1,3}[A-Z]?\.")


def _crop_keeps_anchors(page, crop):
    """True when cropping THIS page keeps every line-initial provision number.

    The per-page form of [`_anchor_loss_rate`], used at extraction time on a
    document the sample already proved has a real catchline margin. It exists for
    the odd page set full-width inside such a document — the SLGA Compendium's
    table of contents, where the crop would have taken all 13 instrument entries.
    """
    try:
        full = len(_ANCHOR_RE.findall(page.extract_text(layout=True) or ""))
        if not full:
            return True  # no provision number on this page, so none can be lost
        kept = len(_ANCHOR_RE.findall(page.crop(crop).extract_text(layout=True) or ""))
    except Exception:
        return False  # cannot verify => do not crop
    return kept >= full


def _anchor_loss_rate(sample, consensus):
    """Fraction of sampled pages that would LOSE a provision number to the crop.

    Geometry alone cannot distinguish a catchline margin from a HANGING INDENT,
    where the provision number itself sits left of the body text and a gutter
    separates the two. Cropping such a document decapitates every provision —
    measured on `01_Sindh_Rules_of_Business_1986` (60 anchors -> 33) and
    `09_Sindh_Delegation_of_Financial_Powers_2019` (37 -> 30).

    So rather than trust a proxy, measure the effect directly, page by page and
    NEVER summed. 2026-07-30, SLGA Compendium: an aggregate form of this check
    approved a crop that decapitated the table of contents (13 anchors -> 0)
    because nine body pages each "gained" 2-3 anchors, totalling 52 -> 57. Those
    gains are an artefact, not a win — removing a column re-flows
    ``layout=True`` so numbers that sat mid-line become line-initial and
    `_ANCHOR_RE` counts them. Summing lets fabricated anchors on one page pay for
    real provisions destroyed on another.

    Returns a RATE rather than a verdict because the rate is what identifies the
    layout: near zero means a real catchline margin (with, at most, an odd
    full-width page), high means a hanging indent.
    """
    checked = losing = 0
    for page in sample:
        try:
            box = _body_box(page, consensus)
            if box is None:
                continue
            full = len(_ANCHOR_RE.findall(page.extract_text(layout=True) or ""))
            cropped = len(
                _ANCHOR_RE.findall(page.crop(box[1]).extract_text(layout=True) or ""))
        except Exception:
            return 1.0  # cannot verify => treat as total loss, so we never crop
        checked += 1
        if cropped < full:
            losing += 1
    return losing / checked if checked else 0.0


def _body_box(page, consensus):
    """``(side, crop_box)`` for ``page``'s body column, or None to use it whole.

    Prefers this page's own gutter; falls back to the consensus for the page's
    parity, because catchlines alternate margins with recto/verso and a page
    whose gutter is bridged still belongs to that alternation.
    """
    if not consensus:
        return None
    split = None
    try:
        split = _page_split(page)
    except Exception:
        split = None
    if split and split[0] in consensus:
        side, gutter = split
    else:
        side = "left" if page.page_number % 2 == 0 else "right"
        if side not in consensus:
            side = next(iter(consensus))
        gutter = consensus[side]
    if side == "left":
        return side, (gutter[1], 0, page.width, page.height)
    return side, (0, 0, gutter[0], page.height)


# A provision opening a body line, for matching a catchline to its provision.
_PROVISION_RE = re.compile(r"^\[?\s*(\d{1,3}[A-Z]?)\.")
# A catchline may start slightly above the provision line it names.
_CATCHLINE_TOP_TOLERANCE = 6.0


def _provision_titles(page, side, gutter):
    """Map ``{provision_number: title}`` from the marginal catchline column.

    The catchline IS the provision's short title — "Summoning and prorogation of
    Majlis-e-Shoora" beside Article 54 — which makes a far more useful citation
    than a bare number. It is typeset in the outer margin beside the provision it
    names, so the pairing is purely vertical.

    Doing this HERE rather than in Rust is deliberate: the match needs the
    y-coordinates of both columns, and this is the only place that has them.
    Downstream receives a plain number→title map and needs no geometry at all.

    A title usually wraps across several margin lines ("The Republic" / "and its"
    / "territories."), so every catchline falling within a provision's vertical
    span is joined, not just the first.
    """
    body_box = ((gutter[1], 0, page.width, page.height) if side == "left"
                else (0, 0, gutter[0], page.height))
    margin_box = ((0, 0, gutter[0], page.height) if side == "left"
                  else (gutter[1], 0, page.width, page.height))
    try:
        body_lines = page.crop(body_box).extract_text_lines()
        margin_lines = page.crop(margin_box).extract_text_lines()
    except Exception:
        return {}
    if not margin_lines:
        return {}

    # Where each provision starts, in page coordinates.
    starts = []
    for ln in body_lines:
        text = (ln.get("text") or "").strip()
        m = _PROVISION_RE.match(text)
        if m:
            starts.append((float(ln.get("top", 0.0)), m.group(1)))
    if not starts:
        return {}
    starts.sort()

    titles = {}
    for i, (top, num) in enumerate(starts):
        end = starts[i + 1][0] if i + 1 < len(starts) else float(page.height)
        parts = []
        for ml in margin_lines:
            mt = float(ml.get("top", 0.0))
            if top - _CATCHLINE_TOP_TOLERANCE <= mt < end:
                frag = " ".join((ml.get("text") or "").split())
                if frag:
                    parts.append(frag)
        # A hyphen at a wrap is part of the word ("Majlis-e-" + "Shoora"), so it
        # must not become "Majlis-e- Shoora".
        title = ""
        for frag in parts:
            if title and not title.endswith("-"):
                title += " "
            title += frag
        title = title.strip()
        # A trailing full stop reads as a sentence; provision titles are labels.
        title = title.rstrip(".").strip()
        if title:
            titles[num] = title
    return titles


def _clean_cell(value):
    """Collapse a (possibly multi-line, wrapped) pdfplumber cell to one verbatim
    line: strip, then squeeze runs of whitespace to single spaces. None → ''."""
    if value is None:
        return ""
    # pdfplumber joins wrapped lines inside a cell with '\n'; flatten to spaces.
    return " ".join(str(value).split())


def _clean_text(value):
    """Normalize a block of extracted prose: drop empty lines, keep line order."""
    if not value:
        return ""
    lines = [ln.rstrip() for ln in value.splitlines()]
    return "\n".join(ln for ln in lines if ln.strip())


def _extract_tables_for_page(page):
    """Return ``{"above","below","tables"}`` for one pdfplumber page, or ``None``
    when the page has no machine-detectable table (the common prose case)."""
    found = page.find_tables()
    if not found:
        return None

    tables = []
    tops = []
    bottoms = []
    for t in found:
        rows = t.extract()  # list[list[str|None]]
        cleaned = [[_clean_cell(c) for c in row] for row in rows]
        # Drop fully-empty rows (pdfplumber sometimes emits ruling-only rows).
        cleaned = [r for r in cleaned if any(c for c in r)]
        if not cleaned:
            continue
        header = cleaned[0]
        body = cleaned[1:]
        if not body:
            # A header with no data rows isn't a useful table unit; skip it so the
            # projection engine / prose path keeps the page.
            continue
        x0, top, x1, bottom = t.bbox
        tops.append(top)
        bottoms.append(bottom)
        tables.append({
            "bbox": [x0, top, x1, bottom],
            "header": header,
            "rows": body,
        })

    if not tables:
        return None

    # Prose above the first table top and below the last table bottom. Crop in
    # page coords (origin top-left); guard against zero-height crops.
    band_top = min(tops)
    band_bottom = max(bottoms)
    above = ""
    below = ""
    try:
        if band_top > 1:
            above = _clean_text(page.crop((0, 0, page.width, band_top)).extract_text())
        if band_bottom < page.height - 1:
            below = _clean_text(
                page.crop((0, band_bottom, page.width, page.height)).extract_text()
            )
    except Exception:
        # Cropping is best-effort; missing prose just means a tighter page.
        above = above or ""
        below = below or ""

    return {"above": above, "below": below, "tables": tables}


def _op_extract_tables(req):
    import pdfplumber  # imported lazily so a bad request fails fast & cheap

    pdf_path = req.get("pdf_path")
    if not pdf_path:
        raise ValueError("extract_tables requires 'pdf_path'")
    wanted = req.get("pages", "all")
    want_all = wanted == "all"
    want_set = None if want_all else {int(p) for p in wanted}

    out = {}
    with pdfplumber.open(pdf_path) as pdf:
        for page in pdf.pages:
            pno = page.page_number  # 1-based
            if not want_all and pno not in want_set:
                continue
            try:
                result = _extract_tables_for_page(page)
            except Exception as exc:  # one bad page must not sink the document
                sys.stderr.write(f"page {pno} table extraction failed: {exc}\n")
                result = None
            if result is not None:
                out[str(pno)] = result
    return {"ok": True, "pages": out}


def _op_extract_document(req):
    """Phase 9c: **pdfplumber as the PRIMARY text+table extractor** (not just
    tables). Returns, per 1-based page, the full layout-preserving page text plus
    the same structured tables ``extract_tables`` emits. The Rust side uses
    ``text`` as the document layout (form-feed-joined, like ``pdftotext -layout``)
    and the ``above/below/tables`` band for the proven sidecar reflow, then
    quality-gates the result — falling back to ``pdftotext`` when pdfplumber is
    unavailable or its text doesn't clear the gate.

    ``extract_text(layout=True)`` preserves spatial columns (matching
    ``pdftotext -layout``); on the rare page where it throws we fall back to the
    plain flow extractor so one page never sinks the document."""
    import pdfplumber  # lazy import — keeps a bad/cheap request from paying for it

    pdf_path = req.get("pdf_path")
    if not pdf_path:
        raise ValueError("extract_document requires 'pdf_path'")

    out = {}
    with pdfplumber.open(pdf_path) as pdf:
        # Decided once for the whole document — a single page is not enough
        # evidence to justify cropping text away. None => single-column, and the
        # extraction below is byte-for-byte what it was before this fix.
        consensus, verify_pages = _column_consensus(pdf.pages)
        if consensus:
            sys.stderr.write(
                f"catchline margin detected: "
                + ", ".join(f"{s} gutter x={g[0]:.0f}-{g[1]:.0f}"
                            for s, g in consensus.items()) + "\n")
        for page in pdf.pages:
            pno = page.page_number  # 1-based
            # Drop superscript amendment-footnote digits BEFORE layout extraction,
            # or they fuse into the following provision number (`54.` -> `154.`).
            # Text only: table extraction below keeps the unfiltered page, since a
            # cell's verbatim content is the G1 invariant.
            text_page = _page_without_superscripts(page)
            # Then crop to the body column, so a marginal catchline cannot sit
            # between the line start and the provision number.
            titles = {}
            box = _body_box(page, consensus)
            if box is not None:
                side, crop = box
                try:
                    # Match titles against the SUPERSCRIPT-FILTERED page: on the
                    # raw page Article 54 still reads `154.`, so every title on a
                    # page carrying an amendment marker would be filed under the
                    # wrong provision — the very defect this release fixes.
                    titles = _provision_titles(text_page, side, consensus[side])
                    # Cropping DELETES text, so it is gated once more here: on a
                    # document that had any losing page in the sample, verify this
                    # page too. A full-width page inside a catchline document
                    # (the Compendium's contents) keeps its whole width.
                    if verify_pages and not _crop_keeps_anchors(text_page, crop):
                        sys.stderr.write(
                            f"page {pno}: margin is load-bearing here, keeping "
                            f"the whole page\n")
                    else:
                        text_page = text_page.crop(crop)
                except Exception as exc:  # keep the whole page rather than lose it
                    sys.stderr.write(f"page {pno} column crop failed: {exc}\n")
            try:
                text = text_page.extract_text(layout=True) or ""
            except Exception:
                try:
                    text = text_page.extract_text() or ""
                except Exception as exc:
                    sys.stderr.write(f"page {pno} text extraction failed: {exc}\n")
                    text = ""
            entry = {"text": text, "above": "", "below": "", "tables": [],
                     "provision_titles": titles}
            try:
                tbl = _extract_tables_for_page(page)
                if tbl is not None:
                    entry["above"] = tbl["above"]
                    entry["below"] = tbl["below"]
                    entry["tables"] = tbl["tables"]
            except Exception as exc:  # one bad page must not sink the document
                sys.stderr.write(f"page {pno} table extraction failed: {exc}\n")
            out[str(pno)] = entry
    return {"ok": True, "pages": out}


def _ocr_backend():
    """Which PP-OCR runtime to use. `ECHO_OCR_BACKEND` forces a choice
    (`rapidocr` | `paddleocr`); otherwise prefer RapidOCR when importable.

    Both run the SAME PaddleOCR (PP-OCR) detection/recognition models; they differ
    only in the inference runtime. RapidOCR executes them on **ONNX Runtime**,
    which is what makes OCR possible on hosts whose CPU lacks AVX — importing
    `paddle` there raises SIGILL, which **kills the process outright** (no Python
    exception, so it cannot be caught). That is exactly the deploy VM (vCPU masked
    to Core2: `sse4_2`, no AVX/FMA), where the paddle wheel — including the
    official `noavx` build — crashes on the first matmul. Never probe `paddle`
    first on such a host: the import itself is fatal.
    """
    forced = (os.environ.get("ECHO_OCR_BACKEND") or "").strip().lower()
    if forced in ("rapidocr", "paddleocr"):
        return forced
    try:
        import rapidocr_onnxruntime  # noqa: F401
        return "rapidocr"
    except Exception:
        return "paddleocr"


def _ocr_gpu_enabled():
    """Should PP-OCR run on CUDA? OFF unless `ECHO_OCR_GPU=1`.

    Deliberately opt-in, and in practice only set while the server holds the GPU
    for bulk indexing (`gpu-hold.sh`). With vLLM serving chat the card has ~878
    MiB free against the ~1 GB a CUDA context plus these models want, so turning
    this on during normal operation would trade working chat for OOM.
    """
    return (os.environ.get("ECHO_OCR_GPU") or "").strip() == "1"


def _ocr_rapidocr(images, lang):
    """PP-OCR models on ONNX Runtime (no `paddle` runtime → no AVX requirement).

    CUDA is opt-in via `ECHO_OCR_GPU=1` and requires an `onnxruntime-gpu` build in
    this interpreter's environment plus the CUDA libraries on `LD_LIBRARY_PATH`
    (the server passes its own environment down, so under GPU mode the systemd
    drop-in already provides them).

    ⚠️ Construction succeeding proves NOTHING about where the models ran —
    onnxruntime falls back to the CPU provider silently when CUDA cannot
    initialise, which is exactly how an entire "GPU" re-embed once ran on CPU.
    So the providers are read back off the live sessions and reported on stderr.
    """
    from rapidocr_onnxruntime import RapidOCR

    ocr = None
    if _ocr_gpu_enabled():
        try:
            ocr = RapidOCR(det_use_cuda=True, cls_use_cuda=True, rec_use_cuda=True)
            # Each sub-model holds its own session, reached by a different
            # attribute path; a missing one must not sink OCR, so this is
            # reporting only.
            on_cuda = []
            for name, path in (
                ("det", ("text_det", "infer", "session")),
                ("cls", ("text_cls", "infer", "session")),
                ("rec", ("text_rec", "session", "session")),
            ):
                try:
                    obj = ocr
                    for attr in path:
                        obj = getattr(obj, attr)
                    if "CUDAExecutionProvider" in obj.get_providers():
                        on_cuda.append(name)
                except Exception:
                    pass
            sys.stderr.write(
                "rapidocr: CUDA requested; on GPU: %s\n" % (",".join(on_cuda) or "NONE")
            )
        except Exception as exc:
            sys.stderr.write(f"rapidocr: CUDA init failed ({exc}); using CPU\n")
            ocr = None

    if ocr is None:
        ocr = RapidOCR()
    pages = []
    for img in images:
        try:
            result, _elapsed = ocr(img)
            texts = [t for (_box, t, _s) in (result or [])]
            scores = [float(s) for (_box, _t, s) in (result or [])]
            text = "\n".join(t for t in texts if t)
            mean_conf = (sum(scores) / len(scores)) if scores else 0.0
        except Exception as exc:  # one bad page must not sink the document
            sys.stderr.write(f"rapidocr failed on {img}: {exc}\n")
            text, mean_conf = "", 0.0
        pages.append({"text": text, "mean_conf": float(mean_conf)})
    return pages


def _ocr_paddleocr(images, lang):
    """PaddleOCR 3.x on the paddle runtime (GPU/AVX hosts).

    `predict(img)` → list of dict-like results carrying `rec_texts` + `rec_scores`.
    Doc-orientation / unwarping / textline-orientation sub-models are disabled
    (faster, unneeded for these scans) and `enable_mkldnn=False` dodges the
    PaddlePaddle 3.x oneDNN/PIR crash (`ConvertPirAttribute2RuntimeAttribute` —
    `FLAGS_use_mkldnn=0` is NOT honoured by the 3.x PIR executor; the constructor
    kwarg is). GPU is used automatically when a GPU build of paddlepaddle is
    installed; CPU otherwise.
    """
    from paddleocr import PaddleOCR

    ocr = PaddleOCR(
        lang=lang,
        use_doc_orientation_classify=False,
        use_doc_unwarping=False,
        use_textline_orientation=False,
        enable_mkldnn=False,
    )
    pages = []
    for img in images:
        try:
            results = ocr.predict(img)
            texts, scores = [], []
            for r in results:
                texts.extend(r.get("rec_texts", []) or [])
                scores.extend(r.get("rec_scores", []) or [])
            text = "\n".join(t for t in texts if t is not None)
            mean_conf = (sum(scores) / len(scores)) if scores else 0.0
        except Exception as exc:  # one bad page must not sink the document
            sys.stderr.write(f"ocr failed on {img}: {exc}\n")
            text, mean_conf = "", 0.0
        pages.append({"text": text, "mean_conf": float(mean_conf)})
    return pages


def _op_ocr(req):
    """PP-OCR over a list of page images (Rust rasterised them with pdftoppm).
    Returns recognised text + mean confidence per page, in input order."""
    images = req.get("images") or []
    if not images:
        raise ValueError("ocr requires non-empty 'images'")
    lang = req.get("lang", "en")

    backend = _ocr_backend()
    sys.stderr.write(f"ocr backend: {backend}\n")
    pages = _ocr_rapidocr(images, lang) if backend == "rapidocr" else _ocr_paddleocr(images, lang)
    return {"ok": True, "pages": pages, "backend": backend}


def main():
    try:
        raw = sys.stdin.read()
        req = json.loads(raw)
        op = req.get("op")
        if op == "extract_tables":
            resp = _op_extract_tables(req)
        elif op == "extract_document":
            resp = _op_extract_document(req)
        elif op == "ocr":
            resp = _op_ocr(req)
        else:
            resp = {"ok": False, "error": f"unknown op: {op!r}"}
    except Exception as exc:  # any failure → structured error so Rust falls back
        resp = {"ok": False, "error": f"{type(exc).__name__}: {exc}"}
    sys.stdout.write(json.dumps(resp))
    sys.stdout.flush()


if __name__ == "__main__":
    main()
