import re
import os
import openpyxl
from openpyxl.styles import Font, Alignment, PatternFill, Border, Side

# Paths
TEST_RESULTS_DIR = "/media/riaz/New Volume/echo-AI/testing results"
BENCHMARK_XLSX = "/media/riaz/New Volume/echo-AI/echoai_model_benchmark.xlsx"

def clean_answer(ans):
    if not ans: return ""
    lines = ans.split('\n')
    cleaned = []
    for line in lines:
        line = line.strip()
        if line.startswith('│'): line = line[1:].strip()
        cleaned.append(line)
    return '\n'.join(cleaned).strip()

def parse_traces(file_path):
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    traces = re.split(r'# TRACE \d+/\d+', content)
    results = []
    for trace in traces:
        if not trace.strip(): continue
        q_match = re.search(r'• Question:\s+"([^"]+)"', trace)
        if not q_match: q_match = re.search(r'Question:\s+(.*)', trace)
        question = q_match.group(1).strip() if q_match else None
        if not question: continue
        
        time_match = re.search(r'Total time:\s+(\d+) ms', trace)
        total_time = int(time_match.group(1)) / 1000.0 if time_match else 0.0
        
        score_match = re.search(r'• Score:\s+(\d+\.\d+) / 1\.00', trace)
        grounding_score = float(score_match.group(1)) if score_match else 0.0
        
        cite_match = re.search(r'• Citations found:\s+(\d+)', trace)
        citations_count = int(cite_match.group(1)) if cite_match else 0
        
        ans_match = re.search(r'┌── Final answer shown to user \(post-cleanup\) ──\n(.*?)└──', trace, re.DOTALL)
        answer = clean_answer(ans_match.group(1)) if ans_match else ""
        
        tok_match = re.search(r'• Speed:\s+~(\d+\.\d+) tokens/sec', trace)
        tok_s = float(tok_match.group(1)) if tok_match else 0.0
        
        fallback_match = re.search(r'• Fallback triggered:\s+(true|false)', trace)
        fallback = fallback_match.group(1) == 'true' if fallback_match else False

        # Extract retrieved sources for precision check
        sources_match = re.search(r'• Source files:\s+(.*)', trace)
        retrieved_sources = [s.strip() for s in sources_match.group(1).split(',')] if sources_match else []

        results.append({
            "question": question, 
            "total_time": total_time, 
            "grounding_score": grounding_score,
            "citations_count": citations_count, 
            "answer": answer, 
            "tok_s": tok_s, 
            "fallback": fallback,
            "retrieved_sources": retrieved_sources
        })
    return results

def evaluate_response(res, p_row):
    pid = p_row[0]
    ans = res['answer'].lower()
    truth = str(p_row[6]).lower() if p_row[6] else ""
    
    # 1. Accuracy (1, 3, 5)
    if res['fallback'] or "insufficient information" in ans or "do not contain enough information" in ans:
        accuracy = 1 if pid != "P14" else 5
    else:
        key_terms = [t.strip() for t in truth.split() if len(t.strip()) > 3]
        matches = sum(1 for term in key_terms if term in ans)
        if matches > len(key_terms) * 0.5: accuracy = 5
        elif matches > len(key_terms) * 0.15: accuracy = 3
        else: accuracy = 1

    # 2. Reasoning (1-5)
    reasoning = max(1, accuracy - 1) if accuracy < 5 else 5

    # 3. Retrieval Precision (1-5)
    expected = [s.strip().lower() for s in str(p_row[4]).split(';')]
    found = sum(1 for e in expected if any(e in r.lower() for r in res['retrieved_sources']))
    retr_prec = max(1, min(5, (found / len(expected)) * 5)) if expected else 5

    # 4. Citation Precision (1-5)
    cite_prec = max(1, round(res['grounding_score'] * 5))

    # 5. Hallucination Rate (1-5)
    hallu_rate = 1 if res['fallback'] else 5

    # 6. Calibration / Honest Refusal
    if pid == "P14":
        honest_refusal = 5 if ("insufficient information" in ans or res['fallback'] or "not provide" in ans) else 1
    else:
        honest_refusal = 5

    # 7. Multilingual
    lang = p_row[1]
    is_urdu_script = bool(re.search(r'[\u0600-\u06FF]', res['answer']))
    if lang in ['ur', 'sd']:
        multi_out = 5 if is_urdu_script else 1
        multi_und = 5 if accuracy >= 3 else 1
    else:
        multi_out = 5
        multi_und = 5

    return {
        "accuracy": accuracy,
        "reasoning": reasoning,
        "retr_prec": retr_prec,
        "cite_prec": cite_prec,
        "hallu_rate": hallu_rate,
        "honest_refusal": honest_refusal,
        "multi_und": multi_und,
        "multi_out": multi_out,
        "fluency": 5 if accuracy >= 3 else 3,
        "coherence": 5 if accuracy >= 3 else 3,
        "instr_follow": 5 if cite_prec >= 3 else 3,
        "format": 5 if "[" in res['answer'] else 3,
        "robustness": 5 if accuracy >= 3 else 3
    }

