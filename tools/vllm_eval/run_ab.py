#!/usr/bin/env python3
"""F016 T029 — A/B no-regression harness for the inference-engine cutover gate.

Runs the SAME question set through the live /api/query stack and records, per
question: the answer, end-to-end latency, citation presence, grounding-fallback,
and refusal behaviour. Engine-agnostic: run it once per arm (llama.cpp GGUF,
vLLM-AWQ, vLLM-GPTQ) by flipping NOVIQ_INFER_ENGINE/NOVIQ_VLLM_MODEL on the
server between runs, then diff the per-engine JSONL with --compare.

Scoring is intentionally behavioural (not exact-match) because the questions are
complex/ambiguous by design: the gate is "does engine X keep citing + keep
refusing out-of-corpus material as well as the llama.cpp baseline", per FR-014.

Usage:
  run_ab.py run --engine awq  --questions eval_set_sindh.json --pass PW --out out_awq.jsonl
  run_ab.py compare out_llamacpp.jsonl out_awq.jsonl out_gptq.jsonl
"""
import argparse, json, re, sys, time, urllib.request, urllib.error

GROUNDING_FALLBACK = "No relevant information found in provided documents"
# Phrases that signal a (correct) refusal / "not determinable" answer. The app's
# own refusal lines are "do not contain enough information to answer" and the
# GROUNDING_FALLBACK string; the rest catch model-phrased refusals.
REFUSAL_PAT = re.compile(
    r"(do(es)? not contain (enough|sufficient|any|the) (information|details)"
    r"|not (contain|include) enough information|no (relevant )?information (was )?(found|available|present)"
    r"|cannot be determined|could not (be )?(determined|found)"
    r"|not (be )?(determined|found|specif|available|present|mention)"
    r"|does not (specify|contain|mention|provide)|not (?:explicitly )?(?:stated|specified) in"
    r"|unable to (find|determine|locate|answer)|no (such|specific) (section|provision|rule)"
    r"|is not (defined|provided|found)|outside the scope|not covered by|i (don'?t|do not) have"
    r"|insufficient information|not available in (the|these) (provided )?document)",
    re.IGNORECASE,
)
CITE_PAT = re.compile(r"\[\d+\]")
ERR_PAT = re.compile(r"^<(ERR|HTTP)\b")


