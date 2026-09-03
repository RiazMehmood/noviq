"""Harvester for sindhlaws.gov.pk — the official Sindh Code.

The site is a classic ASP.NET WebForms GridView: 30 rows a page, and paging is
`__doPostBack` rather than a URL. So we cannot fetch page N directly — we walk
forward, carrying VIEWSTATE from each response into the next POST. That makes
enumeration strictly sequential and worth caching.

Three browse axes exist (Alpha / Chrono / DepartmentCode) over the same rows;
Alpha is used because it is the one verified to render the PDF columns.

Each row carries up to three language editions. The column mapping is by control
id, NOT position — an edition that does not exist still renders an <a> with no
href, so position alone would silently mis-assign languages:

    HyperLink2 -> English      HyperLink1 -> Urdu      HyperLink3 -> Sindhi
"""

from __future__ import annotations

import re

from .common import Doc, clean, get, session

BASE = "https://www.sindhlaws.gov.pk/"
TYPES = ("ACT", "ORDINANCE", "RULE")

# Grab the whole anchor, then look for href INSIDE it. ⚠️ Do not fold the href
# into this pattern as an optional group — a missing edition renders an <a> with
# no href, so an optional group happily matches empty and every link comes back
# blank. That cost one full enumeration run.
ANCHOR_RE = re.compile(r'<a\s[^>]*id="[^"]*(HyperLink[123])_\d+"[^>]*>', re.I)
HREF_RE = re.compile(r'href="([^"]+)"', re.I)
LANG_BY_CONTROL = {"HyperLink2": "english", "HyperLink1": "urdu", "HyperLink3": "sindhi"}
DATE_RE = re.compile(r"[A-Z][a-z]{2}\s+\d{1,2},\s+\d{4}")


def _hidden(html_text: str, name: str) -> str:
    m = re.search(r'id="%s"[^>]*value="([^"]*)"' % re.escape(name), html_text)
    if not m:
        return ""
    # These are base64 blobs with HTML-escaped `+` etc. Unescape or the POST is
    # rejected with a viewstate MAC error.
    import html as _html

    return _html.unescape(m.group(1))


def _rows(page_html: str) -> list[str]:
    start = page_html.find("ContentPlaceHolder1_GridView1")
    if start == -1:
        return []
    grid = page_html[start:]
    end = grid.find("</table>")
    if end != -1:
        grid = grid[:end]
    return re.findall(r"<tr[^>]*>(.*?)</tr>", grid, re.S)


def _parse_rows(rows: list[str], doc_type: str) -> list[Doc]:
    out: list[Doc] = []
    for row in rows:
        cells = re.findall(r"<td[^>]*>(.*?)</td>", row, re.S)
        if len(cells) < 3:
            continue  # header or the pager row
        title = ""
        for c in cells:
            if 'width="55%"' in row and clean(c) and not DATE_RE.fullmatch(clean(c)):
                pass
        # Title is the wide column; fall back to the longest non-date cell.
        m = re.search(r'<td[^>]*width="55%"[^>]*>(.*?)</td>', row, re.S)
        title = clean(m.group(1)) if m else ""
        if not title:
            cands = [clean(c) for c in cells]
            cands = [c for c in cands if c and not DATE_RE.fullmatch(c)]
            title = max(cands, key=len) if cands else ""
        if not title:
            continue

        dm = DATE_RE.search(clean(row))
        as_of = dm.group(0) if dm else ""

        editions: dict[str, str] = {}
        for m_anchor in ANCHOR_RE.finditer(row):
            href = HREF_RE.search(m_anchor.group(0))
            if href:
                editions[LANG_BY_CONTROL[m_anchor.group(1)]] = href.group(1).strip()

        # English is what the corpus indexes; the other editions are recorded so
        # a future Urdu/Sindhi pass does not have to re-crawl the whole site.
        href = editions.get("english")
        if not href:
            continue
        url = href if href.startswith("http") else BASE + href.lstrip("/")
        pub = re.search(r"(PUB-[A-Z0-9-]+)\.pdf", url, re.I)
        out.append(
            Doc(
                site="sindhlaws",
                doc_id=pub.group(1) if pub else url.rsplit("/", 1)[-1][:40],
                title=title,
                source_url=url,
                doc_type=doc_type,
                as_of=as_of,
                extra={k: v for k, v in editions.items() if k != "english"},
            )
        )
    return out


def enumerate_type(doc_type: str, max_pages: int = 200) -> list[Doc]:
    """Walk every GridView page for one instrument type."""
    s = session()
    url = f"{BASE}Alpha.aspx?pg={doc_type}"
    r = get(s, url)
    html_text = r.text

    docs: list[Doc] = []
    seen_pages: set[str] = set()

    for page_no in range(1, max_pages + 1):
        rows = _rows(html_text)
        parsed = _parse_rows(rows, doc_type)

        # Guard against the pager silently returning the same page forever —
        # a real risk with viewstate walks, and an infinite loop otherwise.
        fingerprint = "|".join(d.doc_id for d in parsed)
        if fingerprint and fingerprint in seen_pages:
            print(f"  {doc_type}: page {page_no} repeated — stopping")
            break
        seen_pages.add(fingerprint)

        docs.extend(parsed)
        print(f"  {doc_type}: page {page_no:3d} -> {len(parsed):2d} english pdfs "
              f"({len(rows)} rows), total {len(docs)}")

        # Is there a next page? The pager only renders a window of links, so we
        # ask for page_no+1 specifically rather than trusting the highest link.
        nxt = f"Page${page_no + 1}"
        if nxt not in html_text:
            print(f"  {doc_type}: no link to page {page_no + 1} — done")
            break

        data = {
            "__EVENTTARGET": "ctl00$ContentPlaceHolder1$GridView1",
            "__EVENTARGUMENT": nxt,
            "__VIEWSTATE": _hidden(html_text, "__VIEWSTATE"),
            "__VIEWSTATEGENERATOR": _hidden(html_text, "__VIEWSTATEGENERATOR"),
            "__EVENTVALIDATION": _hidden(html_text, "__EVENTVALIDATION"),
        }
        resp = s.post(url, data=data, timeout=90, headers={"Referer": url})
        resp.raise_for_status()
        html_text = resp.text

    return docs


def enumerate_all() -> list[Doc]:
    docs: list[Doc] = []
    for t in TYPES:
        print(f"[sindhlaws] {t}")
        docs.extend(enumerate_type(t))
    return docs
