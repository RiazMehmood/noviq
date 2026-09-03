# EchoAI vs Enterprise Planning Platforms — Speed & Feature Analysis

**Date:** 2026-05-01
**Author:** Riaz Mehmood
**Purpose:** Cross-analysis of EchoAI against 5 enterprise planning platforms (Vena, Planful, Prophix, Cube, Workday Adaptive Planning) — speed architecture lessons, missing features, and GPU integration impact.

---

## The 5 Enterprise Platforms

| Platform | Core Philosophy | Speed Secret |
|----------|----------------|--------------|
| **Vena** | "Complete Planning" — Excel-first, high-performance backend | Delta sync (only changed data), In-Memory Excel caching |
| **Planful** | "Continuous Planning" — speed and executive clarity | Elastic Calculation Engine (micro-tasks in parallel across server cluster) |
| **Prophix One** | Autonomous finance platform — unified AI hub | Stream Processing (real-time updates, no nightly batch) |
| **Cube** | Semantic Layer — data readable by humans and AI agents | Multi-Level Caching with pre-calculated aggregates (<100ms) |
| **Workday Adaptive** | Most complex global organizations | Hypercube with Dimensional Pointers (in-memory, multi-dimensional) |

---

## What's Relevant

Those 5 tools are cloud FP&A platforms, not local AI engines — most of their *infrastructure* doesn't apply directly. But their **speed architectures** are directly instructive for EchoAI.

---

## Speed Techniques to Adopt

### 1. Delta-Sync Indexing — From Vena

**Vena's trick:** When you open a spreadsheet, only the *changed delta* syncs, not the whole model.

**EchoAI current:** SHA-256 dedup prevents re-indexing identical files, but if a file changes, the *whole document* is re-chunked and re-embedded.

**What to add:** Chunk-level delta detection. Store a hash per chunk. On file change, only re-embed chunks whose content changed. For a 500-page PDF where 2 pages were edited, this means ~0.4% of the embedding work instead of 100%.

---

### 2. Parallel Micro-Task NER — From Planful's Elastic Calculation Engine

**Planful's trick:** Break complex financial logic into micro-tasks running in parallel across a server cluster.

**EchoAI current:** NER Pass 2 (LLM-based) runs sequentially — one chunk batch at a time. This is the biggest indexing bottleneck.

**What to add:** Rayon-based parallel NER dispatch. For a 1000-chunk document, split into N batches (where N = CPU cores), run NER concurrently. The LLM slot is single, but the *pre-processing, chunking, and Pass 1 regex NER* can all parallelize. With GPU support, LLM throughput increases too.

---

### 3. Stream Processing for KG — From Prophix

**Prophix's trick:** Instead of nightly batch consolidation, updates stream in the moment a transaction lands.

**EchoAI current:** KG is built *after* indexing completes (batch mode). User sees 0% KG until the full indexing pipeline finishes.

**What to add:** Pipe entities into KG tables *during* indexing, not after. Each chunk that finishes NER Pass 1 immediately writes its entities. KG grows live as indexing runs. Perceived speed jumps dramatically — user sees the graph forming in real time.

---

### 4. Pre-Computed Aggregates — From Cube's Multi-Level Caching

**Cube's trick:** Pre-calculate summary aggregates so AI queries return in <100ms without touching raw data.

**EchoAI current:** Tier 0 cache exists (exact query hash hit), but there is no *structural pre-computation* of the KG or entity summaries.

**What to add:**
- On indexing completion, pre-compute entity frequency tables, top relations, cluster summaries
- Cache the full KG node/edge JSON in a `kg_snapshot` table, refreshed incrementally on entity writes
- KG visualization loads from snapshot (instant) instead of re-querying 10k+ rows on every open

---

### 5. In-Memory Graph Structure — From Workday's Hypercube

**Workday's trick:** Entire financial model lives in RAM while you work. Dimensional Pointers = instant multi-dimensional lookup.

**EchoAI current:** Every KG filter/zoom triggers SQLite queries.

