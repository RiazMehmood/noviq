# Contract: `IrBuilder` trait + dispatch

**File**: `src-tauri/src/services/doc_ir/builder.rs`

## Trait

```rust
#[async_trait::async_trait]
pub trait IrBuilder: Send + Sync {
    /// Stable identifier surfaced in DocIr.llm_model and logs.
    fn name(&self) -> &'static str;

    /// `true` iff the builder is ready to convert (model file present, deps loaded).
    fn available(&self) -> bool;

    /// Convert a parsed document into a DocIr. Streams progress via `progress`
    /// and respects `cancel`.
    async fn build(
        &self,
        input: BuilderInput<'_>,
        progress: ProgressSink,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<BuilderOutput, IrBuildError>;
}

pub struct BuilderInput<'a> {
    pub source_id: &'a str,
    pub source_sha256: &'a str,
    pub source_pdf_path: &'a Path,
    pub extracted_text: &'a str,         // output of feature 004 OCR pipeline
    pub page_count: u32,
    pub forced_doc_type: Option<DocType>,
}

pub struct BuilderOutput {
    pub doc_ir: DocIr,
    pub issues: Vec<ValidationIssue>,    // total across all chunks, post-retries
    pub retry_counts: RetryCounts,
}

pub struct RetryCounts { pub chunk_retries: u32, pub doc_retries: u32 }

#[derive(thiserror::Error, Debug)]
pub enum IrBuildError {
    #[error("model unavailable: {0}")]
    ModelUnavailable(String),
    #[error("source text too small ({chars} chars)")]
    InsufficientText { chars: usize },
    #[error("cancelled")]
    Cancelled,
    #[error("validator exhausted retries on {failed_chunks} chunk(s)")]
    ValidationExhausted { failed_chunks: usize },
    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}
```

## Dispatch (in `indexer::do_index`)

Ordered try-list (first match wins):

1. `try_load_constitution(path)` → existing constitution path (unchanged).
2. `MistralIrBuilder::default().available()` AND `extracted_text.len() >= MIN_TEXT_CHARS (200)` → LLM IR path.
3. `structured_importer::enabled()` → rule-based parser (feature 005).
4. Otherwise → flat hierarchical chunker (legacy).

`MIN_TEXT_CHARS = 200` is the spec edge-case threshold (FR-028). Below it, `extraction_method = 'unprocessable'`; LLM is not invoked.

## Implementations (v1)

| Impl | name() | available() condition |
|---|---|---|
| `MistralIrBuilder` | `"mistral-7b-instruct-v0.3-q4_k_m"` | GGUF exists at `models/mistral-7b-instruct-v0.3.Q4_K_M.gguf` and llama backend initializes |

No cloud impl in v1 (Constitution I).
