# App Feature & Technology Reference

**Date:** 2026-05-01
**Purpose:** Per-app detailed feature and technology breakdown — quick reference across all analyzed platforms including EchoAI.

---

## Table of Contents

1. [EchoAI](#1-echoai)
2. [Salesforce Agentforce](#2-salesforce-agentforce)
3. [ServiceNow](#3-servicenow)
4. [SAP Joule](#4-sap-joule)
5. [Glean](#5-glean)
6. [Workday Sana](#6-workday-sana)
7. [Workday Adaptive Planning](#7-workday-adaptive-planning)
8. [Cube](#8-cube)
9. [Vena Solutions](#9-vena-solutions)
10. [Planful](#10-planful)
11. [Prophix One](#11-prophix-one)

---

## 1. EchoAI

**Type:** Local AI Desktop Application — Offline Knowledge Engine + Autonomous Agent  
**Version:** 0.1.0  
**Platforms:** Windows, Linux (macOS planned)  
**License:** Commercial (7-day offline trial)

### Core Features

- **Notebook & Knowledge Engine (App 1)**
  - Chat with documents using hybrid retrieval (FTS5 + ANN)
  - Named notebooks for organizing document collections
  - Inline citations with document name and page reference
  - Semantic search (ANN vector similarity)
  - Full-text search (SQLite FTS5)
  - Hybrid search — FTS5 + ANN results merged and re-ranked
  - Grounding enforcement — explicit "no content found" instead of hallucination
  - Cross-document analysis with PDF export
  - Conversation history and session context
  - Streaming LLM responses

- **Knowledge Graph**
  - Named Entity Recognition (NER) — 2-pass pipeline
    - Pass 1: Rule-based regex (fast, no LLM)
    - Pass 2: LLM-powered extraction for complex entities
  - Entity types: PERSON, ORG, LOCATION, DATE, DECISION, MONETARY, custom
  - Relationship extraction between entities
  - Interactive force-directed radial graph visualization
  - Custom relation types defined via user preferences
  - Natural language entity queries mapped to SQL filters
  - Filter by entity type and relation type

- **Indexing Pipeline**
  - PDF parsing (lopdf — pure Rust)
  - DOCX parsing (docx-rs — pure Rust)
  - XLSX/CSV parsing (calamine — pure Rust)
  - TXT, MD, HTML, code files
  - 512-token chunks with 128-token overlap
  - SHA-256 file-level deduplication
  - Background batch processing with ETA, pause, cancel
  - Live file watcher — auto re-index on file create/modify/delete
  - Hardware-adaptive embedding model selection

- **Tiered Intelligence System**
  - Tier 0: Exact query cache (<1ms)
  - Tier 1: FTS5 full-text search (<50ms)
  - Tier 2: ANN vector search (<200ms)
  - Tier 3: Orchestrator 3B LLM (always warm, ~15s CPU)
  - Tier 4: Worker 7B LLM (demand-loaded, evicted after 90s idle)

- **Hardware Tier Auto-Detection**
  - 5 tiers based on RAM + VRAM at startup
  - Tier 1: <8 GB | Tier 2: 8–12 GB | Tier 3: 12–16 GB | Tier 4: 16–32 GB | Tier 5: 32 GB+
  - Features gracefully disabled with user-visible explanation at lower tiers
  - Embedding model swapped by tier (MiniLM-L6-v2 vs nomic-embed-text-v1.5)

- **Agent Overlay (App 2)**
  - Floating transparent always-on-top window
  - Global keyboard shortcut activation
  - ReAct loop (Reason → Act → Observe) in Rust
  - Task decomposition from natural language instruction
  - Tools: file read/write/move/copy/rename/delete, directory list, shell execute, code write + execute
  - Pending Actions Queue (PAQ) safety gate: DANGEROUS / CAUTIOUS / READ_ONLY
  - File operation rollback on task cancel
  - Live step progress in overlay
  - Task history with configurable retention
  - Pause and resume task execution
  - TOML-based agent persona configuration
  - Voice task assignment via wake word

- **Voice Pipeline (100% Offline)**
  - Voice Activity Detection (VAD) — local
  - Speech-to-text: whisper-rs (Rust FFI to whisper.cpp)
  - Text-to-speech: Kokoro TTS (local neural, no cloud)
  - Wake word: "EchoAI" — passive activation
  - Voice and text are equivalent input modes
  - No internet, no API keys required

- **Privacy & Security**
  - Zero telemetry — constitutionally mandated
  - Zero cloud dependency — 100% offline by design
  - No external API keys required
  - Single `echo.db` SQLite file — all data local
  - Audit log: query_answered, folder_revoked, data_deleted
  - 7-day trial lock stored in SQLite — no license server ping
  - PAQ safety gate for all destructive agent operations

- **UI & Application Shell**
  - Two independent windows in one process (Notebook + Agent Overlay)
  - System tray with show/hide per window
  - Markdown rendering (marked + DOMPurify)
  - Code syntax highlighting
  - Dark/light themes
  - PDF export of analysis results
  - Desktop notifications (tauri-plugin-notification)
  - @xyflow/react KG visualization (zoom, pan, filter)

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Desktop shell | Tauri 2.0 |
| Backend language | Rust 1.87 (stable) |
| Frontend language | TypeScript |
| Frontend framework | SolidJS + Tailwind CSS |
| LLM inference | llama-cpp-2 (Rust FFI to llama.cpp) |
| Embedding | fastembed-rs (ONNX, pure Rust) |
| Embedding models | all-MiniLM-L6-v2 (384-dim), nomic-embed-text-v1.5 (768-dim) |
| Model format | GGUF (Q4_K_M minimum) |
| Database | SQLite (sqlx 0.8) |
| Vector search | sqlite-vec (vec0 extension) |
| Full-text search | SQLite FTS5 |
| KG visualization | @xyflow/react 12.x |
| Speech-to-text | whisper-rs (whisper.cpp) |
| Text-to-speech | Kokoro TTS |
| PDF parsing | lopdf |
| DOCX parsing | docx-rs |
| XLSX/CSV parsing | calamine |
| File watching | notify crate |
| Code sandbox | Wasmtime/WASI |
| HTTP client | reqwest (model downloads) |
| Async runtime | Tokio |
| CI/CD | GitHub Actions (Windows build) |
| Binary size | ~10–20 MB |
| GPU support | CPU-first (GPU planned) |

---

## 2. Salesforce Agentforce

**Type:** Enterprise CRM + Agentic Operating System  
**Architecture:** "Deterministic Sandwich" — generative AI wrapped in hardened deterministic logic  
**Target:** Enterprise sales, service, marketing teams

### Core Features

- **Atlas Reasoning Engine**
  - Agentforce Script DSL — domain-specific language for deterministic if-then agent logic
  - LLM must follow DSL rules — cannot deviate or hallucinate actions
  - Wraps generative AI between type-safe deterministic layers
  - Reduces agent error rate vs pure LLM prompting

- **Agentforce Studio**
  - Developer IDE for "Context Engineering"
  - Define Actions as Apex classes or Salesforce Flows
  - Actions become tools the AI agent can invoke
  - Visual testing and debugging environment

- **Data Cloud (Zero-Copy Architecture)**
  - Federated Data via Salesforce Connect
  - Query Snowflake, BigQuery, Redshift without moving data
  - Vector databases built into the Salesforce metadata layer
  - No separate vector DB process required

- **Metadata Grounding**
  - AI understands Salesforce object schema at startup
  - No need to "search" for schema — pre-loaded in context
  - Reduces AI reasoning loop by ~40%
  - Eliminates schema-discovery queries from the reasoning cycle

- **Agent Overlay**
  - Streaming LLM responses to UI
  - Real-time UI updates during agent execution
  - GraphQL Wire Adapters for frontend data binding

- **Einstein AI Features**
  - Natural language CRM queries
  - Automated email/call summaries
  - Lead scoring and opportunity forecasting
  - Case routing and resolution suggestions

- **Security & Governance**
  - Role-based access control on all agent actions
  - Einstein Trust Layer — no customer data sent to LLM providers for training
  - Field-level security respected by AI agents
  - Full audit trail of all agent actions

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | LWC (Lightning Web Components) |
| Data binding | GraphQL Wire Adapters |
| Agent DSL | Agentforce Script (proprietary) |
| Backend logic | Apex (Java-like, compiled) |
| Automation | Salesforce Flow (low-code) |
| Data platform | Salesforce Data Cloud |
| Federation | Salesforce Connect (zero-copy) |
| Vector DB | Built into Salesforce metadata layer |
| AI backbone | Einstein AI Platform |
| LLM providers | Multiple (OpenAI, Anthropic, Google — via Einstein Trust Layer) |
| IDE | Agentforce Studio |

---

## 3. ServiceNow

**Type:** Enterprise ITSM + Agentic Digital Worker Platform  
**Architecture:** High-ingest operations — millions of system signals per second  
**Target:** IT operations, HR service delivery, enterprise workflow automation

### Core Features

- **RaptorDB**
  - 2026-generation hybrid database engine
  - Hybrid row/columnar store
  - Row store: fast transactional writes (ticket creation, updates)
  - Columnar store: fast analytical reads (trend analysis, anomaly detection)
  - Designed specifically for AI write-heavy workloads
  - Replaces legacy relational bottlenecks

- **Now Assist LLM**
  - Specialized "small-but-mighty" model
  - Trained specifically on ITSM and HR schemas
  - Not a general-purpose LLM — domain expert only
  - Faster and cheaper than GPT/Claude for ITSM tasks
  - Understands ServiceNow data model natively

- **Semantic Caching**
  - Every resolved IT ticket is embedded and stored
  - New incoming issue embedded and compared against cache
  - Cosine similarity > 0.90 = cache hit, skip reasoning phase
  - Jump directly to cached remediation script
  - Latency reduction: 1.5–2.5 seconds per query

- **Agent Studio**
  - Build Autonomous Digital Workers
  - Define Missions — high-level agent goals
  - Define Boundaries — ACLs and rate limits on agent actions
  - No agent can exceed its defined boundaries
  - Visual workflow designer for agent decision paths

- **Agentic ATF (Automated Test Framework)**
  - Visual debugger for agent decision-making paths
  - Replay any agent execution step by step
  - Identify where and why an agent made a specific decision
  - Developer-facing — not user-facing

- **Next Experience UI**
  - React-based frontend framework
  - Real-time updates for high-frequency event processing
  - Responsive to millions of concurrent system signals

- **High-Ingest Architecture**
  - Processes millions of system signals per second
  - Event-driven routing to appropriate digital workers
  - Auto-classification of incidents, requests, and changes
  - Predictive alerting before issues escalate

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | Next Experience UI (React-based) |
| Database | RaptorDB (hybrid row/columnar, proprietary) |
| AI model | Now Assist LLM (domain-specialized) |
| Caching | Semantic Cache (embedding-based similarity) |
| Agent framework | Agent Studio (Missions + Boundaries) |
| Embedding | Internal vector embedding (ServiceNow proprietary) |
| Debug tool | Agentic ATF (visual decision tree debugger) |
| Backend | Java / JavaScript (Node.js) |
| Deployment | Cloud (ServiceNow-hosted) + on-premise option |

---

## 4. SAP Joule

**Type:** Enterprise ERP AI Layer + Global Orchestration Platform  
**Architecture:** In-memory HANA database + multi-model AI hub  
**Target:** Large enterprises running SAP ERP, Supply Chain, Finance, HR

### Core Features

- **Joule Action Bar**
  - Cross-application persistent AI agent
  - Stays active as user navigates between ERP, Supply Chain, and HR apps
  - Natural language commands executed across all SAP modules
  - Context preserved across app switches

- **Generative AI Hub (SAP BTP)**
  - Model-as-a-Service layer for developers
  - Toggle between GPT-5.2, Gemini 3.0, Claude 4.6 by task complexity
  - Simple summarization → lighter model; complex reasoning → heavyweight model
  - Single API, multiple underlying models
  - Cost and latency optimized per task type

- **ABAP AI**
  - 2026 upgrade to ABAP programming language
  - Embed AI prompts directly as native class method calls
  - Prompts are first-class language citizens, not external API calls
  - Type-safe, compiled — no Python subprocess overhead

- **HANA Cloud In-Memory Database**
  - Entire data model lives in server RAM
  - Vector Search integrated directly into SQL
  - Single SQL query joins structured financial data with unstructured contract text
  - No separate vector database process
  - Columnar storage for analytical workloads

- **Dimensional Pointers**
  - Every data point has a coordinate in multidimensional space
  - Lookup = pointer dereference, O(1) — not a SQL JOIN or table scan
  - AI "walks" the data graph at hardware memory speeds
  - Sub-100ms responses on billion-row enterprise datasets

- **SAP Business AI**
  - 100+ AI scenarios across all SAP modules
  - Invoice processing and matching
  - Supplier risk scoring
  - Cash flow forecasting
  - Employee attrition prediction
  - Production anomaly detection

- **Integration Hub**
  - Connects to thousands of enterprise systems
  - Pre-built connectors for SAP and non-SAP systems
  - API orchestration across disparate data sources

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | SAP Fiori (OpenUI5 framework) |
| AI interface | Joule Action Bar (cross-app persistent) |
| Backend language | ABAP (SAP-native, compiled) + Java |
| AI embedding | ABAP AI (native prompt embedding) |
| Model platform | Generative AI Hub (BTP) — multi-model |
| LLM providers | GPT-5.2, Gemini 3.0, Claude 4.6 (switchable) |
| Database | HANA Cloud (in-memory, columnar) |
| Vector search | HANA Vector Engine (built into SQL) |
| Pointer system | Dimensional Pointers (proprietary in-memory) |
| Integration | SAP Integration Suite |
| Deployment | SAP BTP (Business Technology Platform) — cloud |

---

## 5. Glean

**Type:** Enterprise Agentic Search + Knowledge Intelligence Platform  
**Architecture:** Pre-Inference Routing with specialized Small Language Model (Waldo)  
**Target:** Knowledge workers, developers, enterprise search

### Core Features

- **Waldo (SLM — Small Language Model)**
  - 7–10B parameter model
  - Trained via DPO (Direct Preference Optimization) on routing decisions
  - Sole purpose: decide which tools to call before waking a large model
  - Does NOT do creative writing or complex reasoning
  - Operates at 250ms latency
  - "Search First, Reason Later" pattern

- **Pre-Inference Routing**
  - Waldo plans the search strategy (which tools, which data sources, which model)
  - Large model (GPT-5, Claude) only invoked for final reasoning step
  - Saves 2–3 seconds per query vs calling large model directly
  - Reduces cost per query by routing simple queries to smaller models

- **Model Context Protocol (MCP) Host**
  - Glean acts as an MCP Host
  - Connects to any third-party agent via standardized JSON-RPC protocol
  - Custom Python bots, internal agents, IDE tools can talk to Glean
  - Enables composable agent architectures

- **Distributed Knowledge Graph**
  - Stores cross-artifact relationships:
    - "Developer X wrote Code Y"
    - "Code Y is discussed in Doc Z"
    - "Doc Z is owned by Team A"
    - "Team A's Slack mentioned this last week"
  - Social graph of organizational knowledge
  - Not just document entities — inter-artifact dependency graph

- **Ubiquitous UI**
  - Chrome extension — search within any web app
  - Slack bot — search from conversations
  - IDE plugin — search from code editor
  - API — embed Glean search in any internal tool
  - Does not force users to a dedicated app

- **Connectors Ecosystem**
  - 100+ pre-built connectors: Google Drive, Confluence, Jira, GitHub, Slack, Salesforce, Zendesk, and more
  - Indexes content from all connected sources into unified knowledge graph
  - Permissions-aware — user only sees content they have access to

- **People Graph**
  - Maps employee expertise based on what they write and work on
  - "Who in our company knows most about Kubernetes?" → returns ranked list
  - Expertise scoring from writing patterns, not self-declaration

- **Real-Time Indexing**
  - Content indexed within minutes of creation/modification
  - Permissions synced in real-time from source systems
  - No stale search results on recently updated documents

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | React / TypeScript |
| UI delivery | Chrome extension, Slack bot, IDE plugin, React web app |
| Routing model | Waldo SLM (7–10B, DPO-trained, Go/proprietary) |
| Primary language | Go (backend), TypeScript (frontend) |
| Protocol | MCP Host (JSON-RPC) |
| Knowledge graph | Distributed index (proprietary graph engine) |
| Vector search | Distributed vector index (proprietary) |
| Permissions | Real-time ACL sync from source systems |
| Deployment | Cloud SaaS |
| Integrations | 100+ connectors (Slack, GitHub, Confluence, Salesforce, etc.) |

---

## 6. Workday Sana

**Type:** AI Operating System for People & Money (HR + Finance)  
**Architecture:** Multi-Agent Swarm with Supervisor-Worker pattern  
**Target:** Enterprise HR, Finance, Payroll teams

### Core Features

- **Multi-Agent Swarm**
  - Supervisor Agent decomposes high-level user requests
  - Specialized Worker Agents: Onboarding Agent, Payroll Agent, Benefits Agent, IT Provisioning Agent, Facilities Agent
  - Workers execute in parallel where independent
  - Supervisor waits for all workers, resolves conflicts, synthesizes result
  - No agent exceeds its defined permission boundaries

- **Delta-Indexing via Webhook Streams**
  - Registers webhooks with source systems (ERP, HRIS, Salesforce)
  - When a contract is signed → webhook fires → Sana index updates in milliseconds
  - No nightly batch re-indexing
  - World view always current
  - Supports millions of events per day

- **Skills Cloud AI**
  - Specialized graph database
  - Maps 50,000+ human skills to job requirements
  - Real-time matching: employee skills ↔ open roles
  - Identifies skill gaps across teams
  - Suggests personalized learning paths

- **Sana UI**
  - AI-native dashboard
  - Abstracts legacy Workday interface complexity
  - Clean, task-focused UX for HR and Finance personas
  - WebAssembly (Wasm) for high-performance data visualizations
  - Million-row data grids with zero browser lag

- **Multi-Tenant Data Partitioning**
  - Company A's AI training data never leaks to Company B
  - Hard partition at the data layer, not just application layer
  - Compliance-ready architecture (GDPR, SOC 2)

- **Workday Illuminate (Business Context Layer)**
  - Knows who is asking a question (role, department, seniority)
  - Adapts answer style, depth, and data access based on identity
  - Manager asking about headcount → sees team data
  - Executive asking same question → sees org-wide data

- **Prism Analytics Integration**
  - Connects external data sources to Workday's AI layer
  - Mix Workday data with external HR benchmarks
  - AI-powered dashboards with natural language drill-down

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | Sana UI (React) + WebAssembly (Wasm) for viz |
| Agent pattern | Supervisor-Worker swarm (proprietary orchestration) |
| AI context | Workday Illuminate (identity-aware reasoning) |
| Skills graph | Skills Cloud AI (proprietary graph DB) |
| Data indexing | Delta-Indexing via Webhook Streams |
| Multi-tenancy | Partitioned architecture (hard data isolation) |
| Analytics | Workday Prism Analytics |
| Backend | XpressO (Workday-native language) + Python |
| Deployment | Cloud SaaS (AWS) |
| Security | SOC 2 Type II, GDPR compliant |

---

## 7. Workday Adaptive Planning

**Type:** Enterprise FP&A (Financial Planning & Analysis) Platform  
**Architecture:** Elastic Hypercube in-memory multidimensional engine  
**Target:** Finance teams, CFO office, corporate planning

### Core Features

- **Elastic Hypercube Engine**
  - Proprietary multidimensional in-memory calculation engine
  - Solves "N-Dimensional Problem": 5,000 products × 100 regions × 60 months × 10 currencies
  - Data stored as coordinates in multidimensional space, not relational rows
  - Not SQL — no table scans, no JOINs
  - Entire financial model lives in server RAM during active sessions

- **Dimensional Pointers**
  - Every data point = a coordinate in the Hypercube
  - Lookup = pointer dereference, O(1) access
  - "Jump" to any data point instantly regardless of model size
  - Enables instant "What-If" scenario recalculation across millions of permutations

- **Parallel Calculation Sharding**
  - "What-If" scenarios split Hypercube into N chunks
  - N chunks calculated simultaneously across CPU cores / compute nodes
  - Results merged and reconciled in milliseconds
  - User sees instant scenario results

- **AI Predictive Forecaster**
  - Time-series ML models embedded in the platform
  - Uses 5+ years of historical data for forecasting
  - Suggests "most likely" outcome for budget planning
  - Flags anomalies vs historical baseline
  - Embedded in planning workflows — not a separate tool

- **WebAssembly Sheet Interface**
  - React-based frontend with Wasm for the "Sheet" view
  - Handles million-cell grids in the browser with zero lag
  - Financial planners work in a spreadsheet-like interface
  - No native app required — full performance in browser

- **Scenario Planning**
  - Unlimited "What-If" scenarios
  - Any scenario change propagates instantly across all dimensions
  - Side-by-side scenario comparison
  - Approval workflows for scenario promotion

- **Financial Consolidation**
  - Multi-entity, multi-currency consolidation
  - Intercompany eliminations
  - Currency translation with configurable rates
  - Minority interest calculations

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | React + WebAssembly (Wasm) |
| Engine | Elastic Hypercube (proprietary in-memory multidimensional) |
| Pointer system | Dimensional Pointers (proprietary) |
| Parallelism | Parallel Calculation Sharding |
| Backend | Java microservices |
| Infrastructure | High-memory AWS / GCP instances |
| AI/ML | Embedded time-series ML models |
| Deployment | Cloud SaaS |
| Integration | Workday HCM, ERP connectors, REST API |

---

## 8. Cube

**Type:** Semantic Layer Platform + Headless BI  
**Architecture:** Rust-based semantic translation layer between data warehouses and AI agents  
**Target:** Data engineers, BI developers, AI agent builders

### Core Features

- **Rust-Based Semantic Layer Core**
  - Cube Core written in Rust — memory safe, extreme performance
  - Defines business metrics once — used everywhere (Excel, Tableau, AI agents, APIs)
  - Universal Semantic Layer: "Gross Margin" defined once = identical in every tool
  - Eliminates metric inconsistency across teams

- **Native MCP Server**
  - Model Context Protocol server built in
  - Any LLM (Claude, GPT, custom) talks to Cube via MCP JSON-RPC
  - LLM receives Metric definitions and Dimension schemas via MCP
  - LLM never writes raw SQL — Cube translates intent to optimized queries
  - Grounded querying at infrastructure level

- **Pre-Aggregations (Rollups)**
  - Automatically detects frequent query patterns
  - Creates materialized rollup tables for common aggregations
  - "Total Revenue by Region" pulled from pre-computed rollup, not raw 100M row table
  - Result: <100ms query time vs 45+ seconds on raw data
  - Rollups refreshed on schedule or incrementally

- **Multi-API Support**
  - GraphQL API
  - REST API
  - SQL API — Cube looks like a Postgres database to other tools
  - Any BI tool, spreadsheet, or agent that speaks SQL/GraphQL connects natively

- **Headless Philosophy**
  - No built-in frontend — pure data API
  - Works with any frontend: Excel, Tableau, Looker, custom React app, AI agent
  - "Bring your own UI"

- **Redis Caching Layer**
  - Sub-second query caching via Redis
  - Cache keyed on semantic query intent
  - Pre-aggregation results cached in Redis for instant retrieval
  - Cache invalidated on data refresh

- **Data Warehouse Connectors**
  - Snowflake, BigQuery, Redshift, DuckDB, MotherDuck, ClickHouse, PostgreSQL, and more
  - Federated queries across multiple warehouses
  - No data movement — queries pushed down to source

- **Access Control**
  - Row-level and column-level security
  - Tenant isolation for multi-tenant deployments
  - API key and JWT authentication

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Core engine | Rust (Cube Core) |
| API | GraphQL + REST + SQL API |
| Protocol | Native MCP Server (JSON-RPC) |
| Caching | Redis (sub-second query cache) |
| Pre-aggregations | Automatic rollup materialization |
| Data warehouses | Snowflake, BigQuery, DuckDB, MotherDuck, ClickHouse, Postgres |
| Auth | API key, JWT |
| Deployment | Self-hosted (Docker) or Cube Cloud (SaaS) |
| Node.js layer | Node.js (API server, schema definitions) |

---

## 9. Vena Solutions

**Type:** Enterprise FP&A Platform — Excel-First with High-Performance Cloud Backend  
**Architecture:** "Bridge" design — Excel as UI, .NET backend as logic/data layer  
**Target:** Finance teams that live in Excel; CFO office

### Core Features

- **Vena Nexus (Core Sync Engine)**
  - Proprietary .NET-based middleware
  - Syncs Excel's XML internal data structure with centralized database in real-time
  - Monitors Excel's Calculation Chain (DAG of cell dependencies)
  - On cell change, identifies downstream dependent cells automatically

- **Delta-Cell Syncing**
  - Does NOT upload the whole Excel file on change
  - Monitors which specific cells changed via Calculation Chain
  - Uploads only the changed cells (deltas)
  - Multi-gigabyte models stay fast because only deltas are transmitted
  - Equivalent of git diff — send changes, not full state

- **Vena Insights (AI Layer)**
  - Knowledge Graph maps how Excel workbooks relate to each other
  - "Cell B3 in Budget.xlsx feeds into Forecast.xlsx column C"
  - AI traces a number back to its source across multiple spreadsheets
  - Natural language variance reports: "Revenue is down 12% vs forecast because..."
  - Proactive anomaly detection — flags unusual numbers before user notices

- **Vena Nexus AI (2026)**
  - Hyperscale infrastructure for training custom AI models on client financial data
  - Finance-specific Knowledge Graph to prevent AI hallucinating numbers
  - Custom model trained on organization's own historical patterns

- **Workflow Automation**
  - Visual drag-and-drop workflow designer
  - Multi-departmental budget cycle orchestration
  - Approval routing — who reviews what, in what order
  - Email notifications and reminders built in
  - Deadline tracking per workflow stage

- **Excel Add-in**
  - Microsoft Office JS API integration
  - Works inside Excel — no context switching
  - Real-time sync visible in Excel without leaving the app
  - Familiar interface for finance teams

- **Version Control**
  - Full version history of all budget and forecast changes
  - Who changed what, when, with comparison views
  - Rollback to any previous version

- **Audit Trail**
  - Complete log of all data changes
  - Who entered, approved, or modified any number
  - Regulatory compliance ready (SOX, IFRS)

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend (Excel) | Microsoft Office JS API (Excel Add-in) |
| Frontend (web) | React + TypeScript web portal |
| Backend | .NET Core + C# microservices |
| Sync engine | Vena Nexus (.NET-based, proprietary) |
| AI layer | Vena Insights (KG-powered AI) |
| AI training | Vena Nexus AI (custom model training) |
| Knowledge graph | Proprietary (workbook relationship graph) |
| Integration | Microsoft 365, ERP connectors |
| Deployment | Cloud SaaS (Vena-hosted) |

---

## 10. Planful

**Type:** Continuous Financial Planning Platform  
**Architecture:** Elastic Calculation Engine (ECE) with async worker pools and ML forecasting  
**Target:** Mid-market to enterprise finance and accounting teams

### Core Features

- **Elastic Calculation Engine (ECE)**
  - Specialized compute layer for complex financial logic
  - Handles currency translations, intercompany eliminations, allocations
  - Asynchronous — budget submissions queued as background jobs
  - UI never freezes during complex calculation
  - Horizontally scalable — more workers added under load

- **Asynchronous Background Processing**
  - All heavy operations run in background worker queues
  - UI returns "calculating..." immediately with progress indicator
  - SignalR WebSocket pushes result to UI when complete
  - User continues working while calculations run

- **Planful Predict (ML Suite)**
  - Time-Series Forecasting using Random Forest and XGBoost
  - Models tuned for financial seasonality (monthly, quarterly, annual cycles)
  - Anomaly Detection — "Is this month's expense unusual?"
  - Runs as background Python processes, not blocking main app
  - Trained on client's own 6–24 months of historical data

- **SignalR Real-Time Streaming**
  - WebSocket-based real-time data push to dashboard
  - Budget changes propagate to all viewing users simultaneously
  - Live collaboration — multiple planners see changes as they happen
  - No page refresh required

- **Analyst & Planner Assistants**
  - Role-specific AI agents
  - Analyst Assistant: builds forecast models from a single prompt
  - Planner Assistant: manages budget workflow and approvals
  - Each agent understands the role's specific data needs

- **Planful Predict Integration**
  - Embedded in planning workflows — not a separate analytics tool
  - "ML-suggested" forecast vs "user-entered" plan shown side by side
  - One-click accept suggestion or override
  - Model explains why it's suggesting a specific number

- **Continuous Planning Workflow**
  - Rolling forecasts — not just annual budgets
  - Triggered updates when actuals differ from plan by threshold
  - Automated reforecast workflows
  - Driver-based planning (revenue drives headcount drives expenses)

- **Reporting & Dashboards**
  - Drag-and-drop report builder
  - Pre-built CFO dashboards
  - Variance analysis (actual vs plan vs forecast vs prior year)
  - Drilldown from summary to transaction level

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | Angular + React |
| Real-time | SignalR (WebSocket-based streaming) |
| Calculation engine | Elastic Calculation Engine (ECE, proprietary) |
| ML/AI | Python — Random Forest, XGBoost (time-series) |
| AI agents | OpenAI Enterprise APIs (grounded by ECE) |
| Agent pattern | Agentic RAG |
| Backend | C# / Azure-native |
| Database | Azure SQL + Azure Data Lake Storage |
| Async | Background worker queue (Azure Service Bus) |
| Deployment | Cloud SaaS (Azure) |

---

## 11. Prophix One

**Type:** All-in-One Autonomous Finance Platform ("Virtual CFO")  
**Architecture:** Unified financial database + Kafka-style event streams + Amazon Bedrock AI  
**Target:** Mid-market companies wanting full CFO office automation

### Core Features

- **Unified Financial Database**
  - Single schema for all financial operations
  - Monthly Close, Operational Planning, Reporting, Consolidation — all one data model
  - No module silos — data flows freely between processes
  - No ETL between planning and close

- **Prophix One Intelligence (AI Hub)**
  - Centralized AI hub powering all platform features
  - Built on Amazon Bedrock — multi-model flexibility
  - Anomaly Detection — flags unusual transactions in real-time
  - Predictive Forecasting — suggests future values based on history
  - Natural language copilot across all modules

- **Agentic Copilot**
  - Natural language commands: "Generate a board deck for Q3"
  - Natural language queries: "Find why electricity costs spiked in Germany"
  - Agentic RAG: retrieve → compute → retrieve more → synthesize
  - Multi-pass retrieval until confident answer found
  - Executes tasks, not just answers questions

- **Kafka-Style Event Streams**
  - Real-time event bus for financial data
  - Transaction enters from NetSuite/SAP → Event fires immediately
  - Event consumers: Anomaly Detector, Dashboard Updater, AI Copilot Context
  - Time from ERP transaction to dashboard update: <500ms
  - No nightly batch consolidation required

- **Stream Processing Architecture**
  - Prophix One Intelligence subscribes to all data events
  - Totals update the moment a transaction is entered
  - Anomaly check runs on every transaction as it arrives
  - All related dashboards update automatically

- **Drag-and-Drop Workflow Designer**
  - Visual workflow builder using React Flow
  - Month-end close checklist with dependencies
  - Budget approval routing
  - Automated journal entry workflows
  - Drag tasks onto canvas, connect with arrows, set conditions

- **Amazon Bedrock Multi-Model**
  - Not locked to one LLM
  - Summarization → Claude
  - Structured extraction → Llama
  - Embeddings → Amazon Titan
  - Task-specific model routing = cost + latency optimization

- **AWS Lambda Serverless Scaling**
  - Calculation tasks run as serverless Lambda functions
  - Scale to zero when idle — no wasted compute
  - Burst capacity for month-end reporting spikes
  - Python-based calculation logic

- **Month-End Close Automation**
  - Automated reconciliations
  - Journal entry suggestions based on historical patterns
  - Variance explanations auto-generated
  - Close checklist with real-time completion tracking

- **Reporting & Narrative AI**
  - Auto-generate board deck from planning data
  - Natural language variance commentary
  - Export to PowerPoint, PDF, Excel
  - Branded report templates

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | React + React Flow (drag-and-drop workflows) |
| Backend | Node.js + Python |
| Event streaming | Kafka-style event streams (AWS MSK) |
| Serverless compute | AWS Lambda (Python) |
| AI platform | Amazon Bedrock (multi-model) |
| LLM providers | Claude, Llama, Amazon Titan (task-routed) |
| AI pattern | Agentic RAG (multi-pass retrieval + tool use) |
| Database | Unified Financial Database (proprietary schema) |
| Workflow designer | React Flow |
| ERP connectors | NetSuite, SAP, Oracle, QuickBooks |
| Deployment | Cloud SaaS (AWS-native) |

---

## Quick Comparison: At a Glance

| App | Primary Language | AI Strategy | Speed Mechanism | Key Differentiator |
|-----|----------------|-------------|----------------|-------------------|
| **EchoAI** | Rust + TypeScript | Tiered escalation + local LLM | Cache→FTS→ANN→LLM | 100% offline, KG + Voice + Agent in one app |
| **Salesforce** | Apex + LWC | Deterministic Scripting (DSL) | Metadata Grounding (~40% loop reduction) | Deterministic AI guardrails via Agentforce Script |
| **ServiceNow** | Java + JavaScript | Domain-specialized LLM | Semantic Caching (90% similarity) | RaptorDB hybrid row/columnar for AI workloads |
| **SAP Joule** | ABAP + Java | Model Orchestration (BTP) | HANA In-Memory Dimensional Pointers | Embedded prompts in ABAP code as native language |
| **Glean** | Go + TypeScript | Agentic Search (MCP Host) | Pre-Inference Routing via Waldo SLM (250ms) | Waldo DPO-trained routing model + MCP protocol |
| **Workday Sana** | XpressO + Python | Multi-Agent Swarms | Delta-Indexing via Webhooks | Supervisor-Worker swarm + Skills Cloud AI graph |
| **Workday Adaptive** | Java + Wasm | Embedded ML Models | In-Memory Hypercube + Parallel Sharding | Elastic Hypercube for N-dimensional financial models |
| **Cube** | Rust + Node.js | MCP + Semantic Layer | Rollup Pre-aggregations + Redis Cache | Native MCP Server — any agent queries financial data |
| **Vena** | .NET + Office JS | Knowledge Graph RAG | Delta-Cell Syncing (Calculation Chain monitoring) | Excel-native with real-time cloud sync |
| **Planful** | C# + Python | Time-Series ML (Predict) | Async ECE + SignalR streaming | Random Forest/XGBoost tuned for financial seasonality |
| **Prophix** | Node.js + Python | Agentic Copilots (Bedrock) | Kafka Event Streams + Lambda scaling | Unified schema + real-time stream processing |

---

*Reference date: 2026-05-01. Information sourced from official documentation, developer blogs, and platform announcements.*
