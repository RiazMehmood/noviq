# Feature Comparison — Echo AI vs Synoptix AI vs 10 Other Enterprise Platforms

**Date:** 2026-05-04
**Sources:**
- Echo AI features → `Desktop/feature details/product details.odt` + project knowledge (CLAUDE.md, CHANGELOG_DEV.md, codebase)
- Other 11 apps → `Desktop/feature details/app_feature_details.pdf` (mirrored in `app_feature_details.md`)

**Legend:** ✅ = feature present · ❌ = feature absent · 🟡 = partial / planned / via 3rd-party

---

## Echo AI — Feature Summary (baseline for the comparison)

From the product brief (`product details.odt`):

- **Why it exists:** prevent the trade-off between AI assistance and data confidentiality; safety from cloud data leaks; AI trained on local datasets.
- **Problem solved:**
  - *Lawyer / compliance officer / manager use case* — turn any folder of documents into something you can talk to in plain English / Urdu / Sindhi, with the exact source page cited in seconds.
  - *Meeting use case* — silent co-participant that flags compliance issues live and auto-produces minutes.
- **Product:** offline agentic AI assistant — document analysis, knowledge graph (who/what/when), templated analysis (summary / comparison / compliance), meeting assistant with live policy checks, action-item & chronological extraction.
- **Differentiators:** folder indexing of thousands of docs, niche-unique, trained on user data, privacy-first, humanoid-robot integration ready, live meeting assistant with departmental knowledge.

---

## Master Comparison Table

