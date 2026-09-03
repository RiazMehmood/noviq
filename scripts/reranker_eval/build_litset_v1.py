"""Build eval set v2 for English-literature corpus (4 Gutenberg classics).

Each row: {"qid": int, "query": str, "pos": [gold_chunk_content], "book": str,
           "chunk_id": str}

The "pos" list contains the FULL content of the indexed chunk that legitimately
contains the answer — same convention as data/reranker/v1/eval.jsonl, so
run_eval.py works unchanged.
"""
from __future__ import annotations
import json
import sqlite3
from pathlib import Path

DB = Path("/media/riaz/New Volume/home-overflow/com.echoai.app/echo.db")
OUT = Path("/media/riaz/New Volume/echo-AI/data/reranker/litset_v1/eval.jsonl")

# (question, anchor_chunk_id, book_slug)
QUESTIONS: list[tuple[str, str, str]] = [
    # ─── Dracula ───────────────────────────────────────────────────────────
    ("Who is Jonathan Harker and how does he identify himself when first asked?",
     "ee2527aa-ac9e-46df-8c6e-5003924bc52b", "dracula"),
    ("What is the name of the post town named by Count Dracula in his correspondence?",
     "5a741cd8-7a1d-4629-b972-4742d1228567", "dracula"),
    ("Who is Professor Van Helsing, and where is he from?",
     "10064738-a2c8-4a38-bba9-fbf2e89e8dba", "dracula"),
    ("What is the name of the ship that carries Dracula from Varna?",
     "a7ec9b1f-4bc6-4804-85ff-8ddd45f1512e", "dracula"),
    ("What is the name of the estate Dracula acquires in England, and what is its supposed etymology?",
     "3a899e53-a0cc-4c62-a13f-5b7a51f5fe7f", "dracula"),
    ("Which mountain pass leads from Dracula's region into Bukovina?",
     "5d33237b-2736-49d6-986a-a7d32c8dac81", "dracula"),
    ("What phrase does Renfield repeat about blood?",
     "ec02ebf8-6407-48ea-b5dd-afb2e271d57d", "dracula"),

    # ─── Pride and Prejudice ───────────────────────────────────────────────
    ("What does Mrs. Bennet first tell Mr. Bennet about Netherfield Park?",
     "b3aab256-8802-47c0-ab59-a0c737d2eebf", "pp"),
    ("What annual income did the gentlemen attribute to Mr. Bingley?",
     "3fd8031f-22f8-4079-a07f-bf12e130f26b", "pp"),
    ("Whose patronage does Mr. Collins boast of, and what is her title?",
     "fca1d450-a4b2-4025-8467-16aa6e407c82", "pp"),
    ("Who is Mr. Collins and why does he correspond with Mr. Bennet?",
     "4f7b933d-019d-4428-8573-30cc954b1fba", "pp"),
    ("Who calls Elizabeth an 'obstinate, headstrong girl' and in what context?",
     "94ef1afa-11c6-49bc-b307-27b0700a60f8", "pp"),
    ("Who introduces Mr. Wickham, and where had he just returned from?",
     "dd6d700d-548c-4938-8f4b-6b04d47ce4ac", "pp"),

    # ─── Frankenstein ──────────────────────────────────────────────────────
    ("On what kind of night did Victor Frankenstein bring his creature to life?",
     "8cfa1efa-f818-4e96-87f5-466e806eb859", "frank"),
    ("At which university did Victor Frankenstein study?",
     "53f3759e-9000-4351-b14b-f8a35a11b807", "frank"),
    ("How did Elizabeth Lavenza come to live with the Frankenstein family?",
     "42dbf031-f004-44b8-bf46-4681431810be", "frank"),
    ("Who was Henry Clerval, and what was his background?",
     "7703bdde-04f4-4031-adca-563a9fb4fddc", "frank"),
    ("Who is Captain Walton and what does the stranger he rescues tell him?",
     "c557cd25-5bbd-4434-9e3c-efa02fa28ef5", "frank"),
    ("What declaration of war does the creature make after being shot?",
     "01c391d0-1a05-4d58-b414-b3312f2ce394", "frank"),

    # ─── The Picture of Dorian Gray ────────────────────────────────────────
    ("Who is Basil Hallward, and what mysterious event surrounds him?",
     "02030283-593a-4ebb-8585-ecabeeff1add", "dorian"),
    ("According to Lord Henry, what is the only thing in the world worse than being talked about?",
     "02030283-593a-4ebb-8585-ecabeeff1add", "dorian"),
    ("What 'yellow book' does Dorian receive, and from whom?",
     "4a8c2a3f-f219-46e2-85b5-afd28ffba63c", "dorian"),
    ("In what place does Dorian decide the fatal portrait must be hidden?",
     "8ff7b330-eb7a-4bdb-84a4-e570426a63b6", "dorian"),
    ("Which Shakespearean roles does Sibyl Vane perform, according to the narration?",
     "e08d1665-56ca-4f73-bf0f-7068b1f31c75", "dorian"),
]


def main() -> None:
    con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    con.row_factory = sqlite3.Row

    OUT.parent.mkdir(parents=True, exist_ok=True)
    n_ok = 0
    with OUT.open("w") as f:
        for qid, (query, cid, book) in enumerate(QUESTIONS):
            row = con.execute(
                "SELECT content FROM chunks WHERE id = ?", (cid,)
            ).fetchone()
            if not row:
                print(f"[warn] q{qid:02d} chunk not found: {cid}")
                continue
            content = row["content"].strip()
            f.write(json.dumps({
                "qid": qid,
                "query": query,
                "pos": [content],
                "book": book,
                "chunk_id": cid,
            }) + "\n")
            n_ok += 1
    print(f"[done] wrote {n_ok} / {len(QUESTIONS)} rows → {OUT}")


if __name__ == "__main__":
    main()
