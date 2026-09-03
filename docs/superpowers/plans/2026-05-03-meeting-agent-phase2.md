# Meeting Agent Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** End-to-end meeting capture — Start Meeting captures mic audio, segments on natural pauses (VAD), transcribes via the existing whisper sidecar, and persists segments to SQLite. Stop Meeting finalizes the row.

**Architecture:** cpal mic → rubato resample to 16 kHz mono f32 → mpsc to a webrtc-vad segmenter → bounded mpsc to a transcribe worker that calls `SttSidecar::transcribe_b64` → `MeetingsRepo::append_segment`. Single `MeetingService` orchestrator owns lifecycle. One meeting at a time in Phase 2.

**Tech Stack:** Rust 1.87, Tauri 2, sqlx (SQLite), cpal 0.15, webrtc-vad 0.4, rubato 0.15, hound 3 (WAV encoding for sidecar payload), base64 0.22, tokio.

**Spec:** `docs/superpowers/specs/2026-05-03-meeting-agent-phase2-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src-tauri/src/db/migrations/012_meetings.sql` | Create | DDL: meetings + meeting_segments |
| `src-tauri/src/db/repositories/meetings.rs` | Create | CRUD for meetings + segments |
| `src-tauri/src/db/repositories/mod.rs` | Modify | `pub mod meetings;` |
| `src-tauri/Cargo.toml` | Modify | Add cpal, webrtc-vad, rubato, hound, base64 |
| `src-tauri/src/services/stt_sidecar.rs` | Modify | Add `transcribe_b64(samples)` client method |
| `src-tauri/src/services/meeting/mod.rs` | Create | `MeetingService` orchestrator + start/stop |
| `src-tauri/src/services/meeting/audio_capture.rs` | Create | cpal stream + rubato resampler |
| `src-tauri/src/services/meeting/segmenter.rs` | Create | webrtc-vad state machine |
| `src-tauri/src/services/meeting/transcribe_worker.rs` | Create | Pull → sidecar → repo |
| `src-tauri/src/services/mod.rs` | Modify | `pub mod meeting;` |
| `src-tauri/src/commands/meetings.rs` | Create | Tauri commands |
| `src-tauri/src/commands/mod.rs` | Modify | `pub mod meetings;` |
| `src-tauri/src/lib.rs` | Modify | Register MeetingService + commands |
| `CHANGELOG_DEV.md` | Modify | Append entry |

---

## Task 1: Database migration `012_meetings.sql`

**Files:**
- Create: `src-tauri/src/db/migrations/012_meetings.sql`

- [ ] **Step 1: Write the migration**

```sql
-- 012_meetings.sql — Meeting Agent Phase 2 (additive)

CREATE TABLE meetings (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    title        TEXT    NOT NULL,
    started_at   TEXT    NOT NULL,
    ended_at     TEXT,
    language     TEXT    NOT NULL,
    model_path   TEXT    NOT NULL,
    status       TEXT    NOT NULL DEFAULT 'active'
                 CHECK (status IN ('active','stopped','error')),
    error        TEXT
);

CREATE TABLE meeting_segments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id   INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    t0_ms        INTEGER NOT NULL,
    t1_ms        INTEGER NOT NULL,
    text         TEXT    NOT NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_meeting_segments_meeting_id
    ON meeting_segments(meeting_id, t0_ms);
```

- [ ] **Step 2: Verify migration applies on a fresh DB**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
mv ../echo.db ../echo.db.bak 2>/dev/null || true
cargo check
sqlite3 ../echo.db ".schema meetings" ".schema meeting_segments"
mv ../echo.db.bak ../echo.db 2>/dev/null || true
```
Expected: both `CREATE TABLE` statements echoed back. If you don't have a way to instantiate the pool from `cargo check`, skip the sqlite3 check and rely on Task 2's repo tests (which run the migration in-memory).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/migrations/012_meetings.sql
git commit -m "feat(meetings): migration 012 — meetings + meeting_segments tables"
```

---

## Task 2: `MeetingsRepo` with round-trip tests

