#!/usr/bin/env python3
"""20-prompt cloud eval — Cerebras gpt-oss-120b — same FTS5+intent_router
pipeline replication as /tmp/eval_cloud.py, extended to the full investor
demo set + machine-checkable expected answers + auto OK/OK?/REFUSED/WRONG
grading.
"""
import os
import json, re, sqlite3, sys, time, urllib.request

DB_PATH = "/home/riaz/.local/share/com.echoai.app/echo.db"
CONSTITUTION_SOURCE_ID = "3cdbe6f5-5353-45a2-a7ed-8f4a52f47a6f"
CB_KEY = os.environ.get("CEREBRAS_API_KEY", "")  # export before running
CB_MODEL = "gpt-oss-120b"
CB_URL = "https://api.cerebras.ai/v1/chat/completions"
TOP_K = 20

# (qid, text, expected_keywords_all, expected_keywords_any, hard_negatives)
# OK   = all `_all` present AND no hard_negatives
# OK?  = all `_all` present BUT also at least one hint of partial
# REFUSED = answer ≤180 chars AND contains a "not found" / "does not contain" phrase
# WRONG  = a hard_negative is matched (factually invented), OR none of `_any` present
PROMPTS = [
    # Tier 1 — Definitional recall
    (1, 'What does the Constitution define as a "Service of Pakistan"?',
        ["service of pakistan"], ["260", "definition", "service"], ["death", "interest"]),
    (2, 'How is "Majlis-e-Shoora (Parliament)" defined in the Constitution?',
        ["majlis", "parliament"], ["50", "senate", "national assembly"], ["article 69", "art 69"]),
    (3, 'What is the constitutional definition of "State" for the purposes of Fundamental Rights?',
        ["state"], ["7", "definition", "executive", "legislature"], ["article 36"]),
    (4, 'Quote the definition of "Money Bill" as it appears in the Constitution.',
        ["money bill"], ["73", "tax", "borrowing", "consolidated fund", "audit"], []),
    (5, 'What does Article 260 say about the term "citizen"?',
        ["citizen"], ["260", "person who is", "citizenship"], []),
    # Tier 2 — Article-level
    (6, 'What does Article 25 guarantee regarding equality of citizens?',
        ["equality"], ["25", "law", "sex"], []),
    (7, 'Summarise Article 19 on freedom of speech, including its stated restrictions.',
        ["freedom of speech"], ["19", "restriction", "islam", "decency", "contempt"], ["article 23"]),
    (8, 'What protections does Article 10A provide?',
        ["fair trial"], ["10a", "due process"], []),
    (9, 'List the languages mentioned in Article 251.',
        ["urdu"], ["english", "251"], []),
    (10, 'What does Article 6 declare about high treason?',
        ["high treason"], ["6", "abrogate", "subvert", "constitution"], ["death penalty", "punishable by death"]),
    # Tier 3 — Structural
    (11, 'Which Articles fall under Chapter 1 of Part II (Fundamental Rights)? List them with titles.',
        ["article 8", "article 28"], ["article 9", "article 10", "article 25"], ["article 41", "article 50"]),
    (12, 'What are the Principles of Policy listed in Chapter 2 of Part II?',
        ["article 29", "article 40"], ["article 31", "article 37"], ["democracy", "freedom", "equality", "tolerance"]),
    (13, 'How many schedules does the Constitution have, and what does the Fourth Schedule cover?',
        ["fourth schedule"], ["seven", "legislative list", "federal", "concurrent"], []),
    (14, 'Walk me through the composition of the National Assembly as set out in Article 51.',
        ["national assembly"], ["51", "seats", "general", "women"], []),
    (15, 'What is the procedure for the election of the President under Article 41?',
        ["president"], ["41", "electoral college", "members"], []),
    # Tier 4 — Multi-hop
    (16, 'Compare the qualifications for membership of the National Assembly (Article 62) with the disqualifications (Article 63). Where do they overlap?',
        ["62", "63"], ["qualification", "disqualification"], []),
    (17, 'Under what combined provisions can a Provincial Assembly be dissolved, and who has that power?',
        ["dissolve"], ["112", "provincial assembly", "governor"], []),
    (18, 'Trace the path of a Money Bill from introduction to assent — cite every Article involved.',
        ["money bill"], ["73", "75", "introduce", "assent", "national assembly"], []),
    (19, 'What rights of an arrested person are guaranteed across Articles 9, 10, and 10A taken together?',
        ["arrest", "fair trial"], ["9", "10", "10a", "magistrate", "twenty-four"], []),
    (20, 'If Fundamental Rights conflict with a Principle of Policy, what does the Constitution say about precedence? Cite the controlling Articles.',
        ["fundamental rights", "principle"], ["29", "30", "8", "non-justiciable", "responsibility"], []),
]

