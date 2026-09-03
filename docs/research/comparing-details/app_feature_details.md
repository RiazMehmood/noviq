# Enterprise App Feature & Technology Reference

**Date:** 2026-05-01
**Purpose:** Per-app detailed feature and technology breakdown across enterprise AI, FP&A, and knowledge platforms.

---

## Table of Contents

1. [Salesforce Agentforce](#1-salesforce-agentforce)
2. [ServiceNow](#2-servicenow)
3. [SAP Joule](#3-sap-joule)
4. [Glean](#4-glean)
5. [Workday Sana](#5-workday-sana)
6. [Workday Adaptive Planning](#6-workday-adaptive-planning)
7. [Cube](#7-cube)
8. [Vena Solutions](#8-vena-solutions)
9. [Planful](#9-planful)
10. [Prophix One](#10-prophix-one)
11. [Synoptix AI](#11-synoptix-ai)

---

## 1. Salesforce Agentforce

**Type:** Enterprise CRM + Agentic Operating System
**Architecture:** "Deterministic Sandwich" — generative AI wrapped in hardened deterministic logic
**Target:** Enterprise sales, service, and marketing teams

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
  - Streaming LLM responses to UI via GraphQL Wire Adapters
  - Real-time UI updates during agent execution

- **Einstein AI Features**
  - Natural language CRM queries
  - Automated email and call summaries
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
| LLM providers | OpenAI, Anthropic, Google (via Einstein Trust Layer) |
| IDE | Agentforce Studio |

---

## 2. ServiceNow

**Type:** Enterprise ITSM + Agentic Digital Worker Platform
**Architecture:** High-ingest operations — millions of system signals processed per second
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
  - Domain expert only — not a general-purpose LLM
  - Faster and cheaper than GPT/Claude for ITSM-specific tasks
  - Understands ServiceNow data model natively

- **Semantic Caching**
  - Every resolved IT ticket is embedded and stored
  - New incoming issue embedded and compared against cache
  - Cosine similarity > 0.90 = cache hit, skip reasoning phase
  - Jumps directly to cached remediation script
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
  - Developer-facing debugging tool

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
| Debug tool | Agentic ATF (visual decision tree debugger) |
| Backend | Java / JavaScript (Node.js) |
| Deployment | Cloud SaaS + on-premise option |

---

## 3. SAP Joule

**Type:** Enterprise ERP AI Layer + Global Orchestration Platform
**Architecture:** In-memory HANA database + multi-model AI hub on SAP BTP
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
  - Single unified API, multiple underlying models
  - Cost and latency optimized per task type

- **ABAP AI**
  - 2026 upgrade to the ABAP programming language
  - Embed AI prompts directly as native class method calls
  - Prompts are first-class language citizens — not external API calls
  - Type-safe, compiled — no Python subprocess overhead

- **HANA Cloud In-Memory Database**
  - Entire data model lives in server RAM
  - Vector Search integrated directly into SQL
  - Single SQL query joins structured financial data with unstructured contract text
  - No separate vector database process needed
  - Columnar storage for analytical workloads

- **Dimensional Pointers**
  - Every data point has a coordinate in multidimensional space
  - Lookup = pointer dereference, O(1) — not a SQL JOIN or table scan
  - AI traverses the data graph at hardware memory speeds
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
| Backend language | ABAP (compiled, SAP-native) + Java |
| AI embedding | ABAP AI (native prompt embedding in language) |
| Model platform | Generative AI Hub (BTP) — multi-model |
| LLM providers | GPT-5.2, Gemini 3.0, Claude 4.6 (task-switchable) |
| Database | HANA Cloud (in-memory, columnar) |
| Vector search | HANA Vector Engine (built into SQL) |
| Pointer system | Dimensional Pointers (proprietary in-memory) |
| Integration | SAP Integration Suite |
| Deployment | SAP BTP (Business Technology Platform) — cloud |

---

## 4. Glean

**Type:** Enterprise Agentic Search + Organizational Knowledge Platform
**Architecture:** Pre-Inference Routing with Waldo — a specialized Small Language Model
**Target:** Knowledge workers, developers, enterprise-wide search

### Core Features

- **Waldo (SLM — Small Language Model)**
  - 7–10B parameter model
  - Trained via DPO (Direct Preference Optimization) on routing decisions
  - Sole purpose: decide which tools to call before waking a large model
  - Does NOT perform creative writing or complex reasoning
  - Operates at 250ms latency
  - "Search First, Reason Later" pattern

- **Pre-Inference Routing**
  - Waldo plans the search strategy (which tools, data sources, model)
  - Large model (GPT-5, Claude) only invoked for final reasoning step
  - Saves 2–3 seconds per query vs calling large model directly
  - Reduces cost per query by routing simple queries to smaller models

- **Model Context Protocol (MCP) Host**
  - Glean acts as an MCP Host
  - Connects to any third-party agent via standardized JSON-RPC protocol
  - Custom Python bots, internal agents, IDE tools can all talk to Glean
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

- **Connectors Ecosystem**
  - 100+ pre-built connectors: Google Drive, Confluence, Jira, GitHub, Slack, Salesforce, Zendesk, and more
  - Indexes content from all connected sources into unified knowledge graph
  - Permissions-aware — user only sees content they have access to

- **People Graph**
  - Maps employee expertise based on what they write and work on
  - "Who in our company knows most about Kubernetes?" → returns ranked list
  - Expertise scoring from writing patterns, not self-declaration

- **Real-Time Indexing**
  - Content indexed within minutes of creation or modification
  - Permissions synced in real-time from source systems
  - No stale search results on recently updated documents

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | React / TypeScript |
| UI delivery | Chrome extension, Slack bot, IDE plugin, web app |
| Routing model | Waldo SLM (7–10B, DPO-trained) |
| Primary language | Go (backend), TypeScript (frontend) |
| Protocol | MCP Host (JSON-RPC) |
| Knowledge graph | Distributed index (proprietary graph engine) |
| Vector search | Distributed vector index (proprietary) |
| Permissions | Real-time ACL sync from source systems |
| Deployment | Cloud SaaS |
| Integrations | 100+ connectors |

---

## 5. Workday Sana

**Type:** AI Operating System for People & Money (HR + Finance)
**Architecture:** Multi-Agent Swarm with Supervisor-Worker pattern + Delta-Indexing
**Target:** Enterprise HR, Finance, and Payroll teams

### Core Features

- **Multi-Agent Swarm**
  - Supervisor Agent decomposes high-level user requests
  - Specialized Worker Agents: Onboarding, Payroll, Benefits, IT Provisioning, Facilities
  - Workers execute in parallel where tasks are independent
  - Supervisor waits for all workers, resolves conflicts, synthesizes result
  - No agent exceeds its defined permission boundaries

- **Delta-Indexing via Webhook Streams**
  - Registers webhooks with source systems (ERP, HRIS, Salesforce)
  - When a contract is signed → webhook fires → Sana index updates in milliseconds
  - No nightly batch re-indexing
  - World view always current
  - Supports millions of webhook events per day

- **Skills Cloud AI**
  - Specialized graph database
  - Maps 50,000+ human skills to job requirements in real-time
  - Real-time matching: employee skills ↔ open roles
  - Identifies skill gaps across teams
  - Suggests personalized learning paths based on career trajectory

- **Sana UI**
  - AI-native dashboard — abstracts legacy Workday interface complexity
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
  - Executive asking the same question → sees org-wide data

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
| Backend | XpressO (Workday-native) + Python |
| Deployment | Cloud SaaS (AWS) |
| Security | SOC 2 Type II, GDPR compliant |

---

## 6. Workday Adaptive Planning

**Type:** Enterprise FP&A (Financial Planning & Analysis) Platform
**Architecture:** Elastic Hypercube in-memory multidimensional engine
**Target:** Finance teams, CFO office, corporate planning departments

### Core Features

- **Elastic Hypercube Engine**
  - Proprietary multidimensional in-memory calculation engine
  - Solves the "N-Dimensional Problem": 5,000 products × 100 regions × 60 months × 10 currencies
  - Data stored as coordinates in multidimensional space — not relational rows
  - No SQL — no table scans, no JOINs
  - Entire financial model lives in server RAM during active sessions

- **Dimensional Pointers**
  - Every data point = a coordinate in the Hypercube
  - Lookup = pointer dereference, O(1) access
  - "Jump" to any data point instantly regardless of model size
  - Enables instant "What-If" scenario recalculation across millions of permutations

- **Parallel Calculation Sharding**
  - "What-If" scenarios split Hypercube into N chunks
  - N chunks calculated simultaneously across CPU cores and compute nodes
  - Results merged and reconciled in milliseconds
  - User sees instant scenario results regardless of model complexity

- **AI Predictive Forecaster**
  - Time-series ML models embedded in the platform
  - Uses 5+ years of historical data for budget forecasting
  - Suggests "most likely" outcome for planning cycles
  - Flags anomalies vs historical baseline automatically
  - Embedded in planning workflows — not a separate tool

- **WebAssembly Sheet Interface**
  - React-based frontend with Wasm for the "Sheet" view
  - Handles million-cell financial grids in the browser with zero lag
  - Spreadsheet-like interface familiar to finance teams
  - Full performance in browser — no native app required

- **Scenario Planning**
  - Unlimited "What-If" scenarios
  - Any scenario change propagates instantly across all dimensions
  - Side-by-side scenario comparison
  - Approval workflows for scenario promotion to plan

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

## 7. Cube

**Type:** Semantic Layer Platform + Headless BI Engine
**Architecture:** Rust-based semantic translation layer between data warehouses and AI agents
**Target:** Data engineers, BI developers, AI agent builders

### Core Features

- **Rust-Based Semantic Layer Core**
  - Cube Core written in Rust — memory safe, extreme performance
  - Defines business metrics once — used everywhere (Excel, Tableau, AI agents, APIs)
  - Universal Semantic Layer: "Gross Margin" defined once = identical result in every tool
  - Eliminates metric inconsistency across teams and tools

- **Native MCP Server**
  - Model Context Protocol server built into the platform
  - Any LLM (Claude, GPT, custom) talks to Cube via MCP JSON-RPC
  - LLM receives Metric definitions and Dimension schemas via MCP
  - LLM never writes raw SQL — Cube translates intent to optimized queries
  - Grounded querying enforced at infrastructure level

- **Pre-Aggregations (Rollups)**
  - Automatically detects frequent query patterns
  - Creates materialized rollup tables for common aggregations
  - "Total Revenue by Region" pulled from pre-computed rollup, not raw 100M row table
  - Result: <100ms query time vs 45+ seconds on raw data lake
  - Rollups refreshed on schedule or incrementally

- **Multi-API Support**
  - GraphQL API
  - REST API
  - SQL API — Cube presents itself as a Postgres-compatible database to other tools
  - Any BI tool, spreadsheet, or AI agent that speaks SQL or GraphQL connects natively

- **Headless Philosophy**
  - No built-in frontend — pure data API layer
  - Works with any frontend: Excel, Tableau, Looker, custom React app, AI agent
  - "Bring your own UI"

- **Redis Caching Layer**
  - Sub-second query caching via Redis
  - Pre-aggregation results cached for instant retrieval
  - Cache invalidated automatically on upstream data refresh

- **Data Warehouse Connectors**
  - Snowflake, BigQuery, Redshift, DuckDB, MotherDuck, ClickHouse, PostgreSQL, and more
  - Federated queries across multiple warehouses
  - No data movement — queries pushed down to source system

- **Access Control**
  - Row-level and column-level security
  - Tenant isolation for multi-tenant deployments
  - API key and JWT authentication

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Core engine | Rust (Cube Core) |
| API layer | GraphQL + REST + SQL API |
| Protocol | Native MCP Server (JSON-RPC) |
| Caching | Redis (sub-second query cache) |
| Pre-aggregations | Automatic rollup materialization |
| Data warehouses | Snowflake, BigQuery, DuckDB, MotherDuck, ClickHouse, Postgres |
| Auth | API key, JWT |
| Node.js layer | Node.js (API server, schema definitions in JS/TS) |
| Deployment | Self-hosted (Docker) or Cube Cloud (SaaS) |

---

## 8. Vena Solutions

**Type:** Enterprise FP&A Platform — Excel-First with High-Performance Cloud Backend
**Architecture:** "Bridge" design — Excel as UI layer, .NET backend as logic and data layer
**Target:** Finance teams that live in Excel; CFO office; FP&A analysts

### Core Features

- **Vena Nexus (Core Sync Engine)**
  - Proprietary .NET-based middleware
  - Syncs Excel's internal XML data structure with centralized database in real-time
  - Monitors Excel's Calculation Chain — the dependency DAG of all cells
  - On any cell change, identifies all downstream dependent cells automatically

- **Delta-Cell Syncing**
  - Does NOT upload the whole Excel file on change
  - Monitors which specific cells changed via Calculation Chain monitoring
  - Uploads only changed cells (deltas) — not the full workbook
  - Multi-gigabyte financial models stay fast because only deltas transmit
  - Equivalent of a git diff — send the change, not the full state

- **Vena Insights (AI Layer)**
  - Knowledge Graph that maps how Excel workbooks relate to each other
  - "Cell B3 in Budget.xlsx feeds into Forecast.xlsx column C"
  - AI traces a number back to its source across multiple linked spreadsheets
  - Natural language variance reports: "Revenue is down 12% vs forecast because..."
  - Proactive anomaly detection — flags unusual numbers before user notices

- **Vena Nexus AI**
  - Hyperscale infrastructure for training custom AI models on client financial data
  - Finance-specific Knowledge Graph to prevent AI hallucinating numbers
  - Custom models trained on the organization's own historical financial patterns

- **Workflow Automation**
  - Visual drag-and-drop workflow designer
  - Multi-departmental budget cycle orchestration
  - Approval routing — who reviews what, in what order
  - Email notifications and reminders built in
  - Deadline tracking per workflow stage

- **Excel Add-in**
  - Microsoft Office JS API integration
  - Works inside Excel — no context switching required
  - Real-time sync visible in Excel without leaving the spreadsheet
  - Familiar interface for finance teams already living in Excel

- **Version Control**
  - Full version history of all budget and forecast changes
  - Who changed what, when, with side-by-side comparison views
  - Rollback to any previous version

- **Audit Trail**
  - Complete log of all data changes with user attribution
  - Who entered, approved, or modified any number
  - Regulatory compliance ready (SOX, IFRS)

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend (Excel) | Microsoft Office JS API (Excel Add-in) |
| Frontend (web) | React + TypeScript web portal |
| Backend | .NET Core + C# microservices |
| Sync engine | Vena Nexus (.NET-based, proprietary) |
| AI layer | Vena Insights (Knowledge Graph-powered AI) |
| AI training | Vena Nexus AI (custom model training on client data) |
| Knowledge graph | Proprietary (workbook relationship graph) |
| Integration | Microsoft 365, ERP connectors (NetSuite, SAP, Oracle) |
| Deployment | Cloud SaaS (Vena-hosted) |

---

## 9. Planful

**Type:** Continuous Financial Planning Platform
**Architecture:** Elastic Calculation Engine (ECE) with async worker pools and ML forecasting
**Target:** Mid-market to enterprise finance and accounting teams

### Core Features

- **Elastic Calculation Engine (ECE)**
  - Specialized compute layer for complex financial logic
  - Handles currency translations, intercompany eliminations, allocations
  - Asynchronous — budget submissions queued as background jobs
  - UI never freezes during complex calculations
  - Horizontally scalable — worker pool grows under load

- **Asynchronous Background Processing**
  - All heavy operations run in background worker queues
  - UI returns "calculating..." immediately with a progress indicator
  - SignalR WebSocket pushes result to UI when complete
  - Users continue working while calculations run in background

- **Planful Predict (ML Suite)**
  - Time-Series Forecasting using Random Forest and XGBoost
  - Models tuned specifically for financial seasonality (monthly, quarterly, annual cycles)
  - Anomaly Detection — "Is this month's expense unusual vs history?"
  - Runs as background Python processes — not blocking the main application
  - Trained on client's own 6–24 months of historical financial data

- **SignalR Real-Time Streaming**
  - WebSocket-based real-time data push to all connected dashboards
  - Budget changes propagate to all viewing users simultaneously
  - Live collaboration — multiple planners see changes as they happen
  - No page refresh required

- **Analyst & Planner Assistants**
  - Role-specific AI agents
  - Analyst Assistant: builds entire forecast models from a single natural language prompt
  - Planner Assistant: manages budget workflow, routing, and approvals
  - Each agent understands the specific data needs of its role

- **Continuous Planning Workflow**
  - Rolling forecasts — not limited to annual budget cycles
  - Triggered updates when actuals differ from plan beyond a set threshold
  - Automated reforecast workflows
  - Driver-based planning: revenue drives headcount, headcount drives expenses

- **Reporting & Dashboards**
  - Drag-and-drop report builder
  - Pre-built CFO dashboards
  - Variance analysis: actual vs plan vs forecast vs prior year
  - Drilldown from summary level to individual transaction

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | Angular + React |
| Real-time streaming | SignalR (WebSocket-based) |
| Calculation engine | Elastic Calculation Engine (ECE, proprietary) |
| ML/AI | Python — Random Forest, XGBoost (time-series tuned) |
| AI agents | OpenAI Enterprise APIs (grounded by ECE) |
| Agent pattern | Agentic RAG |
| Backend | C# / Azure-native microservices |
| Database | Azure SQL + Azure Data Lake Storage |
| Async processing | Background worker queue (Azure Service Bus) |
| Deployment | Cloud SaaS (Azure-hosted) |

---

## 10. Prophix One

**Type:** All-in-One Autonomous Finance Platform ("Virtual CFO")
**Architecture:** Unified financial database + Kafka-style event streams + Amazon Bedrock
**Target:** Mid-market companies wanting full CFO office automation

### Core Features

- **Unified Financial Database**
  - Single schema for all financial operations
  - Monthly Close, Operational Planning, Reporting, Consolidation — one data model
  - No module silos — data flows freely between all processes
  - No ETL between planning and close modules

- **Prophix One Intelligence (AI Hub)**
  - Centralized AI hub powering all platform features
  - Built on Amazon Bedrock — multi-model flexibility
  - Anomaly Detection — flags unusual transactions in real-time as they arrive
  - Predictive Forecasting — suggests future values based on historical patterns
  - Natural language copilot across all modules

- **Agentic Copilot**
  - Natural language commands: "Generate a board deck for Q3"
  - Natural language queries: "Find why electricity costs spiked in Germany"
  - Agentic RAG: retrieve → compute → retrieve more → synthesize final answer
  - Multi-pass retrieval until confident answer is found
  - Executes tasks and produces artifacts — not just text answers

- **Kafka-Style Event Streams**
  - Real-time event bus for all financial data changes
  - Transaction enters from NetSuite/SAP → Event fires immediately
  - Event consumers: Anomaly Detector, Dashboard Updater, AI Copilot Context
  - Time from ERP transaction to dashboard update: <500ms
  - No nightly batch consolidation required

- **Amazon Bedrock Multi-Model Routing**
  - Not locked to one LLM
  - Summarization tasks → Claude
  - Structured extraction → Llama
  - Embeddings → Amazon Titan
  - Task-specific model routing = cost and latency optimization per operation

- **Drag-and-Drop Workflow Designer**
  - Visual workflow builder using React Flow
  - Month-end close checklist with task dependencies
  - Budget approval routing flows
  - Automated journal entry workflows
  - Drag tasks onto canvas, connect with arrows, set conditions

- **AWS Lambda Serverless Scaling**
  - Calculation tasks run as serverless Lambda functions
  - Scale to zero when idle — no wasted compute
  - Burst capacity for month-end reporting spikes
  - Python-based calculation logic for flexibility

- **Month-End Close Automation**
  - Automated reconciliations across accounts
  - Journal entry suggestions based on historical patterns
  - Variance explanations auto-generated in natural language
  - Close checklist with real-time completion tracking

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | React + React Flow (drag-and-drop workflows) |
| Backend | Node.js + Python |
| Event streaming | Kafka-style event streams (AWS MSK / Kinesis) |
| Serverless compute | AWS Lambda (Python) |
| AI platform | Amazon Bedrock (multi-model) |
| LLM providers | Claude, Llama, Amazon Titan (task-routed) |
| AI pattern | Agentic RAG (multi-pass retrieval + tool use) |
| Database | Unified Financial Database (proprietary schema) |
| Workflow designer | React Flow |
| ERP connectors | NetSuite, SAP, Oracle, QuickBooks |
| Deployment | Cloud SaaS (AWS-native) |

---

## 11. Synoptix AI

**Type:** Enterprise AI Orchestration + Agentic Knowledge Platform
**Architecture:** Four-layer architecture — Ingestion → Intelligence (EPS) → Agentic → Governance
**Target:** Enterprises with fragmented data silos (SharePoint, Salesforce, local databases) needing unified intelligent search and automated workflows

### Core Features

- **Enterprise Positioning System (EPS)**
  - Core "logic" engine of the platform
  - Graph-database architecture that maps how entities relate to each other
  - Entities: employees, projects, clients, documents, emails
  - Searches for "Project X budget" → finds the file, the people associated, and emails discussing costs
  - Understands business "logic," not just keyword patterns
  - Goes beyond flat vector similarity to relationship-aware retrieval

- **Multi-Agent Orchestration**
  - Deploy specialized domain agents simultaneously
  - Finance Agent — handles P&L queries, financial data extraction
  - HR Agent — handles resume parsing, org chart queries
  - Agents collaborate and share context to solve complex cross-domain queries
  - "Think-Act-Observe" reasoning loop for multi-step tasks
  - Breaks complex queries into sub-tasks, assigns to agents, compiles final answer

- **Semantic & Graph Search**
  - Graph-Theory Architecture for relationship-aware retrieval
  - Vector store (Pinecone or Milvus) for embedding-based similarity search
  - Semantic Reranker: pulls top 50 results from vector store, then second AI model re-ranks for accuracy
  - Two-stage retrieval: broad vector recall → precise reranker refinement
  - Finds documents, people, and conversations related to a query — not just matching text

- **Universal Connectors via MCP**
  - Model Context Protocol (MCP) for all integrations
  - Connects to: Outlook, Microsoft Teams, SharePoint, Dropbox, Salesforce, and more
  - Real-time sync — no data moved to a new server
  - Federated access — data stays in source systems
  - Live API sync vs one-time data dump

- **Trust & Governance Layer (Security Perimeter)**
  - PII (Personally Identifiable Information) masking — credit card numbers, SSNs, emails masked before reaching LLM
  - Toxicity detection — filters harmful content from prompts and responses
  - Hallucination checks — validates AI-generated content before it reaches the user
  - Human-in-the-Loop (HITL) — admin review interface before AI content is published or sent to clients
  - Full audit trail of all queries and agent actions
  - Enterprise-grade compliance posture

- **LLM Agnostic Engine**
  - Switch between models based on task complexity and cost
  - GPT-4o for high reasoning tasks
  - Claude 3.5 for nuanced analysis
  - Gemini 2.0 for multimodal tasks
  - Lower-cost models for simple summarization
  - Single platform API regardless of which LLM runs underneath

- **Multimodal Data Extraction**
  - High-performance OCR for scanned PDF documents
  - Table extraction from complex Excel sheets
  - Video and audio transcription → clean text for AI indexing
  - Format converters: .xlsx, .pdf, .docx, video, audio → normalized text
  - Handles documents that standard text extraction fails on

- **Memory Manager**
  - Tracks previous user interactions within a session
  - Understands follow-up questions without re-stating context
  - "Now summarize that budget into a table" — remembers what "that budget" refers to
  - Persistent conversational context across multi-turn interactions

- **Real-Time Indexing**
  - Vector stores updated as source data changes
  - No stale search results on recently modified documents
  - Embeddings regenerated on document update via connector webhooks

- **Workflow Automation**
  - Agent executes tasks beyond answering — creates slides, drafts emails, writes code
  - Output is action (artifacts), not just text answers
  - Automated pipeline triggers based on query results

### Architecture Layers

**Layer A — Data Ingestion & Connector Layer**
- Federated Access: connects via live APIs to source systems
- Real-time Indexing: vector stores (Pinecone / Milvus) updated continuously
- Format Converters: .xlsx, .pdf, .docx, video, audio → clean text
- No data migration to a new server — data stays in place

**Layer B — Intelligence Layer ("The Brain")**
- Enterprise Positioning System (EPS): graph database for entity relationships
- LLM Agnostic Engine: model-switching by task (GPT-4o / Claude 3.5 / Gemini 2.0)
- Semantic Reranker: top-50 vector recall → second AI model re-ranks for accuracy

**Layer C — Agentic & Workflow Layer**
- Reasoning Loop: "Think-Act-Observe" multi-agent task decomposition
- Memory Manager: cross-turn context tracking within a session
- Workflow Execution: produces slides, emails, code — not just text answers

**Layer D — Governance & Security Perimeter**
- PII & Compliance Layer: masks sensitive data before prompt reaches LLM
- Hallucination Check: validates AI output before it reaches the user
- Human-in-the-Loop (HITL): admin review gate before publishing AI content
- Audit trail on all agent actions

### Comparison: Standard Vector Search vs Synoptix EPS

| Component | Standard Vector Search | Synoptix EPS |
|-----------|----------------------|--------------|
| Logic | Flat list of similar text chunks | Graph-based entity relationship logic |
| Connectivity | Local file scan | Live API sync (SharePoint, CRM, etc.) |
| Output | Text answer | Task execution (slides, emails, code) |
| Security | Basic or none | Enterprise PII masking + audit trails |

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Frontend | React / TypeScript (web portal) |
| Core intelligence | Enterprise Positioning System — EPS (graph database, proprietary) |
| Graph database | Proprietary graph engine for entity-relationship mapping |
| Vector store | Pinecone or Milvus (configurable) |
| Reranker | Secondary AI model for result re-ranking |
| LLM providers | GPT-4o, Claude 3.5, Gemini 2.0 (task-routed, LLM agnostic) |
| Agent pattern | Multi-Agent Orchestration (Think-Act-Observe loop) |
| Memory | Session-level Memory Manager |
| Protocol | MCP (Model Context Protocol) for all connectors |
| Connectors | Outlook, Teams, SharePoint, Dropbox, Salesforce, and more |
| OCR / Extraction | High-performance OCR + table extractor |
| Audio/Video | Transcription pipeline → text indexing |
| Governance | PII masking, toxicity filter, hallucination check, HITL review |
| Deployment | Cloud SaaS (enterprise-hosted) |

---

## Quick Comparison: All 11 Apps at a Glance

| App | Type | AI Strategy | Speed Mechanism | Standout Feature |
|-----|------|-------------|----------------|-----------------|
| **Salesforce Agentforce** | CRM + Agentic OS | Deterministic DSL wrapping LLM | Metadata Grounding (~40% loop reduction) | Agentforce Script DSL — LLM can't go rogue |
| **ServiceNow** | ITSM + Digital Workers | Domain-specialized LLM (Now Assist) | Semantic Caching (90% similarity = skip reasoning) | RaptorDB hybrid row/columnar for AI workloads |
| **SAP Joule** | ERP AI Layer | Multi-model hub (BTP) — toggle by task complexity | HANA In-Memory + Dimensional Pointers | ABAP AI — prompts as native compiled code |
| **Glean** | Enterprise Search | Pre-Inference Routing via Waldo SLM | Waldo routes in 250ms before large model wakes | DPO-trained routing model + MCP Host |
| **Workday Sana** | HR + Finance AI OS | Multi-Agent Swarm (Supervisor-Worker) | Delta-Indexing via Webhook Streams | Skills Cloud AI — 50,000+ skill graph |
| **Workday Adaptive** | FP&A | Embedded ML time-series forecasting | In-Memory Hypercube + Parallel Sharding | Elastic Hypercube for N-dimensional financial models |
| **Cube** | Semantic Layer / BI | MCP Server + Semantic Layer | Rollup Pre-aggregations + Redis Cache | Native MCP — any agent queries your data |
| **Vena** | FP&A (Excel-first) | Knowledge Graph RAG over workbooks | Delta-Cell Syncing via Calculation Chain | Excel-native real-time sync without full upload |
| **Planful** | FP&A | Agentic RAG + Time-Series ML (Predict) | Async ECE + SignalR streaming | Random Forest / XGBoost tuned for financial seasonality |
| **Prophix One** | FP&A Automation | Agentic Copilots on Amazon Bedrock | Kafka Event Streams + Lambda serverless scaling | Unified schema — no module silos, real-time stream processing |
| **Synoptix AI** | Enterprise AI Orchestration | Multi-Agent + Graph-Based EPS | Semantic Reranker (top-50 → precise re-rank) | EPS graph understands business logic, not just keywords |

---

*Reference date: 2026-05-01. Information sourced from official documentation, developer blogs, and platform announcements.*
