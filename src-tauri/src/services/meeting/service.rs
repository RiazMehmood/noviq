//! Orchestrates audio capture → segmenter → transcribe worker for ONE
//! active meeting at a time. Phase 2 enforces single-meeting concurrency.
//!
//! Threading note: both `cpal::Stream` (inside `CaptureHandle`) and
//! `webrtc_vad::Vad` (inside `Segmenter`) hold raw pointers and are
//! therefore `!Send`. They live on dedicated `spawn_blocking` threads so
//! `MeetingService` itself only stores `Send + Sync` handles (oneshot
//! senders + JoinHandles), making it safe to register as Tauri managed
//! state behind `Arc<MeetingService>`.

use anyhow::{anyhow, Result};
use crate::db::Db;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;

use crate::db::repositories::meetings as meetings_repo;
use crate::services::stt_sidecar::SttSidecar;
use super::audio_capture;
use super::segmenter::{Segment, Segmenter, FRAME_SAMPLES};
use super::transcribe_worker;

pub struct MeetingService {
    pool: Db,
    sidecar: Arc<SttSidecar>,
    active: Mutex<Option<ActiveMeeting>>,
}

struct ActiveMeeting {
    id: i64,
    /// Drop signal for the capture thread (which owns the cpal Stream).
    capture_stop: oneshot::Sender<()>,
    /// Blocking thread that owns the cpal Stream + waits for `capture_stop`.
    capture_task: JoinHandle<()>,
    /// Blocking thread that runs the VAD segmenter.
    seg_task: JoinHandle<()>,
    /// Async task that drains segments → sidecar → DB.
    worker_task: JoinHandle<()>,
}

impl MeetingService {
    pub fn new(pool: Db, sidecar: Arc<SttSidecar>) -> Self {
        Self { pool, sidecar, active: Mutex::new(None) }
    }

    pub async fn start(&self, title: String, language: String, model_path: String) -> Result<i64> {
        let mut guard = self.active.lock().await;
        if guard.is_some() {
            return Err(anyhow!("a meeting is already active; stop it first"));
        }

        let id = meetings_repo::start(&self.pool, &title, &language, &model_path).await?;

        if let Err(e) = self.sidecar.load_model(&model_path, &language, false).await {
            let _ = meetings_repo::mark_error(&self.pool, id, &format!("load_model: {e}")).await;
            return Err(e);
        }

        let (frame_tx, mut frame_rx) = mpsc::channel::<[i16; FRAME_SAMPLES]>(32);
        let (seg_tx, seg_rx) = mpsc::channel::<Segment>(8);
        let (capture_stop_tx, capture_stop_rx) = oneshot::channel::<()>();

        // Spawn capture on a blocking thread that owns the cpal Stream
        // (Stream is !Send so we can't move it into an async task).
        let capture_task = tokio::task::spawn_blocking(move || {
            let _capture = match audio_capture::start(frame_tx) {
                Ok(c) => c,
                Err(e) => {
                    log::error!("meetings: audio capture failed to start: {e}");
                    return;
                }
            };
            // Block this thread until the stop signal fires; dropping
            // `_capture` then stops the underlying cpal stream.
            let _ = capture_stop_rx.blocking_recv();
        });

        // Spawn segmenter on a blocking thread (Vad is !Send).
        let seg_task = tokio::task::spawn_blocking(move || {
            let mut segmenter = Segmenter::new();
            while let Some(frame) = frame_rx.blocking_recv() {
                if let Some(s) = segmenter.push_frame(frame) {
                    if seg_tx.try_send(s).is_err() {
                        log::warn!("meetings: backpressure — dropping segment");
                    }
                }
            }
            if let Some(s) = segmenter.finalize() {
                let _ = seg_tx.blocking_send(s);
            }
        });

        let pool = self.pool.clone();
        let sidecar = self.sidecar.clone();
        let lang_for_worker = language.clone();
        let worker_task = tokio::spawn(async move {
            transcribe_worker::run(seg_rx, sidecar, pool, id, lang_for_worker).await;
        });

        *guard = Some(ActiveMeeting {
            id,
            capture_stop: capture_stop_tx,
            capture_task,
            seg_task,
            worker_task,
        });
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
        // Signal the capture thread to drop its Stream; this closes frame_tx,
        // which in turn ends the segmenter loop and (after final flush) the
        // worker loop.
        let _ = active.capture_stop.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), active.capture_task).await;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), active.seg_task).await;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), active.worker_task).await;
        meetings_repo::stop(&self.pool, id).await?;
        Ok(())
    }
}
