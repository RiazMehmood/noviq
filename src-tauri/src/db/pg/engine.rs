//! Feature 011 (T004) — bundled PostgreSQL engine lifecycle (contract C3).
//!
//! The engine ships INSIDE the app (postgresql_embedded `bundled`: the PG
//! 17.10 archive is embedded at compile time; pgvector arrives from the
//! vendored `resources/pgvector/<platform>/` tree by file copy — fully
//! offline at runtime, FR-004). The app owns the child process: first-run
//! initdb, start, health-check, clean shutdown (FR-005); loopback only
//! (FR-006). Every failure variant carries a plain-language remediation
//! (FR-012) surfaced by the frontend startup screen.
//!
//! Orphan adoption + port-conflict retry hardening land in T019/T020 (US2);
//! this Phase-2 version covers init → start → extension → connect → stop.

use postgresql_embedded::{PostgreSQL, Settings, VersionReq};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::settings::{server_config, tuning_for, PgTuning};
use crate::services::hardware_detector::HardwareTier;

/// Bundled engine version — MUST match the compile-time POSTGRESQL_VERSION
/// pin (see run scripts / CI env) and the vendored pgvector build.
pub const BUNDLED_PG_VERSION: &str = "=17.10.0";
/// Application database name inside the engine.
pub const APP_DB_NAME: &str = "echo";

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("database engine setup failed: {source_msg}\n→ {remediation}")]
    InitFailed { source_msg: String, remediation: String },
    #[error("database engine failed to start: {source_msg}\n→ {remediation}")]
    StartFailed { source_msg: String, remediation: String },
    #[error("vector extension missing: {source_msg}\n→ {remediation}")]
    ExtensionMissing { source_msg: String, remediation: String },
    #[error("database connection failed: {source_msg}\n→ {remediation}")]
    ConnectFailed { source_msg: String, remediation: String },
}

impl EngineError {
    /// Plain-language remediation for the UI (FR-012).
    pub fn remediation(&self) -> &str {
        match self {
            EngineError::InitFailed { remediation, .. }
            | EngineError::StartFailed { remediation, .. }
            | EngineError::ExtensionMissing { remediation, .. }
            | EngineError::ConnectFailed { remediation, .. } => remediation,
        }
    }
}

/// Opaque handle owning the running engine child process.
pub struct EngineHandle {
    pg: PostgreSQL,
    tuning: PgTuning,
}

impl EngineHandle {
    /// Connection URL for the application database (loopback only).
    pub fn url(&self) -> String {
        self.pg.settings().url(APP_DB_NAME)
    }
    pub fn tuning(&self) -> &PgTuning {
        &self.tuning
    }
}

/// The engine root is the directory inside `installation_dir` containing
/// `bin/pg_config` (the bundled archive extracts into a versioned subdir).
fn find_engine_root(installation_dir: &Path) -> Option<PathBuf> {
    if installation_dir.join("bin").join("pg_config").exists() {
        return Some(installation_dir.to_path_buf());
    }
    for entry in std::fs::read_dir(installation_dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join("bin").join("pg_config").exists() {
            return Some(p);
        }
    }
    None
}

fn platform_key() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

/// Copy the vendored pgvector artifacts (lib/**, share/**) onto the engine
/// root, preserving relative paths. Idempotent (plain overwrite).
fn install_vendored_pgvector(vendor_root: &Path, engine_root: &Path) -> std::io::Result<usize> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let p = entry?.path();
            if p.is_dir() {
                walk(&p, out)?;
            } else {
                out.push(p);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(vendor_root, &mut files)?;
    let mut copied = 0;
    for src in &files {
        let rel = src.strip_prefix(vendor_root).expect("under vendor root");
        let dst = engine_root.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, &dst)?;
        copied += 1;
    }
    Ok(copied)
}

