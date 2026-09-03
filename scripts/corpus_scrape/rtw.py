"""Harvester for rtw.sindh.gov.pk — the Right to Information knowledge repository.

⚠️ The browsable page is a decoy. `?page=N` works on it, but the search filters
on that URL are SILENTLY IGNORED — passing `provincial=provincial` returns byte
-identical results to passing nothing, which reads as "the filter does nothing"
rather than "you are talking to the wrong endpoint". The page's own JavaScript
calls a different URL, and that one honours the filters:

    GET /pagination/fetch_data?page=N&sortby=&sorttype=&query=&<filters>

returning bare <tr> rows. ⚠️ The checkbox filters send `Yes`, not their own name.

Row columns, in order:
    record_no | title | department | source | type | year | link
"""

from __future__ import annotations

import re

from .common import Doc, clean, get, session

ENDPOINT = "https://rtw.sindh.gov.pk/pagination/fetch_data"

# Types worth having in a legal corpus. The repository also carries thousands of
# federal audit reports, tender notices and annual accounts — indexing those
# would bury the statutory material we actually answer from.
LEGAL_TYPES = (
    "ACT",
    "ORDINANCE",
    "RULES",
    "REGULATIONS",
    "NOTIFICATION",
    "CIRCULAR",
    "POLICY",
    "AMENDMENT",
    "BILL",
    "GAZETTE",
    "MANUAL",
    "GUIDELINES",
    "ORDER",
    "SRO",
)

DOC_EXT_RE = re.compile(r"\.(pdf|docx?|xlsx?|pptx?)(\?|$)", re.I)


def _parse_rows(html_text: str, doc_type_hint: str) -> list[Doc]:
    out: list[Doc] = []
    for row in re.findall(r"<tr[^>]*>(.*?)</tr>", html_text, re.S):
        cells = re.findall(r"<td[^>]*>(.*?)</td>", row, re.S)
        if len(cells) < 7:
            continue
        record_no, title, dept, source, dtype, year = (clean(c) for c in cells[:6])
        if not title:
            continue
        m = re.search(r'href="([^"]+)"', cells[6])
        if not m:
            continue  # catalogued but no file attached — nothing to fetch
        url = m.group(1).strip()
        if not DOC_EXT_RE.search(url):
            continue  # landing page rather than a document
        out.append(
            Doc(
                site="rtw",
                doc_id=record_no or url.rsplit("/", 1)[-1][:40],
                title=title,
                source_url=url,
                doc_type=dtype or doc_type_hint,
                department=dept,
                as_of=year,
                extra={"source_org": source},
            )
        )
    return out


def enumerate_filter(
    doc_type: str = "", provincial: bool = True, max_pages: int = 400
) -> list[Doc]:
    """Walk one filter combination until the rows run out."""
    s = session()
    s.headers["X-Requested-With"] = "XMLHttpRequest"
    docs: list[Doc] = []
    seen_first: set[str] = set()

    for page in range(1, max_pages + 1):
        params = {"page": page, "sortby": "id", "sorttype": "asc", "query": ""}
        if provincial:
            params["provincial"] = "Yes"  # ⚠️ 'Yes', not 'provincial'
        if doc_type:
            params["type"] = doc_type
        r = get(s, ENDPOINT, params=params)
        rows = _parse_rows(r.text, doc_type)

        # An exhausted filter keeps returning 200 with an empty tbody.
        if not re.search(r"<tr[^>]*>", r.text):
            break

        # Some filters loop back to page 1 instead of ending. Fingerprint to stop.
        fp = "|".join(d.doc_id for d in rows)
        if fp and fp in seen_first:
            break
        seen_first.add(fp)

        docs.extend(rows)
        if page % 10 == 0 or not rows:
            print(f"  rtw[{doc_type or 'ALL'}]: page {page:3d}, total {len(docs)}")
        if not rows:
            break

    print(f"  rtw[{doc_type or 'ALL'}]: {len(docs)} documents")
    return docs


def enumerate_all(provincial: bool = True) -> list[Doc]:
    """Every legal type, deduped on source_url (one file is listed under many
    departments, and the same instrument recurs across types)."""
    seen: set[str] = set()
    docs: list[Doc] = []
    for t in LEGAL_TYPES:
        print(f"[rtw] type={t}")
        for d in enumerate_filter(t, provincial=provincial):
            if d.source_url in seen:
                continue
            seen.add(d.source_url)
            docs.append(d)
    return docs