def login(base, user, pw):
    body = json.dumps({"username": user, "password": pw}).encode()
    req = urllib.request.Request(base + "/api/login", body,
                                 {"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read())["token"]


def ask(base, token, qid, text, timeout):
    body = json.dumps({"conversationId": f"ab-{qid}-{int(time.time())}",
                       "text": text}).encode()
    req = urllib.request.Request(base + "/api/query", body,
                                 {"Content-Type": "application/json",
                                  "Authorization": "Bearer " + token})
    t0 = time.time()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            j = json.loads(r.read())
        dt = time.time() - t0
        return j.get("answer", ""), j.get("capability", ""), round(dt * 1000), 200
    except urllib.error.HTTPError as e:
        return f"<HTTP {e.code}>", "", round((time.time() - t0) * 1000), e.code
    except Exception as e:
        return f"<ERR {e}>", "", round((time.time() - t0) * 1000), 0


def score(ans, expect):
    if ERR_PAT.search(ans):
        return False, False, False, "TIMEOUT_ERR"
    has_cite = bool(CITE_PAT.search(ans))
    is_fallback = GROUNDING_FALLBACK.lower() in ans.lower()
    is_refusal = is_fallback or bool(REFUSAL_PAT.search(ans))
    if expect == "refuse":
        verdict = "REFUSED_OK" if is_refusal else "FABRICATED_RISK"
    else:
        # an answer arm: good = engaged + grounded (cited), not a fallback/refusal
        if is_fallback:
            verdict = "FALLBACK"
        elif is_refusal:
            verdict = "REFUSED"  # may be acceptable for the ambiguous ones
        elif has_cite:
            verdict = "CITED"
        else:
            verdict = "UNCITED"
    return has_cite, is_refusal, is_fallback, verdict


def run(a):
    qs = json.load(open(a.questions, encoding="utf-8"))["questions"]
    token = login(a.base, a.user, a.__dict__["pass"])
    out = open(a.out, "w", encoding="utf-8")
    print(f"=== engine={a.engine}  {len(qs)} questions ===")
    for q in qs:
        ans, cap, ms, code = ask(a.base, token, q["id"], q["question"], a.timeout)
        has_cite, is_ref, is_fb, verdict = score(ans, q["expect"])
        rec = {"id": q["id"], "category": q["category"], "expect": q["expect"],
               "engine": a.engine, "http": code, "latency_ms": ms,
               "capability": cap, "has_cite": has_cite, "is_refusal": is_ref,
               "verdict": verdict, "answer": ans}
        out.write(json.dumps(rec, ensure_ascii=False) + "\n"); out.flush()
        print(f"  {q['id']} [{q['category']:14s} exp:{q['expect']:6s}] "
              f"{ms:6d}ms  {verdict:15s} cite={int(has_cite)} :: {ans[:90].replace(chr(10),' ')}")
    out.close()
    print(f"written {a.out}")


def compare(files):
    by_id, engines = {}, []
    for f in files:
        eng = None
        for line in open(f, encoding="utf-8"):
            r = json.loads(line); eng = r["engine"]
            # Re-score from the stored answer so scorer fixes apply retroactively.
            r["_v"] = score(r["answer"], r["expect"])[3]
            by_id.setdefault(r["id"], {})[eng] = r
        engines.append(eng)
    print("\n=== A/B COMPARISON (verdict | latency_ms) ===")
    print("ID    category        expect  " + "  ".join(f"{e:>20s}" for e in engines))
    agg = {e: {"cited": 0, "refused_ok": 0, "fabricated": 0, "fallback": 0, "timeout": 0,
               "lat": [], "n_answer": 0, "n_refuse": 0} for e in engines}
    for qid in sorted(by_id):
        row = by_id[qid]
        any_r = next(iter(row.values()))
        cells = []
        for e in engines:
            r = row.get(e)
            if not r:
                cells.append(f"{'--':>20s}"); continue
            v = r["_v"]
            cells.append(f"{v:>13s}|{r['latency_ms']:>5d}")
            s = agg[e]
            if v == "TIMEOUT_ERR":
                s["timeout"] += 1; continue       # exclude from latency + quality denom
            s["lat"].append(r["latency_ms"])
            if r["expect"] == "answer":
                s["n_answer"] += 1
                if v == "CITED": s["cited"] += 1
                if v == "FALLBACK": s["fallback"] += 1
            else:
                s["n_refuse"] += 1
                if v == "REFUSED_OK": s["refused_ok"] += 1
                if v == "FABRICATED_RISK": s["fabricated"] += 1
        print(f"{qid:5s} {any_r['category']:14s} {any_r['expect']:6s}  " + "  ".join(cells))
    print("\n=== AGGREGATE (timeouts excluded from rates) ===")
    for e in engines:
        s = agg[e]
        lat = sorted(s["lat"]); p50 = lat[len(lat)//2] if lat else 0
        p95 = lat[int(0.95*len(lat))-1] if lat else 0
        print(f"{e:>20s}: answer CITED {s['cited']}/{s['n_answer']}  FALLBACK {s['fallback']}  "
              f"| refuse REFUSED_OK {s['refused_ok']}/{s['n_refuse']}  FABRICATED {s['fabricated']}  "
              f"| timeouts {s['timeout']}  | p50 {p50}ms p95 {p95}ms")
    print("\nGATE (FR-014): a vLLM arm passes if, vs llama.cpp, it does NOT lose citations on "
          "answer-arm Qs and does NOT increase FABRICATED on refuse-arm Qs. Hedged fabrications "
          "(e.g. 'not stated, but generally X [1]') need an eyeball — see per-question answers.")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--engine", required=True)
    r.add_argument("--questions", required=True)
    r.add_argument("--base", default="http://127.0.0.1:8080")
    r.add_argument("--user", default="admin")
    r.add_argument("--pass", required=True, dest="pass")
    r.add_argument("--out", required=True)
    r.add_argument("--timeout", type=int, default=180)
    c = sub.add_parser("compare")
    c.add_argument("files", nargs="+")
    a = p.parse_args()
    if a.cmd == "run":
        run(a)
    else:
        compare(a.files)
