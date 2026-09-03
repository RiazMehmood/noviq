//! cpal mic capture → 16 kHz mono i16 frames of 480 samples (30 ms).
//!
//! Resampling: rubato SincFixedIn from device rate to 16 kHz.
//! Channel mixdown: average all input channels to mono before resampling.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc;

use super::segmenter::{FRAME_SAMPLES, SAMPLE_RATE};

pub struct CaptureHandle {
    _stream: Stream, // dropped on stop
}

pub fn start(frame_tx: mpsc::Sender<[i16; FRAME_SAMPLES]>) -> Result<CaptureHandle> {
    let host = cpal::default_host();
    let device = host.default_input_device()
        .ok_or_else(|| anyhow!("no default input device"))?;
    let supported = device.default_input_config().context("default_input_config")?;
    let sample_format = supported.sample_format();
    let device_rate = supported.sample_rate().0 as usize;
    let channels = supported.channels() as usize;
    let config: StreamConfig = supported.into();

    const RUBATO_IN: usize = 1024;
    let params = SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        oversampling_factor: 128,
        interpolation: SincInterpolationType::Linear,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedIn::<f32>::new(
        SAMPLE_RATE as f64 / device_rate as f64,
        2.0,
        params,
        RUBATO_IN,
        1,
    ).context("rubato init")?;

    let (raw_tx, raw_rx) = std_mpsc::channel::<Vec<f32>>();

    let err_fn = |e| log::error!("cpal stream error: {e}");
    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| {
                let mono = mixdown_f32(data, channels);
                let _ = raw_tx.send(mono);
            }, err_fn, None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| {
                let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                let mono = mixdown_f32(&f, channels);
                let _ = raw_tx.send(mono);
            }, err_fn, None,
        ),
        fmt => return Err(anyhow!("unsupported sample format: {fmt:?}")),
    }.context("build_input_stream")?;
    stream.play().context("stream.play")?;

    tokio::task::spawn_blocking(move || {
        let mut input_buf: Vec<f32> = Vec::with_capacity(RUBATO_IN * 4);
        let mut out_buf: Vec<f32> = Vec::with_capacity(SAMPLE_RATE);

        while let Ok(chunk) = raw_rx.recv() {
            input_buf.extend(chunk);
            while input_buf.len() >= RUBATO_IN {
                let in_chunk: Vec<f32> = input_buf.drain(..RUBATO_IN).collect();
                match resampler.process(&[&in_chunk[..]], None) {
                    Ok(out) => out_buf.extend(&out[0]),
                    Err(e) => { log::error!("resample error: {e}"); continue; }
                }
                while out_buf.len() >= FRAME_SAMPLES {
                    let frame_f32: Vec<f32> = out_buf.drain(..FRAME_SAMPLES).collect();
                    let mut frame = [0i16; FRAME_SAMPLES];
                    for (i, &s) in frame_f32.iter().enumerate() {
                        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                        frame[i] = v;
                    }
                    if frame_tx.blocking_send(frame).is_err() { return; }
                }
            }
        }
    });

    Ok(CaptureHandle { _stream: stream })
}

fn mixdown_f32(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 { return interleaved.to_vec(); }
    interleaved.chunks_exact(channels)
        .map(|c| c.iter().sum::<f32>() / channels as f32)
        .collect()
}
