//! Tauri commands: multi-source analysis templates — T063 (US3).

use serde::{Deserialize, Serialize};
use crate::db::Db;
use std::sync::Arc;
use tauri::State;

use crate::services::analysis_engine::{self, AnalysisResult as EngineResult};
use crate::services::exporters;
use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::ModelSlot;

/// DTO returned to the frontend.
#[derive(Debug, Serialize, Deserialize)]
pub struct AnalysisResultDto {
    pub template: String,
    pub title: String,
    pub output: String,
    pub citations: Vec<serde_json::Value>,
    pub quality_constrained: bool,
}

impl From<EngineResult> for AnalysisResultDto {
    fn from(r: EngineResult) -> Self {
        let citations = r
            .citations
            .into_iter()
            .map(|c| serde_json::to_value(c).unwrap_or(serde_json::Value::Null))
            .collect();
        AnalysisResultDto {
            template: r.template,
            title: r.title,
            output: r.output,
            citations,
            quality_constrained: r.quality_constrained,
        }
    }
}

/// Run an analysis template against a notebook's indexed content.
#[tauri::command]
pub async fn run_analysis(
    app: tauri::AppHandle,
    pool: State<'_, Db>,
    model_slot: State<'_, Arc<ModelSlot>>,
    tier: State<'_, HardwareTier>,
    notebook_id: String,
    template: String,
) -> Result<AnalysisResultDto, String> {
    analysis_engine::run_template(&pool, &model_slot, &tier, &notebook_id, &template, Some(app))
        .await
        .map(Into::into)
        .map_err(|e| e.to_string())
}

/// Run analysis only on the specified source IDs.
/// If source_ids is empty, falls back to all sources in the notebook.
#[tauri::command]
pub async fn analyze_selected(
    app: tauri::AppHandle,
    pool: State<'_, Db>,
    model_slot: State<'_, Arc<ModelSlot>>,
    tier: State<'_, HardwareTier>,
    notebook_id: String,
    source_ids: Vec<String>,
    template: String,
) -> Result<AnalysisResultDto, String> {
    analysis_engine::run_template_selected(
        &pool, &model_slot, &tier, &notebook_id, &template, &source_ids, Some(app),
    )
    .await
    .map(Into::into)
    .map_err(|e| e.to_string())
}

/// Export a previously-run analysis result to disk.
///
/// `format`: "pdf" | "txt"
/// `result_json`: the serialised AnalysisResultDto from `run_analysis`
/// `destination_path`: absolute path to write the output file
#[tauri::command]
pub async fn export_analysis(
    result_json: String,
    format: String,
    destination_path: String,
) -> Result<(), String> {
    // Deserialise the result the frontend passes back.
    let dto: AnalysisResultDto =
        serde_json::from_str(&result_json).map_err(|e| format!("invalid result JSON: {e}"))?;

    // Re-construct a minimal EngineResult for the exporters.
    let engine_result = EngineResult {
        template: dto.template,
        title: dto.title,
        output: dto.output,
        citations: dto
            .citations
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
        quality_constrained: dto.quality_constrained,
    };

    match format.as_str() {
        "pdf" => exporters::pdf::export(&engine_result, &destination_path)
            .map_err(|e| e.to_string()),
        "txt" => exporters::txt::export(&engine_result, &destination_path)
            .map_err(|e| e.to_string()),
        other => Err(format!("unsupported format: {other} (use 'pdf' or 'txt')")),
    }
}
