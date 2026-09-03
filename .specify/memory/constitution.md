<!--
Sync Impact Report — 2026-06-02

Version change: 1.0.0 → 1.1.0  (MINOR — five new principles added under a new
"Domain & Product Principles" section; no existing principle redefined or removed.)

Modified principles:
  (none redefined)

Added sections:
  • Domain & Product Principles
    – IX. Pakistan Government Legal Domain (NON-NEGOTIABLE)
    – X. Officer-First UX
    – XI. Citation Discipline (NON-NEGOTIABLE)
    – XII. Latest Text + Amendment Chain
    – XIII. Cross-Reference Graph as First-Class

Removed sections:
  (none)

Templates requiring updates:
  ✅ .specify/memory/constitution.md (this file)
  ✅ .specify/templates/plan-template.md — Constitution Check section is
     parameterized ("[Gates determined based on constitution file]"); no edits
     needed. Future /sp.plan runs will pick up the 13-principle set.
  ✅ .specify/templates/spec-template.md — no principle references; no edits.
  ✅ .specify/templates/tasks-template.md — no principle references; no edits.

Reference compatibility notes:
  • Principle I (Privacy-First, Offline-Always) explicitly permits outbound
    network access to URLs provided by the user. The Feature 007 design's
    eval-only cloud LLM toggle (FR-028, FR-029) is officer-initiated and thus
    compatible. No amendment to Principle I required.
  • Existing Principles II (Tiered Intelligence) and III (Single Model Slot)
    accommodate the new offline-mid tier (Mistral 7B already covered as the
    "worker" model in II/III).

Manual follow-ups (outside repo):
  • User memory file /home/riaz/.claude/projects/-media-riaz-New-Volume-echo-AI/memory/project_echoai_domain_scope.md
    — prepend SUPERSEDED header pointing to Principle IX.
  • User memory file /home/riaz/.claude/projects/-media-riaz-New-Volume-echo-AI/memory/feedback_llm_ir_only_pdf_scanned_images.md
    — prepend PARTIALLY SUPERSEDED header (PDF/scanned/image scope rule still
    in force; the "domain stays generic" framing is retired by Principle IX).
  • MEMORY.md index — annotate both entries with
    "— SUPERSEDED 2026-06-02 by constitution §IX" (full or partial).
-->

# EchoAI Constitution

**Version**: 1.1.0 | **Ratified**: 2026-04-13 | **Last Amended**: 2026-06-02

---

## Core Principles

### I. Privacy-First, Offline-Always (NON-NEGOTIABLE)
All inference, storage, and processing MUST occur on the local device. Zero telemetry, zero cloud API calls, zero remote model hosting. The only permitted outbound network access is to URLs explicitly provided by the user in a task or query. Violation of this principle is a release blocker regardless of feature value.

### II. Tiered Intelligence — Cheapest Tier First
Every LLM call MUST attempt the lowest capable tier before escalating. Tier 0 (cache hit) → Tier 1 (FTS/regex) → Tier 2 (ANN semantic) → Tier 3 (Orchestrator 3B, always warm) → Tier 4 (Worker 7B, demand-loaded). Skipping tiers wastes RAM and inference time; escalation MUST only occur when a lower tier demonstrably cannot satisfy the request.

### III. Single Model Slot — Bounded RAM Budget (NON-NEGOTIABLE)
At most one LLM is resident in RAM at any time across the entire process. Models MUST be GGUF Q4_K_M minimum quantization. The orchestrator (3B) stays warm; the worker (7B) is demand-loaded and evicted after 90 seconds of idle. No two LLM instances may be loaded simultaneously. This constraint is the primary defence against OOM on 8–16 GB machines.

### IV. SQLite as Single Source of Truth
All persistent state lives in a single `echo.db` SQLite file. No external database processes, no Redis, no Postgres. sqlite-vec for vector embeddings, FTS5 for full-text search. Multiple feature areas add their own tables via migrations; they MUST NOT create separate database files.

### V. Pending Actions Queue — Safety Gate (NON-NEGOTIABLE)
Any tool call classified as DANGEROUS (file delete, shell command with side effects, system modification, code execution with write access) MUST be placed in the Pending Actions Queue and require explicit user approval before execution. CAUTIOUS tools (file read, directory list, web fetch) may execute with a brief confirmation. READ-ONLY tools execute immediately. This gate cannot be bypassed programmatically.

### VI. Persona-as-Config — Agents Defined in TOML
Agent behaviour (system prompt, tool schema, capability list, safety tier per tool) MUST be defined in TOML configuration files, not hardcoded. Adding a new agent persona or tool requires only a TOML edit, not a code change. The Rust ReAct loop reads persona config at runtime. No Python orchestration frameworks.

### VII. Hardware-Adaptive Behavior
The app MUST detect available RAM at startup and select the appropriate hardware tier (Tier 1: <8 GB, Tier 2: 8–12 GB, Tier 3: 12–16 GB, Tier 4: 16–32 GB, Tier 5: 32 GB+). Features unavailable on the current tier MUST degrade gracefully with a user-visible explanation — never silently fail or crash. ECHO_FORCE_TIER overrides detection for testing only.

