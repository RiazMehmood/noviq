import re
import json

def parse_traces(file_path):
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Split by TRACE marker
    traces = re.split(r'# TRACE \d+/\d+', content)
    results = []
    
    for trace in traces:
        if not trace.strip():
            continue
            
        # Extract Question
        q_match = re.search(r'• Question:\s+"([^"]+)"', trace)
        if not q_match:
            # Try Step 1 format
            q_match = re.search(r'Question:\s+(.*)', trace)
        
        question = q_match.group(1) if q_match else "Unknown"
        
        # Extract ID and Session
        id_match = re.search(r'Trace ID:\s+(\w+)', trace)
        trace_id = id_match.group(1) if id_match else "Unknown"
        
        # Extract Time
        time_match = re.search(r'Total time:\s+(\d+) ms', trace)
        total_time = int(time_match.group(1)) / 1000.0 if time_match else 0.0
        
        # Extract Model
        model_match = re.search(r'Model tier:\s+(.*)', trace)
        model = model_match.group(1).strip() if model_match else "Unknown"
        
        # Extract Grounding Score
        score_match = re.search(r'• Score:\s+(\d+\.\d+) / 1\.00', trace)
        score = float(score_match.group(1)) if score_match else 0.0
        
        # Extract Citations
        cite_match = re.search(r'• Citations found:\s+(\d+)', trace)
        citations = int(cite_match.group(1)) if cite_match else 0
        
        # Extract Final Answer
        ans_match = re.search(r'┌── Final answer shown to user \(post-cleanup\) ──\n(.*?)└──', trace, re.DOTALL)
        answer = ans_match.group(1).strip() if ans_match else ""
        
        # Extract Tokens/sec
        tok_match = re.search(r'• Speed:\s+~(\d+\.\d+) tokens/sec', trace)
        tok_s = float(tok_match.group(1)) if tok_match else 0.0
        
        results.append({
            "trace_id": trace_id,
            "question": question,
            "total_time": total_time,
            "model": model,
            "score": score,
            "citations": citations,
            "answer": answer,
            "tok_s": tok_s
        })
    
    return results

if __name__ == "__main__":
    path = "/media/riaz/New Volume/echo-AI/testing results/Llama3.2_1B/echoai-traces-2026-05-12T07-38-40.md"
    data = parse_traces(path)
    print(json.dumps(data[:2], indent=2))
