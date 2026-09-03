//! Tauri commands: system information + model management.

use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::{calculate_gpu_layers, estimate_model_layers_from_name, ModelSlot};
use std::sync::Arc;

// ─── Hardware ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_hardware_tier(tier: State<'_, HardwareTier>) -> Result<HardwareTier, String> {
    Ok(tier.inner().clone())
}

// ─── Model management ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelStatus {
    pub exists: bool,
    pub model_name: String,
    pub model_path: String,
    pub size_mb: Option<f64>,
}

/// GPU acceleration status returned to the frontend.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuSettings {
    /// Whether GPU acceleration is currently enabled (persisted preference).
    pub enabled: bool,
    /// Vendor string: "Nvidia", "Amd", or "None".
    pub vendor: String,
    /// Detected VRAM in GB (0.0 if no GPU).
    pub vram_gb: f32,
    /// Display name e.g. "NVIDIA GeForce RTX 3060".
    pub gpu_name: String,
    /// Auto-calculated layer count when enabled (display only).
    pub layers_auto: u32,
    /// Total layers in current model (display only).
    pub layers_total: u32,
}

/// A model the app provisions on first launch. `relative_path` is rooted at the
/// project/install dir (e.g. `models/...`); `url` is a direct GGUF download.
pub struct ModelSpec {
    pub relative_path: &'static str,
    pub url: &'static str,
    pub display_name: &'static str,
}

/// Primary chat/generation model — Noviq's fine-tuned Sindh Qwen2.5-0.5B.
/// Discovery (`query_router::discover_local_gguf()`) still picks up any other
/// GGUF dropped into `models/`.
pub fn model_info_for_tier(_tier: &HardwareTier) -> (&'static str, &'static str, &'static str) {
    (
        "models/noviq-qwen2.5-0.5b-instruct.Q4_K_M.gguf",
        "https://huggingface.co/riazmehmood/noviq_qwen2.5_0.5B/resolve/main/noviq-qwen2.5-0.5b-instruct.Q4_K_M.gguf",
        "Noviq Qwen2.5-0.5B Instruct Q4_K_M",
    )
}

/// The full set of models provisioned on first launch — and *only* these:
///   1. Noviq Sindh Qwen2.5-0.5B  (chat / grounded generation)
///
/// The Mistral-7B IR-builder model was dropped 2026-06: the doc_ir/Mistral
/// engine (Feature 006) and one-ask/template-ingestion (F009/F010) are retired,
/// so it is no longer required or provisioned.
pub fn required_models(tier: &HardwareTier) -> Vec<ModelSpec> {
    let (relative_path, url, display_name) = model_info_for_tier(tier);
    vec![
        ModelSpec { relative_path, url, display_name },
    ]
}

/// Resolve a relative model path using the same 3-source strategy as query_router.rs:
/// 1. ECHO_MODELS_DIR env var (explicit override)
/// 2. CARGO_MANIFEST_DIR at compile time → ../models/ (debug builds, survives CARGO_TARGET_DIR moves)
/// 3. Walk up from exe (release bundles)
fn resolve_model_path(relative_path: &str) -> String {
    // 1. Explicit env var override.
    if let Ok(base) = std::env::var("ECHO_MODELS_DIR") {
        let candidate = std::path::Path::new(&base).join(
            relative_path.trim_start_matches("models/"),
        );
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }

    // 2. Compile-time project root (debug builds).
    #[cfg(debug_assertions)]
    {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        if let Some(project_root) = std::path::Path::new(manifest_dir).parent() {
            let candidate = project_root.join(relative_path);
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }

    // 3. Walk up from exe (release bundles).
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..6 {
            if let Some(d) = &dir {
                let candidate = d.join(relative_path);
                if candidate.exists() {
                    return candidate.to_string_lossy().to_string();
                }
                dir = d.parent().map(|p| p.to_path_buf());
            } else {
                break;
            }
        }
    }
    relative_path.to_string()
}

