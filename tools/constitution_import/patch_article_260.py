#!/usr/bin/env python3.14
"""Patch constitution_ir.json Article 260 with the definitions list scraped
from https://www.pakistani.org/pakistan/constitution/part12.ch5.html .

pakistani.org HTML shape: one outer <tr> per clause where col-1 is the
clause id "(1)/(2)/(3)" and col-2 contains the clause text + a nested
<table> of sub-clauses. Each sub-clause is another <tr> with empty col-1
(just <nobr></nobr>) and col-2 = the definition text "\"term\" means …".

Strategy:
1. Find the outer <tr> whose first cell is "260" (Article header)
2. Walk its body's nested <tr> rows, bucketing by which clause they're
   under via the clause-id column.
3. For each entry under clause (1) (the master definitions list) emit
   a sub-clause with sequential alphabetic id.
"""
import json, re, sys, shutil, os
from bs4 import BeautifulSoup

DEFAULT_IR = "/media/riaz/New Volume/echo-AI/tools/constitution_import/constitute_IR/constitution_ir.json"
HTML = "/tmp/part12ch5.html"

soup = BeautifulSoup(open(HTML).read(), "html.parser")

def cleantext(node) -> str:
    # Drop footnote superscripts but keep the [bracketed amendment text]
    # they wrap around (pakistani.org wraps amendments in [..] with sup tags
    # on either side).
    for sup in node.find_all("sup"):
        sup.decompose()
    txt = node.get_text(separator=" ")
    txt = re.sub(r"\s+", " ", txt).strip()
    # collapse remaining [ ... ] empties caused by stripped sups (rare)
    txt = re.sub(r"\[\s*\]", "", txt)
    return txt

# ── 1. Locate Article 260's outer <tr> ─────────────────────────────────────
art_tr = None
for tr in soup.find_all("tr"):
    tds = tr.find_all("td", recursive=False)
    if len(tds) >= 2:
        first = cleantext(tds[0])
        if first == "260" and "Definitions" in cleantext(tds[1]):
            art_tr = tr
            break
if art_tr is None:
    sys.exit("Article 260 outer <tr> not found")
print("✓ Found Article 260 row")

body_cell = art_tr.find_all("td", recursive=False)[1]

# ── 2. Walk clause rows (direct children of the inner <table>) ─────────────
# body_cell has: <b>Definitions.</b><br><table>...clause rows...</table>
inner_clause_table = body_cell.find("table", recursive=False)
if inner_clause_table is None:
    sys.exit("Inner clause table not found")

clause_1_defs = []
clause_2_text = None
clause_3_defs = []

current_clause_id = None
for clause_row in inner_clause_table.find_all("tr", recursive=False):
    tds = clause_row.find_all("td", recursive=False)
    if len(tds) < 2:
        continue
    clause_id_text = cleantext(tds[0])
    body = tds[1]
    if clause_id_text in ("(1)", "(2)", "(3)"):
        current_clause_id = clause_id_text

    # Collect lead-in prose (text before first nested table)
    lead_in = ""
    for child in body.children:
        if getattr(child, "name", None) == "table":
            break
        if hasattr(child, "decompose") or isinstance(child, str):
            piece = child if isinstance(child, str) else cleantext(child)
            if piece.strip():
                lead_in += " " + piece
    lead_in = re.sub(r"\s+", " ", lead_in).strip().rstrip(",;").strip()

    if current_clause_id == "(2)":
        clause_2_text = lead_in

    # Look inside the body for ALL nested <table> elements containing definitions
    for inner_table in body.find_all("table", recursive=True):
        for def_row in inner_table.find_all("tr", recursive=False):
            def_tds = def_row.find_all("td", recursive=False)
            if len(def_tds) < 2:
                continue
            entry = cleantext(def_tds[1])
            if not entry:
                continue
            # Bucket by current clause
            m = re.match(r'^"([^"]{1,80})"\s+(?:means|includes)\b', entry)
            if not m:
                continue
            term = m.group(1).strip()
            if current_clause_id == "(3)" or term.lower() in ("muslim", "non-muslim"):
                clause_3_defs.append((term, entry))
            elif current_clause_id == "(1)":
                clause_1_defs.append((term, entry))

# Dedupe (since the inner table walks may overlap)
seen = set(); deduped = []
for t, e in clause_1_defs:
    if (t, e[:60]) in seen: continue
    seen.add((t, e[:60])); deduped.append((t, e))
clause_1_defs = deduped

seen = set(); deduped = []
for t, e in clause_3_defs:
    if t in seen: continue
    seen.add(t); deduped.append((t, e))
clause_3_defs = deduped

print(f"Clause (1) definitions: {len(clause_1_defs)}")
for term, _ in clause_1_defs[:5]:
    print(f"  - {term}")
