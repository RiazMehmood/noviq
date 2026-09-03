import re
import json
import os
import zipfile
import openpyxl
from openpyxl.styles import Font, Alignment, PatternFill
import xml.etree.ElementTree as ET

# Paths
TEST_RESULTS_DIR = "/media/riaz/New Volume/echo-AI/testing results"
BENCHMARK_XLSX = "/media/riaz/New Volume/echo-AI/echoai_model_benchmark.xlsx"

def clean_answer(ans):
    lines = ans.split('\n')
    cleaned = []
    for line in lines:
        line = line.strip()
        if line.startswith('│'):
            line = line[1:].strip()
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
            "question": question,
            "total_time": total_time,
            "score": score,
            "citations": citations,
            "answer": answer,
            "tok_s": tok_s,
            "fallback": fallback
        })
    return results

def extract_odt_text(file_path):
    try:
        with zipfile.ZipFile(file_path, 'r') as z:
            content = z.read('content.xml')
        root = ET.fromstring(content)
        text_content = []
        for p in root.findall('.//{urn:oasis:names:tc:opendocument:xmlns:text:1.0}p'):
            text = ''.join(p.itertext())
            text_content.append(text)
        return '\n'.join(text_content)
    except Exception as e:
        print(f"Error reading ODT {file_path}: {e}")
        return ""

def match_prompt(question, prompts_data):
    # prompts_data is list of [id, lang, prompt, category, expected, dimensions, truth, truth_source]
    # starts from index 3
    for row in prompts_data[3:]:
        if not row or len(row) < 3: continue
        p_text = row[2]
        if not p_text: continue
        # Fuzzy match or simple contains
        if question.lower().strip() in p_text.lower().strip() or p_text.lower().strip() in question.lower().strip():
            return row[0], row # ID
    return None, None

def update_excel():
    wb = openpyxl.load_workbook(BENCHMARK_XLSX)
    
    # Load Prompts for matching
    prompts_sheet = wb['Prompts']
    prompts_data = [[cell.value for cell in row] for row in prompts_sheet.iter_rows()]
    
    # Load Models for mapping
    models_sheet = wb['Models']
    # Map folder names to Model names in sheet
    folder_to_model = {
        "EXAONE 3.5 Instruct_2.4B": "EXAONE 3.5 Instruct",
        "Gemma2_2B": "Gemma 2 IT",
        "Granite 3.1 MoE_1B": "Granite 3.1 MoE Instruct",
        "Llama3.2_1B": "Llama 3.2 Instruct",
        "Llama3.2_3B": "Llama 3.2 Instruct",
        "Qwen2.5_0.5B": "Qwen 2.5 Instruct",
        "qwen2.5_3B": "Qwen 2.5 Instruct",
        "Qwen3_1.7B": "Qwen 3",
        "SmolLM_1.7B": "SmolLM2 Instruct",
        "SmolLM2_ 135M": "SmolLM2 Instruct",
        "SmolLM2_ 360M": "SmolLM2 Instruct"
    }

    results_sheet = wb['Results']
    summary_sheet = wb['Summary']
    
    model_stats = {}

    for folder in os.listdir(TEST_RESULTS_DIR):
        folder_path = os.path.join(TEST_RESULTS_DIR, folder)
        if not os.path.isdir(folder_path): continue
        
        model_name = folder_to_model.get(folder, folder)
        print(f"Processing model: {model_name} (folder: {folder})")
        
        all_results = []
        
        # Look for traces
        trace_files = [f for f in os.listdir(folder_path) if f.startswith('echoai-traces-') and f.endswith('.md')]
        if trace_files:
            for tf in trace_files:
                all_results.extend(parse_traces(os.path.join(folder_path, tf)))
        
        # Look for individual logs if traces missing
        if not all_results:
            log_files = [f for f in os.listdir(folder_path) if f.endswith('.log')]
            for lf in log_files:
                with open(os.path.join(folder_path, lf), 'r') as f:
                    content = f.read()
                    # Wrap in a fake trace header to use same parser
                    all_results.extend(parse_traces_str("# TRACE 1/1\n" + content))

        # Check ODT for answers if not in logs
        odt_path = os.path.join(folder_path, "responses.odt")
        if os.path.exists(odt_path):
            odt_text = extract_odt_text(odt_path)
            # This is harder to parse, but let's assume it's fallback
        
        if not all_results: continue

        # Create Fact Sheet for model
        fact_sheet_name = f"{folder} Facts"
        if fact_sheet_name in wb.sheetnames:
            del wb[fact_sheet_name]
        fs = wb.create_sheet(fact_sheet_name)
        fs.append(["P#", "Lang", "Prompt", f"Model Response ({model_name})", "Ground-Truth Answer", "Accuracy /5", "Reasoning /5", "Instr-Follow /5", "Citation Precision /5", "Hallucination /5 (5=none)", "Calibration /5", "Fluency /5", "Coherence /5", "Format /5", "Multilingual /5", "Total /50", "Verdict", "Notes"])
        
        # Fill Results and Fact Sheet
        model_total_time = 0
        model_total_tok_s = 0
        model_count = 0
        model_hallucinations = 0
        model_refusals = 0
        
        for res in all_results:
            pid, p_row = match_prompt(res['question'], prompts_data)
            if not pid: continue
            
            # Add to Results sheet
            # ["Model", "Prompt ID", "Lang", "Total time (s)", "TTFT (s)", "Tok/s", "Accuracy", "Reasoning", "Instruction-Following", "Retrieval Precision", "Citation Precision", "Hallucination Rate", "Format Adherence", "Notes"]
            results_sheet.append([
                model_name, pid, p_row[1], res['total_time'], None, res['tok_s'], 
                None, None, None, None, res['score'] * 5, 5 if not res['fallback'] else 1, None, f"Citations: {res['citations']}"
            ])
            
            # Add to Fact Sheet
            # ["P#", "Lang", "Prompt", "Response", "Truth", "Acc", "Reas", "Instr", "Cite", "Hallu", "Calib", "Fluency", "Coher", "Format", "Multi", "Total", "Verdict", "Notes"]
            fs.append([
                pid, p_row[1], res['question'], res['answer'], p_row[6], 
                None, None, None, res['score'] * 5, 5 if not res['fallback'] else 1, None, None, None, None, None, None, 
                "Fallback" if res['fallback'] else "Grounded", ""
            ])
            
            model_total_time += res['total_time']
            model_total_tok_s += res['tok_s']
            model_count += 1
            if res['fallback']: model_hallucinations += 1 # Rough proxy
            if "insufficient information" in res['answer'].lower(): model_refusals += 1
            
        if model_count > 0:
            avg_time = model_total_time / model_count
            avg_tok = model_total_tok_s / model_count
            # ["Model", "Avg Total Time (s)", "Avg Tok/s", "Avg Accuracy", "Avg Reasoning", "Avg Format", "Hallucination Count", "Refused Out-of-scope?", "Recommendation"]
            summary_sheet.append([
                model_name, avg_time, avg_tok, None, None, None, model_hallucinations, model_refusals > 0, "Evaluation Needed"
            ])
            
        # Update Models status
        for row in models_sheet.iter_rows(min_row=4):
            if row[1].value == model_name:
                row[6].value = "Completed (v5.2.0 sweep)"
                break

    wb.save(BENCHMARK_XLSX)
    print("Benchmark Excel updated successfully.")

def parse_traces_str(content):
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
        results.append({"question": question, "total_time": total_time, "score": score, "citations": citations, "answer": answer, "tok_s": tok_s, "fallback": fallback})
    return results

if __name__ == "__main__":
    update_excel()
