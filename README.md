# Noviq

**Your own rules, made answerable.**

Noviq is a sovereign, on-premises AI system that makes an organisation's own
written record answerable. It indexes the corpus — acts, regulations, contracts,
tender documents, SOPs, compliance manuals, specifications, circulars — and
answers questions in plain language, **citing the document, the provision and the
page for every substantive claim**, so the reader can open the original and verify
before acting.

The model, the embeddings, the database and the documents all run on the
organisation's own server. There is no outbound call at query time.

Submitted to the **Alibaba Cloud AI Hackathon Pakistan 2026** (Bano Qabil).
Live at **[noviq.pk](https://noviq.pk)**.

---

## The problem

Every organisation runs on written rules, and that record is the authority for
what it may and may not do. Yet it is scattered, amended in place, rarely indexed
and mostly trapped in scanned PDFs. The person who needs the rule cannot find the
rule, so the decision gets made from memory — and the knowledge leaves when they do.

It is not a problem keyword search solved thirty years ago. An officer does not
know the rule number, and the clause that governs them is three amendments away
from the text in front of them.

## What it does

| | |
|---|---|
| **Chat** | Grounded answers with clickable citations down to the page |
| **Global Search** | Keyword and semantic retrieval across the whole corpus |
| **My Projects** | Answers scoped to a pinned set of documents, enforced server-side |
| **Ingestion** | Layout-aware parsing with OCR, unattended, at scale |
| **Audit trail** | Every read logged, exportable to CSV and JSON |
| **Admin console** | User management, corpus administration, feedback capture |

Conflicting sources are reported separately rather than merged, and when the
answer is not in the corpus Noviq says so instead of inventing one.

## Architecture

A single Rust binary serves both the API and the web frontend, backed by a local
database and a local inference server. One machine, no external dependency at
query time — because an organisation that cannot let its documents leave its
network needs a system with nowhere else to send them.

```
 documents ──> ingest ──> index ──────┐
 (pdf/docx/    poppler    embeddings  │
  xlsx/scan)   -bbox      + FTS       │
               Tesseract              v
                              ┌───────────────┐
 question ────────────────>   │  hybrid       │
                              │  retrieval    │  keyword ∪ vector
                              └───────┬───────┘
                                      v
                              ┌───────────────┐
                              │  generation   │  Qwen3-8B-AWQ, local
                              └───────┬───────┘
                                      v
                        answer + citation (document · provision · page)
```

| Layer | Choice | Why |
|---|---|---|
| Server | Rust 1.87 · Axum | One deployable binary; no ops team on site |
| Interface | TypeScript · SolidJS | Fast on the older desktops actually in these offices |
| Store | PostgreSQL 17 · pgvector | Documents, passages, embeddings, users and audit log in one store |
| Ingestion | poppler `-bbox` · Tesseract | Geometry preserves tables and clause numbering |
| Embeddings | nomic-embed-text-v1.5 | 768-dim, runs locally on CPU or GPU |
| Retrieval | Hybrid union | A section number and a fuzzy concept need opposite retrievers |
| Generation | Qwen3-8B-AWQ on vLLM | Open weights are a hard requirement — sovereignty is the product |

**Why 8B is the design, not a concession.** In a retrieval-grounded system the
model reads the retrieved provisions and states what they say with a citation; it
is never asked to recall the rule from its parameters. The memory a smaller model
frees becomes key-value cache, and cache is what buys concurrent users. Buy VRAM,
not FLOPs.

## Measured

All figures below are measurements on our own hardware, not estimates.
Inference numbers are Qwen3-8B-AWQ on an A40-12Q.

| | |
|---|---|
| Production-shaped query, end to end | **7.7 s** |
| Aggregate across 8 concurrent users | **596.8 tok/s** |
| Slowest of those 8 requests | **4.02 s** |
| Documents indexed (proving corpus) | **4,074** across 49 departments |
| Retrievable passages | **304,617** |

**It also runs with no GPU at all.** On 2026-09-03 the whole stack came up in one
container on a commodity CPU box: healthy in 20 s from cold, a passage batch
embedded in 97 ms, a grounded and correctly cited answer in 12.5 s, and an
out-of-corpus question refused in 1.8 s. A GPU makes Noviq fast; it is not what
makes Noviq possible.

## What we do not claim

- We have **not** out-scored a hosted assistant on answer quality. We benchmarked
  NotebookLM against our own corpus and it scored 5/5. Our claim is narrower: the
  corpus never leaves the building, and every sentence is checkable against a page.
- Cross-document conflict comparison answers *"not yet"* by design rather than
  guessing.
- A bad scan indexes but does not read well — where the only copy is a poor
  photocopy, OCR quality is the ceiling, and we say so to the user.
- **Workflow** (mapping the route a file travels between offices) and **Meeting
  Assistant** (on-device transcription and minutes) are designed and in research.
  They carry no dates because we have not committed to any.

## Repository layout

```
src/               SolidJS frontend
src-tauri/         Rust backend — API, retrieval, ingestion, inference
  src/services/    indexer, embedder, query_router, retrieval, drafting
  src/db/pg/       PostgreSQL schema and migrations
  tests/           integration and golden-file tests
specs/             Feature specifications (spec / plan / tasks per feature)
history/adr/       Architecture Decision Records
history/prompts/   Prompt History Records — the development trail
deploy/            Deployment configuration
docs/              Engineering notes and design reviews
```

The project is built spec-first: every feature has a written specification and
plan under `specs/`, and significant decisions are recorded as ADRs under
`history/adr/` before the code is written.

## Running it

Requires Rust 1.87, Node 20+, and poppler + Tesseract on PATH. PostgreSQL is
bundled and provisioned on first boot.

```bash
cp .env.example .env      # then edit — never commit .env
npm install
npm run build             # build the frontend
cargo run --release --bin noviq-server --no-default-features --features server
```

See `RUNNING.md` for the full setup and `deploy/` for the server deployment.

**Configuration is entirely by environment variable.** No credential is ever
committed: API keys for optional cloud providers are read from the environment
(`GROQ_API_KEY`, `CEREBRAS_API_KEY`), and `.env` is git-ignored.

## Team

**Riaz Mehmood** — backend, retrieval, inference, deployment
**Younus Shaikh** — evaluation and instrumentation

## Note on this repository

This is the public source snapshot prepared for hackathon review. Deployment
runbooks and server addresses are kept out of it deliberately; the application
code, specifications and decision records are complete.
