#!/usr/bin/env python3
"""Render constitution_ir.json into human-readable Markdown for manual review.

Companion to the S1 importer: shows every field the IR carries (preamble,
amendment catalogue, footnote glossary, per-node footnote refs, provisos /
explanations, sub-clauses) so a reviewer can diff it against pakistani.org
without reading raw JSON.

    python3 render_review_md.py constitution_ir.json out.md
"""
import json
import sys

# footnote id -> change kind, filled in main().
FN = {}
AMENDMENT_KINDS = {"substituted", "inserted", "omitted", "added", "renumbered"}


def refs(node):
    """Render a node's footnote refs, split into amendments vs editorial notes."""
    ids = node.get("footnote_refs") or []
    if not ids:
        return ""
    amends = [i for i in ids if FN.get(i, "other") in AMENDMENT_KINDS]
    notes = [i for i in ids if FN.get(i, "other") not in AMENDMENT_KINDS]
    parts = []
    if amends:
        parts.append("amends: " + ", ".join(amends))
    if notes:
        parts.append("see: " + ", ".join(notes))
    return "  _(" + "; ".join(parts) + ")_"


def emit_blockquotes(out, items, kind, indent=""):
    for it in items:
        label = it.get("label")
        tag = f"{kind} {label}".strip() if kind == "Explanation" else kind
        out.append(f"{indent}> **{tag}.** {it.get('text', '')}")
        out.append("")


def emit_article(out, a):
    out.append(f"### Article {a['number']} — {a.get('marginal_note') or ''}{refs(a)}")
    out.append("")
    text = (a.get("text") or "").strip()
    if text:
        out.append(text)
        out.append("")
    for c in a.get("clauses") or []:
        out.append(f"**{c['id']}** {c.get('text', '')}{refs(c)}")
        out.append("")
        for s in c.get("subclauses") or []:
            out.append(f"  - **{s['id']}** {s.get('text', '')}{refs(s)}")
            emit_blockquotes(out, s.get("provisos") or [], "Proviso", "    ")
            emit_blockquotes(out, s.get("explanations") or [], "Explanation", "    ")
        if c.get("subclauses"):
            out.append("")
        emit_blockquotes(out, c.get("provisos") or [], "Proviso")
        emit_blockquotes(out, c.get("explanations") or [], "Explanation")
    emit_blockquotes(out, a.get("provisos") or [], "Proviso")
    emit_blockquotes(out, a.get("explanations") or [], "Explanation")


def main():
    src, dst = sys.argv[1], sys.argv[2]
    d = json.load(open(src))
    for f in d.get("footnotes") or []:
        FN[f["id"]] = f.get("change", "other")

    out = [f"# {d['title']}", ""]
    out.append(f"- **Source:** {d['source_url']}")
    out.append(f"- **Retrieved:** {d['retrieved']}")
    out.append(f"- **Amendments through:** {d['amendments_through']}")
    out.append("")
    out += ["_In the body below, `amends:` lists amendment footnotes and `see:` "
            "lists editorial-reference footnotes — both resolve in the glossary._", ""]

    out += ["## Preamble", "", d.get("preamble") or "_(none captured)_", ""]

    out += ["## Amendments (enacted)", ""]
    for am in d.get("amendments") or []:
        s = f" — {am['summary']}" if am.get("summary") else ""
        out.append(f"- **{am['title']}**{s}")
    out.append("")

    for p in d["parts"]:
        out += [f"## Part {p['number']} — {p.get('title')}", ""]
        for a in p.get("articles") or []:
            emit_article(out, a)
        for ch in p.get("chapters") or []:
            out += [f"### Chapter {ch['number']} — {ch.get('title')}", ""]
            for a in ch.get("articles") or []:
                emit_article(out, a)

    out += ["## Footnote glossary", "",
            "`amends:`/`see:` ids above resolve here. Kind in parentheses. "
            "`note`/`other` footnotes are not amendments, so they carry no "
            "instrument — their reference text is shown instead.", ""]
    for f in d.get("footnotes") or []:
        inst = f.get("instrument")
        if inst:
            detail = inst
        elif f["change"] in ("note", "other"):
            detail = "_" + " ".join((f.get("raw") or "").split())[:120] + "_"
        else:
            detail = "_(instrument blank in pakistani.org source)_"
        out.append(f"- **{f['id']}** ({f['change']}) — {detail}")
    out.append("")

    open(dst, "w").write("\n".join(out))
    print(f"wrote {dst} ({len(out)} lines)")


if __name__ == "__main__":
    main()
