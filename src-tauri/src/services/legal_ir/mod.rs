//! Feature 007 — Legal IR pipeline (Pakistan government instruments).
//!
//! This module is the Slice-A/C/D home for the legal IR — the structured
//! representation a Deep re-index produces from a Pakistan government PDF.
//! In Slice 0 (Foundation) this is a skeleton: submodule files exist as
//! empty stubs and `mistral_builder` / `progress_store` re-export the
//! identically-named modules from Feature 006's `doc_ir` (constitution
//! §VIII "incremental delivery" — no fork, no duplication).
//!
//! Submodules:
//! - `types` — legal `NodeKind`, `LegalIRNode`, `LegalIRDocument` (Slice A T043).
//! - `schema` — llguidance JSON schema string (Slice A T044).
//! - `prompts` — Mistral build/classify prompts (Slice A T045).
//! - `amendment_parser` — deterministic regex pass (Slice A T046).
//! - `xref_extractor` — typed cross-reference edges (Slice C T068).
//! - `procedure_extractor` — procedure flow extraction (Slice D T076).
//! - `mistral_builder` — re-exports Feature 006's mistral_builder.
//! - `progress_store` — re-exports Feature 006's progress_store.

pub mod amendment_parser;
pub mod procedure_extractor;
pub mod prompts;
pub mod schema;
pub mod types;
pub mod xref_extractor;

// 2026-06: the Feature 006 doc_ir/Mistral engine was removed (one-ask drafting +
// template ingestion retired in F015). The former `mistral_builder` /
// `progress_store` re-exports are gone with it; legal_ir keeps only its own
// deterministic parsers (amendment/procedure/xref), used by structured_importer.
