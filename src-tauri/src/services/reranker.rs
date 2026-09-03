//! Cross-encoder reranker (bge-reranker-base ONNX).
//!
//! Loaded lazily on first query; kept resident in a Mutex<Option<Reranker>>.
//! Adds ~1.2s per query for 30 (query, chunk) pairs on CPU.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_PAIR_TOKENS: usize = 256;
const REL_THRESHOLD: f32 = -10.0; // very permissive; we use ranking, not gating

pub struct Reranker {
    session: ort::session::Session,
    tokenizer: tokenizers::Tokenizer,
}

pub struct RerankerSlot {
    inner: Mutex<Option<Reranker>>,
    model_path: PathBuf,
    tokenizer_path: PathBuf,
}

impl RerankerSlot {
    pub fn new(model_path: impl Into<PathBuf>, tokenizer_path: impl Into<PathBuf>) -> Self {
        Self {
            inner: Mutex::new(None),
            model_path: model_path.into(),
            tokenizer_path: tokenizer_path.into(),
        }
    }

    /// Score an iterable of (query, candidate) pairs. Higher = more relevant.
    /// Returns scores in input order.
    pub fn score_pairs(&self, query: &str, candidates: &[String]) -> Result<Vec<f32>> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("reranker mutex poisoned"))?;
        if g.is_none() {
            *g = Some(Self::load(&self.model_path, &self.tokenizer_path)?);
        }
        let r = g.as_ref().unwrap();
        r.score(query, candidates)
    }

    fn load(model: &Path, tok: &Path) -> Result<Reranker> {
        let _environment = ort::init().with_name("reranker").commit()?;
        let session = ort::session::Session::builder()?
            .with_optimization_level(
                ort::session::builder::GraphOptimizationLevel::Level3,
            )?
            .commit_from_file(model)
            .with_context(|| format!("failed to load reranker ONNX from {model:?}"))?;
        let tokenizer = tokenizers::Tokenizer::from_file(tok)
            .map_err(|e| anyhow::anyhow!("failed to load tokenizer: {e}"))?;
        Ok(Reranker { session, tokenizer })
    }
}

impl Reranker {
    fn score(&self, query: &str, candidates: &[String]) -> Result<Vec<f32>> {
        if candidates.is_empty() {
            return Ok(vec![]);
        }
        // Build (query, candidate) pair tokens. bge-reranker-base uses XLMRoberta
        // tokenizer which expects `[query] [SEP] [candidate]` — encode_pair handles this.
        let pairs: Vec<(String, String)> = candidates
            .iter()
            .map(|c| (query.to_string(), c.clone()))
            .collect();
        let encodings = self
            .tokenizer
            .encode_batch(pairs, true)
            .map_err(|e| anyhow::anyhow!("tokenize failed: {e}"))?;

        // Pad to longest in batch (capped at MAX_PAIR_TOKENS).
        let max_len = encodings
            .iter()
            .map(|e| e.get_ids().len())
            .max()
            .unwrap_or(0)
            .min(MAX_PAIR_TOKENS);
        let batch = encodings.len();

        let mut input_ids = ndarray::Array2::<i64>::zeros((batch, max_len));
        let mut attention_mask = ndarray::Array2::<i64>::zeros((batch, max_len));
        for (i, enc) in encodings.iter().enumerate() {
            for (j, &id) in enc.get_ids().iter().take(max_len).enumerate() {
                input_ids[[i, j]] = id as i64;
                attention_mask[[i, j]] = 1;
            }
        }

        let outputs = self.session.run(ort::inputs! {
            "input_ids" => input_ids,
            "attention_mask" => attention_mask,
        }?)?;
        // try_extract_tensor returns ArrayViewD<'_, f32> directly (ort 2.0.0-rc.9).
        let logits = outputs[0].try_extract_tensor::<f32>()?;
        // bge-reranker-base outputs shape [batch, 1] — single relevance logit.
        let scores: Vec<f32> = (0..batch).map(|i| logits[[i, 0]]).collect();
        let _ = REL_THRESHOLD; // silence unused; reserved for future gating
        Ok(scores)
    }
}
