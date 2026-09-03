use std::sync::Arc;
use crate::db::Db;
use tauri::State;
use crate::services::meeting::MeetingService;
use crate::db::repositories::meetings as meetings_repo;
use crate::db::repositories::meetings::{Meeting, MeetingSegment};

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

#[tauri::command]
pub async fn list_meeting_segments(
    pool: State<'_, Db>,
    meeting_id: i64,
) -> Result<Vec<MeetingSegment>, String> {
    meetings_repo::list_segments(&pool, meeting_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_meeting(
    pool: State<'_, Db>,
    id: i64,
) -> Result<Option<Meeting>, String> {
    meetings_repo::get(&pool, id).await.map_err(|e| e.to_string())
}