/// Find the models/ directory using the same 3-source strategy.
fn models_dir() -> std::path::PathBuf {
    // 1. Explicit env var override.
    if let Ok(base) = std::env::var("ECHO_MODELS_DIR") {
        let p = std::path::PathBuf::from(&base);
        if p.is_dir() {
            return p;
        }
    }

    // 2. Compile-time project root (debug builds).
    #[cfg(debug_assertions)]
    {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        if let Some(project_root) = std::path::Path::new(manifest_dir).parent() {
            let candidate = project_root.join("models");
            if candidate.is_dir() {
                return candidate;
            }
            // Create it if missing.
            if std::fs::create_dir_all(&candidate).is_ok() {
                return candidate;
            }
        }
    }

    // 3. Walk up from exe.
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..6 {
            if let Some(d) = &dir {
                let candidate = d.join("models");
                if candidate.is_dir() {
                    return candidate;
                }
                dir = d.parent().map(|p| p.to_path_buf());
            } else {
                break;
            }
        }
        // Create in first writable parent.
        if let Ok(exe2) = std::env::current_exe() {
            let mut d = exe2.parent().map(|p| p.to_path_buf());
            for _ in 0..6 {
                if let Some(ref dir) = d {
                    let candidate = dir.join("models");
                    if std::fs::create_dir_all(&candidate).is_ok() {
                        return candidate;
                    }
                    d = dir.parent().map(|p| p.to_path_buf());
                } else {
                    break;
                }
            }
        }
    }
    std::path::PathBuf::from("models")
}

/// Whether a required model is already present, returning its resolved path.
/// Honours the hot-swap discovery paths so a manually-placed GGUF counts:
/// `discover_local_gguf()` for the chat model, `discover_ir_builder_gguf()`
/// for the IR-builder (Mistral) slot.
fn model_present(spec: &ModelSpec) -> Option<String> {
    let resolved = resolve_model_path(spec.relative_path);
    if std::path::Path::new(&resolved).exists() {
        return Some(resolved);
    }
    crate::services::query_router::discover_local_gguf().map(|(p, _)| p)
}

#[tauri::command]
pub async fn check_model(tier: State<'_, HardwareTier>) -> Result<ModelStatus, String> {
    // All required models must be present; report the first one still missing
    // so the banner prompts the user to download it.
    for spec in required_models(tier.inner()) {
        if model_present(&spec).is_none() {
            return Ok(ModelStatus {
                exists: false,
                model_name: spec.display_name.to_string(),
                model_path: resolve_model_path(spec.relative_path),
                size_mb: None,
            });
        }
    }

    // Everything present — report the primary chat model.
    let (relative_path, _url, display_name) = model_info_for_tier(tier.inner());
    let primary = resolve_model_path(relative_path);
    let size_mb = std::fs::metadata(&primary)
        .map(|m| m.len() as f64 / 1_048_576.0)
        .ok();
    Ok(ModelStatus {
        exists: true,
        model_name: display_name.to_string(),
        model_path: primary,
        size_mb,
    })
}

#[derive(Debug, Serialize, Clone)]
struct DownloadProgress {
    percent: u8,
    downloaded_mb: f64,
    total_mb: f64,
}

