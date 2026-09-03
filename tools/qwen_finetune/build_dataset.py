#!/usr/bin/env python3
"""Tier A dataset generator for Qwen2.5-3B-Instruct fine-tune on the Pakistan Constitution.

Reads:  tools/constitution_import/constitute_IR/constitution_ir.json
Writes: tools/qwen_finetune/data/dataset_tier_a.jsonl  (Qwen ChatML messages format)

No external deps — pure stdlib. Runs in ~30s on any machine.
Expected output: ~10-15k samples (article Q/A + completions + hierarchy nav + preamble + 500 refusals).
"""

import json
import random
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
IR_PATH = REPO_ROOT / "tools" / "constitution_import" / "constitute_IR" / "constitution_ir.json"
OUT_DIR = REPO_ROOT / "tools" / "qwen_finetune" / "data"
OUT_PATH = OUT_DIR / "dataset_tier_a.jsonl"

SYSTEM_PROMPT = (
    "You are an expert on the Constitution of the Islamic Republic of Pakistan. "
    "Answer using only the text of the Constitution. Quote the relevant clause verbatim "
    "and end every answer with the citation in square brackets, e.g. [Art 25(1)] or "
    "[Art 260(1)]. If the question is not addressed in the Constitution, reply with: "
    "'This is not addressed in the Constitution of Pakistan. [no citation]'"
)

random.seed(42)


# ---------- helpers ----------

def msg(user: str, assistant: str) -> dict:
    return {
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user},
            {"role": "assistant", "content": assistant},
        ]
    }


def cite(art_num, clause_id=None, sub_id=None):
    tag = f"Art {art_num}"
    if clause_id:
        tag += clause_id
    if sub_id:
        tag += sub_id
    return f"[{tag}]"


def article_full_text(article: dict) -> str:
    """Reconstruct verbatim article text from clauses/subclauses/provisos/explanations."""
    parts = []
    if article.get("text"):
        parts.append(article["text"].strip())
    for cl in article.get("clauses", []) or []:
        cid = cl.get("id", "")
        ctext = (cl.get("text") or "").strip()
        if ctext:
            parts.append(f"{cid} {ctext}".strip())
        for sc in cl.get("subclauses", []) or []:
            sid = sc.get("id", "")
            stext = (sc.get("text") or "").strip()
            if stext:
                parts.append(f"    {sid} {stext}".strip())
        for pr in cl.get("provisos", []) or []:
            ptext = (pr.get("text") or "").strip()
            if ptext:
                parts.append(f"  Provided that {ptext}")
        for ex in cl.get("explanations", []) or []:
            etext = (ex.get("text") or "").strip()
            if etext:
                parts.append(f"  Explanation: {etext}")
    return "\n".join(parts).strip()


# ---------- article-level templates ----------

PARAPHRASE_Q = [
    "Tell me about Article {n} of the Pakistani Constitution.",
    "Explain Article {n}.",
    "What is Article {n} about?",
    "Describe Article {n} of Pakistan's Constitution.",
    "What does the Constitution of Pakistan say in Article {n}?",
]