# ── classifier + slot extractor (same as eval_cloud.py) ──
INTENT_CUES = [
    ("Lookup",       r"\b(what (does|is)|define|definition of|meaning of|quote|exact text|verbatim|wording of)\b", 1.0),
    ("Lookup",       r"\b(article|articles|section|sections|clause|clauses|rule|rules|regulation|regulations) \d+[a-z]?\b", 0.8),
    ("Lookup",       r"\b(how is|how does) .+? (defined|define)\b", 0.9),
    ("Lookup",       r"\bunder what( \w+){0,3} (provisions?|articles?|sections?|rules?|clauses?)\b", 0.9),
    ("Lookup",       r"\bwhat (rights?|protections?|powers?|duties|obligations?) (of|do)\b", 0.9),
    ("ListChildren", r"\b(list|enumerate|index of|outline of|table of contents)\b", 1.0),
    ("ListChildren", r"\b(all|every) (articles?|sections?|clauses?|rules?|chapters?|parts?)\b", 0.9),
    ("ListChildren", r"\bwhat (articles?|sections?|clauses?|rules?) (are|fall)\b", 0.9),
    ("ListChildren", r"\b(listed in|mentioned in)\b", 0.9),
    ("ListChildren", r"\bwhat are the .+ (listed|mentioned)\b", 0.9),
    ("Summarize",    r"\b(summarize|summarise|summary of|tldr|brief overview|in brief|key points of|gist of|overview of)\b", 1.0),
    ("Summarize",    r"\bwalk me through\b", 1.2),
    ("Difference",   r"\b(difference|diff|changed|change log|what changed|amend(ed|ment)?|substituted|inserted|repealed|omitted)\b", 0.9),
    ("ActionItems",  r"\b(action items?|action points?|to-?dos?|checklist|duties|obligations?|responsibilities|musts?|shall do|what (must|should|do) i do)\b", 1.0),
    ("Timeline",     r"\b(timeline|history|chronology|chronological|evolution|over time|when was|since \d{4})\b", 1.0),
    ("Procedure",    r"\b(procedure|process|workflow|how (do|does|to)|step-?by-?step|stepwise|what is the process)\b", 1.0),
    ("Procedure",    r"\bprocedure (for|of|to)\b", 1.5),
    ("Procedure",    r"\btrace the (path|flow|process|route)\b", 1.2),
    ("CrossCompare", r"\bhow does .+ (compare|differ) (to|from|with)\b", 1.0),
    ("CrossCompare", r"\bcompare the .+ with the\b", 0.9),
    ("CrossCompare", r"\bwhere do they overlap\b", 0.7),
]
def classify(q):
    lo = q.lower(); s = {}
    for i,p,w in INTENT_CUES:
        if re.search(p, lo): s[i] = s.get(i,0)+w
    if not s: return ("Fallback", 0.0)
    b = max(s.items(), key=lambda kv: kv[1])
    return (b[0], min(0.6 + (b[1]/(b[1]+1))*0.4, 1.0))

def extract_paths(q):
    paths=[]
    m=re.search(r"(?i)\b(article|articles|section|sections|clause|clauses|rule|rules|chapter|part|schedule) ([0-9IVXLCDM]+[A-Za-z]?(?:\([A-Za-z0-9]+\))*)", q)
    if not m: return paths
    head=m.group(1).rstrip("s").title()
    paths.append(f"{head} {m.group(2).strip()}")
    tail=q[m.end():]
    cut=re.search(r"[.?!;]", tail)
    if cut: tail=tail[:cut.start()]
    for sib in re.finditer(r"(?i)(?:,\s*|\s+and\s+)([0-9IVXLCDM]+[A-Za-z]?(?:\([A-Za-z0-9]+\))*)", tail):
        p=f"{head} {sib.group(1).strip()}"
        if p not in paths: paths.append(p)
    return paths

