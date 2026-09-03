#!/usr/bin/env python3
"""
Cloud eval for EchoAI Feature 007 — runs the 5 US1 queries against
Groq llama-3.3-70b-versatile, replicating the intent_router pipeline
end-to-end:

  query → classify (rule-based) → extract slots → retrieve chunks
        → build per-intent system prompt → Groq → answer + grading

Why Python and not the Rust harness: the Rust `intent_router::dispatch`
needs `libonnxruntime.so` for the embedder, which isn't on the test
binary's library path outside of Tauri's startup. This script bypasses
the embedder by using FTS5 (works fine in Python sqlite3) for retrieval
— so the answer-quality side is tested against the same chunks the
chat path would surface, just via keyword recall instead of dense
similarity.

Output: /tmp/echoai_eval_results.md
"""

import os
import json
import re
import sqlite3
import sys
import time
import urllib.request
import urllib.error

DB_PATH = "/home/riaz/.local/share/com.echoai.app/echo.db"
CONSTITUTION_SOURCE_ID = "3cdbe6f5-5353-45a2-a7ed-8f4a52f47a6f"
# Groq key expired (2026-06-04 13:01); using Cerebras gpt-oss-120b instead.
GROQ_API_KEY = os.environ.get("CEREBRAS_API_KEY", "")  # export before running
GROQ_MODEL = "gpt-oss-120b"
GROQ_URL = "https://api.cerebras.ai/v1/chat/completions"
RESULTS_FILE = "/tmp/echoai_eval_results.md"
TOP_K = 20  # matches CONTEXT_TOP_K after Fix C

QUERIES = [
    ("Q1",       "What are the Principles of Policy listed in Chapter 2 of Part II?"),
    ("Q11",      "Which Articles fall under Chapter 1 of Part II (Fundamental Rights)? List them with titles."),
    ("Q1-fake",  "What are the duties listed in Chapter 3 of Part XYZ?"),
    ("Q3",       "What rights of an arrested person are guaranteed across Articles 9, 10, and 10A taken together?"),
    ("Q2",       "Summarise Article 19 on freedom of speech, including its stated restrictions."),
]

# ─── Classifier — exact-rule mirror of intent_router::classifier ──────────────

INTENT_CUES = [
    # (intent, regex, weight)
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
]

def classify(query):
    lower = query.lower()
    scores = {}
    for intent, pattern, weight in INTENT_CUES:
        if re.search(pattern, lower):
            scores[intent] = scores.get(intent, 0.0) + weight
    if not scores:
        return ("Fallback", 0.0)
    best = max(scores.items(), key=lambda kv: kv[1])
    return (best[0], min(0.6 + (best[1] / (best[1] + 1.0)) * 0.4, 1.0))

# ─── Slot extractor — multi-target section_paths ──────────────────────────────

def extract_section_paths(query):
    paths = []
    m = re.search(
        r"(?i)\b(article|articles|section|sections|clause|clauses|rule|rules|chapter|part|schedule) ([0-9IVXLCDM]+[A-Za-z]?(?:\([A-Za-z0-9]+\))*)",
        query,
    )
    if not m:
        return paths
    head = m.group(1).rstrip("s").title()
    paths.append(f"{head} {m.group(2).strip()}")
    # comma + and joined siblings
    tail = query[m.end():]
    cutoff = re.search(r"[.?!;]", tail)
    if cutoff:
        tail = tail[:cutoff.start()]
    for sib in re.finditer(r"(?i)(?:,\s*|\s+and\s+)([0-9IVXLCDM]+[A-Za-z]?(?:\([A-Za-z0-9]+\))*)", tail):
        p = f"{head} {sib.group(1).strip()}"
        if p not in paths:
            paths.append(p)
    return paths

# ─── Retrieval (FTS5 only) ────────────────────────────────────────────────────