def gen_for_article(samples: list, article: dict, part: dict, chapter: dict | None) -> None:
    art_num = article.get("number")
    mn = (article.get("marginal_note") or "").strip()
    if not art_num:
        return

    full_text = article_full_text(article)
    part_label = f"Part {part.get('number', '')}"
    if chapter:
        location = f"{part_label}, Chapter {chapter.get('number', '')} ({chapter.get('title', '')})"
    else:
        location = f"{part_label} ({part.get('title', '')})"

    # T1: verbatim lookup
    if full_text:
        samples.append(msg(
            f"What does Article {art_num} of the Constitution of Pakistan say?",
            f"Article {art_num}{' — ' + mn if mn else ''}:\n\n{full_text}\n\n{cite(art_num)}"
        ))

    # T2: marginal note
    if mn:
        samples.append(msg(
            f"What is the marginal note of Article {art_num}?",
            f"The marginal note of Article {art_num} is: \"{mn}\". {cite(art_num)}"
        ))

    # T3: location in hierarchy
    samples.append(msg(
        f"Where in the Constitution is Article {art_num} found?",
        f"Article {art_num} is found in {location}. {cite(art_num)}"
    ))

    # T4: per-clause verbatim + paraphrase
    for cl in article.get("clauses", []) or []:
        cid = cl.get("id", "")
        ctext = (cl.get("text") or "").strip()
        if cid and ctext:
            samples.append(msg(
                f"Quote Article {art_num} clause {cid} verbatim.",
                f"Article {art_num}{cid}: \"{ctext}\" {cite(art_num, cid)}"
            ))
            samples.append(msg(
                f"What does Article {art_num}{cid} provide?",
                f"Article {art_num}{cid} provides: \"{ctext}\" {cite(art_num, cid)}"
            ))

        # T5: per-subclause verbatim (matters for Article 260)
        for sc in cl.get("subclauses", []) or []:
            sid = sc.get("id", "")
            stext = (sc.get("text") or "").strip()
            if sid and stext:
                samples.append(msg(
                    f"Quote Article {art_num}{cid}{sid} verbatim.",
                    f"Article {art_num}{cid}{sid}: \"{stext}\" {cite(art_num, cid, sid)}"
                ))
                samples.append(msg(
                    f"What does Article {art_num}{cid}{sid} say?",
                    f"Article {art_num}{cid}{sid}: \"{stext}\" {cite(art_num, cid, sid)}"
                ))
                # definition-style if the sub-clause looks like 'X means ...'
                if " means " in stext.lower() or stext.lstrip().startswith('"') or len(stext) < 250:
                    samples.append(msg(
                        f"Define {sid.strip('()')} per Article {art_num}{cid}.",
                        f"Per Article {art_num}{cid}{sid}: \"{stext}\" {cite(art_num, cid, sid)}"
                    ))

    # T6: list clauses
    clause_ids = [cl.get("id") for cl in article.get("clauses", []) or [] if cl.get("id")]
    if len(clause_ids) >= 2:
        samples.append(msg(
            f"List all clauses of Article {art_num}.",
            f"Article {art_num} contains the following clauses: {', '.join(clause_ids)}. {cite(art_num)}"
        ))
        samples.append(msg(
            f"How many clauses does Article {art_num} have?",
            f"Article {art_num} has {len(clause_ids)} clauses: {', '.join(clause_ids)}. {cite(art_num)}"
        ))

    # T7: paraphrased "tell me about"
    if full_text and mn:
        q = random.choice(PARAPHRASE_Q).format(n=art_num)
        # first substantive clause
        first_cl = next((cl for cl in (article.get("clauses") or []) if (cl.get("text") or "").strip()), None)
        body = (first_cl.get("text") or "").strip() if first_cl else full_text[:400]
        samples.append(msg(
            q,
            f"Article {art_num} concerns {mn.rstrip('.').lower()}. It provides: \"{body}\" {cite(art_num)}"
        ))

    # T8: structure-aware completions
    if mn:
        samples.append(msg(
            f"Complete this sentence: \"The marginal note of Article {art_num} is...\"",
            f"\"{mn}\" {cite(art_num)}"
        ))
    if clause_ids:
        samples.append(msg(
            f"Complete this sentence: \"Article {art_num} contains clauses...\"",
            f"{', '.join(clause_ids)}. {cite(art_num)}"
        ))


# ---------- hierarchy navigation ----------

def gen_hierarchy_nav(samples: list, ir: dict) -> int:
    before = len(samples)
    for part in ir.get("parts", []):
        pnum = part.get("number", "")
        ptitle = (part.get("title") or "").strip()

        # Articles directly under part + nested
        flat_arts = [a.get("number") for a in (part.get("articles") or []) if a.get("number")]
        nested_arts: list = []
        for ch in part.get("chapters", []) or []:
            nested_arts.extend(a.get("number") for a in (ch.get("articles") or []) if a.get("number"))
        all_arts = flat_arts + nested_arts

        pcite = f"[Part {pnum}]"
        if ptitle:
            samples.append(msg(
                f"What is the title of Part {pnum} of the Constitution of Pakistan?",
                f"Part {pnum} is titled \"{ptitle}\". {pcite}"
            ))
        if all_arts:
            samples.append(msg(
                f"Which articles are in Part {pnum} of the Constitution of Pakistan?",
                f"Part {pnum} ({ptitle}) contains Articles {', '.join(all_arts)}. {pcite}"
            ))
            samples.append(msg(
                f"How many articles are in Part {pnum}?",
                f"Part {pnum} ({ptitle}) contains {len(all_arts)} articles. {pcite}"
            ))

        # Chapter-level
        for ch in part.get("chapters", []) or []:
            cnum = ch.get("number", "")
            ctitle = (ch.get("title") or "").strip()
            ch_arts = [a.get("number") for a in (ch.get("articles") or []) if a.get("number")]
            chcite = f"[Part {pnum}, Chapter {cnum}]"
            if ctitle:
                samples.append(msg(
                    f"What is the title of Chapter {cnum} of Part {pnum}?",
                    f"Chapter {cnum} of Part {pnum} is titled \"{ctitle}\". {chcite}"
                ))
            if ch_arts:
                samples.append(msg(
                    f"Which articles are in Chapter {cnum} of Part {pnum}?",
                    f"Chapter {cnum} ({ctitle}) of Part {pnum} contains Articles {', '.join(ch_arts)}. {chcite}"
                ))
                samples.append(msg(
                    f"List the articles in Part {pnum} Chapter {cnum}.",
                    f"Part {pnum} Chapter {cnum} ({ctitle}) contains Articles {', '.join(ch_arts)}. {chcite}"
                ))
    return len(samples) - before


# ---------- preamble ----------

def gen_preamble(samples: list, ir: dict) -> int:
    pre = (ir.get("preamble") or "").strip()
    if not pre:
        return 0
    before = len(samples)
    samples.append(msg(
        "Quote the Preamble of the Constitution of Pakistan.",
        f"{pre}\n\n[Preamble]"
    ))
    samples.append(msg(
        "What does the Preamble of the Constitution say?",
        f"The Preamble states:\n\n{pre}\n\n[Preamble]"
    ))
    samples.append(msg(
        "Summarize the Preamble of the Constitution of Pakistan.",
        f"The Preamble declares sovereignty over the entire Universe to belong to Almighty Allah alone and "
        f"frames the constitutional order accordingly. Verbatim:\n\n{pre[:600]}...\n\n[Preamble]"
    ))
    return len(samples) - before


