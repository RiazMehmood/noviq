//! whisper-sidecar — minimal NDJSON stdio protocol for Whisper STT.
//!
//! Runs as a long-lived child process. Lives in its own Cargo project so its
//! bundled ggml does not collide with llama-cpp-2's bundled ggml at link time.
//!
//! Protocol (one JSON object per line, both directions):
//!
//!   IN  { "id":"r1", "cmd":"load",       "model_path":"/abs/path/ggml-...bin",
//!                                         "language":"auto", "use_gpu":false }
//!   OUT { "id":"r1", "ok":true,          "model":"medium" }
//!
//!   IN  { "id":"r2", "cmd":"transcribe", "wav_path":"/abs/path/clip.wav" }
//!   OUT { "id":"r2", "ok":true,          "segments":[{"t0":0.0,"t1":1.4,"text":"..."}] }
//!
//!   IN  { "id":"r3", "cmd":"transcribe_b64",
//!                    "format":"wav",
//!                    "audio_b64":"UklGRiQ..." }
//!   OUT { "id":"r3", "ok":true,          "segments":[...] }
//!
//!   IN  { "cmd":"shutdown" }   → process exits 0
//!
//! Errors:
//!   OUT { "id":"...", "ok":false, "error":"message" }
//!
//! Streaming PCM (continuous meeting capture) will be added in a follow-up:
//! the parent sends 16 kHz mono f32 frames via a side pipe; each VAD-bounded
//! segment yields a transcribe response keyed by the parent's segment id.

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Command {
    Load {
        #[serde(default)]
        id: Option<String>,
        model_path: String,
        #[serde(default = "default_lang")]
        language: String,
        #[serde(default)]
        use_gpu: bool,
    },
    Transcribe {
        #[serde(default)]
        id: Option<String>,
        wav_path: String,
        #[serde(default)]
        language: Option<String>,
    },
    TranscribeB64 {
        #[serde(default)]
        id: Option<String>,
        format: String, // "wav" only for now
        audio_b64: String,
        #[serde(default)]
        language: Option<String>,
    },
    Shutdown,
}

fn default_lang() -> String { "auto".to_string() }

#[derive(Debug, Serialize)]
struct Segment {
    t0: f32,
    t1: f32,
    text: String,
}

#[derive(Debug, Serialize)]
struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    segments: Option<Vec<Segment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();

    let mut state: Option<WhisperContext> = None;
    let mut current_lang: String = "auto".to_string();

    let banner = serde_json::json!({ "ok": true, "ready": true, "version": env!("CARGO_PKG_VERSION") });
    let _ = writeln!(stdout, "{banner}");
    let _ = stdout.flush();

    for line in BufReader::new(stdin.lock()).lines() {
        let line = match line { Ok(l) => l, Err(e) => { eprintln!("stdin read error: {e}"); break; } };
        if line.trim().is_empty() { continue; }

        let cmd: Command = match serde_json::from_str(&line) {
            Ok(c) => c,
            Err(e) => { emit_err(&mut stdout, None, format!("bad json: {e}")); continue; }
        };

        match cmd {
            Command::Shutdown => break,

            Command::Load { id, model_path, language, use_gpu } => {
                let mut params = WhisperContextParameters::default();
                params.use_gpu = use_gpu;
                match WhisperContext::new_with_params(&model_path, params) {
                    Ok(ctx) => {
                        state = Some(ctx);
                        current_lang = language;
                        let model = PathBuf::from(&model_path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        emit(&mut stdout, &Response {
                            id, ok: true, model: Some(model),
                            segments: None, error: None,
                        });
                    }
                    Err(e) => emit_err(&mut stdout, id, format!("load failed: {e}")),
                }
            }

            Command::Transcribe { id, wav_path, language } => {
                let lang = language.unwrap_or_else(|| current_lang.clone());
                match state.as_ref() {
                    None => emit_err(&mut stdout, id, "model not loaded".into()),
                    Some(ctx) => match read_wav_mono_f32(&wav_path) {
                        Ok(pcm) => match transcribe_pcm(ctx, &pcm, &lang) {
                            Ok(segments) => emit(&mut stdout, &Response {
                                id, ok: true, model: None,
                                segments: Some(segments), error: None,
                            }),
                            Err(e) => emit_err(&mut stdout, id, format!("transcribe failed: {e}")),
                        },
                        Err(e) => emit_err(&mut stdout, id, format!("wav decode failed: {e}")),
                    },
                }
            }

            Command::TranscribeB64 { id, format, audio_b64, language } => {
                if format != "wav" {
                    emit_err(&mut stdout, id, format!("unsupported format: {format}"));
                    continue;
                }
                let lang = language.unwrap_or_else(|| current_lang.clone());
                let bytes = match B64.decode(audio_b64.as_bytes()) {
                    Ok(b) => b,
                    Err(e) => { emit_err(&mut stdout, id, format!("base64 decode: {e}")); continue; }
                };
                match (state.as_ref(), decode_wav_bytes(&bytes)) {
                    (None, _) => emit_err(&mut stdout, id, "model not loaded".into()),
                    (_, Err(e)) => emit_err(&mut stdout, id, format!("wav decode failed: {e}")),
                    (Some(ctx), Ok(pcm)) => match transcribe_pcm(ctx, &pcm, &lang) {
                        Ok(segments) => emit(&mut stdout, &Response {
                            id, ok: true, model: None,
                            segments: Some(segments), error: None,
                        }),
                        Err(e) => emit_err(&mut stdout, id, format!("transcribe failed: {e}")),
                    },
                }
            }
        }
    }
}

