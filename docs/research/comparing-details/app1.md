# App 1 — EchoAI Notebook & Knowledge Engine

A fully-offline desktop "private NotebookLM + Recall" that turns your local files into a cited, queryable knowledge base. No cloud, no account, no telemetry.

---

## 1. Technologies Used

### Backend (Rust)
| Layer | Library | Purpose |
|---|---|---|
| Shell | **Tauri 2.0** | Cross-platform desktop app (Rust backend + WebView frontend) |
| Async runtime | **Tokio 1.x** | Concurrency, background indexing, TaskBudget scheduler |
| LLM inference | **llama-cpp-2** (FFI to llama.cpp) | Local 3B orchestrator + 7B worker (GGUF, Q4_K_M) |
| Embeddings | **fastembed-rs** | nomic-embed-text-v1.5 (primary), all-MiniLM (fallback) |
| Speech-to-text | **whisper-rs** | Local audio/video transcription |
| Database | **SQLite (WAL)** + **sqlx** | Single `echo.db` file, async, compile-time query checking |
| Keyword search | **FTS5** (SQLite virtual table) | BM25 keyword retrieval |
| Vector search | **sqlite-vec** extension | Cosine similarity ANN |
| File watcher | **notify 6.x** | Detects modified files, triggers re-index |
| Hardware probe | **sysinfo** | CPU / RAM / battery-aware TaskBudget |
| Document parsers | **lopdf, docx-rs, calamine, zip** | PDF, DOCX, XLSX, archive I/O |

### Frontend (TypeScript)
- **SolidJS 1.x** + **@solidjs/router**
- **Tailwind CSS**
- **Vitest** for unit tests

### Data Model
Single SQLite file with tables: `notebooks`, `sources`, `chunks`, `chunks_vec` (sqlite-vec), `chunks_fts` (FTS5), `entities`, `relationships`, `query_sessions`, `batch_jobs`, `indexed_folders`, `audit_log`.

---

## 2. Working Flow

### 2.1 High-level component diagram

```
 ┌──────────────────────────────────────────────────────────────┐
 │                      SolidJS Frontend                        │
 │   [Notebook list] [Chat] [Global search] [KG view] [Batch]   │
 └──────────────▲─────────────────────────────────▲─────────────┘
                │ Tauri IPC (invoke / event)      │
 ┌──────────────┴─────────────────────────────────┴─────────────┐
 │                       Rust Backend                           │
 │                                                              │
 │  ┌───────────┐  ┌─────────────┐  ┌────────────────────────┐  │
 │  │ Ingestor  │─▶│ Chunker +   │─▶│ Embedder (fastembed)   │  │
 │  │ (parsers) │  │ normaliser  │  │                        │  │
 │  └───────────┘  └─────────────┘  └────────────────────────┘  │
 │        │                                       │             │
 │        ▼                                       ▼             │
 │  ┌──────────────────────────────────────────────────────┐    │
 │  │             SQLite (echo.db) — WAL                   │    │
 │  │  sources | chunks | chunks_vec | chunks_fts | …      │    │
 │  └──────────────────────────────────────────────────────┘    │
 │        ▲                                       ▲             │
 │        │                                       │             │
 │  ┌─────┴──────┐   ┌──────────────┐    ┌────────┴──────────┐  │
 │  │ Query      │──▶│ RRF Merger + │──▶ │ Grounded LLM      │  │
 │  │ Router     │   │ Ctx Builder  │    │ (3B / 7B)         │  │
 │  │ (5 tiers)  │   └──────────────┘    └───────────────────┘  │
 │  └────────────┘                                   │          │
 │                                                   ▼          │
 │                                   ┌──────────────────────┐   │
 │                                   │ Grounding Validator  │   │
 │                                   └──────────────────────┘   │
 └──────────────────────────────────────────────────────────────┘
```

### 2.2 Indexing flow (when a file is added)

```
  [User adds file / folder]
           │
           ▼
  ┌────────────────┐   unsupported   ┌──────────────┐
  │ Detect format  │────────────────▶│ Flag: failed │
  └────────┬───────┘                 └──────────────┘
           │ ok
           ▼
  ┌────────────────┐   status: pending → processing
  │ Parse text /   │
  │ transcribe A/V │
  │ describe image │
  └────────┬───────┘
           ▼
  ┌────────────────┐
  │ Chunk (≈512 t) │
  └────────┬───────┘
           ▼
  ┌────────────────┐     ┌──────────────────┐
  │ Embed chunks   │────▶│ Write to chunks, │
  │ (fastembed)    │     │ chunks_vec, fts  │
  └────────┬───────┘     └──────────────────┘
           ▼
  ┌────────────────────┐
  │ Extract entities / │  status: indexed
  │ relationships (KG) │
  └────────────────────┘
```

Throughout: **TaskBudget** pauses indexing when CPU > 40 %, user is active, or battery < 20 %.

### 2.3 Query flow (when the user asks a question)