def retrieve(db, query, k=TOP_K):
    """FTS5-only retrieval. Token-cleans the query so MATCH doesn't choke."""
    tokens = re.findall(r"[A-Za-z0-9]+", query)
    # Drop stopwords and short tokens
    stop = {"the","a","an","of","in","on","at","to","for","with","what","is","are",
            "do","does","that","this","and","or","by","be","its"}
    tokens = [t for t in tokens if len(t) > 1 and t.lower() not in stop]
    if not tokens:
        return []
    fts = " OR ".join(f'"{t}"' for t in tokens)
    rows = db.execute(
        """SELECT c.id, c.content, c.unit_label
           FROM chunks_fts
           JOIN chunks c ON c.rowid = chunks_fts.rowid
           WHERE chunks_fts MATCH ?
             AND c.source_id = ?
           ORDER BY bm25(chunks_fts) ASC
           LIMIT ?""",
        (fts, CONSTITUTION_SOURCE_ID, k),
    ).fetchall()
    return rows

def build_context(chunks):
    blocks = []
    for i, (cid, content, label) in enumerate(chunks, 1):
        blocks.append(f"[{i}] ({label}) {content.strip()}")
    return "\n\n".join(blocks)

# ─── Per-intent system prompts — exact mirror of intent_router::mod.rs ────────

def system_prompt(intent, slots):
    multi = len(slots) >= 2
    if not slots:
        target = "No specific provision was named."
    elif multi:
        target = f"The named targets are **{', '.join(slots)}**."
    else:
        target = f"The named target is **{slots[0]}**."

    if intent == "Lookup" and multi:
        return (f"You are a Pakistan government legal assistant. {target}\n"
                "Task: Provide the verbatim relevant text for EACH named provision in turn, then "
                "synthesise the combined rights/duties they create together. RULES:\n"
                "1. Organise the answer one paragraph (or bullet) per named provision, in the order given.\n"
                "2. For each provision: quote or closely paraphrase the relevant clauses, end with its citation [n].\n"
                "3. After the per-provision section, add a short 'Taken together' paragraph that combines them.\n"
                "4. Do NOT invent provisions. Do NOT pull content from articles that were not named.\n"
                "5. If a named provision is absent from the retrieved context, say so explicitly for that provision.")
    if intent == "Lookup":
        return (f"You are a Pakistan government legal assistant. {target}\n"
                "Task: Answer using ONLY the verbatim text of the named provision found in the context. RULES:\n"
                "1. If the named provision appears in the context, reply with its exact text. Do not paraphrase.\n"
                "2. Do NOT pull content from adjacent or related provisions, even if retrieval returned them.\n"
                "3. If the named provision is NOT in the context, reply exactly: "
                "'The named provision was not found in the retrieved passages.'\n"
                "4. End every claim with its citation in the form [n] matching a context passage.")
    if intent == "ListChildren":
        return (f"You are a Pakistan government legal assistant. {target}\n"
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
        return (f"You are a Pakistan government legal assistant. {target}\n"
                "Task: Summarise ONLY the named provision. RULES:\n"
                "1. Use 3–5 short sentences OR up to 6 bullets. Officer-grade, no rhetoric.\n"
                "2. Do NOT bring in content from adjacent provisions even if retrieved.\n"
                "3. If the named provision is not in the retrieved context, reply exactly: "
                "'The named provision was not found in the retrieved passages.'\n"
                "4. Cite every claim inline as [n].")
    return (f"You are a Pakistan government legal assistant. {target}\n"
            "Answer the officer's question using ONLY the retrieved context. Cite every claim as [n]. "
            "If the answer is not in the context, say so explicitly.")

# ─── Groq HTTP ────────────────────────────────────────────────────────────────

def groq_call(system, user, model=GROQ_MODEL):
    body = {
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user",   "content": user},
        ],
        "temperature": 0.2,
        # gpt-oss-120b is a reasoning model — needs headroom for thought tokens
        # in addition to the visible answer. 4096 leaves ~2k for thinking + 2k answer.
        "max_tokens": 4096,
    }
    req = urllib.request.Request(
        GROQ_URL,
        data=json.dumps(body).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {GROQ_API_KEY}",
            "Content-Type": "application/json",
            # Cloudflare error 1010 blocks Python-urllib's default User-Agent.
            "User-Agent": "curl/8.0",
            "Accept": "*/*",
        },
        method="POST",
    )
    t0 = time.time()
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            data = json.loads(r.read().decode("utf-8"))
        msg = data["choices"][0]["message"]
        # Reasoning models put visible answer in `content`; some emit only `reasoning`.
        text = msg.get("content") or msg.get("reasoning") or ""
        return (text, time.time() - t0, None)
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")
        return (None, time.time() - t0, f"HTTP {e.code}: {body[:300]}")
    except Exception as e:
        return (None, time.time() - t0, f"{type(e).__name__}: {e}")