fn emit<W: Write>(w: &mut W, r: &Response) {
    if let Ok(s) = serde_json::to_string(r) {
        let _ = writeln!(w, "{s}");
        let _ = w.flush();
    }
}

fn emit_err<W: Write>(w: &mut W, id: Option<String>, error: String) {
    emit(w, &Response { id, ok: false, model: None, segments: None, error: Some(error) });
}

/// Decode a WAV file from disk into mono 16 kHz f32 samples (whisper input).
fn read_wav_mono_f32(path: &str) -> Result<Vec<f32>> {
    let bytes = std::fs::read(path)?;
    decode_wav_bytes(&bytes)
}

fn decode_wav_bytes(bytes: &[u8]) -> Result<Vec<f32>> {
    let cursor = std::io::Cursor::new(bytes);
    let mut reader = hound::WavReader::new(cursor)?;
    let spec = reader.spec();

    // Mix to mono.
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let max = (1i32 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<_, _>>()?
        }
    };
    let chans = spec.channels as usize;
    if chans == 0 { return Err(anyhow!("zero channels")); }
    let mut mono: Vec<f32> = Vec::with_capacity(interleaved.len() / chans);
    for frame in interleaved.chunks(chans) {
        mono.push(frame.iter().sum::<f32>() / chans as f32);
    }

    // Whisper requires 16 kHz mono. Resample if needed (linear; good enough
    // for speech — Phase 3 will swap to a proper sinc resampler).
    if spec.sample_rate == 16_000 {
        Ok(mono)
    } else {
        Ok(linear_resample(&mono, spec.sample_rate, 16_000))
    }
}

fn linear_resample(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if input.is_empty() { return vec![]; }
    let ratio = to_hz as f64 / from_hz as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let i0 = src_pos.floor() as usize;
        let i1 = (i0 + 1).min(input.len() - 1);
        let frac = (src_pos - i0 as f64) as f32;
        out.push(input[i0] * (1.0 - frac) + input[i1] * frac);
    }
    out
}

fn transcribe_pcm(ctx: &WhisperContext, pcm: &[f32], language: &str) -> Result<Vec<Segment>> {
    let mut state = ctx.create_state()?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    if language != "auto" {
        params.set_language(Some(language));
    }
    state.full(params, pcm)?;

    let n = state.full_n_segments()?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let text = state.full_get_segment_text(i)?;
        let t0 = state.full_get_segment_t0(i)? as f32 / 100.0;
        let t1 = state.full_get_segment_t1(i)? as f32 / 100.0;
        out.push(Segment { t0, t1, text });
    }
    Ok(out)
}
