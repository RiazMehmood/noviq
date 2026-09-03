pub mod hardware_detector;
// Live chat token/phase broadcast for the headless web SSE transport (F014+).
pub mod chat_stream;
pub mod embedder;
pub mod chunker;
pub mod display_title;
pub mod llm;
pub mod inference; // F016 — engine-agnostic TextGenerator seam (vLLM / llama.cpp)
// F014: Meeting Agent (audio capture + Whisper sidecar) is desktop-only.
#[cfg(feature = "desktop")]
pub mod meeting;
pub mod task_budget;
pub mod file_watcher;
pub mod indexer;
/// Upload-time check: does this PDF hold one legal instrument, or several?
pub mod instrument_detector;
/// Carries out a confirmed split of a compendium into one file per instrument.
pub mod instrument_split;
// F014: server-safe indexing orchestration + DTOs shared by the desktop
// commands and the headless HTTP API (moved out of the desktop-only `commands`).
pub mod indexing;
pub mod query_router;
pub mod query_trace;
pub mod entity_extractor;
pub mod knowledge_graph;
pub mod analysis_engine;
// F014: batch job runner is driven by desktop commands + Tauri notifications.
#[cfg(feature = "desktop")]
pub mod batch_processor;
pub mod archive;
pub mod parsers;
pub mod exporters;
pub mod scheduler;
pub mod scheduler_eta;
pub mod priority_model_slot;
pub mod cloud_llm;
// F014: Whisper STT sidecar (Tauri shell plugin) is desktop-only.
#[cfg(feature = "desktop")]
pub mod stt_sidecar;
pub mod heading_detector;
pub mod reranker;
pub mod grounding;
// Feature 003 — Constitution v1 hierarchical+graph (S2: doc profile + parser;
// S4: graph builder).
pub mod constitution_ir;
pub mod doc_profiles;
pub mod structure_parser;
pub mod graph_builder;
// GraphRAG Phase 3 — corpus cross-document `cross_reference` linker.
pub mod graph_linker;
// ADR-0019 — deterministic structured counting (Track 1).
pub mod counting;
// ADR-0019 §2 — Track 2: per-provision recall summaries (summaries-as-index).
pub mod summary_builder;
// Feature 005 — generic structured importer.
pub mod text_normalizer;
pub mod structured_importer;
// Feature 006 (doc_ir — LLM/Mistral PDF→IR importer) removed 2026-06: ingestion
// is a single deterministic path; one-ask/template-ingestion retired (F015).
// Files left dormant on disk per the additive policy.
// pub mod doc_ir;
// Feature 007 — Pakistan government legal RAG.
// Single source of truth for officer-grade legal citation rendering
// (constitution §XI). Used by every chat handler, every "see also" surface,
// every export — there must be exactly one citation formatter.
pub mod citation;
// Legal IR pipeline (Slice A) — reuses 006 mistral_builder + progress_store.
pub mod legal_ir;
// Officer-initiated deep re-index orchestrator (Slice A T050).
pub mod deep_reindex;
// Single chat surface — eight-intent router (Slices B/C/D).
pub mod intent_router;

pub mod deep_search; // F025 — bounded agentic multi-pass deep search (ADR-0021)
// Feature 009 — LGD Summary & Note drafting (template slot-schema + render).
pub mod drafting;
// Feature 015 — Unified Bureaucrat Chat Assistant (intent-routed single surface).
pub mod assistant;
// Feature 017 — Constrained Legal Retrieval Agent (bounded LLM-controller /
// Rust-executor loop over the F016 TextGenerator seam). Server-only per spec.
#[cfg(feature = "server")]
pub mod agent;
// F014 follow-on — per-query resource sampling for admin usage visibility.
pub mod usage_sampler;
// 2026-07-29 — web search as a SEPARATE retrieval lane (never blended with the
// legal corpus). Opt-in per turn; see the module docs for why.
pub mod websearch;

// Deterministic computation over an attached spreadsheet. The model plans, Rust
// computes — same split as ADR-0019's count capability.
pub mod tabular;

// Feature 027 — user-owned project workspaces: the deployment-wide availability
// gate and copy-on-share orchestration (ADR-0028, ADR-0029).
pub mod projects;