| # | Feature | **Echo AI** | **Synoptix AI** | Salesforce Agentforce | ServiceNow | SAP Joule | Glean | Workday Sana | Workday Adaptive | Cube | Vena | Planful | Prophix One |
|---|---------|:-----------:|:---------------:|:---------------------:|:----------:|:---------:|:-----:|:------------:|:----------------:|:----:|:----:|:-------:|:-----------:|
| 1 | **Fully offline / local-first deployment** (runs without internet) | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | 🟡 (self-host Docker) | ❌ | ❌ | ❌ |
| 2 | **On-device / on-prem LLM inference** (no data sent to cloud LLM) | ✅ | ❌ | ❌ | 🟡 (Now Assist on-prem option) | ❌ | ❌ | ❌ | ❌ | 🟡 | ❌ | ❌ | ❌ |
| 3 | **Privacy-first — data never leaves user's machine** | ✅ | 🟡 (PII masking, but cloud) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | 🟡 | ❌ | ❌ | ❌ |
| 4 | **Folder-of-documents indexing (1000s of files)** | ✅ | ✅ | 🟡 (via Data Cloud) | ❌ | 🟡 | ✅ | ❌ | ❌ | ❌ | 🟡 (Excel only) | ❌ | ❌ |
| 5 | **Multilingual Q&A incl. Urdu / Sindhi (low-resource langs)** | ✅ | 🟡 (multilingual via LLM) | 🟡 | 🟡 | ✅ | 🟡 | 🟡 | ❌ | ❌ | ❌ | ❌ | 🟡 |
| 6 | **Source-grounded answers with exact page citation** | ✅ | ✅ | ✅ | 🟡 | ✅ | ✅ | 🟡 | ❌ | ❌ | ✅ | ❌ | ✅ |
| 7 | **Knowledge Graph of entities (who / what / when)** | ✅ | ✅ (EPS) | 🟡 (Data Graph) | ❌ | 🟡 | ✅ (People + Knowledge) | ✅ (Skills graph) | ❌ | ❌ | ✅ (workbook KG) | ❌ | ❌ |
| 8 | **Templated analysis: summaries / comparisons / compliance** | ✅ | ✅ | 🟡 | 🟡 | ✅ | 🟡 | ✅ | ❌ | ❌ | 🟡 | 🟡 | ✅ |
| 9 | **Live meeting assistant (real-time during meeting)** | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 10 | **Real-time compliance flagging during meeting** | ✅ | 🟡 (post-hoc HITL) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 11 | **Auto minutes-of-meeting with timestamps** | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 12 | **Action-item extraction from documents/meetings** | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 | ✅ | ❌ | ❌ | 🟡 | ❌ | ✅ |
| 13 | **Multi-document comparison / conflict detection** | ✅ | ✅ | ❌ | ❌ | 🟡 | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ | 🟡 |
| 14 | **Chronological / timeline extraction** | ✅ | 🟡 | ❌ | ❌ | ❌ | 🟡 | ❌ | 🟡 | ❌ | ❌ | ❌ | 🟡 |
| 15 | **Trained / fine-tunable on user's own datasets** | ✅ | 🟡 | 🟡 | ✅ (Now Assist) | 🟡 | 🟡 | ❌ | ❌ | ❌ | ✅ (Vena Nexus AI) | ✅ (Predict ML) | ❌ |
| 16 | **Humanoid-robot / embedded-device integration ready** | ✅ (roadmap) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 17 | **Multi-agent orchestration (specialized agents)** | 🟡 (single ReAct agent) | ✅ | ✅ | ✅ (Agent Studio) | ✅ | ✅ | ✅ (Supervisor-Worker) | ❌ | ❌ | ❌ | ✅ (Analyst/Planner) | ✅ |
| 18 | **Model Context Protocol (MCP) support** | ❌ | ✅ | ❌ | ❌ | ❌ | ✅ (MCP Host) | ❌ | ❌ | ✅ (native MCP) | ❌ | ❌ | ❌ |
| 19 | **Vector search / semantic retrieval (RAG)** | ✅ | ✅ | ✅ | ✅ (semantic cache) | ✅ (HANA Vector) | ✅ | 🟡 | ❌ | ❌ | ✅ | 🟡 | ✅ |
| 20 | **Two-stage retrieval / semantic reranker** | 🟡 (hybrid retrieval) | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | 🟡 |
| 21 | **Real-time event streams / webhook indexing** | 🟡 (file watcher) | ✅ | ❌ | ✅ (high-ingest) | ❌ | ✅ | ✅ (delta-index) | ❌ | ❌ | ✅ (delta-cell) | ✅ (SignalR) | ✅ (Kafka) |
| 22 | **Cloud SaaS deployment** | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 23 | **Excel / spreadsheet integration** | ❌ | ✅ (extraction) | ❌ | ❌ | 🟡 | 🟡 | ❌ | 🟡 | ✅ (SQL API) | ✅ (Excel-first) | 🟡 | 🟡 |
| 24 | **Financial Planning & Analysis (FP&A) features** | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ | ✅ | ✅ | 🟡 | ✅ | ✅ | ✅ |
| 25 | **Multi-model LLM routing (cost/latency optimization)** | 🟡 (local + Groq fallback) | ✅ | ✅ | ❌ (single model) | ✅ (BTP hub) | ✅ (Waldo router) | 🟡 | ❌ | ❌ | ❌ | ❌ | ✅ (Bedrock) |
| 26 | **PII masking / hallucination check / governance layer** | ❌ | ✅ | ✅ (Trust Layer) | ✅ | 🟡 | 🟡 | ✅ | 🟡 | 🟡 | ✅ | 🟡 | 🟡 |
| 27 | **Pre-built enterprise connectors (Slack/SharePoint/CRM)** | ❌ | ✅ (100+) | ✅ | ✅ | ✅ | ✅ (100+) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 28 | **Drag-and-drop visual workflow designer** | ❌ | 🟡 | ✅ (Flow) | ✅ (Agent Studio) | 🟡 | ❌ | ❌ | 🟡 | ❌ | ✅ | 🟡 | ✅ (React Flow) |
| 29 | **Full audit trail of queries & actions** | ✅ (audit_log) | ✅ | ✅ | ✅ | ✅ | ✅ (perm-aware) | ✅ (SOC 2) | ✅ | ✅ | ✅ (SOX/IFRS) | ✅ | ✅ |
| 30 | **Domain-specialized agents (Finance / HR / Legal)** | 🟡 (templates) | ✅ | ✅ | ✅ | ✅ (Business AI) | ❌ | ✅ | ❌ | ❌ | ❌ | ✅ | ✅ |
| 31 | **OCR / scanned-PDF + audio/video transcription** | 🟡 (PDF+OCR; meeting STT shipped) | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 32 | **Conversational memory across multi-turn sessions** | ✅ | ✅ (Memory Manager) | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | 🟡 | ✅ |

---

## Where Echo AI is Unique (no other app on the list has it)

| Capability | Why it matters |
|---|---|
| **Fully offline + on-device LLM** with no cloud dependency | Only Echo AI keeps confidential documents 100% on the user's machine. All other 11 platforms are cloud-SaaS by design. |
| **Live meeting assistant with real-time compliance flagging + auto minutes** | None of the 11 enterprise platforms includes a silent meeting co-participant that flags rule/policy violations *during* the meeting. |
| **Multilingual Q&A in low-resource languages (Urdu / Sindhi)** | Critical for Pakistan-public-sector and similar markets the global vendors don't target. |
| **Humanoid-robot integration on the roadmap** | None of the 11 apps is positioned for embedded / robotic deployment. |

## Where the cloud platforms beat Echo AI today

- **Pre-built enterprise connectors** (Salesforce, Glean, Synoptix all ship 100+).
- **MCP-based agent interoperability** (Cube, Glean, Synoptix).
- **FP&A depth** (SAP, Workday Adaptive, Vena, Planful, Prophix).
- **Mature governance / PII masking layer** (Synoptix, Salesforce Trust Layer, Workday).
- **Multi-agent orchestration at scale** (Workday Sana Supervisor-Worker, ServiceNow Agent Studio, Synoptix multi-agent).

---

*Generated: 2026-05-04. Cross-check against `app_feature_details.md` and `product details.odt`.*