def retrieve(db, q, k=TOP_K):
    toks=[t for t in re.findall(r"[A-Za-z0-9]+", q) if len(t)>1 and t.lower() not in
          {"the","a","an","of","in","on","at","to","for","with","what","is","are","do","does","that","this","and","or","by","be","its"}]
    if not toks: return []
    fts=" OR ".join(f'"{t}"' for t in toks)
    return db.execute(
        """SELECT c.id, c.content, c.unit_label
           FROM chunks_fts JOIN chunks c ON c.rowid=chunks_fts.rowid
           WHERE chunks_fts MATCH ? AND c.source_id=?
           ORDER BY bm25(chunks_fts) ASC LIMIT ?""",
        (fts, CONSTITUTION_SOURCE_ID, k)).fetchall()

def build_context(chs):
    return "\n\n".join(f"[{i}] ({lbl}) {c.strip()}" for i,(_,c,lbl) in enumerate(chs,1))

def sys_prompt(intent, slots):
    multi = len(slots) >= 2
    if not slots:                tgt = "No specific provision was named."
    elif multi:                  tgt = f"The named targets are **{', '.join(slots)}**."
    else:                        tgt = f"The named target is **{slots[0]}**."
    if intent == "Lookup" and multi:
        return (f"You are a Pakistan government legal assistant. {tgt}\n"
                "Task: Provide the verbatim relevant text for EACH named provision in turn, then "
                "synthesise the combined rights/duties they create together. RULES:\n"
                "1. Organise the answer one paragraph (or bullet) per named provision, in the order given.\n"
                "2. For each provision: quote or closely paraphrase the relevant clauses, end with its citation [n].\n"
                "3. After the per-provision section, add a short 'Taken together' paragraph that combines them.\n"
                "4. Do NOT invent provisions. Do NOT pull content from articles that were not named.\n"
                "5. If a named provision is absent from the retrieved context, say so explicitly for that provision.")
    if intent == "Lookup":
        return (f"You are a Pakistan government legal assistant. {tgt}\n"
                "Task: Answer using ONLY the verbatim text of the named provision found in the context. RULES:\n"
                "1. If the named provision appears in the context, reply with its exact text. Do not paraphrase.\n"
                "2. Do NOT pull content from adjacent or related provisions, even if retrieval returned them.\n"
                "3. If the named provision is NOT in the context, reply exactly: "
                "'The named provision was not found in the retrieved passages.'\n"
                "4. End every claim with its citation in the form [n] matching a context passage.")
    if intent == "ListChildren":
        return (f"You are a Pakistan government legal assistant. {tgt}\n"
                "Task: List the immediate children of the named container, copying their labels and "
                "titles directly from the retrieved context. RULES:\n"
                "1. Output ONLY a bulleted list. No tables. No prose preamble. No tail commentary.\n"
                "2. Each bullet: '- <exact child label as written> — <one-line title from context> [n]'.\n"
                "3. CONFABULATION GUARD: every bullet MUST appear verbatim in a retrieved passage. "
                "If the only retrieved passage about the container is a meta-description "
                "(e.g. 'these shall be known as the X' or 'the provisions of this Chapter…') "
                "WITHOUT the enumeration itself, you MUST reply exactly: "
                "'No enumerable children found in the retrieved passages.'\n"
                "4. Do NOT invent items, values, principles, or rights based on general knowledge.\n"
                "5. Do NOT include children of children — only the immediate level.")
    if intent == "Summarize":
        return (f"You are a Pakistan government legal assistant. {tgt}\n"
                "Task: Summarise ONLY the named provision. RULES:\n"
                "1. Use 3–5 short sentences OR up to 6 bullets. Officer-grade, no rhetoric.\n"
                "2. Do NOT bring in content from adjacent provisions even if retrieved.\n"
                "3. If the named provision is not in the retrieved context, reply exactly: "
                "'The named provision was not found in the retrieved passages.'\n"
                "4. Cite every claim inline as [n].")
    return (f"You are a Pakistan government legal assistant. {tgt}\n"
            "Answer the officer's question using ONLY the retrieved context. Cite every claim as [n]. "
            "If the answer is not in the context, say so explicitly.")

