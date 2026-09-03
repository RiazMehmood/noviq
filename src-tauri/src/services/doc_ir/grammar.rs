//! T019 — llguidance JSON-schema sampler binding for the IR builder.
//!
//! Feature-gated under `llm` (matches the rest of the llama-cpp-2 surface).
//! The `llguidance` sub-feature is enabled by the workspace's `llm` feature
//! (see Cargo.toml).

use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;

use crate::services::doc_ir::types::SCHEMA_JSON;

/// Build a JSON-schema-constrained sampler for the canonical IR schema.
///
/// llguidance enforces the schema during token sampling so the model can only
/// emit tokens that keep the partial JSON valid. Bumps first-try validity
/// from ~85% (free-form) to ~99% (constrained).
pub fn build_ir_sampler(model: &LlamaModel) -> anyhow::Result<LlamaSampler> {
    LlamaSampler::llguidance(model, "json_schema", SCHEMA_JSON)
        .map_err(|e| anyhow::anyhow!("failed to build IR llguidance sampler: {e}"))
}

/// Build a tiny classifier-output sampler (constrains output to the
/// `{"doc_type": "<one of six>"}` shape).
pub fn build_classifier_sampler(model: &LlamaModel) -> anyhow::Result<LlamaSampler> {
    const CLASSIFIER_SCHEMA: &str = r#"{
        "type": "object",
        "properties": {
            "doc_type": {
                "type": "string",
                "enum": ["constitution","act","rules","sro","contract","generic"]
            }
        },
        "required": ["doc_type"],
        "additionalProperties": false
    }"#;
    LlamaSampler::llguidance(model, "json_schema", CLASSIFIER_SCHEMA)
        .map_err(|e| anyhow::anyhow!("failed to build classifier sampler: {e}"))
}
