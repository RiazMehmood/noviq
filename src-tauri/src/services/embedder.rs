//! Embedding service — fastembed-rs wrapper with hardware-adaptive model selection.
//!
//! The model is loaded **lazily** on first use (first index or query call) so
//! app startup stays fast and light. Eager loading at startup consumed several
//! GB of RAM before any indexing had been requested.

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Mutex;

use crate::services::hardware_detector::HardwareTier;

/// GPU embeddings opt-in (admin indexing host only). `ECHO_EMBED_GPU=1` runs the
/// fastembed ONNX model on the CUDA execution provider; default (unset/`0`) = CPU.
fn embed_gpu_enabled() -> bool {
    std::env::var("ECHO_EMBED_GPU").map(|v| v == "1").unwrap_or(false)
}

/// Shared state: lazy-loaded embedding model.
pub struct EmbedderState {
    inner: Mutex<Option<TextEmbedding>>,
    model_name: EmbeddingModel,
    pub dim: u32,
}

impl EmbedderState {
    /// Create the state struct without loading the model yet.
    pub fn new(tier: &HardwareTier) -> Self {
        let (model_name, dim) = if tier.embedding_model == "nomic-embed-text-v1.5" {
            (EmbeddingModel::NomicEmbedTextV15, 768u32)
        } else {
            (EmbeddingModel::AllMiniLML6V2, 384u32)
        };
        log::info!("Embedding model configured: {model_name:?} ({dim}d) — will load on first use");
        EmbedderState { inner: Mutex::new(None), model_name, dim }
    }

    /// Resolve the fastembed cache directory.
    /// Prefers the project-local `.fastembed_cache/` (already populated) so the
    /// app works fully offline after initial download.
    fn cache_dir() -> std::path::PathBuf {
        // Debug builds: use CARGO_MANIFEST_DIR to find project root reliably
        // even when CARGO_TARGET_DIR is outside the project tree.
        #[cfg(debug_assertions)]
        {
            let manifest = env!("CARGO_MANIFEST_DIR");
            if let Some(root) = std::path::Path::new(manifest).parent() {
                let candidate = root.join(".fastembed_cache");
                if candidate.is_dir() {
                    return candidate;
                }
                // Create it so fastembed can download there on first run.
                if std::fs::create_dir_all(&candidate).is_ok() {
                    return candidate;
                }
            }
        }
        // Release / fallback: walk up from exe.
        if let Ok(exe) = std::env::current_exe() {
            let mut dir = exe.parent().map(|p| p.to_path_buf());
            for _ in 0..6 {
                if let Some(d) = &dir {
                    let candidate = d.join(".fastembed_cache");
                    if candidate.is_dir() {
                        return candidate;
                    }
                    dir = d.parent().map(|p| p.to_path_buf());
                } else {
                    break;
                }
            }
        }
        // Windows: use %APPDATA%\EchoAI\.fastembed_cache (writable, survives Program Files)
        #[cfg(target_os = "windows")]
        if let Ok(appdata) = std::env::var("APPDATA") {
            let p = std::path::PathBuf::from(appdata)
                .join("EchoAI")
                .join(".fastembed_cache");
            if std::fs::create_dir_all(&p).is_ok() {
                return p;
            }
        }
        std::path::PathBuf::from(".fastembed_cache")
    }

    /// Ensure the model is loaded, initialising it on first call.
    fn ensure_loaded(&self) -> Result<()> {
        let mut g = self.inner.lock().map_err(|_| anyhow::anyhow!("embedder lock poisoned"))?;
        if g.is_none() {
            let cache = Self::cache_dir();
            log::info!("Loading embedding model: {:?} (cache: {})…", self.model_name, cache.display());
            let mut opts = InitOptions::new(self.model_name.clone())
                .with_cache_dir(cache)
                .with_show_download_progress(true);
            // GPU embeddings (admin indexing only): when ECHO_EMBED_GPU=1, run the
            // ONNX model on the CUDA execution provider. Requires a GPU-enabled
            // onnxruntime via ORT_DYLIB_PATH + the CUDA libs on LD_LIBRARY_PATH
            // (see noviq-server.service). CPU EP is appended as fallback so a
            // missing/incompatible CUDA runtime degrades gracefully rather than
            // failing the index. Desktop builds leave the flag unset → CPU.
            if embed_gpu_enabled() {
                use ort::execution_providers::{
                    ArenaExtendStrategy, CPUExecutionProvider, CUDAExecutionProvider,
                };
                // ⚠️ BOUND THE ARENA. Unbounded, this ate the entire vGPU during a
                // bulk index (2026-08-15 03:16): ONNX Runtime's CUDA arena grows
                // and never shrinks, and with vLLM stopped it had ~11 GB of
                // headroom to grow into. It took all of it, then failed to
                // allocate 256 bytes — every subsequent file died with
                // "embedding inference failed" and the run sat dead for 70
                // minutes. Nothing was wrong with the GPU; nothing capped it.
                //
                // `SameAsRequested` also matters: the default kNextPowerOfTwo
                // doubles each extension, which overshoots badly on the varying
                // batch sizes a mixed corpus produces.
                //
                // The limit is deliberately well under the card so a failure to
                // fit shows up as a clean CPU fallback rather than starving
                // whatever else is resident. Tune with ECHO_EMBED_GPU_MEM_MB.
                let mem_mb: usize = std::env::var("ECHO_EMBED_GPU_MEM_MB")
                    .ok()
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(6144);
                log::info!(
                    "Embedding: CUDA execution provider requested (ECHO_EMBED_GPU=1, \
                     arena limit {mem_mb} MiB)"
                );
                opts = opts.with_execution_providers(vec![
                    CUDAExecutionProvider::default()
                        .with_memory_limit(mem_mb * 1024 * 1024)
                        .with_arena_extend_strategy(ArenaExtendStrategy::SameAsRequested)
                        .build(),
                    CPUExecutionProvider::default().build(),
                ]);
            }
            let te = TextEmbedding::try_new(opts)
                .context("failed to initialise fastembed model")?;
            *g = Some(te);
            log::info!("Embedding model ready (gpu={})", embed_gpu_enabled());
        }
        Ok(())
    }

    /// Embed a batch of texts. Loads the model on first call.
    pub fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        self.ensure_loaded()?;
        let guard = self.inner.lock().map_err(|_| anyhow::anyhow!("embedder lock poisoned"))?;
        let te = guard.as_ref().unwrap();
        // Cap fastembed's internal forward-pass batch to 32 to bound peak ONNX
        // tensor allocation regardless of how many texts are passed.
        let embeddings = te.embed(texts, Some(32)).context("embedding inference failed")?;
        Ok(embeddings)
    }

    /// Embed a single query string.
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let mut results = self.embed(vec![text.to_string()])?;
        results.pop().ok_or_else(|| anyhow::anyhow!("empty embedding result"))
    }
}