def call_cloud(system, user):
    body=json.dumps({"model":CB_MODEL,"messages":[{"role":"system","content":system},{"role":"user","content":user}],"temperature":0.2,"max_tokens":4096}).encode()
    req=urllib.request.Request(CB_URL, data=body, headers={"Authorization":f"Bearer {CB_KEY}","Content-Type":"application/json","User-Agent":"curl/8.0","Accept":"*/*"}, method="POST")
    t0=time.time()
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            d=json.loads(r.read().decode())
        m=d["choices"][0]["message"]
        return (m.get("content") or m.get("reasoning") or "", time.time()-t0, None)
    except Exception as e:
        return (None, time.time()-t0, f"{type(e).__name__}: {e}")

REFUSAL_HINTS = [
    "not found in the retrieved passages",
    "no enumerable children found",
    "does not contain",
    "do not contain",
    "is not present in the context",
    "no information about",
    "i don't have enough",
    "i do not have enough",
    "cannot be answered",
]

def grade(answer, all_kw, any_kw, hard_neg):
    if answer is None:
        return ("ERR", "Cloud call failed")
    a = answer.lower()
    if any(h in a for h in REFUSAL_HINTS) and len(a) < 400:
        return ("REFUSED", "honest refusal")
    if any(neg.lower() in a for neg in hard_neg) and hard_neg:
        return ("WRONG", f"matched hard-negative: {[n for n in hard_neg if n.lower() in a][:2]}")
    all_hit = all(kw.lower() in a for kw in all_kw)
    any_hit = sum(1 for kw in any_kw if kw.lower() in a)
    if all_hit and any_hit >= max(1, len(any_kw)//2):
        return ("OK", f"all-kw + {any_hit}/{len(any_kw)} any-kw")
    if all_hit or any_hit >= max(1, len(any_kw)//2):
        return ("OK?", f"all={all_hit}, any={any_hit}/{len(any_kw)}")
    return ("WRONG", f"all={all_hit}, any={any_hit}/{len(any_kw)} — neither anchor met")

def main():
    db = sqlite3.connect(DB_PATH)
    results = []
    for qid, qtext, all_kw, any_kw, hard_neg in PROMPTS:
        print(f"→ Q{qid}: {qtext[:70]}...", file=sys.stderr)
        intent, conf = classify(qtext)
        slots = extract_paths(qtext)
        chunks = retrieve(db, qtext)
        if intent in ("Difference", "CrossCompare", "Timeline", "Procedure"):
            # out-of-scope on fast tier → still ask cloud (Phase 4 will do these
            # via deep IR; for now route everything to the LLM with a fallback prompt)
            intent_for_prompt = "Fallback"
        else:
            intent_for_prompt = intent
        sp = sys_prompt(intent_for_prompt, slots)
        up = f"Context passages:\n{build_context(chunks)}\n\nQuestion: {qtext}"
        ans, dt, err = call_cloud(sp, up)
        verdict, reason = (("ERR", err) if err else grade(ans, all_kw, any_kw, hard_neg))
        results.append((qid, qtext, intent, slots, len(chunks), dt, verdict, reason, ans))
        print(f"   {verdict}  ({dt:.1f}s)  — {reason}", file=sys.stderr)
    # Save raw
    with open("/tmp/echoai_20prompt_cloud.json", "w") as f:
        json.dump([{
            "qid": r[0], "question": r[1], "intent": r[2], "slots": r[3],
            "chunks": r[4], "latency_s": round(r[5],2), "verdict": r[6],
            "reason": r[7], "answer": r[8],
        } for r in results], f, indent=2)

    # Build the comparison row
    tallies = {"OK":0, "OK?":0, "REFUSED":0, "WRONG":0, "ERR":0}
    for r in results:
        tallies[r[6]] = tallies.get(r[6], 0) + 1
    total = len(results)
    avg = sum(r[5] for r in results) / total
    print(f"\nTotal: {total}", file=sys.stderr)
    print(f"OK={tallies['OK']} OK?={tallies['OK?']} REFUSED={tallies['REFUSED']} WRONG={tallies['WRONG']} ERR={tallies['ERR']}", file=sys.stderr)
    print(f"Avg latency: {avg:.2f}s", file=sys.stderr)
    print(f"Wall total: {sum(r[5] for r in results):.1f}s", file=sys.stderr)

if __name__ == "__main__":
    main()