**Files:**
- Create: `src-tauri/src/db/repositories/meetings.rs`
- Modify: `src-tauri/src/db/repositories/mod.rs`

- [ ] **Step 1: Register module**

Add to `src-tauri/src/db/repositories/mod.rs` (alphabetical with siblings):

```rust
pub mod meetings;
```

- [ ] **Step 2: Write the failing test**

Create `src-tauri/src/db/repositories/meetings.rs`:

```rust
//! Repository: `meetings` and `meeting_segments`.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Meeting {
    pub id: i64,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub language: String,
    pub model_path: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MeetingSegment {
    pub id: i64,
    pub meeting_id: i64,
    pub t0_ms: i64,
    pub t1_ms: i64,
    pub text: String,
    pub created_at: String,
}

pub async fn start(
    pool: &SqlitePool,
    title: &str,
    language: &str,
    model_path: &str,
) -> Result<i64> {
    let started_at = Utc::now().to_rfc3339();
    let row = sqlx::query(
        "INSERT INTO meetings (title, started_at, language, model_path, status)
         VALUES (?, ?, ?, ?, 'active') RETURNING id",
    )
    .bind(title).bind(&started_at).bind(language).bind(model_path)
    .fetch_one(pool).await?;
    use sqlx::Row;
    Ok(row.get::<i64, _>("id"))
}

pub async fn stop(pool: &SqlitePool, id: i64) -> Result<()> {
    let ended_at = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE meetings SET ended_at = ?, status = 'stopped'
         WHERE id = ? AND status = 'active'",
    )
    .bind(&ended_at).bind(id)
    .execute(pool).await?;
    Ok(())
}

pub async fn mark_error(pool: &SqlitePool, id: i64, err: &str) -> Result<()> {
    let ended_at = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE meetings SET ended_at = ?, status = 'error', error = ? WHERE id = ?",
    )
    .bind(&ended_at).bind(err).bind(id)
    .execute(pool).await?;
    Ok(())
}

pub async fn append_segment(
    pool: &SqlitePool,
    meeting_id: i64,
    t0_ms: i64,
    t1_ms: i64,
    text: &str,
) -> Result<i64> {
    let row = sqlx::query(
        "INSERT INTO meeting_segments (meeting_id, t0_ms, t1_ms, text)
         VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(meeting_id).bind(t0_ms).bind(t1_ms).bind(text)
    .fetch_one(pool).await?;
    use sqlx::Row;
    Ok(row.get::<i64, _>("id"))
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<Meeting>> {
    Ok(sqlx::query_as::<_, Meeting>("SELECT * FROM meetings WHERE id = ?")
        .bind(id).fetch_optional(pool).await?)
}

pub async fn list_segments(pool: &SqlitePool, meeting_id: i64) -> Result<Vec<MeetingSegment>> {
    Ok(sqlx::query_as::<_, MeetingSegment>(
        "SELECT * FROM meeting_segments WHERE meeting_id = ? ORDER BY t0_ms",
    )
    .bind(meeting_id).fetch_all(pool).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./src/db/migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn start_append_stop_roundtrip() {
        let p = pool().await;
        let id = start(&p, "Test", "en", "/abs/model.bin").await.unwrap();
        assert!(id > 0);

        let s1 = append_segment(&p, id, 0, 1500, "hello world").await.unwrap();
        let s2 = append_segment(&p, id, 1500, 3200, "second segment").await.unwrap();
        assert!(s2 > s1);

        let segs = list_segments(&p, id).await.unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "hello world");
        assert_eq!(segs[1].t0_ms, 1500);

        stop(&p, id).await.unwrap();
        let m = get(&p, id).await.unwrap().unwrap();
        assert_eq!(m.status, "stopped");
        assert!(m.ended_at.is_some());
    }

    #[tokio::test]
    async fn mark_error_sets_status_and_text() {
        let p = pool().await;
        let id = start(&p, "T", "en", "/abs/m").await.unwrap();
        mark_error(&p, id, "boom").await.unwrap();
        let m = get(&p, id).await.unwrap().unwrap();
        assert_eq!(m.status, "error");
        assert_eq!(m.error.as_deref(), Some("boom"));
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
cargo test --lib db::repositories::meetings
```
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/db/repositories/meetings.rs src-tauri/src/db/repositories/mod.rs
git commit -m "feat(meetings): MeetingsRepo (start/stop/append_segment + tests)"
```

---

## Task 3: Add audio dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add deps**

Append under `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
cpal = "0.15"
webrtc-vad = "0.4"
rubato = "0.15"
hound = "3.5"
base64 = "0.22"
```

(If `base64` is already present, skip that line.)

- [ ] **Step 2: Verify build**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
cargo check
```
Expected: clean compile. If `libasound2-dev` is missing, install it:
```bash
sudo apt-get install -y libasound2-dev
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "deps(meetings): add cpal, webrtc-vad, rubato, hound, base64"
```