/// Download a single model spec to its destination (honouring sub-dirs such as
/// `ir_builder/`). Emits per-file progress but never emits `percent == 100` —
/// the caller emits a single completion event once *all* models are present so
/// the UI doesn't prompt to restart after only the first of several downloads.
async fn download_one(app: &tauri::AppHandle, spec: &ModelSpec) -> Result<(), String> {
    // Skip if already present (incl. a manually-placed / discovered copy).
    if model_present(spec).is_some() {
        return Ok(());
    }

    let rel = spec
        .relative_path
        .strip_prefix("models/")
        .unwrap_or(spec.relative_path);
    let dest_path = models_dir().join(rel);
    if let Some(parent) = dest_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    log::info!("Downloading model {} from {}", spec.display_name, spec.url);
    log::info!("Destination: {}", dest_path.display());

    let client = reqwest::Client::new();
    let response = client
        .get(spec.url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {} — download failed", response.status()));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let total_mb = total_bytes as f64 / 1_048_576.0;

    // Write to a temp file, rename on completion to avoid partial files.
    let tmp_path = dest_path.with_extension("gguf.tmp");
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| format!("Failed to create file: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut last_pct: u8 = 0;
    let mut stream = response.bytes_stream();

    use tokio::io::AsyncWriteExt;
    use futures_util::StreamExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download stream error: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {e}"))?;
        downloaded += chunk.len() as u64;

        if total_bytes > 0 {
            // Clamp below 100 — completion is signalled once after all models.
            let pct = (((downloaded * 100) / total_bytes) as u8).min(99);
            if pct != last_pct {
                last_pct = pct;
                let _ = app.emit(
                    "model:download-progress",
                    DownloadProgress {
                        percent: pct,
                        downloaded_mb: downloaded as f64 / 1_048_576.0,
                        total_mb,
                    },
                );
            }
        }
    }

    file.flush().await.map_err(|e| format!("Flush error: {e}"))?;
    drop(file);

    tokio::fs::rename(&tmp_path, &dest_path)
        .await
        .map_err(|e| format!("Failed to finalise download: {e}"))?;

    log::info!("Model downloaded to {}", dest_path.display());
    Ok(())
}

#[tauri::command]
pub async fn download_model(
    app: tauri::AppHandle,
    tier: State<'_, HardwareTier>,
) -> Result<(), String> {
    // Provision every required model (Sindh 0.5B chat + Mistral-7B), in order,
    // skipping any already present.
    for spec in required_models(tier.inner()) {
        download_one(&app, &spec).await?;
    }

    // All models present — signal completion (prompts restart to load them).
    let _ = app.emit(
        "model:download-progress",
        DownloadProgress {
            percent: 100,
            downloaded_mb: 0.0,
            total_mb: 0.0,
        },
    );

    Ok(())
}

#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

#[tauri::command]
pub async fn get_gpu_settings(
    tier: State<'_, HardwareTier>,
    pool: State<'_, crate::db::Db>,
) -> Result<GpuSettings, String> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT value FROM db_config WHERE key = 'gpu_enabled'"
    )
    .fetch_optional(&*pool)
    .await
    .map_err(|e| e.to_string())?;
    let enabled = raw.map(|v| v != "0").unwrap_or(true);

    let t = tier.inner();
    let (rel_path, _, _) = model_info_for_tier(t);
    let filename = std::path::Path::new(rel_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let layers_total = estimate_model_layers_from_name(&filename);
    let model_size_mb: u64 = if filename.contains("7b") { 4100 } else { 2400 };
    let layers_auto = if t.has_discrete_gpu {
        calculate_gpu_layers(t.vram_gb, layers_total, model_size_mb)
    } else {
        0
    };

    Ok(GpuSettings {
        enabled,
        vendor: format!("{:?}", t.gpu_vendor),
        vram_gb: t.vram_gb,
        gpu_name: t.gpu_name.clone(),
        layers_auto,
        layers_total,
    })
}

#[tauri::command]
pub async fn set_gpu_enabled(
    enabled: bool,
    pool: State<'_, crate::db::Db>,
    model_slot: State<'_, Arc<ModelSlot>>,
) -> Result<(), String> {
    let value = if enabled { "1" } else { "0" };
    sqlx::query(
        "INSERT INTO db_config (key, value) VALUES ($1, $2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value"
    )
    .bind("gpu_enabled")
    .bind(value)
    .execute(&*pool)
    .await
    .map_err(|e| e.to_string())?;

    model_slot.set_gpu_enabled(enabled);
    log::info!("GPU enabled set to {enabled} by user");
    Ok(())
}

/// Chat answer mode. `true` = extractive (verbatim provisions, no LLM); `false`
/// = LLM synthesis (**default** — the model answers). Stored in db_config
/// 'chat_extractive'; only the value `'1'` turns the extractive bypass on.
#[tauri::command]
pub async fn get_chat_extractive(pool: State<'_, crate::db::Db>) -> Result<bool, String> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT value FROM db_config WHERE key = 'chat_extractive'",
    )
    .fetch_optional(&*pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(raw.map(|v| v == "1").unwrap_or(false))
}