### VIII. Incremental Feature-App Delivery
Each Feature-App (App 1: Notebook, App 2: Agent Overlay, App 3: TBD, App 4: TBD) MUST be independently shippable and testable without requiring other Feature-Apps to be complete. User stories within each app MUST be prioritised so P1 alone delivers a usable MVP. No cross-app hard dependencies at the feature level.

---

## Domain & Product Principles

*Added 2026-06-02 (constitution v1.1.0). These principles fix the product's
domain, primary user, and the legal-RAG-specific quality bar. They supersede
the prior "domain is generic, do not narrow to legal" guidance recorded in
the user-memory tree (see Sync Impact Report at top of this file).*

### IX. Pakistan Government Legal Domain (NON-NEGOTIABLE)
EchoAI is a Pakistan government legal RAG system. The supported artifact types are: the Constitution, Acts, Ordinances, Rules, Regulations, and Policies, together with their cross-references. The following are explicitly out of scope: case law and judgments, foreign jurisdictions, and private legal practice content. Features, prompts, evaluators, and acceptance gates MUST be tuned for this domain. A change that broadens scope (re-introducing case law, adding a new jurisdiction, or supporting private-practice workflows) requires a constitution amendment.

### X. Officer-First UX
The primary user is a Pakistan government officer or policy analyst. All product defaults — citation form, answer length, multi-turn behaviour, export shape, and "see also" surfacing — MUST optimize for this persona. Citizen-friendly summaries and lawyer-grade parallel citations MAY be supported as secondary modes but MUST NOT take priority over officer defaults. UX choices that improve a secondary persona at measurable cost to the officer flow MUST be rejected.

### XI. Citation Discipline (NON-NEGOTIABLE)
Every answer that surfaces legal content MUST carry at least one citation. Paraphrased legal text without a citation is a release blocker. For deep-indexed sources, the citation MUST render the full hierarchical legal path (instrument name + identifier + year + Chapter/Part → Section → Sub-section → Clause → Sub-clause as applicable) and the latest amendment status. For fast-indexed sources, the citation MUST render the instrument name plus a page or span reference and MUST disclose that hierarchical detail is unavailable on the fast tier. A single canonical citation format MUST be used wherever citations appear — chat, "see also", exports, copy-as-note.

### XII. Latest Text + Amendment Chain
The system's truth model is the latest consolidated text of every instrument. Each section MUST carry its amendment chain — who inserted/substituted/repealed/omitted it, by which amending instrument, and when (where extractable from the source). Point-in-time reconstruction of historical text ("as of date X") is out of scope for v1 and MUST NOT be implemented without a constitution amendment. Unrecognized amendment markers MUST be tolerated as "unparsed marker" rather than failing the document.

### XIII. Cross-Reference Graph as First-Class
Acts, Rules, and Policies form a typed cross-reference graph (`cites`, `implements`, `under`, `amends`). This graph is a first-class retrieval surface, not a derived view: queries that benefit from it (e.g., "Rules under Section X of Act Y", "Policies citing Section X", procedure flows that span an Act and its Rules) MUST use the graph rather than re-deriving relationships at query time. The graph is populated on the deep tier only; fast-tier docs participate as endpoints when graph edges land on them post-upgrade.

---

## Technology Stack Invariants

These are locked for all Feature-Apps. Deviation requires a constitution amendment with ADR.

| Layer | Technology |
|-------|-----------|
| Desktop shell | Tauri 2.0 |
| Frontend | SolidJS + Tailwind CSS |
| Backend | Rust (stable) |
| LLM inference | llama-cpp-2 (llama.cpp bindings) |
| Embedding | fastembed-rs (ONNX, local) |
| Speech-to-text | whisper-rs (whisper.cpp bindings) |
| Text-to-speech | Kokoro TTS (local neural, no cloud) |
| Vector search | sqlite-vec (vec0 extension) |
| Full-text search | SQLite FTS5 |
| Database | SQLite via sqlx |
| Sandboxed execution | Wasmtime / WASI |
| Model format | GGUF Q4_K_M minimum |

---

## Development Constraints

- All changes MUST be the smallest viable diff; no refactoring of unrelated code.
- No secrets or tokens hardcoded; use `.env` for all credentials and paths.
- Every user-facing error MUST have a human-readable message and a suggested remediation.
- Performance budgets: p95 query latency < 2s (Tier 2), < 10s (Tier 3/4); overlay activation < 1s.
- All DB migrations MUST be additive (no column drops, no table drops) unless a dedicated migration task is explicitly planned.

## Governance

This constitution supersedes all other practices. Amendments require:
1. A documented ADR explaining the reason and tradeoffs.
2. Update to this file with version bump and amendment date.
3. Review of all existing specs/plans for conflicts.

All plans generated by `/sp.plan` MUST include a Constitution Check section verifying compliance with all 13 principles (I–VIII Core + IX–XIII Domain & Product) before tasks are generated.

**Versioning policy.** MAJOR bumps for backward-incompatible governance or principle redefinitions/removals; MINOR bumps for new principles or materially expanded guidance; PATCH bumps for clarifications, wording, and non-semantic refinements. Each amendment writes a Sync Impact Report at the top of this file recording the old → new version, the affected principles, and template-propagation status.
