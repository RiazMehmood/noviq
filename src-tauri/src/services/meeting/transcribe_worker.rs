//! Pull `Segment`s, transcribe via the sidecar, persist to MeetingsRepo.

use std::sync::Arc;
use crate::db::Db;
use tokio::sync::mpsc;

use crate::db::repositories::meetings as meetings_repo;
use crate::services::stt_sidecar::SttSidecar;
use super::segmenter::Segment;

pub async fn run(
    mut rx: mpsc::Receiver<Segment>,
    sidecar: Arc<SttSidecar>,
    pool: Db,
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