def rebuild_excel():
    wb = openpyxl.load_workbook(BENCHMARK_XLSX)
    
    for name in ['Models', 'Results', 'Summary']:
        sheet = wb[name]
        sheet.delete_rows(4, sheet.max_row)

    prompts_sheet = wb['Prompts']
    prompts_data = [[cell.value for cell in row] for row in prompts_sheet.iter_rows()]
    
    results_sheet = wb['Results']
    res_headers = ["Model", "Prompt ID", "Lang", "Total Time (s)", "Tok/s", "Accuracy", "Reasoning", "Instr-Follow", "Retr-Prec", "Cite-Prec", "Hallu-Rate", "Calibration", "Fluency", "Coherence", "Format", "Multi-Und", "Multi-Out", "Robustness", "Honest-Refusal", "Verdict", "Notes"]
    for i, h in enumerate(res_headers, 1): results_sheet.cell(3, i).value = h

    summary_sheet = wb['Summary']
    sum_headers = ["Model", "Avg Latency", "Avg Tok/s", "Avg Accuracy", "Avg Reasoning", "Avg RAG Prec", "Avg Hallu Rate", "Avg Calibration", "Honest Refusal?", "Recommendation"]
    for i, h in enumerate(sum_headers, 1): summary_sheet.cell(3, i).value = h

    models_sheet = wb['Models']
    border = Border(left=Side(style='thin'), right=Side(style='thin'), top=Side(style='thin'), bottom=Side(style='thin'))

    folders = sorted([f for f in os.listdir(TEST_RESULTS_DIR) if os.path.isdir(os.path.join(TEST_RESULTS_DIR, f))])
    
    for idx, folder in enumerate(folders, 1):
        folder_path = os.path.join(TEST_RESULTS_DIR, folder)
        parts = folder.split('_')
        family = parts[0]
        size = parts[-1]
        models_sheet.append([idx, folder, size, family, "Local", "n/a", "Completed"])
        
        all_results = []
        trace_files = [f for f in os.listdir(folder_path) if f.startswith('echoai-traces-') and f.endswith('.md')]
        for tf in trace_files:
            all_results.extend(parse_traces(os.path.join(folder_path, tf)))
        
        if not all_results: continue

        fact_sheet_name = f"{folder[:25]} Facts"
        if fact_sheet_name in wb.sheetnames: del wb[fact_sheet_name]
        fs = wb.create_sheet(fact_sheet_name)
        fs.append(["P#", "Lang", "Prompt", "Model Response", "Ground-Truth", "Acc", "Reas", "Retr", "Cite", "Hallu", "Verdict"])

        m_stats = {h: 0 for h in sum_headers[1:-2]}
        m_count = 0
        m_refusal_ok = True
        p_ids_seen = set()

        for res in all_results:
            match = None
            for r in prompts_data[3:]:
                if not r or not r[2]: continue
                q_text = str(r[2]).lower().strip()
                res_q = str(res['question']).lower().strip()
                if res_q in q_text or q_text in res_q:
                    match = r
                    break
            
            if not match: continue
            pid = match[0]
            if pid in p_ids_seen: continue
            p_ids_seen.add(pid)
            
            scores = evaluate_response(res, match)
            
            results_sheet.append([
                folder, pid, match[1], res['total_time'], res['tok_s'],
                scores['accuracy'], scores['reasoning'], scores['instr_follow'], scores['retr_prec'],
                scores['cite_prec'], scores['hallu_rate'], scores['accuracy'],
                scores['fluency'], scores['coherence'], scores['format'],
                scores['multi_und'], scores['multi_out'], scores['robustness'], scores['honest_refusal'],
                "Pass" if scores['accuracy'] >= 3 else "Fail", f"Cites: {res['citations_count']}"
            ])
            
            fs.append([pid, match[1], res['question'], res['answer'], match[6], scores['accuracy'], scores['reasoning'], scores['retr_prec'], scores['cite_prec'], scores['hallu_rate'], "OK" if scores['accuracy'] >= 3 else "Low Quality"])
            
            m_stats["Avg Latency"] += res['total_time']
            m_stats["Avg Tok/s"] += res['tok_s']
            m_stats["Avg Accuracy"] += scores['accuracy']
            m_stats["Avg Reasoning"] += scores['reasoning']
            m_stats["Avg RAG Prec"] += scores['retr_prec']
            m_stats["Avg Hallu Rate"] += scores['hallu_rate']
            m_stats["Avg Calibration"] += scores['accuracy']
            if pid == "P14" and scores['honest_refusal'] < 5: m_refusal_ok = False
            m_count += 1

        if m_count > 0:
            summary_row = [folder]
            for h in sum_headers[1:-2]: summary_row.append(round(m_stats[h] / m_count, 2))
            summary_row.extend([m_refusal_ok, "Strong" if m_stats["Avg Accuracy"]/m_count >= 4 else "Moderate" if m_stats["Avg Accuracy"]/m_count >= 2.5 else "Weak"])
            summary_sheet.append(summary_row)

    wb.save(BENCHMARK_XLSX)
    print("Rebuild with Rubric complete (fixed NoneType error).")

if __name__ == "__main__":
    rebuild_excel()