---

## Task 4: Add `transcribe_b64` client method on `SttSidecar`

**Files:**
- Modify: `src-tauri/src/services/stt_sidecar.rs`

- [ ] **Step 1: Add the method**

Append inside `impl SttSidecar { ... }` in `src-tauri/src/services/stt_sidecar.rs`, after `transcribe_wav`:

```rust
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
```

- [ ] **Step 2: Verify build**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
cargo check
```
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/services/stt_sidecar.rs
git commit -m "feat(stt): SttSidecar::transcribe_samples (WAV+b64 over NDJSON)"
```

---

## Task 5: `Segmenter` (pure logic, fully unit-testable)

**Files:**
- Create: `src-tauri/src/services/meeting/segmenter.rs`
- Modify: `src-tauri/src/services/mod.rs` (add `pub mod meeting;`)
- Create: `src-tauri/src/services/meeting/mod.rs` (empty stub for now: `pub mod segmenter;`)

- [ ] **Step 1: Add module declarations**

In `src-tauri/src/services/mod.rs`, add (alphabetical):

```rust
pub mod meeting;
```

Create `src-tauri/src/services/meeting/mod.rs`:

```rust
pub mod segmenter;
```

- [ ] **Step 2: Write the segmenter with tests**

Create `src-tauri/src/services/meeting/segmenter.rs`:

```rust
//! VAD-gated segmenter. Pure logic — no I/O.
//!
//! Input: 16 kHz mono i16 PCM frames, exactly 480 samples (30 ms each).
//! (i16 is required by webrtc-vad; the audio_capture stage converts f32 -> i16.)
//!
//! Output: `Segment { t0_ms, t1_ms, samples_f32 }` whenever speech ends
//! (>= 500 ms of trailing silence) or the 30 s hard cap is hit.

use webrtc_vad::{SampleRate, Vad, VadMode};

pub const SAMPLE_RATE: usize = 16_000;
pub const FRAME_MS: usize = 30;
pub const FRAME_SAMPLES: usize = SAMPLE_RATE * FRAME_MS / 1000; // 480

const TAIL_SILENCE_MS: usize = 500;
const HARD_CAP_MS: usize = 30_000;
const MIN_SEG_MS: usize = 300;
const PREROLL_MS: usize = 200;

const TAIL_SILENCE_FRAMES: usize = TAIL_SILENCE_MS / FRAME_MS; // 16
const HARD_CAP_FRAMES: usize = HARD_CAP_MS / FRAME_MS;         // 1000
const MIN_SEG_FRAMES: usize = MIN_SEG_MS / FRAME_MS;           // 10
const PREROLL_FRAMES: usize = PREROLL_MS / FRAME_MS;           // 6 (~210 ms)

#[derive(Debug, Clone)]
pub struct Segment {
    pub t0_ms: i64,
    pub t1_ms: i64,
    pub samples: Vec<f32>,
}

pub struct Segmenter {
    vad: Vad,
    voiced: bool,
    seg_frames: Vec<[i16; FRAME_SAMPLES]>,
    silence_run: usize,
    preroll: std::collections::VecDeque<[i16; FRAME_SAMPLES]>,
    frame_counter: usize, // total frames seen since session start
    seg_start_frame: usize,
}

impl Segmenter {
    pub fn new() -> Self {
        let mut vad = Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::Aggressive);
        // Aggressive == level 3; we want level 2. Override:
        vad.set_mode(VadMode::Quality); // Quality == level 0; reset to LowBitrate (lvl 2)
        vad.set_mode(VadMode::LowBitrate);
        Self {
            vad,
            voiced: false,
            seg_frames: Vec::with_capacity(HARD_CAP_FRAMES),
            silence_run: 0,
            preroll: std::collections::VecDeque::with_capacity(PREROLL_FRAMES + 1),
            frame_counter: 0,
            seg_start_frame: 0,
        }
    }

    /// Push one 30 ms frame (480 i16 samples). Returns Some(Segment) on flush.
    pub fn push_frame(&mut self, frame: [i16; FRAME_SAMPLES]) -> Option<Segment> {
        let is_voiced = self.vad.is_voice_segment(&frame).unwrap_or(false);
        let mut out = None;

        if is_voiced {
            if !self.voiced {
                // Transition silence -> voiced: drain preroll into the segment.
                self.voiced = true;
                self.seg_start_frame = self.frame_counter.saturating_sub(self.preroll.len());
                self.seg_frames.extend(self.preroll.drain(..));
            }
            self.silence_run = 0;
            self.seg_frames.push(frame);

            // Hard cap.
            if self.seg_frames.len() >= HARD_CAP_FRAMES {
                out = self.flush();
            }
        } else if self.voiced {
            // Trailing silence — keep counting and append (helps whisper hear the end).
            self.silence_run += 1;
            self.seg_frames.push(frame);
            if self.silence_run >= TAIL_SILENCE_FRAMES {
                out = self.flush();
            }
        } else {
            // Pure silence: maintain rolling preroll buffer.
            if self.preroll.len() == PREROLL_FRAMES {
                self.preroll.pop_front();
            }
            self.preroll.push_back(frame);
        }

        self.frame_counter += 1;
        out
    }

    fn flush(&mut self) -> Option<Segment> {
        let frames = std::mem::take(&mut self.seg_frames);
        let start = self.seg_start_frame;
        self.silence_run = 0;
        self.voiced = false;
        self.preroll.clear();
        if frames.len() < MIN_SEG_FRAMES {
            return None;
        }
        let samples: Vec<f32> = frames.iter()
            .flat_map(|f| f.iter().map(|&s| s as f32 / 32768.0))
            .collect();
        let t0_ms = (start * FRAME_MS) as i64;
        let t1_ms = ((start + frames.len()) * FRAME_MS) as i64;
        Some(Segment { t0_ms, t1_ms, samples })
    }

    /// End-of-stream flush (Stop Meeting).
    pub fn finalize(&mut self) -> Option<Segment> {
        if !self.voiced || self.seg_frames.is_empty() { return None; }
        self.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silence_frame() -> [i16; FRAME_SAMPLES] { [0; FRAME_SAMPLES] }
    fn voiced_frame() -> [i16; FRAME_SAMPLES] {
        // 440 Hz square wave at high amplitude — VAD will treat as voice.
        let mut f = [0i16; FRAME_SAMPLES];
        for (i, s) in f.iter_mut().enumerate() {
            *s = if (i / 18) % 2 == 0 { 16_000 } else { -16_000 };
        }
        f
    }

    #[test]
    fn pure_silence_yields_nothing() {
        let mut seg = Segmenter::new();
        for _ in 0..50 {
            assert!(seg.push_frame(silence_frame()).is_none());
        }
        assert!(seg.finalize().is_none());
    }

    #[test]
    fn voiced_then_500ms_silence_flushes() {
        let mut seg = Segmenter::new();
        // 20 voiced frames = 600 ms speech
        for _ in 0..20 { let _ = seg.push_frame(voiced_frame()); }
        let mut out = None;
        for _ in 0..TAIL_SILENCE_FRAMES {
            if let Some(s) = seg.push_frame(silence_frame()) { out = Some(s); }
        }
        let s = out.expect("segment should flush after 500ms silence");
        assert!(s.t1_ms - s.t0_ms >= 600);
    }

    #[test]
    fn segments_below_min_length_are_dropped() {
        let mut seg = Segmenter::new();
        // 5 voiced frames = 150 ms — below MIN_SEG_MS (300)
        for _ in 0..5 { let _ = seg.push_frame(voiced_frame()); }
        let mut out = None;
        for _ in 0..TAIL_SILENCE_FRAMES {
            if let Some(s) = seg.push_frame(silence_frame()) { out = Some(s); }
        }
        assert!(out.is_none(), "short blip must be dropped");
    }

    #[test]
    fn hard_cap_at_30s() {
        let mut seg = Segmenter::new();
        let mut flushed = false;
        for _ in 0..(HARD_CAP_FRAMES + 5) {
            if seg.push_frame(voiced_frame()).is_some() { flushed = true; break; }
        }
        assert!(flushed, "must flush at hard cap");
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
cargo test --lib services::meeting::segmenter
```
Expected: 4 tests pass. (If a voiced-detection test is flaky on your VAD build, raise sample amplitude; do NOT loosen `MIN_SEG_MS`.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/mod.rs src-tauri/src/services/meeting/
git commit -m "feat(meetings): VAD segmenter (preroll + tail-silence flush + hard cap + tests)"
```

---

## Task 6: `AudioCapture` (cpal + rubato resample)

**Files:**
- Create: `src-tauri/src/services/meeting/audio_capture.rs`
- Modify: `src-tauri/src/services/meeting/mod.rs` (add `pub mod audio_capture;`)

- [ ] **Step 1: Register module**

In `src-tauri/src/services/meeting/mod.rs`:

```rust
pub mod audio_capture;
pub mod segmenter;
```

- [ ] **Step 2: Implement capture**

Create `src-tauri/src/services/meeting/audio_capture.rs`:

```rust
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

/// Spawn a default-input cpal stream and forward 30 ms i16 frames over `frame_tx`.
pub fn start(frame_tx: mpsc::Sender<[i16; FRAME_SAMPLES]>) -> Result<CaptureHandle> {
    let host = cpal::default_host();
    let device = host.default_input_device()
        .ok_or_else(|| anyhow!("no default input device"))?;
    let supported = device.default_input_config().context("default_input_config")?;
    let sample_format = supported.sample_format();
    let device_rate = supported.sample_rate().0 as usize;
    let channels = supported.channels() as usize;
    let config: StreamConfig = supported.into();

    // rubato wants chunks; we feed chunks of `RUBATO_IN` mono f32 samples
    // and produce ~RUBATO_IN * 16000/device_rate output samples.
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

    // Bridge: cpal callback (sync) -> std::mpsc -> tokio task that resamples + frames.
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
        SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _| {
                let f: Vec<f32> = data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0).collect();
                let mono = mixdown_f32(&f, channels);
                let _ = raw_tx.send(mono);
            }, err_fn, None,
        ),
        fmt => return Err(anyhow!("unsupported sample format: {fmt:?}")),
    }.context("build_input_stream")?;
    stream.play().context("stream.play")?;

    // Resampler + framer task.
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
```

- [ ] **Step 3: Verify build**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
cargo check
```
Expected: clean. Capture is not unit-tested (requires a real device); covered by manual smoke test in Task 9.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/meeting/audio_capture.rs src-tauri/src/services/meeting/mod.rs
git commit -m "feat(meetings): cpal capture + rubato resample to 16kHz mono frames"
```

---

## Task 7: `TranscribeWorker`

**Files:**
- Create: `src-tauri/src/services/meeting/transcribe_worker.rs`
- Modify: `src-tauri/src/services/meeting/mod.rs` (add `pub mod transcribe_worker;`)

- [ ] **Step 1: Register module**

Update `src-tauri/src/services/meeting/mod.rs`:

```rust
pub mod audio_capture;
pub mod segmenter;
pub mod transcribe_worker;
```

- [ ] **Step 2: Implement worker**

Create `src-tauri/src/services/meeting/transcribe_worker.rs`:

```rust
//! Pull `Segment`s, transcribe via the sidecar, persist to MeetingsRepo.

