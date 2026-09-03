//! Client for the `whisper-sidecar` subprocess.
//!
//! Why a sidecar? The main `echo-ai` binary already links `llama-cpp-2`, which
//! bundles its own `ggml`. `whisper-rs` also bundles `ggml` with the same
//! C symbol names, so linking both into one binary fails. Running Whisper in
//! a separate process eliminates the symbol collision and isolates audio
//! crashes from the main app.
//!
//! Protocol: NDJSON over stdin/stdout (see `sidecars/whisper-sidecar/src/main.rs`).
//!
//! Status (2026-05-03): scaffolding only. The sidecar binary must be built
//! once via `cd src-tauri/sidecars/whisper-sidecar && cargo build --release`
//! and the resulting binary copied to `src-tauri/binaries/whisper-sidecar`
//! before the Meeting Agent can spawn it.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::{mpsc, oneshot, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub t0: f32,
    pub t1: f32,
    pub text: String,
}

#[derive(Debug, Deserialize)]
struct Response {
    #[serde(default)]
    id: Option<String>,
    ok: bool,
    #[serde(default)]
    segments: Option<Vec<Segment>>,
    #[serde(default)]
    error: Option<String>,
}

type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Response>>>>;

/// Long-lived handle to the running sidecar.
pub struct SttSidecar {
    pending: Pending,
    stdin_tx: mpsc::Sender<String>,
    _child: Arc<Mutex<CommandChild>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl SttSidecar {
    /// Spawn the sidecar and wait for its `ready` banner.
    pub async fn spawn(app: &AppHandle) -> Result<Self> {
        let (mut rx, child) = app
            .shell()
            .sidecar("whisper-sidecar")
            .context("sidecar binary not bundled — build it via `cd sidecars/whisper-sidecar && cargo build --release`")?
            .spawn()
            .context("failed to spawn whisper-sidecar")?;

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(64);
        let child = Arc::new(Mutex::new(child));

        // Drive stdin from the channel.
        let stdin_handle = child.clone();
        tokio::spawn(async move {
            while let Some(line) = stdin_rx.recv().await {
                let mut guard = stdin_handle.lock().await;
                if guard.write(line.as_bytes()).is_err() { break; }
            }
        });

        // Drive stdout: route each NDJSON response back to the matching pending request.
        let pending_rx = pending.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let CommandEvent::Stdout(buf) = event {
                    let line = String::from_utf8_lossy(&buf).trim().to_string();
                    if line.is_empty() { continue; }
                    let resp: Response = match serde_json::from_str(&line) {
                        Ok(r) => r,
                        Err(e) => { log::warn!("stt-sidecar bad json: {e} | line={line}"); continue; }
                    };
                    if let Some(id) = resp.id.as_ref() {
                        if let Some(tx) = pending_rx.lock().await.remove(id) {
                            let _ = tx.send(resp);
                        }
                    } else {
                        log::info!("stt-sidecar banner: {line}");
                    }
                }
            }
        });

        Ok(SttSidecar {
            pending,
            stdin_tx,
            _child: child,
            next_id: std::sync::atomic::AtomicU64::new(0),
        })
        // NOTE: SttSidecar is intentionally not registered in lib.rs yet —
        // the sidecar binary doesn't exist on disk until Phase 1 step 2
        // (build whisper-sidecar and copy to src-tauri/binaries/).
    }

    async fn request(&self, mut payload: serde_json::Value) -> Result<Response> {
        let id = format!("r{}", self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        payload["id"] = serde_json::Value::String(id.clone());
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let mut line = serde_json::to_string(&payload)?;
        line.push('\n');
        self.stdin_tx.send(line).await.map_err(|e| anyhow!("sidecar stdin closed: {e}"))?;
        let resp = rx.await.map_err(|e| anyhow!("sidecar dropped response: {e}"))?;
        if !resp.ok {
            return Err(anyhow!(resp.error.unwrap_or_else(|| "unknown sidecar error".into())));
        }
        Ok(resp)
    }

    pub async fn load_model(&self, model_path: &str, language: &str, use_gpu: bool) -> Result<()> {
        self.request(serde_json::json!({
            "cmd": "load",
            "model_path": model_path,
            "language": language,
            "use_gpu": use_gpu,
        })).await?;
        Ok(())
    }

    pub async fn transcribe_wav(&self, wav_path: &str, language: Option<&str>) -> Result<Vec<Segment>> {
        let resp = self.request(serde_json::json!({
            "cmd": "transcribe",
            "wav_path": wav_path,
            "language": language,
        })).await?;
        Ok(resp.segments.unwrap_or_default())
    }

    /// Transcribe a 16 kHz mono f32 sample buffer by encoding it as a WAV
    /// payload and shipping it base64 to the sidecar.
    pub async fn transcribe_samples(&self, samples: &[f32], language: Option<&str>) -> Result<Vec<Segment>> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        use std::io::Cursor;

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut buf: Vec<u8> = Vec::with_capacity(samples.len() * 4 + 64);
        {
            let cursor = Cursor::new(&mut buf);
            let mut writer = hound::WavWriter::new(cursor, spec)
                .context("hound writer")?;
            for &s in samples {
                writer.write_sample(s).context("hound write_sample")?;
            }
            writer.finalize().context("hound finalize")?;
        }
        let audio_b64 = STANDARD.encode(&buf);

        let resp = self.request(serde_json::json!({
            "cmd": "transcribe_b64",
            "format": "wav",
            "audio_b64": audio_b64,
            "language": language,
        })).await?;
        Ok(resp.segments.unwrap_or_default())
    }
}
