//! Notebook archive — export to / import from a `.echoai` ZIP file.
//!
//! Archive format:
//!   manifest.json — notebook metadata + source list
//!   sources/<source_id>/<file_name> — copy of the original file (if still on disk)
//!
//! Export: reads notebook + sources from DB, copies files into a ZIP.
//! Import: unzips to a temp directory, re-creates the notebook + sources record,
//!         then queues re-indexing of each source file.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use crate::db::Db;
use std::io::{Read, Write};
use std::path::Path;

use crate::db::repositories::{notebooks as nb_repo, sources as source_repo};

// ─── Manifest schema ──────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct ArchiveManifest {
    version: u32,
    notebook_id: String,
    notebook_name: String,
    exported_at: i64,
    sources: Vec<SourceEntry>,
}

#[derive(Serialize, Deserialize)]
struct SourceEntry {
    id: String,
    file_name: String,
    format: String,
    file_size_bytes: i64,
    original_path: String,
}

// ─── Export ───────────────────────────────────────────────────────────────────

/// Export a notebook and all its indexed source files to a `.echoai` ZIP archive.
///
/// Files that no longer exist on disk are recorded in the manifest but skipped
/// in the ZIP (the import path will re-index what it can find).
pub async fn export_notebook(
    pool: &Db,
    notebook_id: &str,
    destination: &str,
) -> Result<()> {
    let notebook = nb_repo::get(pool, notebook_id)
        .await?
        .with_context(|| format!("Notebook {notebook_id} not found"))?;

    let sources = source_repo::list_by_notebook(pool, notebook_id).await?;

    let dest_path = Path::new(destination);
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let file = std::fs::File::create(dest_path)
        .with_context(|| format!("Cannot create archive at {destination}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Build manifest.
    let mut entries: Vec<SourceEntry> = Vec::new();
    for s in &sources {
        entries.push(SourceEntry {
            id: s.id.clone(),
            file_name: s.file_name.clone(),
            format: s.format.clone(),
            file_size_bytes: s.file_size_bytes,
            original_path: s.file_path.clone(),
        });
    }

    let manifest = ArchiveManifest {
        version: 1,
        notebook_id: notebook.id.clone(),
        notebook_name: notebook.name.clone(),
        exported_at: chrono::Utc::now().timestamp_millis(),
        sources: entries,
    };

    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    zip.start_file("manifest.json", opts)?;
    zip.write_all(manifest_json.as_bytes())?;

    // Copy source files into the archive.
    let mut copied = 0usize;
    for s in &sources {
        let src_path = Path::new(&s.file_path);
        if !src_path.exists() {
            log::warn!("Archive: source file not found, skipping: {}", s.file_path);
            continue;
        }
        let arc_path = format!("sources/{}/{}", s.id, s.file_name);
        zip.start_file(&arc_path, opts)?;
        let mut f = std::fs::File::open(src_path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        zip.write_all(&buf)?;
        copied += 1;
    }

    zip.finish()?;
    log::info!(
        "Archive exported: notebook '{}' ({} sources, {} files copied) → {destination}",
        notebook.name,
        sources.len(),
        copied,
    );
    Ok(())
}

// ─── Import ───────────────────────────────────────────────────────────────────

/// Import a `.echoai` archive: unzips to a temp directory, creates a new notebook,
/// copies files to the temp location, then returns the new notebook id and the list
/// of file paths ready for re-indexing.
///
/// The caller (archive command) is responsible for queueing the re-index so we
/// avoid pulling in the full indexer dependency here.
pub async fn import_notebook(
    pool: &Db,
    archive_path: &str,
) -> Result<(String, Vec<String>)> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("Cannot open archive: {archive_path}"))?;
    let mut zip = zip::ZipArchive::new(file)?;

    // Read manifest.
    let manifest: ArchiveManifest = {
        let mut entry = zip.by_name("manifest.json")
            .context("Archive is missing manifest.json")?;
        let mut buf = String::new();
        entry.read_to_string(&mut buf)?;
        serde_json::from_str(&buf).context("Failed to parse manifest.json")?
    };

    // Create a new notebook (use the original name; append " (imported)" if collision).
    let new_nb = nb_repo::create(pool, &format!("{} (imported)", manifest.notebook_name))
        .await?;

    // Extract source files to a temp directory.
    let tmp_dir = std::env::temp_dir().join(format!("echoai_import_{}", new_nb.id));
    std::fs::create_dir_all(&tmp_dir)?;

    let mut restored_paths: Vec<String> = Vec::new();
    for entry in &manifest.sources {
        let arc_path = format!("sources/{}/{}", entry.id, entry.file_name);
        if let Ok(mut zip_entry) = zip.by_name(&arc_path) {
            let dest = tmp_dir.join(&entry.file_name);
            let mut out = std::fs::File::create(&dest)?;
            let mut buf = Vec::new();
            zip_entry.read_to_end(&mut buf)?;
            out.write_all(&buf)?;
            restored_paths.push(dest.to_string_lossy().to_string());
            log::info!("Archive import: restored {} → {}", entry.file_name, dest.display());
        } else {
            log::warn!(
                "Archive import: source file missing in archive ({}) — will attempt original path: {}",
                entry.file_name,
                entry.original_path,
            );
            // Fall back to original path if it still exists.
            if Path::new(&entry.original_path).exists() {
                restored_paths.push(entry.original_path.clone());
            }
        }
    }

    log::info!(
        "Archive imported: '{}' → new notebook id {} ({} files ready for indexing)",
        manifest.notebook_name,
        new_nb.id,
        restored_paths.len(),
    );

    Ok((new_nb.id, restored_paths))
}
