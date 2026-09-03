"""
Shared helpers for the RAG-SFT distillation pipeline (Feature 019).

Pure stdlib (urllib) — no pip deps — so generate/validate run anywhere with just an
auth token. See specs/019-rag-sft-distillation/spec.md.

Auth + query contracts (verified against the codebase 2026-07-03):
  POST /api/login  {username, password}            -> { token: "<bearer>" , ... }
  POST /api/query  {conversationId, text} + Bearer -> TurnResponse:
       { conversation_id, message_id, answer, citations: [
             { n, instrument, path, verbatim, page } ], offer, ... }
"""

import json
import urllib.request
import urllib.error
import uuid

# The production chat system prompt (guidance capability, prompt v5.4.0), captured
# from a live query trace 2026-07-03. VERIFY against the current prod prompt before a
# real run — the fine-tune must be trained on the exact system message it will be
# served with. Kept as one canonical block here so every record shares it.
SYSTEM_PROMPT = (
    "You are a professional Pakistan government bureaucrat assistant with complete "
    "knowledge of the indexed laws, rules, and procedures. You guide officers through "
    "procedures and the governing laws, citing every legal reference verbatim from the "
    "provided context. You never invent a law, number, or quote: if the context does not "
    "support a claim, you say you could not find it in the indexed documents and offer to "
    "index the relevant document. You answer like a knowledgeable colleague — clear, "
    "precise, and grounded.\n\n"
    "Answer the officer's question in two parts. FIRST give a complete general answer: the "
    "standard procedure and the governing laws, each cited. THEN, on a new line, add ONE "
    "short sentence offering to refine it. Do not ask questions before giving the general "
    "answer.\n\n"
    "You answer the user's question using ONLY the retrieved passages in the Context block. "
    "If the passages do not contain enough information, reply exactly: "
    "'The retrieved documents do not contain enough information to answer this question.'\n"
    "- CRITICAL: Only use citation numbers that appear in the Context passages. If you see "
    "[1], [2], [3] in the context, you MUST NOT use [4] or higher.\n"
    "- Do not output a References section — the UI renders sources separately.\n"
    "- Output only the final answer."
)

REFUSAL_TEXT = "The retrieved documents do not contain enough information to answer this question."


class ApiError(Exception):
    pass


def _post(base_url, path, body, token=None, timeout=180):
    url = base_url.rstrip("/") + path
    data = json.dumps(body).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode("utf-8")
            return json.loads(raw) if raw else {}
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf-8", "replace")
        raise ApiError(f"{path} -> HTTP {e.code}: {detail[:300]}")
    except urllib.error.URLError as e:
        raise ApiError(f"{path} -> {e}")


def login(base_url, username, password):
    """Return a bearer token."""
    resp = _post(base_url, "/api/login", {"username": username, "password": password})
    token = resp.get("token") or resp.get("bearer") or resp.get("access_token")
    if not token:
        raise ApiError(f"login: no token in response keys={list(resp.keys())}")
    return token


def ask(base_url, token, question, conversation_id=None):
    """Run ONE chat turn. Fresh conversation per call avoids carry-over affecting
    retrieval. Returns the raw TurnResponse dict."""
    cid = conversation_id or str(uuid.uuid4())
    return _post(base_url, "/api/query", {"conversationId": cid, "text": question}, token=token)


def build_context_block(citations):
    """Rebuild the [N] Context block from TurnResponse.citations, ordered by n."""
    lines = ["Context:"]
    for c in sorted(citations, key=lambda x: x.get("n", 0)):
        n = c.get("n")
        instrument = (c.get("instrument") or "").strip()
        path = (c.get("path") or "").strip()
        verbatim = (c.get("verbatim") or "").strip().replace("\n", " ")
        head = " — ".join([p for p in (instrument, path) if p])
        lines.append(f"[{n}] ({head}) {verbatim}" if head else f"[{n}] {verbatim}")
    return "\n".join(lines)


def to_record(question, turn_response, source_q="unknown", kind=None):
    """Turn a (question, TurnResponse) into an SFT record. `kind` is auto-derived
    (answer vs refusal) when not supplied."""
    citations = turn_response.get("citations") or []
    answer = (turn_response.get("answer") or "").strip()
    ctx = build_context_block(citations)
    user = f"{ctx}\n\nQuestion: {question}"
    if kind is None:
        kind = "refusal" if (not citations or REFUSAL_TEXT.lower() in answer.lower()) else "answer"
    return {
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user},
            {"role": "assistant", "content": answer},
        ],
        "meta": {
            "kind": kind,
            "source_q": source_q,
            "citations": [{"n": c.get("n"), "path": c.get("path")} for c in citations],
            "message_id": turn_response.get("message_id"),
            "offer": turn_response.get("offer"),
        },
    }


def read_jsonl(path):
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                yield json.loads(line)


def write_jsonl(path, records):
    n = 0
    with open(path, "w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
            n += 1
    return n