**What to add:** At startup (or after indexing), load the KG into a Rust in-memory graph structure (`petgraph` crate). All visualization queries hit RAM, not SQLite. SQLite remains the source of truth; the in-memory graph is the hot read path. With Rust's ownership model, this is safe and fast.

---

## Features Those 5 Apps Have That EchoAI Doesn't

| Feature | Source | Applicability to EchoAI |
|---------|--------|------------------------|
| **Proactive anomaly detection** | Vena Insights | Detect document anomalies (missing sections, conflicting data) — feasible |
| **Role-specific AI agents** | Planful (Analyst vs Planner) | "Researcher mode" vs "Writer mode" persona toggle — easy via TOML |
| **ML that learns from your history** | Planful Predict | Cache + query-history re-ranking — partially done, can deepen |
| **Agentic RAG with real-time data pull** | Planful | Agent pulling live file changes during task — close with file watcher |
| **MCP Server support** | Cube | EchoAI has no MCP — AnythingLLM does. High value for extensibility |
| **Universal Semantic Layer** | Cube | Define entity schemas once in TOML, used everywhere across chat/KG/search |
| **Multi-model flexibility** | Prophix (Bedrock) | EchoAI locked to llama.cpp GGUF; no Mistral API, no Ollama passthrough |
| **Business context awareness** | Workday Illuminate | Know WHO is asking and adapt answer style/depth — persona-aware queries |
| **Time-series trend prediction** | Workday | Predict what topics will surface in future documents based on query history |

---

## Key Differences (Honest)

| Dimension | Those 5 Apps | EchoAI |
|-----------|-------------|--------|
| Compute | Cloud server clusters, GPU farms | Consumer hardware, 8–16 GB RAM |
| Data | Structured financial ledgers | Unstructured docs (PDF, DOCX, etc.) |
| Speed source | Pre-computed aggregates + cloud scale | Tiered escalation + Rust efficiency |
| KG | Finance-specific (proprietary KG for hallucination prevention) | General-purpose NER + relations |
| Caching | Multi-level distributed cache | Single SQLite Tier 0 cache |
| Parallelism | Elastic horizontal scaling | Single-machine thread parallelism |

**The biggest structural gap:** They pre-compute everything offline so query time is near-zero. EchoAI computes *at query time*. Adopting more aggressive pre-computation at index time is the highest-ROI speed change.

---

## GPU Integration Impact (Planned)

With GPU support enabled:

| Stage | Before GPU | After GPU (estimate) |
|-------|-----------|----------------------|
| LLM inference (Tier 3, 3B) | ~15s | ~2–4s |
| LLM inference (Tier 4, 7B) | ~65s | ~7–12s |
| NER Pass 2 (LLM-based) | slow | 4–5x faster |
| Embedding (fastembed ONNX) | ~95ms | Marginal — already fast |
| KG construction | bottleneck = LLM | Significantly faster |

GPU will be the single biggest speed gain — effectively free since LLM inference dominates everything else. Tier 3 response going from 15s to ~3s changes perceived app quality dramatically.

---

## Priority Order for Speed Work

| Priority | Task | Expected Gain |
|----------|------|---------------|
| **1 — Now** | GPU support integration | 5–10x LLM inference speed |
| **2 — Next** | Chunk-level delta hashing | 80–99% reduction in re-index cost |
| **3 — Then** | Streaming KG construction | Live graph during indexing |
| **4 — Then** | In-memory KG (petgraph) | Sub-5ms visualization queries |
| **5 — Later** | MCP server support | Opens EchoAI to Claude/Cursor ecosystem |

---

## Strategic Note: MCP Server

The MCP angle from Cube is worth noting: Cube gets adopted because *any AI agent can talk to it*. EchoAI adding an MCP server would let Claude Code, Cursor, and custom agents query your local knowledge graph — a significant differentiator in 2026. AnythingLLM already has MCP support; this is a gap worth closing.

---

*Analysis date: 2026-05-01. Based on publicly available information about Vena, Planful, Prophix One, Cube, and Workday Adaptive Planning, cross-referenced against EchoAI architecture as documented in compare_app.md.*