print(f"  …")
for term, _ in clause_1_defs[-5:]:
    print(f"  - {term}")
print(f"Clause (2) text: {clause_2_text[:90] if clause_2_text else '(none)'}…")
print(f"Clause (3) definitions: {[t for t, _ in clause_3_defs]}")

# Print ALL definitions captured for visibility
print("\nAll definitions captured:")
for i, (term, _) in enumerate(clause_1_defs):
    print(f"  {i+1:2d}. {term}")

# Minimum required for Q5 + Q1: "citizen" and "Service of Pakistan"
critical = {"citizen", "Service of Pakistan"}
found = {t for t, _ in clause_1_defs}
missing_critical = critical - found
if missing_critical:
    sys.exit(f"⚠ MISSING CRITICAL definitions: {missing_critical}")
nice_to_have = {"State", "Constitution", "Pakistan"} - found
if nice_to_have:
    print(f"\n(note: not captured — likely in a separately-bracketed amendment table: {nice_to_have})")
print("\n✓ Critical definitions ('citizen', 'Service of Pakistan') present")

# ── 3. Patch the IR ───────────────────────────────────────────────────────
ir_path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_IR
ir = json.load(open(ir_path))

art_ref = None
for part in ir["parts"]:
    for ch in part.get("chapters", []):
        for art in ch.get("articles", []):
            if art.get("number") == "260":
                art_ref = art
                break
    for art in part.get("articles", []):
        if art.get("number") == "260":
            art_ref = art

if art_ref is None:
    sys.exit("Article 260 not found in IR")

def sub_id(i):
    if i < 26:
        return f"({chr(ord('a') + i)})"
    return f"({chr(ord('a') + (i // 26 - 1))}{chr(ord('a') + (i % 26))})"

new_clause_1 = {
    "id": "(1)",
    "text": "In the Constitution, unless the context otherwise requires, the following expressions have the meaning hereby respectively assigned to them, that is to say,",
    "subclauses": [
        {"id": sub_id(i), "text": text, "provisos": [], "explanations": [], "footnote_refs": []}
        for i, (_, text) in enumerate(clause_1_defs)
    ],
    "provisos": [], "explanations": [],
    "footnote_refs": art_ref["clauses"][0].get("footnote_refs", []),
}

# Clause (2) — keep existing IR text if scrape didn't pick one up
new_clause_2 = {
    "id": "(2)",
    "text": clause_2_text or art_ref["clauses"][1]["text"],
    "subclauses": [],
    "provisos": [], "explanations": [],
    "footnote_refs": art_ref["clauses"][1].get("footnote_refs", []),
}

# Clause (3) — Muslim + non-Muslim. Prefer scrape; fall back to existing.
existing_c3 = art_ref["clauses"][2] if len(art_ref["clauses"]) > 2 else None
muslim_text  = next((e for t, e in clause_3_defs if t.lower() == "muslim"), None)
nonmuslim_text = next((e for t, e in clause_3_defs if t.lower() == "non-muslim"), None)
if not muslim_text and existing_c3:
    muslim_text = next((s.get("text") for s in existing_c3.get("subclauses", []) if "Muslim" in s.get("text","")), None)
if not nonmuslim_text and existing_c3:
    nonmuslim_text = next((s.get("text") for s in existing_c3.get("subclauses", []) if "non-Muslim" in s.get("text","")), None)

c3_subs = []
if muslim_text:
    c3_subs.append({"id": "(a)", "text": muslim_text, "provisos": [], "explanations": [], "footnote_refs": []})
if nonmuslim_text:
    c3_subs.append({"id": "(b)", "text": nonmuslim_text, "provisos": [], "explanations": [], "footnote_refs": []})

new_clause_3 = {
    "id": "(3)",
    "text": "In the Constitution and all enactments and other legal instruments, unless there is anything repugnant in the subject or context",
    "subclauses": c3_subs,
    "provisos": [], "explanations": [],
    "footnote_refs": existing_c3.get("footnote_refs", []) if existing_c3 else [],
}

art_ref["clauses"] = [new_clause_1, new_clause_2, new_clause_3]

# Backup + write
backup = ir_path + ".bak-pre-art260-patch"
if not os.path.exists(backup):
    shutil.copy(ir_path, backup)
    print(f"\n✓ Backed up to {backup}")
with open(ir_path, "w") as f:
    json.dump(ir, f, indent=2, ensure_ascii=False)
print(f"✓ Wrote patched IR to {ir_path}")
print(f"  Article 260(1) sub-clauses: {len(new_clause_1['subclauses'])}")
print(f"  Article 260(3) sub-clauses: {len(new_clause_3['subclauses'])}")