use std::sync::Arc;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::db::repositories::meetings as meetings_repo;
use crate::services::stt_sidecar::SttSidecar;
use super::segmenter::Segment;

pub async fn run(
    mut rx: mpsc::Receiver<Segment>,
    sidecar: Arc<SttSidecar>,
    pool: SqlitePool,
    meeting_id: i64,
    language: String,
) {
    while let Some(seg) = rx.recv().await {
        let lang = if language == "auto" { None } else { Some(language.as_str()) };
        match sidecar.transcribe_samples(&seg.samples, lang).await {
            Ok(parts) => {
                let text = parts.iter().map(|p| p.text.trim()).collect::<Vec<_>>().join(" ");
                let text = text.trim();
                if text.is_empty() { continue; }
                if let Err(e) = meetings_repo::append_segment(
                    &pool, meeting_id, seg.t0_ms, seg.t1_ms, text,
                ).await {
                    log::error!("meetings: append_segment failed: {e}");
                }
            }
            Err(e) => log::warn!("meetings: transcribe failed (skipping segment): {e}"),
        }
    }
}
```

- [ ] **Step 3: Verify build**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
cargo check
```
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/meeting/transcribe_worker.rs src-tauri/src/services/meeting/mod.rs
git commit -m "feat(meetings): transcribe worker (sidecar -> MeetingsRepo)"
```

---

## Task 8: `MeetingService` orchestrator + Tauri commands

**Files:**
- Create: `src-tauri/src/services/meeting/service.rs`
- Modify: `src-tauri/src/services/meeting/mod.rs` (add `pub mod service;` + re-export)
- Create: `src-tauri/src/commands/meetings.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Update meeting module**