# ---------- hard refusals ----------

REFUSAL_TOPICS = [
    "the Pakistan Penal Code",
    "the Civil Servants Act 1973",
    "the Sindh Service Rules",
    "the United States Constitution",
    "the Constitution of India",
    "UK common law",
    "specific Sharia rulings",
    "FBR tax rates",
    "traffic laws in Pakistan",
    "marriage registration procedure",
    "driving licence requirements",
    "Article 500 of the Constitution",
    "Article 999 of the Constitution",
    "the 30th Amendment to the Constitution",
    "Article -1 of the Constitution",
    "Pakistan Penal Code Section 302",
    "the CSS exam syllabus",
    "the Hudood Ordinance",
    "specific Lahore High Court judgments",
    "how to file an FIR",
    "the Pakistan Election Act 2017",
    "the NAB Ordinance",
    "ECP procedural rules",
    "specific Rules of Business",
    "Pakistan Cricket Board governance",
    "PIA aviation rules",
    "the Constitution of Bangladesh",
    "the Constitution of Iran",
    "the Saudi Arabia Basic Law",
    "specific provisions of blasphemy law in the Penal Code",
    "zakat collection procedure under the separate ordinance",
    "court martial procedure under the Army Act",
    "PEMRA licensing rules",
    "FIA cybercrime jurisdiction under PECA 2016",
    "the Sindh local government act",
    "the Punjab Civil Servants Act",
    "the Anti-Terrorism Act 1997",
    "ICC cricket rules",
    "specific Quranic verses",
    "specific Hadith collections",
    "Sufi orders in Pakistan",
    "passport application procedure",
    "NADRA CNIC rules",
    "army recruitment rules",
    "specific election dates",
    "the name of the current Prime Minister",
    "the name of the current Chief Justice",
    "cricket scores",
    "weather forecasts",
    "cooking recipes",
    "cryptocurrency prices",
]

REFUSAL_Q_TEMPLATES = [
    "What does {x} say?",
    "Explain {x}.",
    "Quote {x}.",
    "Summarize {x}.",
    "What is the position of {x}?",
    "Tell me about {x}.",
    "What does the Constitution of Pakistan say about {x}?",
    "Per the Constitution, what is {x}?",
    "How does the Constitution treat {x}?",
    "Cite {x} from the Constitution.",
]

REFUSAL_ANSWER = "This is not addressed in the Constitution of Pakistan. [no citation]"


def gen_refusals(samples: list, target: int = 500) -> int:
    pairs = []
    for topic in REFUSAL_TOPICS:
        for tmpl in REFUSAL_Q_TEMPLATES:
            pairs.append(tmpl.format(x=topic))
    random.shuffle(pairs)
    for q in pairs[:target]:
        samples.append(msg(q, REFUSAL_ANSWER))
    return min(target, len(pairs))


# ---------- main ----------

def main():
    print(f"[*] Reading IR from {IR_PATH}")
    ir = json.loads(IR_PATH.read_text(encoding="utf-8"))
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    samples: list = []

    # 1. Article-level templates
    art_count = 0
    for part in ir.get("parts", []):
        for art in part.get("articles", []) or []:
            gen_for_article(samples, art, part, None)
            art_count += 1
        for ch in part.get("chapters", []) or []:
            for art in ch.get("articles", []) or []:
                gen_for_article(samples, art, part, ch)
                art_count += 1
    print(f"[+] Article-level samples: {len(samples)} (from {art_count} articles)")

    # 2. Hierarchy navigation
    n_nav = gen_hierarchy_nav(samples, ir)
    print(f"[+] Hierarchy navigation samples: {n_nav}")

    # 3. Preamble
    n_pre = gen_preamble(samples, ir)
    print(f"[+] Preamble samples: {n_pre}")

    # 4. Hard refusals (500)
    n_ref = gen_refusals(samples, target=500)
    print(f"[+] Refusal samples: {n_ref}")

    # Shuffle
    random.shuffle(samples)

    # Write JSONL
    with OUT_PATH.open("w", encoding="utf-8") as f:
        for s in samples:
            f.write(json.dumps(s, ensure_ascii=False) + "\n")
    print(f"\n[✓] Wrote {len(samples)} samples to {OUT_PATH}")

    # Stats
    print("\n--- Dataset summary ---")
    print(f"Total samples       : {len(samples)}")
    avg_q = sum(len(s['messages'][1]['content']) for s in samples) / len(samples)
    avg_a = sum(len(s['messages'][2]['content']) for s in samples) / len(samples)
    print(f"Avg question length : {avg_q:.0f} chars")
    print(f"Avg answer length   : {avg_a:.0f} chars")
    print(f"Output file size    : {OUT_PATH.stat().st_size / 1024:.0f} KB")


if __name__ == "__main__":
    main()