#[tauri::command]
pub async fn set_chat_extractive(
    extractive: bool,
    pool: State<'_, crate::db::Db>,
) -> Result<(), String> {
    let value = if extractive { "1" } else { "0" };
    sqlx::query(
        "INSERT INTO db_config (key, value) VALUES ($1, $2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind("chat_extractive")
    .bind(value)
    .execute(&*pool)
    .await
    .map_err(|e| e.to_string())?;
    log::info!("Chat extractive mode set to {extractive} by user");
    Ok(())
}

// ─── Query trace logs ────────────────────────────────────────────────────────

/// Return the absolute path to the per-query trace log directory and reveal it
/// in the user's file manager. The folder contains one `.log` file per query
/// and a rolling `queries.jsonl` analytics file.
#[tauri::command]
pub async fn open_query_logs_dir(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_shell::ShellExt;
    let some_app = Some(app.clone());
    // Open the queries/ subdir directly so user sees the .log files, not the parent.
    let dir = crate::services::query_trace::QueryTrace::resolve_log_dir(&some_app).join("queries");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path_str = dir.to_string_lossy().to_string();
    // Best-effort reveal — never fail the command if the OS opener is missing.
    // `Shell::open` is deprecated in favour of tauri-plugin-opener, but adding
    // that plugin (+ capability wiring) is out of scope here; allow the call.
    #[allow(deprecated)]
    let _ = app.shell().open(path_str.clone(), None);
    Ok(path_str)
}

/// Write the latest query trace to a user-chosen destination path.
/// Done in Rust to avoid Tauri fs-plugin scope/permission constraints on
/// arbitrary user-picked paths (writeTextFile from JS requires per-path scope).
#[tauri::command]
pub async fn download_query_trace_to(app: tauri::AppHandle, dest: String) -> Result<String, String> {
    let (name, body) = read_latest_query_trace(app).await?;
    if name.is_empty() {
        return Err("No trace yet — run a query first.".to_string());
    }
    let path = std::path::PathBuf::from(&dest);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir failed: {e}"))?;
    }
    std::fs::write(&path, body).map_err(|e| format!("write failed: {e}"))?;
    Ok(dest)
}

/// Concatenate every `.log` trace in the queries directory into a single file.
/// Files are sorted by modified-time ascending (oldest first), separated by a
/// banner header so a human can scroll through them in run order. Returns the
/// number of traces written and the destination path.
///
/// Used by the benchmarking workflow: run all 17 prompts in one app session,
/// then export the whole batch with one click instead of clicking download
/// after each query.
#[tauri::command]
pub async fn download_all_query_traces_to(
    app: tauri::AppHandle,
    dest: String,
) -> Result<(usize, String), String> {
    let some_app = Some(app.clone());
    let dir = crate::services::query_trace::QueryTrace::resolve_log_dir(&some_app).join("queries");
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read dir failed: {e}"))?;
    let mut logs: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("log") { continue; }
        if let Ok(meta) = e.metadata() {
            if let Ok(modified) = meta.modified() {
                logs.push((modified, p));
            }
        }
    }
    if logs.is_empty() {
        return Err("No traces in queries folder yet — run at least one query.".to_string());
    }
    logs.sort_by_key(|(t, _)| *t);

    let mut combined = String::with_capacity(64 * 1024);
    combined.push_str(&format!(
        "EchoAI — Combined Query Trace Export\nGenerated: {}\nTraces in this file: {}\nSource folder: {}\n\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        logs.len(),
        dir.display()
    ));
    for (i, (_, path)) in logs.iter().enumerate() {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?.log");
        combined.push_str(&"#".repeat(80));
        combined.push_str(&format!("\n# TRACE {}/{} — {}\n", i + 1, logs.len(), name));
        combined.push_str(&"#".repeat(80));
        combined.push('\n');
        match std::fs::read_to_string(path) {
            Ok(body) => combined.push_str(&body),
            Err(e)   => combined.push_str(&format!("[failed to read {name}: {e}]\n")),
        }
        combined.push_str("\n\n");
    }

    let out_path = std::path::PathBuf::from(&dest);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir failed: {e}"))?;
    }
    std::fs::write(&out_path, combined).map_err(|e| format!("write failed: {e}"))?;
    Ok((logs.len(), dest))
}