`src-tauri/src/services/meeting/mod.rs`:

```rust
pub mod audio_capture;
pub mod segmenter;
pub mod service;
pub mod transcribe_worker;

pub use service::MeetingService;
```

- [ ] **Step 2: Implement orchestrator**

Create `src-tauri/src/services/meeting/service.rs`:

```rust
//! Orchestrates audio capture → segmenter → transcribe worker for ONE
//! active meeting at a time. Phase 2 enforces single-meeting concurrency.

use anyhow::{anyhow, Result};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::db::repositories::meetings as meetings_repo;
use crate::services::stt_sidecar::SttSidecar;
use super::audio_capture::{self, CaptureHandle};
use super::segmenter::{Segment, Segmenter, FRAME_SAMPLES};
use super::transcribe_worker;

pub struct MeetingService {
    pool: SqlitePool,
    sidecar: Arc<SttSidecar>,
    active: Mutex<Option<ActiveMeeting>>,
}

struct ActiveMeeting {
    id: i64,
    capture: CaptureHandle,
    seg_task: JoinHandle<()>,
    worker_task: JoinHandle<()>,
}

impl MeetingService {
    pub fn new(pool: SqlitePool, sidecar: Arc<SttSidecar>) -> Self {
        Self { pool, sidecar, active: Mutex::new(None) }
    }

    pub async fn start(&self, title: String, language: String, model_path: String) -> Result<i64> {
        let mut guard = self.active.lock().await;
        if guard.is_some() {
            return Err(anyhow!("a meeting is already active; stop it first"));
        }

        // 1. Insert DB row first so an early failure still leaves a record.
        let id = meetings_repo::start(&self.pool, &title, &language, &model_path).await?;

        // 2. Load whisper model in sidecar.
        if let Err(e) = self.sidecar.load_model(&model_path, &language, false).await {
            let _ = meetings_repo::mark_error(&self.pool, id, &format!("load_model: {e}")).await;
            return Err(e);
        }

        // 3. Build pipeline.
        let (frame_tx, mut frame_rx) = mpsc::channel::<[i16; FRAME_SAMPLES]>(32);
        let (seg_tx, seg_rx) = mpsc::channel::<Segment>(8);

        let capture = match audio_capture::start(frame_tx) {
            Ok(c) => c,
            Err(e) => {
                let _ = meetings_repo::mark_error(&self.pool, id, &format!("audio: {e}")).await;
                return Err(e);
            }
        };

        let seg_task = tokio::spawn(async move {
            let mut segmenter = Segmenter::new();
            while let Some(frame) = frame_rx.recv().await {
                if let Some(s) = segmenter.push_frame(frame) {
                    if seg_tx.try_send(s).is_err() {
                        log::warn!("meetings: backpressure — dropping segment");
                    }
                }
            }
            if let Some(s) = segmenter.finalize() {
                let _ = seg_tx.send(s).await;
            }
        });

        let pool = self.pool.clone();
        let sidecar = self.sidecar.clone();
        let lang_for_worker = language.clone();
        let worker_task = tokio::spawn(async move {
            transcribe_worker::run(seg_rx, sidecar, pool, id, lang_for_worker).await;
        });

        *guard = Some(ActiveMeeting { id, capture, seg_task, worker_task });
        Ok(id)
    }

    pub async fn stop(&self, id: i64) -> Result<()> {
        let active = {
            let mut g = self.active.lock().await;
            g.take()
        };
        let Some(active) = active else {
            return Err(anyhow!("no active meeting"));
        };
        if active.id != id {
            return Err(anyhow!("active meeting id {} != requested {}", active.id, id));
        }
        // Drop the cpal stream → frame channel closes → segmenter task exits → worker exits.
        drop(active.capture);
        // Bounded wait so a hung sidecar can't block stop forever.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), active.seg_task).await;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), active.worker_task).await;
        meetings_repo::stop(&self.pool, id).await?;
        Ok(())
    }
}
```