# ─── Main ─────────────────────────────────────────────────────────────────────

def main():
    db = sqlite3.connect(DB_PATH)
    db.row_factory = None  # tuples

    out = []
    out.append(f"# EchoAI cloud eval — Cerebras {GROQ_MODEL}")
    out.append("")
    out.append(f"Date: 2026-06-04")
    out.append(f"DB: `{DB_PATH}`")
    out.append(f"Source: constitution_ir.json (re-indexed after Fix B)")
    out.append(f"Retrieval: FTS5-only (BM25), K={TOP_K}, source-scoped to constitution")
    out.append("")
    out.append("This script replicates the `intent_router::dispatch` pipeline in Python and calls Groq")
    out.append("for the LLM step. The 5 queries are the same we've been testing against locally.")
    out.append("")

    per_query = []

    for qid, qtext in QUERIES:
        print(f"→ {qid}: {qtext}", file=sys.stderr)
        intent, conf = classify(qtext)
        slots = extract_section_paths(qtext)
        chunks = retrieve(db, qtext, k=TOP_K)

        section = []
        section.append(f"\n---\n\n## {qid} — `{qtext}`\n")
        section.append(f"- Intent: `{intent}` (conf {conf:.2f})")
        section.append(f"- Section paths extracted: `{slots}`")
        section.append(f"- Chunks retrieved: **{len(chunks)}**")

        # Out-of-scope intents short-circuit with upgrade prompt (same as dispatch)
        if intent in ("Difference", "CrossCompare", "Timeline", "Procedure"):
            section.append(f"- Out-of-scope on fast tier → UpgradePrompt")
            section.append(f"\n**Answer:** _(skipped; needs deep re-index)_")
            per_query.append((qid, intent, None, 0.0, "OUT_OF_SCOPE"))
            out.extend(section)
            continue

        if not chunks:
            section.append(f"\n**Answer:** _(no chunks; would refuse)_")
            per_query.append((qid, intent, "no_chunks", 0.0, "NO_CHUNKS"))
            out.extend(section)
            continue

        ctx = build_context(chunks)
        sys_p = system_prompt(intent, slots)
        user_p = f"Context passages:\n{ctx}\n\nQuestion: {qtext}"

        ans, elapsed, err = groq_call(sys_p, user_p)
        if err:
            section.append(f"- ❌ Groq error: {err}")
            per_query.append((qid, intent, None, elapsed, f"ERR: {err}"))
        else:
            section.append(f"- Groq elapsed: **{elapsed:.2f}s**")
            section.append(f"\n**Answer:**\n\n```\n{ans}\n```\n")
            per_query.append((qid, intent, ans, elapsed, "OK"))
        out.extend(section)

    # ── Summary table ────────────────────────────────────────────────────────
    out.append("\n---\n\n## Summary")
    out.append("")
    out.append("| QID | Intent | Status | Latency |")
    out.append("|-----|--------|--------|---------|")
    for qid, intent, ans, elapsed, status in per_query:
        sym = {"OK": "✓", "OUT_OF_SCOPE": "↪ bail", "NO_CHUNKS": "—"}.get(status, "?")
        out.append(f"| {qid} | {intent} | {sym} {status} | {elapsed:.2f}s |")
    out.append("")
    out.append("Expected outcomes:")
    out.append("- **Q1**: enumerate Articles 29–40 with titles (Principles of Policy)")
    out.append("- **Q11**: enumerate Articles 8–28 with titles (Fundamental Rights)")
    out.append("- **Q1-fake**: refuse exactly ('No enumerable children found in the retrieved passages.')")
    out.append("- **Q3**: cite rights from all three Articles 9, 10, 10A")
    out.append("- **Q2**: clean Article 19 summary with restriction grounds")

    with open(RESULTS_FILE, "w") as f:
        f.write("\n".join(out))
    print(f"\nWrote {RESULTS_FILE}", file=sys.stderr)

if __name__ == "__main__":
    main()