```
  User question
       │
       ▼
  ┌─────────────────────────────────────────┐
  │ Tier 0: Semantic cache (session + 7-day)│──hit──▶ return
  └─────────────────────────────────────────┘
       │ miss
       ▼
  ┌─────────────────────────┐
  │ Tier 1: FTS5 keyword    │─┐
  └─────────────────────────┘ │
  ┌─────────────────────────┐ ├─▶ RRF merge (k=60) ─▶ top 5–8 chunks
  │ Tier 2: sqlite-vec ANN  │─┘
  └─────────────────────────┘
       │
       ▼
  ┌─────────────────────────┐
  │ Context Builder         │   dedupe + token-fit
  └───────────┬─────────────┘
              ▼
  ┌─────────────────────────┐
  │ Tier 3: 3B orchestrator │  always warm
  └───────────┬─────────────┘
              │ needs depth?
              ▼
  ┌─────────────────────────┐
  │ Tier 4: 7B worker       │  demand-loaded, 90s idle evict
  └───────────┬─────────────┘
              ▼
  ┌─────────────────────────┐
  │ Grounding Validator     │  every factual claim must cite
  └───────────┬─────────────┘
              ▼
  Cited answer + inline [source, page] refs
```

### 2.4 What App 1 does for the user

- Create unlimited **notebooks**; drop in PDFs, DOCX, XLSX, PPTX, TXT, MD, images, audio, video, code files (up to 500 MB, A/V up to 2 GB).
- **Chat with your docs** — answers cite `[file, page N]` for every fact; refuses rather than hallucinates.
- **Global semantic search** across whole folders ("private Recall") — find ideas even without the exact words.
- **Knowledge Graph** — auto-extracted people, orgs, dates, decisions, amounts; click an entity → every document that mentions it.
- **Analysis templates** — Compare Sources · Extract Action Items · Find Contradictions · Generate Timeline · Summarize All.
- **Batch processing** — apply an operation to every file in a folder, unified output.
- **Export / import** any notebook as a portable archive.
- **Audit log** of every indexing, query and deletion event.

---

## 3. Pros & Cons

### Pros
- **100 % offline** — zero network calls after one-time model download; nothing leaves the device.
- **Grounded by construction** — RRF retrieval + grounding validator block hallucinations.
- **Hardware-adaptive** — usable on 8 GB RAM laptops, scales up on 16 / 32 GB boxes.
- **Single-file storage** — whole workspace is one `echo.db`; easy backup.
- **No subscriptions** — one-time install, unlimited notebooks and files.
- **Privacy-compliant by design** — matches GDPR / HIPAA data-residency requirements trivially.
- **Portable notebooks** — export/import archives between devices.

### Cons
- **First-run download** (~274 MB embedder + GGUF LLMs) — slow on poor connections.
- **Cold 7B answers** take 5–15 s on minimum hardware.
- **No real-time collaboration** — single user, single device.
- **English-first** in v1; multilingual is v1.1.
- **Large scanned-PDF OCR** is heavier than plain text ingestion.
- **Graph view caps at ~500 entities** interactively (filtered beyond).

---

## 4. Usage Benefits by Role

### For the CEO
- Drop every board pack, investor deck, and strategy memo into one notebook → "What did we commit to in the last three board meetings about hiring?" → cited answer in seconds.
- Confidential: nothing touches the cloud — safe for M&A docs, term sheets, cap tables.

### For the CTO
- Index the entire `/docs`, RFCs, ADRs and code repo → "Where did we decide to drop Kafka?" → jumps straight to the ADR passage.
- Batch-summarise every design doc before a quarterly review.

### For the CFO
- Notebook per quarter with all invoices, contracts, board reports → "All invoices over $10,000 in Q1" answered by the structured entity layer in <1 s, no AI re-reading.
- Find contradictions between a forecast and actuals across 10 reports automatically.

### For Directors / VPs
- Compare 3 vendor proposals side-by-side with a single click (Compare Sources template).
- Timeline template reconstructs project history from scattered status emails and PDFs.

### For Secretaries / Executive Assistants
- Drop the week's meeting minutes into a notebook → "List every action item assigned to Sarah, with deadlines" → cited table, exportable to PDF.
- Voice-free, keyboard-only workflow; no training beyond "drop files and ask."

### For Employees / Knowledge Workers
- Personal notebook per project; ask "summarise what I agreed with the client last month" across emails, specs, and notes.
- Global Recall — forget *where* you saved something; just describe it.

### For Personal / Home Use
- Index tax returns, receipts, appliance manuals, kids' school PDFs → "When does the boiler warranty expire?" → instant answer, no files uploaded anywhere.
- Run on a 5-year-old laptop; battery-safe background indexing.

### Office-wide Benefits
- Replaces ad-hoc ChatGPT use with sensitive company docs → eliminates data-leakage risk.
- No per-seat cloud cost; one install per workstation.
- Works on air-gapped machines (legal, defence, healthcare).
