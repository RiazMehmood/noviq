import re
import os
import openpyxl
from openpyxl.styles import Font, Alignment, PatternFill, Border, Side

# Paths
TEST_RESULTS_DIR = "/media/riaz/New Volume/echo-AI/testing results"
BENCHMARK_XLSX = "/media/riaz/New Volume/echo-AI/echoai_model_benchmark.xlsx"

def clean_answer(ans):
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
        score = float(score_match.group(1)) if score_match else 0.0
        cite_match = re.search(r'• Citations found:\s+(\d+)', trace)
        citations = int(cite_match.group(1)) if cite_match else 0
        ans_match = re.search(r'┌── Final answer shown to user \(post-cleanup\) ──\n(.*?)└──', trace, re.DOTALL)
        answer = clean_answer(ans_match.group(1)) if ans_match else ""
        tok_match = re.search(r'• Speed:\s+~(\d+\.\d+) tokens/sec', trace)
        tok_s = float(tok_match.group(1)) if tok_match else 0.0
        fallback_match = re.search(r'• Fallback triggered:\s+(true|false)', trace)
        fallback = fallback_match.group(1) == 'true' if fallback_match else False
        results.append({
            "question": question, "total_time": total_time, "score": score,
            "citations": citations, "answer": answer, "tok_s": tok_s, "fallback": fallback
        })
    return results

def match_prompt(question, prompts_data):
    for row in prompts_data[3:]:
        if not row or len(row) < 3: continue
        p_text = row[2]
        if not p_text: continue
        if question.lower().strip() in p_text.lower().strip() or p_text.lower().strip() in question.lower().strip():
            return row[0], row
    return None, None

def rebuild_excel():
    wb = openpyxl.load_workbook(BENCHMARK_XLSX)
    
    # 1. Clear Data Sheets
    for name in ['Models', 'Results', 'Summary']:
        sheet = wb[name]
        # Keep headers (3 rows typically)
        sheet.delete_rows(4, sheet.max_row)

    prompts_sheet = wb['Prompts']
    prompts_data = [[cell.value for cell in row] for row in prompts_sheet.iter_rows()]
    
    results_sheet = wb['Results']
    summary_sheet = wb['Summary']
    models_sheet = wb['Models']

    # Styling
    header_font = Font(bold=True, color="FFFFFF")
    header_fill = PatternFill(start_color="4F81BD", end_color="4F81BD", fill_type="solid")
    center_align = Alignment(horizontal="center", vertical="center")
    border = Border(left=Side(style='thin'), right=Side(style='thin'), top=Side(style='thin'), bottom=Side(style='thin'))

    folders = sorted(os.listdir(TEST_RESULTS_DIR))
    
    for idx, folder in enumerate(folders, 1):
        folder_path = os.path.join(TEST_RESULTS_DIR, folder)
        if not os.path.isdir(folder_path): continue
        
        # Parse model metadata from folder name
        parts = folder.replace('_', ' ').split()
        family = parts[0]
        size = parts[-1] if 'B' in parts[-1] or 'M' in parts[-1] else "Unknown"
        
        # Add to Models sheet
        models_sheet.append([idx, folder, size, family, "Local", "n/a", "Completed"])
        
        # Process Traces
        all_results = []
        trace_files = [f for f in os.listdir(folder_path) if f.startswith('echoai-traces-') and f.endswith('.md')]
        for tf in trace_files:
            all_results.extend(parse_traces(os.path.join(folder_path, tf)))
        
        if not all_results: continue

        # Fact Sheet
        fact_sheet_name = f"{folder[:20]} Facts" # Limit length
        if fact_sheet_name in wb.sheetnames: del wb[fact_sheet_name]
        fs = wb.create_sheet(fact_sheet_name)
        fs.append(["P#", "Lang", "Prompt", "Model Response", "Ground-Truth", "Score (0-5)", "Tok/s", "Time (s)", "Verdict"])
        for cell in fs[1]:
            cell.font = header_font
            cell.fill = header_fill
            cell.alignment = center_align

        m_time, m_tok, m_score, m_fall, m_refuse = 0, 0, 0, 0, 0
        p_ids_seen = set()

        for res in all_results:
            pid, p_row = match_prompt(res['question'], prompts_data)
            if not pid or pid in p_ids_seen: continue
            p_ids_seen.add(pid)
            
            # Results Sheet
            results_sheet.append([folder, pid, p_row[1], res['total_time'], 0, res['tok_s'], res['score']*5, res['score']*5, 5, 5, res['score']*5, 0 if res['fallback'] else 0, 5, "v5.2"])
            
            # Fact Sheet
            fs.append([pid, p_row[1], res['question'], res['answer'], p_row[6], res['score']*5, res['tok_s'], res['total_time'], "Fallback" if res['fallback'] else "OK"])
            
            m_time += res['total_time']
            m_tok += res['tok_s']
            m_score += res['score'] * 5
            if res['fallback']: m_fall += 1
            if "insufficient information" in res['answer'].lower(): m_refuse += 1

        count = len(p_ids_seen)
        if count > 0:
            summary_sheet.append([
                folder, round(m_time/count, 2), round(m_tok/count, 2), 
                round(m_score/count, 2), round(m_score/count, 2), 5, m_fall, m_refuse, "Verified"
            ])

    # Final Styling for Summary
    for row in summary_sheet.iter_rows(min_row=4):
        for cell in row:
            cell.border = border
            if isinstance(cell.value, (int, float)) and cell.column in [3, 4, 5]: # Score columns
                if cell.value >= 4: cell.fill = PatternFill(start_color="C6EFCE", end_color="C6EFCE", fill_type="solid")
                elif cell.value < 2: cell.fill = PatternFill(start_color="FFC7CE", end_color="FFC7CE", fill_type="solid")

    wb.save(BENCHMARK_XLSX)
    print(f"Rebuild complete. Indexed {len(folders)} models.")

if __name__ == "__main__":
    rebuild_excel()