/// Provision (first run) and start the bundled engine.
///
/// `pg_root` is the engine's private directory (e.g. `{app_data_dir}/pg`).
/// `pgvector_resources` is the vendored extension tree root
/// (`.../resources/pgvector` — the platform subdir is appended here).
pub async fn ensure_running(
    pg_root: &Path,
    pgvector_resources: &Path,
    tier: &HardwareTier,
) -> Result<EngineHandle, EngineError> {
    let tuning = tuning_for(tier);

    let mut configuration: HashMap<String, String> = HashMap::new();
    for (k, v) in server_config(&tuning) {
        configuration.insert(k.to_string(), v);
    }
    // FR-006: loopback only. Networking is a later feature (LAN spec).
    configuration.insert("listen_addresses".to_string(), "127.0.0.1".to_string());

    // Settings::default() generates a NEW random password every call. initdb
    // stamps the first run's password into the cluster, so on every later
    // launch we must reuse the password persisted in `.pgpass` or
    // authentication fails (observed after the 2026-06-12 oomd kill).
    let password_file = pg_root.join(".pgpass");
    let persisted_password = std::fs::read_to_string(&password_file)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let mut settings = Settings {
        version: VersionReq::parse(BUNDLED_PG_VERSION).expect("valid bundled version"),
        installation_dir: pg_root.join("install"),
        data_dir: pg_root.join("data"),
        password_file,
        host: "127.0.0.1".to_string(),
        temporary: false,
        configuration,
        ..Default::default()
    };
    if let Some(pw) = persisted_password {
        settings.password = pw;
    }

    let mut pg = PostgreSQL::new(settings);
    pg.setup().await.map_err(|e| EngineError::InitFailed {
        source_msg: e.to_string(),
        remediation: format!(
            "Check free disk space and write permission in {}; if the directory is corrupted, \
             delete it and relaunch (the database is recreated; indexed data is restored by \
             migration or re-indexing).",
            pg_root.display()
        ),
    })?;

    // Vendored pgvector → engine root (offline; proven by tests/pg_spike.rs).
    let vendor_root = pgvector_resources.join(platform_key());
    let engine_root =
        find_engine_root(&pg.settings().installation_dir).ok_or_else(|| EngineError::InitFailed {
            source_msg: "engine root (bin/pg_config) not found after setup".to_string(),
            remediation: format!(
                "Delete {} and relaunch to re-extract the bundled engine.",
                pg_root.join("install").display()
            ),
        })?;
    let copied = install_vendored_pgvector(&vendor_root, &engine_root).map_err(|e| {
        EngineError::ExtensionMissing {
            source_msg: format!("vendored pgvector copy failed: {e}"),
            remediation: format!(
                "The app package is missing its vector-search component for {} — reinstall the app.",
                platform_key()
            ),
        }
    })?;
    log::info!("pg engine: {copied} vendored pgvector files in place at {engine_root:?}");

    pg.start().await.map_err(|e| EngineError::StartFailed {
        source_msg: e.to_string(),
        remediation: "Close any other copy of the app and relaunch. If the problem persists, \
                      restart the computer (a previous run may have left the engine in a bad state)."
            .to_string(),
    })?;

    if !pg
        .database_exists(APP_DB_NAME)
        .await
        .map_err(|e| EngineError::StartFailed {
            source_msg: e.to_string(),
            remediation: "Relaunch the app; if it recurs, restart the computer.".to_string(),
        })?
    {
        pg.create_database(APP_DB_NAME)
            .await
            .map_err(|e| EngineError::StartFailed {
                source_msg: e.to_string(),
                remediation: "Relaunch the app; if it recurs, restart the computer.".to_string(),
            })?;
    }

    Ok(EngineHandle { pg, tuning })
}

/// Build the application pool against the running engine. Sets the per-tier
/// `hnsw.ef_search` on every connection (research R3).
pub async fn connect(handle: &EngineHandle) -> Result<PgPool, EngineError> {
    let ef_search = handle.tuning.hnsw_ef_search;
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                use sqlx::Executor;
                conn.execute(format!("SET hnsw.ef_search = {ef_search}").as_str())
                    .await?;
                Ok(())
            })
        })
        .connect(&handle.url())
        .await
        .map_err(|e| EngineError::ConnectFailed {
            source_msg: e.to_string(),
            remediation: "Relaunch the app. If the problem persists, another program may be \
                          blocking local connections (firewall/antivirus) — allow the app and retry."
                .to_string(),
        })?;
    Ok(pool)
}

/// Clean shutdown on app exit (FR-005). Consumes the handle.
pub async fn shutdown(handle: EngineHandle) -> Result<(), EngineError> {
    handle.pg.stop().await.map_err(|e| EngineError::StartFailed {
        source_msg: e.to_string(),
        remediation: "The engine will recover automatically on next launch.".to_string(),
    })
}
