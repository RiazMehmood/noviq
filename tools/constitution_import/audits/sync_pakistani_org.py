#!/usr/bin/env python3.14
"""Audit constitution_ir.json against pakistani.org.

For each Part/Chapter page on pakistani.org, extract the article numbers
and titles, then compare to what's present in the IR. Reports:

  - Articles present in source but missing from IR
  - Articles present in IR but not on source page (rare; could be amendments)
  - Articles where the count of clauses differs
  - Articles where the count of sub-clauses differs
  - Top-level structures (preamble, schedules, amendments)

Output: /tmp/ir_sync_report.md
"""
import json, re, sys, urllib.request, time
from bs4 import BeautifulSoup

IR_PATH = "/media/riaz/New Volume/echo-AI/tools/constitution_import/constitute_IR/constitution_ir.json"
BASE = "https://www.pakistani.org/pakistan/constitution/"
REPORT = "/tmp/ir_sync_report.md"
CACHE_DIR = "/tmp/pakorg_cache"

import os
os.makedirs(CACHE_DIR, exist_ok=True)

# Map IR Part numbers (Roman) to URL stems
PART_URLS = {
    "I":    ["part1.html"],
    "II":   ["part2.ch1.html", "part2.ch2.html"],
    "III":  ["part3.ch1.html", "part3.ch2.html", "part3.ch3.html"],
    "IV":   ["part4.ch1.html", "part4.ch2.html", "part4.ch3.html"],
    "V":    ["part5.ch1.html", "part5.ch2.html", "part5.ch3.html"],
    "VI":   ["part6.ch1.html", "part6.ch2.html", "part6.ch3.html"],
    "VII":  ["part7.ch1.html", "part7.ch1A.html", "part7.ch2.html", "part7.ch3.html", "part7.ch3A.html", "part7.ch4.html"],
    "VIII": ["part8.ch1.html", "part8.ch2.html"],
    "IX":   ["part9.html"],
    "X":    ["part10.html"],
    "XI":   ["part11.html"],
    "XII":  ["part12.ch1.html", "part12.ch2.html", "part12.ch3.html", "part12.ch4.html", "part12.ch5.html", "part12.ch6.html", "part12.ch7.html"],
}

def fetch(url_stem: str) -> str:
    cache_path = os.path.join(CACHE_DIR, url_stem.replace("/", "_"))
    if os.path.exists(cache_path):
        return open(cache_path).read()
    req = urllib.request.Request(BASE + url_stem, headers={"User-Agent": "curl/8.0"})
    with urllib.request.urlopen(req, timeout=30) as r:
        body = r.read().decode("utf-8", errors="replace")
    open(cache_path, "w").write(body)
    time.sleep(0.5)  # be nice
    return body

def cleantext(node):
    for sup in node.find_all("sup"):
        sup.decompose()
    txt = node.get_text(separator=" ")
    return re.sub(r"\s+", " ", txt).strip()

def extract_articles_from_html(html: str):
    """Return list of (article_number, marginal_note, clause_count, subclause_count).

    pakistani.org Article rows look like:
      <tr><td>NN</td><td><b>Marginal note.</b><br><table>(clause rows)</table></td></tr>
    """
    soup = BeautifulSoup(html, "html.parser")
    articles = []
    for tr in soup.find_all("tr"):
        tds = tr.find_all("td", recursive=False)
        if len(tds) < 2:
            continue
        num_text = cleantext(tds[0])
        # match "<num>" or "<num>A" etc (just the article number, no clause id)
        m = re.fullmatch(r"(\d+[A-Z]?)\.?", num_text)
        if not m:
            continue
        article_num = m.group(1)
        body_cell = tds[1]
        marginal = ""
        first_b = body_cell.find("b")
        if first_b:
            marginal = cleantext(first_b).rstrip(".").strip()
        # Count clauses + sub-clauses inside this article
        clause_count = 0
        subclause_count = 0
        # The body cell may contain a single <table> of clauses
        for clause_table in body_cell.find_all("table", recursive=False):
            for clause_row in clause_table.find_all("tr", recursive=False):
                ctds = clause_row.find_all("td", recursive=False)
                if len(ctds) < 2:
                    continue
                clause_id = cleantext(ctds[0])
                if re.fullmatch(r"\(\d+[A-Za-z]?\)", clause_id):
                    clause_count += 1
                # Sub-clauses live inside the clause's body cell
                for inner in ctds[1].find_all("table", recursive=True):
                    for sub_row in inner.find_all("tr", recursive=False):
                        std = sub_row.find_all("td", recursive=False)
                        if len(std) < 2:
                            continue
                        sub_id = cleantext(std[0])
                        if re.fullmatch(r"\([a-z]+\)", sub_id) or sub_id == "":
                            # empty sub-id rows are the "lettered" entries
                            subclause_count += 1
        articles.append((article_num, marginal, clause_count, subclause_count))
    return articles

# ── Walk IR ────────────────────────────────────────────────────────────────
ir = json.load(open(IR_PATH))

def ir_articles():
    """Yield (part_num, chapter_num or None, article_dict) for every article."""
    for part in ir["parts"]:
        # Direct articles under the part (e.g. Part I, Part IX)
        for art in part.get("articles", []):
            yield (part["number"], None, art)
        for ch in part.get("chapters", []):
            for art in ch.get("articles", []):
                yield (part["number"], ch.get("number"), art)

# Index IR by article number for fast lookup
ir_by_num = {art["number"]: art for _, _, art in ir_articles()}