/// Concatenate the most recent `limit` `.log` traces in the queries directory
/// into a single file (newest-last so the combined file reads in run order).
/// Mirrors `download_all_query_traces_to` but capped at `limit` entries — used
/// for "download last N queries" quick-export from Settings.
#[tauri::command]
pub async fn download_recent_query_traces_to(
    app: tauri::AppHandle,
    dest: String,
    limit: usize,
) -> Result<(usize, String), String> {
    let some_app = Some(app.clone());
    let dir = crate::services::query_trace::QueryTrace::resolve_log_dir(&some_app).join("queries");
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("read dir failed: {e}"))?;
    let mut logs: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("log") { continue; }
        if let Ok(meta) = e.metadata() {
            if let Ok(modified) = meta.modified() {
                logs.push((modified, p));
            }
        }
    }
    if logs.is_empty() {
        return Err("No traces in queries folder yet — run at least one query.".to_string());
    }
    logs.sort_by_key(|(t, _)| *t);
    let take = limit.max(1).min(logs.len());
    let logs: Vec<_> = logs.into_iter().rev().take(take).collect::<Vec<_>>()
        .into_iter().rev().collect();

    let mut combined = String::with_capacity(64 * 1024);
    combined.push_str(&format!(
        "EchoAI — Recent Query Trace Export (last {})\nGenerated: {}\nTraces in this file: {}\nSource folder: {}\n\n",
        take,
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        logs.len(),
        dir.display()
    ));
    for (i, (_, path)) in logs.iter().enumerate() {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?.log");
        combined.push_str(&"#".repeat(80));
        combined.push_str(&format!("\n# TRACE {}/{} — {}\n", i + 1, logs.len(), name));
        combined.push_str(&"#".repeat(80));
        combined.push('\n');
        match std::fs::read_to_string(path) {
            Ok(body) => combined.push_str(&body),
            Err(e)   => combined.push_str(&format!("[failed to read {name}: {e}]\n")),
        }
        combined.push_str("\n\n");
    }

    let out_path = std::path::PathBuf::from(&dest);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir failed: {e}"))?;
    }
    std::fs::write(&out_path, combined).map_err(|e| format!("write failed: {e}"))?;
    Ok((logs.len(), dest))
}

/// Get current cloud-inference (Groq) settings.
#[tauri::command]
pub async fn get_cloud_settings(app: tauri::AppHandle) -> Result<crate::services::cloud_llm::CloudSettings, String> {
    Ok(crate::services::cloud_llm::load_settings(&app))
}

/// Save cloud-inference settings.
#[tauri::command]
pub async fn set_cloud_settings(
    app: tauri::AppHandle,
    settings: crate::services::cloud_llm::CloudSettings,
) -> Result<(), String> {
    crate::services::cloud_llm::save_settings(&app, &settings).map_err(|e| e.to_string())
}

/// Return the contents of the most recent per-query trace log.
/// Returns `(filename, contents)`. Empty string if no trace exists yet.
#[tauri::command]
pub async fn read_latest_query_trace(app: tauri::AppHandle) -> Result<(String, String), String> {
    let some_app = Some(app.clone());
    // Per-query .log files live in <log_dir>/queries/, not <log_dir>/ (see
    // query_trace.rs::write_inner). Bug-fix 2026-05-03 — see CHANGELOG_DEV.md.
    let dir = crate::services::query_trace::QueryTrace::resolve_log_dir(&some_app).join("queries");
    let entries = match std::fs::read_dir(&dir) {
        Ok(it) => it,
        Err(_) => return Ok(("".to_string(), "".to_string())),
    };
    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("log") { continue; }
        if let Ok(meta) = e.metadata() {
            if let Ok(modified) = meta.modified() {
                if newest.as_ref().map_or(true, |(t, _)| modified > *t) {
                    newest = Some((modified, p));
                }
            }
        }
    }
    match newest {
        Some((_, path)) => {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("trace.log").to_string();
            let body = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            Ok((name, body))
        }
        None => Ok(("".to_string(), "".to_string())),
    }
}