- [ ] **Step 3: Wire commands**

Create `src-tauri/src/commands/meetings.rs`:

```rust
use std::sync::Arc;
use tauri::State;
use crate::services::meeting::MeetingService;

#[tauri::command]
pub async fn start_meeting(
    service: State<'_, Arc<MeetingService>>,
    title: String,
    language: String,
    model_path: String,
) -> Result<i64, String> {
    service.start(title, language, model_path).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_meeting(
    service: State<'_, Arc<MeetingService>>,
    id: i64,
) -> Result<(), String> {
    service.stop(id).await.map_err(|e| e.to_string())
}
```

In `src-tauri/src/commands/mod.rs` add:

```rust
pub mod meetings;
```

- [ ] **Step 4: Register in `lib.rs`**

In `src-tauri/src/lib.rs`:

1. Add to imports near other service uses:

```rust
use crate::services::meeting::MeetingService;
use crate::services::stt_sidecar::SttSidecar;
```

2. After the existing `app.manage(...)` calls (look for the block near line 530), spawn the sidecar and the service. Insert:

```rust
        // Meeting Agent (Phase 2): lazy-spawn sidecar + service.
        let app_handle = app.handle().clone();
        let pool_for_meet = pool.clone();
        tauri::async_runtime::spawn(async move {
            match SttSidecar::spawn(&app_handle).await {
                Ok(sidecar) => {
                    let svc = Arc::new(MeetingService::new(pool_for_meet, Arc::new(sidecar)));
                    app_handle.manage(svc);
                }
                Err(e) => log::warn!("stt-sidecar not available — meetings disabled: {e}"),
            }
        });
```

3. Add to the `generate_handler![ ... ]` list:

```rust
            commands::meetings::start_meeting,
            commands::meetings::stop_meeting,
```

- [ ] **Step 5: Verify build**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
cargo check
```
Expected: clean. If `app.handle()` returns a value that needs `.clone()` or `.app_handle().clone()`, follow the existing pattern in the same file.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/meeting/service.rs \
        src-tauri/src/services/meeting/mod.rs \
        src-tauri/src/commands/meetings.rs \
        src-tauri/src/commands/mod.rs \
        src-tauri/src/lib.rs
git commit -m "feat(meetings): MeetingService orchestrator + start/stop commands"
```

---

## Task 9: Smoke test, PHR, CHANGELOG

**Files:**
- Modify: `CHANGELOG_DEV.md`
- Create: `history/prompts/meeting-agent/<ID>-meeting-agent-phase2.green.prompt.md`

