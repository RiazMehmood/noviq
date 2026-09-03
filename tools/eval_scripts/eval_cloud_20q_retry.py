#!/usr/bin/env python3
"""Retry only the ERR rows from /tmp/echoai_20prompt_cloud.json with longer
timeouts + 3s spacing to avoid Cerebras 429 / read-timeout on big queries."""
import json, time, sys, urllib.request, urllib.error, importlib.util, sqlite3

spec = importlib.util.spec_from_file_location("ec", "/tmp/eval_cloud_20.py")
ec = importlib.util.module_from_spec(spec); spec.loader.exec_module(ec)

DELAY_BETWEEN = 4.0
TIMEOUT = 240

def call_cloud(system, user):
    body=json.dumps({"model":ec.CB_MODEL,"messages":[{"role":"system","content":system},{"role":"user","content":user}],"temperature":0.2,"max_tokens":4096}).encode()
    req=urllib.request.Request(ec.CB_URL, data=body, headers={"Authorization":f"Bearer {ec.CB_KEY}","Content-Type":"application/json","User-Agent":"curl/8.0","Accept":"*/*"}, method="POST")
    t0=time.time()
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
            d=json.loads(r.read().decode())
        m=d["choices"][0]["message"]
        return (m.get("content") or m.get("reasoning") or "", time.time()-t0, None)
    except urllib.error.HTTPError as e:
        return (None, time.time()-t0, f"HTTP {e.code}: {e.read().decode('utf-8','replace')[:200]}")
    except Exception as e:
        return (None, time.time()-t0, f"{type(e).__name__}: {e}")

def main():
    rows = json.load(open("/tmp/echoai_20prompt_cloud.json"))
    by_qid = {r["qid"]: r for r in rows}
    err_qids = [r["qid"] for r in rows if r["verdict"] == "ERR"]
    print(f"Retrying {len(err_qids)} ERR rows: {err_qids}", file=sys.stderr)

    db = sqlite3.connect(ec.DB_PATH)
    # rebuild prompts lookup
    prompts = {p[0]: p for p in ec.PROMPTS}

    for i, qid in enumerate(err_qids):
        qid_int, qtext, all_kw, any_kw, hard_neg = prompts[qid]
        intent, conf = ec.classify(qtext)
        slots = ec.extract_paths(qtext)
        chunks = ec.retrieve(db, qtext)
        intent_for_prompt = "Fallback" if intent in ("Difference","CrossCompare","Timeline","Procedure") else intent
        sp = ec.sys_prompt(intent_for_prompt, slots)
        up = f"Context passages:\n{ec.build_context(chunks)}\n\nQuestion: {qtext}"

        # up to 3 attempts with backoff
        ans, dt, err = None, 0.0, None
        for attempt in range(3):
            ans, dt, err = call_cloud(sp, up)
            if err is None:
                break
            print(f"  Q{qid} attempt {attempt+1} failed ({err[:60]}); backoff {(attempt+1)*5}s", file=sys.stderr)
            time.sleep((attempt+1)*5)

        verdict, reason = (("ERR", err) if err else ec.grade(ans, all_kw, any_kw, hard_neg))
        by_qid[qid].update({"verdict": verdict, "reason": reason, "latency_s": round(dt,2), "answer": ans})
        print(f"  Q{qid} → {verdict} ({dt:.1f}s) — {reason[:80] if reason else ''}", file=sys.stderr)
        if i < len(err_qids) - 1:
            time.sleep(DELAY_BETWEEN)

    out = sorted(by_qid.values(), key=lambda r: r["qid"])
    json.dump(out, open("/tmp/echoai_20prompt_cloud.json","w"), indent=2)
    tallies={}
    for r in out: tallies[r["verdict"]]=tallies.get(r["verdict"],0)+1
    print(f"\nFinal: {tallies}", file=sys.stderr)
    print(f"Avg latency: {sum(r['latency_s'] for r in out)/len(out):.2f}s", file=sys.stderr)

if __name__ == "__main__":
    main()