# ── Walk source ────────────────────────────────────────────────────────────
print("Fetching pakistani.org chapter pages...", file=sys.stderr)
source_articles = {}  # article_num -> (marginal, clauses, subclauses, source_page)
for part_num, urls in PART_URLS.items():
    for u in urls:
        try:
            html = fetch(u)
        except Exception as e:
            print(f"  ✗ {u}: {e}", file=sys.stderr)
            continue
        arts = extract_articles_from_html(html)
        for a_num, marg, cc, sc in arts:
            source_articles[a_num] = (marg, cc, sc, u)
        print(f"  ✓ {u}: {len(arts)} articles", file=sys.stderr)

# Also fetch schedules + preamble + amendments for completeness reporting
source_top = {}
for slug, label in [("preamble.html","preamble"),
                    ("annex_objres.html","annex_objres"),
                    ("schedules/schedule1.html","schedule1"),
                    ("schedules/schedule2.html","schedule2"),
                    ("schedules/schedule3.html","schedule3"),
                    ("schedules/schedule4.html","schedule4"),
                    ("schedules/schedule5.html","schedule5")]:
    try:
        html = fetch(slug)
        # crude — body text size
        soup = BeautifulSoup(html, "html.parser")
        body_chars = len(cleantext(soup))
        source_top[label] = (body_chars, slug)
    except Exception as e:
        source_top[label] = (None, str(e))

# ── Diff ────────────────────────────────────────────────────────────────────
report = []
report.append(f"# constitution_ir.json ↔ pakistani.org sync audit\n")
report.append(f"Generated: 2026-06-04\n")
report.append(f"Source: pakistani.org/pakistan/constitution/")
report.append(f"IR file: `{IR_PATH}`\n")

report.append(f"## Top-level scoreboard")
report.append(f"")
report.append(f"| Section | IR | pakistani.org |")
report.append(f"|---|---|---|")
report.append(f"| Total articles | **{len(ir_by_num)}** | **{len(source_articles)}** |")
report.append(f"| Preamble (chars) | {len(ir.get('preamble') or '')} | {source_top.get('preamble',(0,))[0]} |")
report.append(f"| Schedules in IR | {('schedules' in ir or 'annexes' in ir)} | 5+ on source |")

# Missing / extra
src_nums = set(source_articles.keys())
ir_nums = set(ir_by_num.keys())
missing_in_ir = sorted(src_nums - ir_nums, key=lambda s: (int(re.match(r'\d+', s).group()), s))
extra_in_ir = sorted(ir_nums - src_nums, key=lambda s: (int(re.match(r'\d+', s).group()), s))

report.append(f"\n## Missing in IR (present on source, absent from constitution_ir.json)")
if missing_in_ir:
    report.append(f"\n**{len(missing_in_ir)} articles missing:**")
    for a in missing_in_ir:
        marg, cc, sc, page = source_articles[a]
        report.append(f"- Article **{a}** — {marg or '(no marginal note)'} · {cc} clauses · {sc} sub-clauses · source: `{page}`")
else:
    report.append("\n✅ None — every article on the source pages is present in the IR.")

report.append(f"\n## Extra in IR (in JSON but not found on source pages)")
if extra_in_ir:
    report.append(f"\n**{len(extra_in_ir)} articles in IR but not matched on source pages:**")
    for a in extra_in_ir:
        art = ir_by_num[a]
        report.append(f"- Article **{a}** — {art.get('marginal_note','')} · {len(art.get('clauses',[]))} clauses")
else:
    report.append("\n✅ None.")

# Per-article clause/subclause comparison
report.append(f"\n## Clause / sub-clause count mismatches\n")
report.append("Articles where the IR's clause OR sub-clause count differs from the source page.")
report.append("(Counts on the source are extracted by regex over the HTML and can be approximate for amendment-bracketed sections; treat as a hint not gospel.)\n")
report.append("| Art | IR clauses | Src clauses | IR sub-clauses | Src sub-clauses | Marginal note |")
report.append("|---|---:|---:|---:|---:|---|")

def total_subclauses(art):
    return sum(len(cl.get("subclauses", [])) for cl in art.get("clauses", []))

mismatches = 0
for a in sorted(src_nums & ir_nums, key=lambda s: (int(re.match(r'\d+', s).group()), s)):
    art = ir_by_num[a]
    ir_cc = len(art.get("clauses", []))
    ir_sc = total_subclauses(art)
    s_marg, s_cc, s_sc, _ = source_articles[a]
    if ir_cc != s_cc or ir_sc != s_sc:
        mismatches += 1
        flag = "⚠" if abs(ir_cc - s_cc) > 0 or ir_sc < s_sc else ""
        report.append(f"| {a} | {ir_cc} | {s_cc} | {ir_sc} | {s_sc} | {flag} {art.get('marginal_note','')[:60]} |")

if mismatches == 0:
    report.append("| _none_ |   |   |   |   | All article counts match. |")
report.append(f"\n**{mismatches}** articles have differing clause/sub-clause counts.")

# Headline summary
report.insert(4, f"## Headline\n")
report.insert(5, f"- **Articles missing in IR: {len(missing_in_ir)}**")
report.insert(6, f"- **Articles only in IR: {len(extra_in_ir)}**")
report.insert(7, f"- **Articles with clause/sub-clause count drift: {mismatches}**")
if not missing_in_ir and not extra_in_ir and mismatches == 0:
    report.insert(8, "- ✅ **IR is fully synced with pakistani.org at article-level granularity.**\n")
else:
    report.insert(8, "- ⚠ See sections below for details.\n")

open(REPORT, "w").write("\n".join(report))
print(f"\n✓ Wrote {REPORT}", file=sys.stderr)
print(f"  Missing in IR: {len(missing_in_ir)}", file=sys.stderr)
print(f"  Only in IR:    {len(extra_in_ir)}", file=sys.stderr)
print(f"  Count drift:   {mismatches}", file=sys.stderr)