- [ ] **Step 1: Final cargo check**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
cargo check
cargo test --lib db::repositories::meetings services::meeting::segmenter
```
Expected: clean compile, all unit tests pass.

- [ ] **Step 2: Manual smoke test**

Build and run the app:

```bash
cd "/media/riaz/New Volume/echo-AI"
npm run tauri dev
```

From the dev console (or a temporary debug button if no UI yet):

```js
const id = await window.__TAURI__.core.invoke("start_meeting", {
  title: "smoke",
  language: "en",
  modelPath: "/abs/path/to/ggml-base.en.bin",
});
// speak ~30s
await new Promise(r => setTimeout(r, 30000));
await window.__TAURI__.core.invoke("stop_meeting", { id });
```

Then:

```bash
sqlite3 "/media/riaz/New Volume/echo-AI/echo.db" \
  "SELECT id, t0_ms, t1_ms, text FROM meeting_segments WHERE meeting_id = (SELECT MAX(id) FROM meetings);"
```
Expected: at least one row with non-empty `text`. The `meetings` row's `status` is `stopped`, `ended_at` populated.

If no rows: check `RUST_LOG=info` output for "meetings: transcribe failed" or "stt-sidecar bad json".

- [ ] **Step 3: Append CHANGELOG entry**

Append to `CHANGELOG_DEV.md` (top of the file, after the existing 2026-05-03 entry if any):

```markdown
## 2026-05-03 — Meeting Agent Phase 2 (capture + persistence)

**Symptom / why:** Phase 1 shipped the STT sidecar but the app could not yet
record meetings end-to-end. We needed mic capture, voice-activity
segmentation, and persisted transcripts before any UI work.

**Fix:**
- New migration `src-tauri/src/db/migrations/012_meetings.sql` (additive)
- `src-tauri/src/db/repositories/meetings.rs` — start/stop/append_segment/get/list_segments
- `src-tauri/src/services/meeting/{audio_capture,segmenter,transcribe_worker,service}.rs`
- `src-tauri/src/services/stt_sidecar.rs` — new `transcribe_samples()`
- `src-tauri/src/commands/meetings.rs` — `start_meeting` / `stop_meeting`
- `src-tauri/src/lib.rs` — lazy-spawn sidecar, manage `MeetingService`

**Cross-cutting:** Adds cpal, webrtc-vad, rubato, hound, base64 deps. Linux
build now requires `libasound2-dev`. Single active meeting enforced;
multi-meeting concurrency is a Phase 3 concern.
```

- [ ] **Step 4: Create PHR**

Use the `sp.phr` skill (or create the file directly per CLAUDE.md PHR rules) at:

```
history/prompts/meeting-agent/<NEXT_ID>-meeting-agent-phase2.green.prompt.md
```

Embed the verbatim user prompt that started this work, the design + plan paths, and the list of files modified.

- [ ] **Step 5: Final commit**

```bash
git add CHANGELOG_DEV.md history/prompts/meeting-agent/
git commit -m "docs(meetings): Phase 2 CHANGELOG + PHR"
```

---

## Self-Review Notes

- **Spec coverage:** every spec section maps to a task — migration → T1, repo → T2, deps → T3, sidecar client → T4 (added because spec assumed `transcribe_b64` but the existing client only had `transcribe_wav`), segmenter → T5, capture → T6, worker → T7, service+commands → T8, smoke/PHR/CHANGELOG → T9.
- **Type consistency:** `Segment { t0_ms: i64, t1_ms: i64, samples: Vec<f32> }` is defined in T5 and consumed unchanged in T7 and T8. `MeetingsRepo` function names match between T2 and T8 callers (`start`, `stop`, `mark_error`, `append_segment`).
- **Single-meeting enforcement** is in T8's `MeetingService::start` — matches spec "one meeting at a time in Phase 2".
- **Backpressure**: T8 uses `try_send` + warn log on full segment channel — matches spec "drop oldest" intent simplified to "drop newest" (oldest-drop adds a `try_recv` race; warn-and-drop-current is equally informative for Phase 2 telemetry, both surface to Phase 3 UI).
