//! T023 / T050 / T053 — disk JSON cache for `DocIr` records.
//!
//! Disk: `{app_data_dir}/ir_cache/<sha256>.json`
//! Index: `ir_cache_index` SQLite table (see `db::repositories::ir_cache`).
//!
//! See data-model.md §4.

use std::path::{Path, PathBuf};

use crate::db::Db;
// F014: `Manager` (Tauri path resolver) is desktop-only — see cache_dir, which
// falls back to NOVIQ_DATA_DIR in the headless server build.
#[cfg(feature = "desktop")]
use tauri::Manager;

use crate::db::repositories::ir_cache as repo;
use crate::services::doc_ir::types::{DocIr, DocIrEnvelope};

/// Computed quality score (used by both build and re-import).
pub fn quality_score(nodes_total: usize, nodes_ok: usize) -> f64 {
    if nodes_total == 0 {
        1.0
    } else {
        nodes_ok as f64 / nodes_total as f64
    }
}

/// Compute `{app_data_dir}/ir_cache` and create it if missing.
pub fn cache_dir(app: &crate::compat::AppHandle) -> anyhow::Result<PathBuf> {
    // Desktop: resolve via the Tauri path API. Headless server: the compat
    // AppHandle has no path resolver, so derive the data dir from NOVIQ_DATA_DIR
    // (same convention as the other server-side caches).
    #[cfg(feature = "desktop")]
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| anyhow::anyhow!("app_data_dir: {e}"))?;
    #[cfg(not(feature = "desktop"))]
    let base = {
        let _ = app;
        PathBuf::from(std::env::var("NOVIQ_DATA_DIR").unwrap_or_else(|_| ".noviq-data".to_string()))
    };
    let dir = base.join("ir_cache");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Path for a given sha256.
pub fn cache_path(app: &crate::compat::AppHandle, sha256: &str) -> anyhow::Result<PathBuf> {
    Ok(cache_dir(app)?.join(format!("{sha256}.json")))
}

/// Write a `DocIr` to disk AND upsert its index row. The caller has already
/// validated the IR.
///
/// Returns the absolute path of the written JSON file.
pub async fn write_doc_ir(
    pool: &Db,
    app: &crate::compat::AppHandle,
    ir: &DocIr,
) -> anyhow::Result<PathBuf> {
    let nodes_total = ir.nodes.len();
    let nodes_ok = ir
        .nodes
        .iter()
        .filter(|n| matches!(n.quality_flag, crate::services::doc_ir::types::QualityFlag::Ok))
        .count();
    let nodes_low_confidence = ir
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n.quality_flag,
                crate::services::doc_ir::types::QualityFlag::LowConfidence
            )
        })
        .count();
    let nodes_user_repaired = ir
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n.quality_flag,
                crate::services::doc_ir::types::QualityFlag::UserRepaired
            )
        })
        .count();

    let envelope = DocIrEnvelope::wrap(ir.clone());
    let json = serde_json::to_string_pretty(&envelope)?;
    let path = cache_path(app, &ir.doc_id)?;
    write_atomic(&path, json.as_bytes())?;

    repo::insert_or_update(
        pool,
        repo::InsertOrUpdate {
            sha256: &ir.doc_id,
            schema_version: ir.schema_version as i64,
            doc_type: ir.doc_type.as_str(),
            nodes_total: nodes_total as i64,
            nodes_ok: nodes_ok as i64,
            nodes_low_confidence: nodes_low_confidence as i64,
            nodes_user_repaired: nodes_user_repaired as i64,
            quality_score: quality_score(nodes_total, nodes_ok),
        },
    )
    .await?;

    Ok(path)
}

/// Read a `DocIr` back from disk by sha. Returns `None` when missing.
pub async fn read_doc_ir(
    app: &crate::compat::AppHandle,
    sha256: &str,
) -> anyhow::Result<Option<DocIr>> {
    let path = cache_path(app, sha256)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    let env: DocIrEnvelope = serde_json::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("malformed cache JSON at {path:?}: {e}"))?;
    Ok(Some(env.doc_ir))
}

/// T050 — cache hit short-circuit: lookup by sha; reactivate from orphan when
/// found; return the loaded IR or `None`.
pub async fn lookup_by_sha(
    pool: &Db,
    app: &crate::compat::AppHandle,
    sha256: &str,
    current_schema_version: i64,
) -> anyhow::Result<Option<DocIr>> {
    let row = match repo::get(pool, sha256).await? {
        Some(r) => r,
        None => return Ok(None),
    };
    if row.schema_version != current_schema_version {
        // Schema bump — caller will re-run and migrate (T056).
        return Ok(None);
    }
    let ir = match read_doc_ir(app, sha256).await? {
        Some(ir) => ir,
        None => {
            // Index row exists but file is gone — clean up.
            repo::delete(pool, sha256).await?;
            return Ok(None);
        }
    };
    if row.orphaned_at.is_some() {
        repo::clear_orphan(pool, sha256).await?;
    }
    Ok(Some(ir))
}

/// T053 — sweep orphan records older than `cutoff_unix`. Deletes both the
/// JSON file and the index row in one transaction.
pub async fn sweep_orphans(
    pool: &Db,
    app: &crate::compat::AppHandle,
    cutoff_unix: i64,
) -> anyhow::Result<usize> {
    let to_delete = repo::find_orphans_older_than(pool, cutoff_unix).await?;
    let mut count = 0;
    for sha in &to_delete {
        let path = cache_path(app, sha)?;
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        repo::delete(pool, sha).await?;
        count += 1;
    }
    Ok(count)
}

/// Atomic write: tmp file in same dir, then rename. Avoids half-written JSON
/// when the process crashes mid-write.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("ir_cache")
    ));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}
