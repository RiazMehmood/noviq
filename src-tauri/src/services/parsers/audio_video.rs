//! Audio/video transcription via whisper-rs (feature = "stt").
//!
//! When compiled without the "stt" feature, returns an informational error.

use std::path::Path;
use super::{ParseError, ParsedDocument};

const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GB hard limit

/// Transcribe an audio or video file.
///
/// `model_path`: path to the Whisper GGML model file.
pub fn transcribe(path: &Path, model_path: &str) -> Result<ParsedDocument, ParseError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(ParseError::FileTooLarge {
            size_bytes: metadata.len(),
            limit_bytes: MAX_FILE_SIZE,
        });
    }

    #[cfg(feature = "stt")]
    {
        transcribe_impl(path, model_path)
    }

    #[cfg(not(feature = "stt"))]
    {
        let _ = model_path;
        Err(ParseError::CapabilityNotAvailable {
            message: "Audio transcription requires the 'stt' feature and a Whisper model file. \
                      Rebuild with --features stt and place ggml-base.bin in models/.".to_string(),
        })
    }
}

#[cfg(feature = "stt")]
fn transcribe_impl(path: &Path, model_path: &str) -> Result<ParsedDocument, ParseError> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    let samples = load_audio_as_f32(path)?;

    let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
        .map_err(|e| ParseError::Other(format!("whisper context: {e}")))?;
    let mut state = ctx.create_state()
        .map_err(|e| ParseError::Other(format!("whisper state: {e}")))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("auto"));
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    state.full(params, &samples)
        .map_err(|e| ParseError::Other(format!("whisper full: {e}")))?;

    let n = state.full_n_segments()
        .map_err(|e| ParseError::Other(format!("segment count: {e}")))?;
    let mut transcript = String::new();
    for i in 0..n {
        let text = state.full_get_segment_text(i)
            .map_err(|e| ParseError::Other(format!("segment {i}: {e}")))?;
        transcript.push_str(text.trim());
        transcript.push(' ');
    }

    Ok(ParsedDocument {
        text: transcript.trim().to_string(),
        page_offsets: vec![],
        extraction_method: "native",
        quality_score: 1.0,
        reconstructed_rows: Vec::new(),
        reconstruction_method: "",
        tables: Vec::new(),
        provision_titles: Default::default(),
    })
}

#[cfg(feature = "stt")]
fn load_audio_as_f32(path: &Path) -> Result<Vec<f32>, ParseError> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "wav" => load_wav(path),
        _ => Err(ParseError::Other(format!(
            "Audio format '{ext}' requires FFmpeg. Convert to WAV first."
        ))),
    }
}

#[cfg(feature = "stt")]
fn load_wav(path: &Path) -> Result<Vec<f32>, ParseError> {
    let data = std::fs::read(path)?;
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(ParseError::Other("Not a valid WAV file".to_string()));
    }
    let mut offset = 12usize;
    let (mut data_start, mut data_len) = (0usize, 0usize);
    let (mut channels, mut sample_rate, mut bits) = (1u16, 16000u32, 16u16);
    while offset + 8 <= data.len() {
        let chunk_id = &data[offset..offset + 4];
        let chunk_size = u32::from_le_bytes(data[offset+4..offset+8].try_into().unwrap()) as usize;
        if chunk_id == b"fmt " && offset + 24 <= data.len() {
            channels    = u16::from_le_bytes(data[offset+10..offset+12].try_into().unwrap());
            sample_rate = u32::from_le_bytes(data[offset+12..offset+16].try_into().unwrap());
            bits        = u16::from_le_bytes(data[offset+22..offset+24].try_into().unwrap());
        } else if chunk_id == b"data" {
            data_start = offset + 8;
            data_len = chunk_size;
            break;
        }
        offset += 8 + chunk_size;
    }
    if data_start == 0 { return Err(ParseError::Other("WAV data chunk not found".to_string())); }
    let raw = &data[data_start..(data_start + data_len).min(data.len())];
    let samples_i16: Vec<i16> = raw.chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    let mono: Vec<f32> = if channels == 2 {
        samples_i16.chunks(2)
            .map(|p| (p[0] as f32 + p[1] as f32) / 2.0 / i16::MAX as f32)
            .collect()
    } else {
        samples_i16.iter().map(|&s| s as f32 / i16::MAX as f32).collect()
    };
    if sample_rate == 16000 { return Ok(mono); }
    let ratio = sample_rate as f64 / 16000.0;
    let target_len = (mono.len() as f64 / ratio) as usize;
    Ok((0..target_len).map(|i| mono.get((i as f64 * ratio) as usize).copied().unwrap_or(0.0)).collect())
}
