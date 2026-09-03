//! Document upload endpoint (F014) — lets a signed-in browser user index files
//! from their own device. Files are streamed to a per-user directory on the
//! server, then run through the existing folder-indexing pipeline (FK-safe stub
//! notebook + `/api/index/events` progress). Indexing is serial for now (single
//! pipeline); concurrent uploads queue behind it.

use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::Json;

use super::auth::AuthUser;
use super::{ApiError, ApiResult, AppState};
use crate::db::repositories::folders as folder_repo;
use crate::services::indexing::{index_folder, index_sources, FolderDto};

fn err(e: impl std::fmt::Display) -> ApiError {
    ApiError::internal(e.to_string())
}

/// Flatten an uploaded name to a safe basename: drop any directory components
/// (incl. `webkitdirectory` relative paths) and leading dots so a file can
/// never be written outside the user's upload dir.
fn safe_basename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    base.trim().trim_start_matches('.').replace('\0', "")
}

/// Per-user directory name — alphanumerics/`-`/`_` only.
fn safe_user(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    if s.is_empty() { "user".to_string() } else { s }
}

/// F021 — the user's upload directory (shared with `promote_attachment`).
pub fn user_upload_dir(username: &str) -> std::path::PathBuf {
    let data_dir = std::env::var("NOVIQ_DATA_DIR").unwrap_or_else(|_| ".noviq-data".to_string());
    std::path::Path::new(&data_dir)
        .join("uploads")
        .join(safe_user(username))
}

/// F021 — global per-user allowance (bytes, files). Admin-configurable via
/// `.env`: `NOVIQ_USER_QUOTA_MB` (default 500), `NOVIQ_USER_QUOTA_FILES`
/// (default 200).
pub fn quota_limits() -> (i64, i64) {
    let mb = std::env::var("NOVIQ_USER_QUOTA_MB")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(500);
    let files = std::env::var("NOVIQ_USER_QUOTA_FILES")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(200);
    (mb * 1024 * 1024, files)
}

/// F022 — a conversation attachment must be a SMALL document (a single notice
/// or one audit para) so the local model reads it whole and the browser isn't
/// blocked OCR-ing a 200-page report before it can reply. A full document
/// belongs in My Documents (the folder-indexing pipeline), not an attachment.
/// Admin-configurable via `.env`: `NOVIQ_ATTACH_MAX_PAGES` (default 15),
/// `NOVIQ_ATTACH_MAX_MB` (default 8).
pub fn attach_limits() -> (usize, i64) {
    let pages = std::env::var("NOVIQ_ATTACH_MAX_PAGES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(15);
    let mb = std::env::var("NOVIQ_ATTACH_MAX_MB")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(8);
    (pages, mb * 1024 * 1024)
}

/// Cheap page count for a PDF from its bytes. `lopdf` parses the page tree only
/// — no text extraction, no OCR — so it's fast and works on scanned PDFs too.
/// `None` for non-PDFs or an unreadable/encrypted PDF, in which case the page
/// cap simply doesn't apply (the byte cap still does).
fn pdf_page_count(base: &str, data: &[u8]) -> Option<usize> {
    if !base.to_ascii_lowercase().ends_with(".pdf") {
        return None;
    }
    lopdf::Document::load_mem(data).ok().map(|d| d.get_pages().len())
}

/// F021 (FR-009) — refuse an upload that would exceed the user's allowance.
/// `incoming` = (bytes, files) about to be added.
async fn check_quota(
    pool: &crate::db::Db,
    user_id: &str,
    incoming: (i64, i64),
) -> Result<(), ApiError> {
    let (limit_bytes, limit_files) = quota_limits();
    let (used_bytes, used_files) =
        crate::db::repositories::sources::usage_for_owner(pool, user_id)
            .await
            .map_err(err)?;
    if used_bytes + incoming.0 > limit_bytes || used_files + incoming.1 > limit_files {
        let body = serde_json::json!({
            "code": "quota_exceeded",
            "message": format!(
                "Storage allowance exceeded: {:.1} MB of {:.0} MB and {} of {} files used.",
                used_bytes as f64 / (1024.0 * 1024.0),
                limit_bytes as f64 / (1024.0 * 1024.0),
                used_files, limit_files
            ),
            "used_bytes": used_bytes,
            "used_files": used_files,
            "limit_bytes": limit_bytes,
            "limit_files": limit_files,
        });
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "quota_exceeded",
            body["message"].as_str().unwrap_or("quota exceeded"),
        ));
    }
    Ok(())
}

/// One file the server declined, with a user-readable reason (rendered as a
/// list in the My documents panel — never a raw error).
#[derive(serde::Serialize)]
pub struct RejectedFile {
    pub file_name: String,
    pub reason: String,
}

/// Response of `upload_documents`: whatever was accepted (folder DTO of the
/// user's uploads folder, `None` when nothing was) + per-file rejections.
#[derive(serde::Serialize)]
pub struct UploadOutcome {
    pub folder: Option<FolderDto>,
    pub rejected: Vec<RejectedFile>,
    /// Uploads that hold more than one legal instrument. Indexing still runs —
    /// see `detect_multi_instrument` for why this warns rather than blocks — but
    /// the corpus owner is told, because a compendium indexed as one document is
    /// the difference between "Rule 4" being a citation and being ambiguous.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub multi_instrument: Vec<MultiInstrumentNotice>,
    /// Uploads whose TITLE closely matches something already indexed. Warns, never
    /// blocks: a near-identical name is genuinely ambiguous — "SPPRA Rules 2010"
    /// and "SPPRA Rules 2010 Amended 15Jul2022" are two documents that must both
    /// exist, while the same file re-uploaded with a "(1)" suffix is one document
    /// twice. Only the uploader can tell those apart, so the server surfaces the
    /// match and lets them decide. Identical BYTES never reach here — those are
    /// deduped by content in `attach`/`index_sources` and never re-indexed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub similar_existing: Vec<SimilarNotice>,
    /// Source ids this upload bound to the project (empty when not uploading
    /// into one). ⚠️ The caller MUST NOT re-derive these by listing the uploads
    /// folder: since migration 024 a project document is deliberately absent
    /// from folder listings, so that lookup returns nothing and a successful
    /// upload reads as a failure. The server did the attaching; it is the only
    /// party that can say what was attached.
    #[serde(default)]
    pub attached: Vec<String>,
}

/// One upload that looks like a document already in the corpus.
#[derive(Debug, serde::Serialize)]
pub struct SimilarNotice {
    pub file_name: String,
    /// Existing titles that matched, closest first.
    pub matches: Vec<String>,
}

/// Reduce a file name to comparable content tokens.
///
/// Separators vary freely across the corpus for the same instrument
/// (`Sindh_Civil_Servants_Act_1973.pdf`, `Sindh Civil Servants Act 1973.pdf`,
/// `sindh-civil-servants-act-1973 (1).pdf`), so they all collapse to the same
/// token set. A trailing `(1)`/`copy` is dropped: those are what a browser or a
/// file manager adds to a RE-download of the same thing, which is exactly the
/// case worth warning about.
pub(crate) fn title_tokens(file_name: &str) -> Vec<String> {
    let stem = std::path::Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name)
        .to_ascii_lowercase();
    stem.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .filter(|t| !matches!(*t, "copy" | "final" | "v" | "the" | "of" | "and" | "for"))
        // A bare "1"/"2" is a copy counter, not content. Years (4 digits) are
        // content and must survive — they are what separates the 2009 Act from
        // the 2010 Rules, a confusion this corpus has already hit once.
        .filter(|t| !(t.len() < 2 && t.chars().all(|c| c.is_ascii_digit())))
        .map(|t| t.to_string())
        .collect()
}

/// Jaccard overlap of two title token sets, 0.0–1.0.
pub(crate) fn title_similarity(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
    let ta: HashSet<String> = title_tokens(a).into_iter().collect();
    let tb: HashSet<String> = title_tokens(b).into_iter().collect();
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f32;
    let union = ta.union(&tb).count() as f32;
    inter / union
}

/// Titles are "confusably similar" at or above this overlap. Tuned so that
/// `SPPRA Rules 2010 Amended 15Jul2022` vs `SPPRA Rules 2010` (0.5) DOES warn —
/// an amended instrument sitting beside its base text is worth a second look —
/// while merely sharing a subject (`Sindh Local Government Act 2013` vs
/// `Sindh Civil Servants Act 1973`, 0.33) does not.
pub(crate) const SIMILAR_TITLE_THRESHOLD: f32 = 0.5;

/// Existing titles that look like `file_name`, closest first, capped at 3.
///
/// Scoped to what the uploader can already see — the shared corpus plus their own
/// documents — using the same predicate as every other listing read. Warning a
/// user that "a similar document exists" about a file they cannot see would leak
/// another user's document titles, which is the whole reason this is not a plain
/// corpus-wide search.
async fn similar_titles(
    pool: &crate::db::Db,
    owner_id: &str,
    file_name: &str,
) -> Vec<String> {
    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT COALESCE(display_name, file_name) FROM sources \
         WHERE conversation_id IS NULL AND disabled_at IS NULL \
           AND (visibility = 'shared' OR owner_id = $1)",
    )
    .bind(owner_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut scored: Vec<(f32, String)> = existing
        .into_iter()
        // Same name is not "similar", it is the SAME document being replaced in
        // place by the path-keyed upload flow — not a duplicate to warn about.
        .filter(|t| !t.eq_ignore_ascii_case(file_name))
        .map(|t| (title_similarity(file_name, &t), t))
        .filter(|(s, _)| *s >= SIMILAR_TITLE_THRESHOLD)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.truncate(3);
    scored.into_iter().map(|(_, t)| t).collect()
}

#[derive(Debug, serde::Serialize)]
pub struct MultiInstrumentNotice {
    pub file_name: String,
    pub instrument_count: usize,
    /// First few instrument names, so the notice is recognisable without opening
    /// the review panel.
    pub sample_titles: Vec<String>,
}

/// `POST /api/upload_documents` (multipart) — save uploaded files under
/// `{NOVIQ_DATA_DIR}/uploads/<user>/` and index them. Files are accepted
/// individually: unsupported types and files past the allowance are reported
/// per-file in `rejected` while the rest of the batch still indexes (FR-009).
pub async fn upload_documents(
    State(st): State<AppState>,
    user: AuthUser,
    mut multipart: Multipart,
) -> ApiResult<Json<UploadOutcome>> {
    let dir = user_upload_dir(&user.username);
    std::fs::create_dir_all(&dir).map_err(err)?;

    // Read the whole batch first so acceptance is decided against a stable
    // usage snapshot before anything is written.
    let mut incoming: Vec<(String, axum::body::Bytes)> = Vec::new();
    // F027 — an optional `projectId` text field puts the uploads straight into a
    // project. Uploads still count against the uploader's own allowance (FR-009);
    // only COPIES are free, because only copies store no new bytes.
    let mut project_id: Option<String> = None;
    // The category the admin picked in the add-folder dialog, applied to every
    // document in this batch as it finishes indexing. Chosen up front rather
    // than after the fact so a 3,900-file corpus never lands in one
    // undifferentiated Uncategorised pile that then has to be sorted by hand.
    let mut category_id: Option<String> = None;
    while let Some(field) = multipart.next_field().await.map_err(err)? {
        let Some(fname) = field.file_name().map(|s| s.to_string()) else {
            // A field with no filename is a text field. Both spellings are
            // accepted for each: a multipart field name is not covered by serde's
            // rename_all, so mismatching it would silently drop the value and
            // fail open (unscoped upload / Uncategorised) rather than erroring.
            match field.name() {
                Some("projectId") | Some("project_id") => {
                    let v = field.text().await.map_err(err)?;
                    if !v.trim().is_empty() {
                        project_id = Some(v.trim().to_string());
                    }
                }
                Some("categoryId") | Some("category_id") => {
                    let v = field.text().await.map_err(err)?;
                    if !v.trim().is_empty() {
                        category_id = Some(v.trim().to_string());
                    }
                }
                _ => {}
            }
            continue;
        };
        let base = safe_basename(&fname);
        if base.is_empty() {
            continue;
        }
        let data = field.bytes().await.map_err(err)?;
        incoming.push((base, data));
    }

    // Verified BEFORE anything is written: naming a project the caller does not
    // own must refuse outright, never fall back to an unscoped upload that then
    // sits in their document list unexplained.
    if let Some(pid) = project_id.as_deref() {
        crate::api::projects::require_projects_enabled(&st).await?;
        crate::db::repositories::projects::owned_project(&st.pool, &user.id, pid)
            .await
            .map_err(err)?
            .ok_or_else(crate::api::projects::project_not_found)?;
    }

    // Same rule for the category: verified BEFORE any bytes are written, so a
    // typo'd or stale id refuses the upload outright instead of accepting the
    // files and quietly filing them as Uncategorised — the failure mode that
    // would be invisible until someone counted a department months later.
    // Categories are admin-managed corpus organisation, so only an admin may set
    // one; a non-admin naming a category is a bug in the caller, not a silent
    // downgrade.
    if let Some(cid) = category_id.as_deref() {
        if user.role != "admin" {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "not_admin",
                "only an administrator can file uploads into a category",
            ));
        }
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM corpus_category WHERE id = $1")
                .bind(cid)
                .fetch_optional(&st.pool)
                .await
                .map_err(err)?;
        if exists.is_none() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "no_such_category",
                "that category no longer exists",
            ));
        }
    }

    // ⚠️ F027 (2026-08-11) — a project upload gets its OWN folder, never the
    // officer's general uploads folder.
    //
    // A source's visibility is inherited from the folder it is indexed into
    // (`scope_for_notebook` → `indexed_folders.visibility`), and that folder is
    // created ONCE per path and reused forever. Two consequences, both wrong:
    //   1. An ADMIN's uploads folder is created ownerless, i.e. `visibility =
    //      'shared'` — so a document dropped into their private project became
    //      part of the shared corpus, visible to every user and listed in the
    //      indexed documents. That is the leak this fixes.
    //   2. Even for a regular user, project files landed in the same folder as
    //      their ordinary documents, so the two could never be told apart.
    // A per-project directory means `get_by_path` misses, a fresh folder row is
    // created below with the uploader as owner, and it is private by
    // construction (`folders::insert`: owned ⇒ private).
    let dir = match project_id.as_deref() {
        Some(pid) => {
            let d = user_upload_dir(&user.username).join(format!("__project__{pid}"));
            std::fs::create_dir_all(&d).map_err(err)?;
            d
        }
        None => dir,
    };
    if incoming.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "no_files",
            "no files were uploaded",
        ));
    }

    // Per-file accept/reject: unsupported type first, then allowance in batch
    // order — earlier files in the picker win the remaining quota.
    let (limit_bytes, limit_files) = quota_limits();
    let (used_bytes, used_files) =
        crate::db::repositories::sources::usage_for_owner(&st.pool, &user.id)
            .await
            .map_err(err)?;
    let mut accepted: Vec<(String, axum::body::Bytes)> = Vec::new();
    let mut rejected: Vec<RejectedFile> = Vec::new();
    let (mut acc_bytes, mut acc_files) = (0i64, 0i64);
    for (base, data) in incoming {
        // ⚠️ Narrower than `FileFormat` on purpose (2026-08-11). Audio, video and
        // code parse, but nothing transcribes or indexes them into retrievable
        // text today, so accepting them produced sources that could never answer
        // anything. Rejected at the boundary rather than removed from the enum —
        // folder indexing and the parser tests still use the full set.
        let fmt = crate::services::parsers::FileFormat::from_path(std::path::Path::new(&base));
        let supported = matches!(
            fmt,
            Some(
                crate::services::parsers::FileFormat::Pdf
                    | crate::services::parsers::FileFormat::Docx
                    | crate::services::parsers::FileFormat::Xlsx
                    | crate::services::parsers::FileFormat::Pptx
                    | crate::services::parsers::FileFormat::Txt
                    | crate::services::parsers::FileFormat::Markdown
                    | crate::services::parsers::FileFormat::Image
            )
        );
        if !supported {
            let ext = std::path::Path::new(&base)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{e}"))
                .unwrap_or_else(|| "no extension".to_string());
            rejected.push(RejectedFile {
                file_name: base,
                reason: format!(
                    "Unsupported file type ({ext}). Supported: PDF, Word, Excel, \
                     PowerPoint, text/Markdown and images."
                ),
            });
            continue;
        }
        let len = data.len() as i64;
        if used_bytes + acc_bytes + len > limit_bytes || used_files + acc_files + 1 > limit_files {
            rejected.push(RejectedFile {
                file_name: base,
                reason: format!(
                    "Storage allowance reached ({} of {} files, {:.1} of {:.0} MB used). \
                     Delete a document to free space.",
                    used_files + acc_files,
                    limit_files,
                    (used_bytes + acc_bytes) as f64 / (1024.0 * 1024.0),
                    limit_bytes as f64 / (1024.0 * 1024.0),
                ),
            });
            continue;
        }
        acc_bytes += len;
        acc_files += 1;
        accepted.push((base, data));
    }
    if accepted.is_empty() {
        return Ok(Json(UploadOutcome {
            folder: None,
            rejected,
            multi_instrument: Vec::new(),
            similar_existing: Vec::new(),
            attached: Vec::new(),
        }));
    }

    // Before writing anything: is one of these a near-duplicate of a document
    // that already exists? Computed against the pre-upload corpus so an upload
    // never matches itself, and reported alongside the result rather than
    // blocking it — see `SimilarNotice`.
    let mut similar_existing: Vec<SimilarNotice> = Vec::new();
    for (base, _) in &accepted {
        let matches = similar_titles(&st.pool, &user.id, base).await;
        if !matches.is_empty() {
            similar_existing.push(SimilarNotice { file_name: base.clone(), matches });
        }
    }

    let mut saved: Vec<String> = Vec::new();
    for (base, data) in &accepted {
        let dest = dir.join(base);
        std::fs::write(&dest, data).map_err(err)?;
        saved.push(dest.to_string_lossy().to_string());
    }

    // Before indexing: does any of these hold more than one legal instrument?
    let multi_instrument = detect_multi_instrument(&saved, user.role == "admin").await;

    let dir_str = dir.to_string_lossy().to_string();
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool,
        Some(&user.id),
        "upload",
        Some(&dir_str),
        None,
    )
    .await;

    // Reuse the folder pipeline. If this user already has an uploads folder,
    // index just the new files into its stub notebook; otherwise create it.
    let dto = match folder_repo::get_by_path(&st.pool, &dir_str).await.map_err(err)? {
        Some(existing) => {
            let nb = format!("__folder__{}", existing.id);
            // Publish the denominator BEFORE queueing, so the first poll after
            // this response already reads "0 of N" rather than an idle folder.
            folder_repo::begin_batch(
                &st.pool,
                &existing.id,
                saved.len() as i64,
                category_id.as_deref(),
            )
            .await
            .ok();
            index_sources(
                &st.pool,
                &st.embedder,
                &st.tier,
                &nb,
                &saved,
                None,
                Some(&existing.id),
                category_id.as_deref(),
            )
            .await
            .map_err(err)?;
            let total = std::fs::read_dir(&dir)
                .map(|rd| rd.flatten().filter(|e| e.path().is_file()).count() as i64)
                .unwrap_or(existing.file_count);
            folder_repo::update_file_count(&st.pool, &existing.id, total)
                .await
                .ok();
            FolderDto {
                id: existing.id,
                folder_path: existing.folder_path,
                indexing_status: existing.indexing_status,
                file_count: total,
                last_scanned_at: existing.last_scanned_at,
                created_at: existing.created_at,
                visibility: existing.visibility,
                owner_username: existing.owner_username,
                // Read back from `begin_batch` above rather than echoing the
                // pre-batch row, so the response itself already carries the
                // denominator the poller is about to confirm.
                indexing_total: saved.len() as i64,
                indexing_done: 0,
            }
        }
        // Admin uploads are shared with everyone (owner_id NULL = global, so
        // list_for_owner surfaces them to every user); a regular user's uploads
        // stay private to them.
        // ⚠️ EXCEPT into a project: a project is one officer's private working
        // set, so its documents are owned (and therefore private) whoever
        // uploads them. An admin putting a file in their own project is not
        // publishing it to the whole deployment.
        None => {
            let owner = if user.role == "admin" && project_id.is_none() {
                None
            } else {
                Some(user.id.clone())
            };
            index_folder(
                &st.pool,
                &st.embedder,
                &st.tier,
                &dir_str,
                owner,
                None,
                category_id.as_deref(),
            )
            .await
            .map_err(err)?
        }
    };

    // F027 — bind the freshly indexed files to the project. Done AFTER indexing
    // so the membership names a row that exists, and matched on the saved paths
    // rather than on the whole uploads folder: this user may already own dozens
    // of documents there, and only the ones in this batch belong to the project.
    let mut attached: Vec<String> = Vec::new();
    if let Some(pid) = project_id.as_deref() {
        // ⚠️ WAIT for the rows. Both indexing paths above SPAWN — `index_sources`
        // one task per file, `index_folder` a directory scan — so a lookup fired
        // immediately after them usually finds nothing, `attached` comes back
        // empty, and the project dialog reports "Uploaded, but no document was
        // indexed" for an upload that actually succeeded. Reported from a real
        // project upload, 2026-08-15.
        //
        // A short wait is enough because `index_file` inserts the source row
        // EARLY — after hashing, but before it queues on the indexing semaphore
        // — so the row exists long before the document is parsed. We are waiting
        // for registration, not for indexing to finish.
        //
        // On timeout we attach whatever did appear rather than failing: the
        // files are on disk and indexing continues regardless, so a partial
        // attach is recoverable, and refusing here would lose the upload.
        let mut new_ids: Vec<String> = Vec::new();
        for attempt in 0..40 {
            new_ids.clear();
            for path in &saved {
                if let Ok(Some(s)) =
                    crate::db::repositories::sources::get_by_path(&st.pool, path).await
                {
                    new_ids.push(s.id);
                }
            }
            if new_ids.len() == saved.len() {
                break;
            }
            if attempt == 39 {
                log::warn!(
                    "project {pid}: only {}/{} uploaded files registered within 10s; \
                     attaching what exists",
                    new_ids.len(),
                    saved.len()
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        if !new_ids.is_empty() {
            crate::db::repositories::projects::add_sources(&st.pool, pid, &new_ids)
                .await
                .map_err(err)?;
            // Migration 024 — record that these documents ORIGINATE here, which
            // is what keeps them out of the officer's global document tree and
            // out of global retrieval. Distinct from the membership rows above:
            // membership is what a document ADDED from the corpus gets, and such
            // a document stays globally visible on purpose.
            sqlx::query("UPDATE sources SET project_id = $1 WHERE id = ANY($2)")
                .bind(pid)
                .bind(&new_ids)
                .execute(&st.pool)
                .await
                .map_err(err)?;
            attached = new_ids;
        }
    }

    Ok(Json(UploadOutcome {
        folder: Some(dto),
        rejected,
        multi_instrument,
        similar_existing,
        attached,
    }))
}

/// Which of these uploads hold several legal instruments rather than one?
///
/// Runs BEFORE indexing, on the saved files, costing one `pdftotext` pass each
/// (about a second for thirty pages) — cheap enough that an upload does not
/// visibly wait for it.
///
/// **It warns; it does not block.** Refusing to index would need a lifecycle
/// state the pipeline has no notion of — a source that exists but is neither
/// indexed nor failed, held until someone reviews it — and a half-built version
/// of that would strand documents with no way to release them. Indexing and then
/// splitting reaches the same end state, because the split deletes the
/// container's row and its chunks along with it.
///
/// Only for admins. The corpus is theirs to curate and only they can reach the
/// review panel; telling an ordinary user their document "contains 12
/// instruments" would be noise they cannot act on.
async fn detect_multi_instrument(saved: &[String], is_admin: bool) -> Vec<MultiInstrumentNotice> {
    if !is_admin {
        return Vec::new();
    }
    let pdfs: Vec<String> = saved
        .iter()
        .filter(|p| p.to_lowercase().ends_with(".pdf"))
        .cloned()
        .collect();
    if pdfs.is_empty() {
        return Vec::new();
    }

    // pdftotext + regex over whole documents: keep it off the async workers.
    let notices = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for path in pdfs {
            let p = std::path::Path::new(&path);
            let Ok(ins) = crate::services::instrument_detector::inspect_pdf(p) else {
                continue; // unreadable here is not fatal; indexing reports it
            };
            if !ins.is_container() {
                continue;
            }
            out.push(MultiInstrumentNotice {
                file_name: p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.clone()),
                instrument_count: ins.instrument_count(),
                sample_titles: ins
                    .segments
                    .iter()
                    .filter(|s| s.is_instrument)
                    .take(3)
                    .map(|s| s.title.clone())
                    .collect(),
            });
        }
        out
    })
    .await
    .unwrap_or_default();

    for n in &notices {
        log::warn!(
            "upload: {} holds {} legal instruments — indexed as ONE document, so \
             provision numbers will be ambiguous across them until it is split \
             (Admin → Indexing & Data → Document structure)",
            n.file_name,
            n.instrument_count
        );
    }
    notices
}

// ── F021 (US4): conversation attachments ─────────────────────────────────────

/// `POST /api/attach_document` (multipart: `conversationId` text field +
/// `files`) — index files for ONE conversation only. The source rows are
/// pre-created here with full ownership (owner + private + conversation_id) so
/// the attachment is never visible outside the conversation, even mid-index.
/// Progress streams over the existing `/api/index/events` SSE.
pub async fn attach_document(
    State(st): State<AppState>,
    user: AuthUser,
    mut multipart: Multipart,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let mut conversation_id: Option<String> = None;
    let mut incoming: Vec<(String, axum::body::Bytes)> = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(err)? {
        match field.name() {
            Some("conversationId") => {
                conversation_id = Some(field.text().await.map_err(err)?);
            }
            _ => {
                let Some(fname) = field.file_name().map(|s| s.to_string()) else { continue };
                let base = safe_basename(&fname);
                if base.is_empty() {
                    continue;
                }
                let data = field.bytes().await.map_err(err)?;
                incoming.push((base, data));
            }
        }
    }
    let Some(conv_id) = conversation_id.filter(|c| !c.trim().is_empty()) else {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "missing_conversation",
            "conversationId is required",
        ));
    };
    // Conversation ids are client-generated — allow only safe chars in paths.
    let conv_id: String = conv_id
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    if incoming.is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "no_files", "no files were uploaded"));
    }

    // F022: an attachment must be a SMALL document (one notice / one audit para).
    // A large document must be indexed under My Documents instead — attaching it
    // would block the reply on synchronous OCR+embed and the model would read
    // only its front matter. Enforced per file before anything is written.
    let (max_pages, max_bytes) = attach_limits();
    for (base, data) in &incoming {
        // Byte cap first so an oversized file is rejected without lopdf parsing
        // it; only page-count PDFs that already fit the byte cap.
        let over_bytes = data.len() as i64 > max_bytes;
        let pages = if over_bytes { None } else { pdf_page_count(base, data) };
        let over_pages = pages.is_some_and(|p| p > max_pages);
        if over_bytes || over_pages {
            let detail = pages
                .map(|p| format!("{p} pages"))
                .unwrap_or_else(|| format!("{:.1} MB", data.len() as f64 / (1024.0 * 1024.0)));
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "attachment_too_large",
                format!(
                    "\u{201c}{base}\u{201d} is too large to attach ({detail}). Attachments are \
                     limited to {max_pages} pages / {} MB so Noviq can read the whole document in \
                     chat. To work with a full document, add it under \u{201c}My Documents\u{201d} \
                     to index it, then ask about it here.",
                    max_bytes / (1024 * 1024),
                ),
            ));
        }
    }

    // Migration 022 — content dedup, BEFORE the quota check. Bytes this user has
    // already indexed cost no new storage, so re-attaching a file they already
    // have must not be charged to their allowance a second time. Hashing here
    // (rather than in the indexer) is what lets the whole write+index be skipped:
    // the indexer dedups on `file_path`, and the per-conversation directory means
    // the path is always new even when the content is not.
    let mut hashes: Vec<String> = Vec::with_capacity(incoming.len());
    for (_, data) in &incoming {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data.as_ref());
        hashes.push(format!("{:x}", hasher.finalize()));
    }
    let mut reused: Vec<Option<crate::db::repositories::sources::Source>> =
        Vec::with_capacity(incoming.len());
    for hash in &hashes {
        reused.push(
            crate::db::repositories::sources::find_indexed_by_owner_hash(
                &st.pool, &user.id, hash,
            )
            .await
            .map_err(err)?,
        );
    }

    // Allowance covers live attachments too (FR-019) — but only the files that
    // will actually be stored.
    let total_bytes: i64 = incoming
        .iter()
        .zip(&reused)
        .filter(|(_, r)| r.is_none())
        .map(|((_, d), _)| d.len() as i64)
        .sum();
    let new_files = reused.iter().filter(|r| r.is_none()).count() as i64;
    if new_files > 0 {
        check_quota(&st.pool, &user.id, (total_bytes, new_files)).await?;
    }

    let dir = user_upload_dir(&user.username).join(format!("__conv__{conv_id}"));
    std::fs::create_dir_all(&dir).map_err(err)?;

    // Stub notebook for the conversation (FK target for notebook_sources).
    let now = chrono::Utc::now().timestamp_millis();
    let nb = format!("__conv__{conv_id}");
    sqlx::query(
        "INSERT INTO notebooks (id, name, created_at, updated_at) \
         VALUES ($1, $2, $3, $3) ON CONFLICT (id) DO NOTHING",
    )
    .bind(&nb)
    .bind(format!("Conversation attachments: {conv_id}"))
    .bind(now)
    .execute(&st.pool)
    .await
    .map_err(err)?;

    let scope = crate::db::repositories::sources::SourceScope {
        owner_id: Some(user.id.clone()),
        visibility: Some("private".to_string()),
        conversation_id: Some(conv_id.clone()),
    };

    let mut out = Vec::new();
    let mut paths = Vec::new();
    for (((base, data), hash), reuse) in incoming.iter().zip(&hashes).zip(&reused) {
        // ⚠️ B2 (2026-08-07) — validate the type HERE, the way the document path
        // at the top of this file already does.
        //
        // This path used to pass the RAW extension (`.unwrap_or("bin")`) into
        // `sources.format`, and `insert_scoped` binds it unnormalised. The
        // migration-001 CHECK allows only the canonical set, so every extension
        // `FileFormat` accepts but does not spell identically — `.html`,
        // `.markdown`, `.xlsm`, `.wav/.ogg/.flac/.m4a`, `.mkv/.avi/.mov/.webm`
        // and EVERY code file — died on `sources_format_check` and leaked the raw
        // constraint name to the user. `FileFormat::as_str()` returns exactly the
        // values the CHECK permits, so routing through it is the fix.
        //
        // One bad file must not fail the whole batch, and `failed` is already in
        // the client's status union: it is neither polled nor sent to the model.
        let fmt = match crate::services::parsers::FileFormat::from_path(std::path::Path::new(base)) {
            Some(f) => f,
            None => {
                let ext = std::path::Path::new(base)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| format!(".{e}"))
                    .unwrap_or_else(|| "no extension".to_string());
                out.push(serde_json::json!({
                    "sourceId": serde_json::Value::Null,
                    "fileName": base,
                    "status": "failed",
                    "reused": false,
                    "reason": format!(
                        "Unsupported file type ({ext}). Supported: PDF, Word, Excel, \
                         PowerPoint, text/Markdown, images, audio, video and code files."
                    ),
                }));
                continue;
            }
        };

        // Already indexed for this user: link it into this conversation and stop.
        // No file written, no source row, no chunks, no embeddings — this is the
        // whole point of the change. It reports `indexed` rather than
        // `processing` because it already is, so the chat chip settles at once
        // instead of waiting for an index that will never run.
        if let Some(existing) = reuse {
            crate::db::repositories::sources::link_to_conversation(
                &st.pool, &existing.id, &conv_id, &user.id,
            )
            .await
            .map_err(err)?;
            sqlx::query(
                "INSERT INTO notebook_sources (notebook_id, source_id, added_at) \
                 VALUES ($1, $2, $3) ON CONFLICT (notebook_id, source_id) DO NOTHING",
            )
            .bind(&nb)
            .bind(&existing.id)
            .bind(now)
            .execute(&st.pool)
            .await
            .map_err(err)?;
            out.push(serde_json::json!({
                "sourceId": existing.id,
                "fileName": base,
                "status": "indexed",
                "reused": true,
            }));
            continue;
        }

        let dest = dir.join(base);
        std::fs::write(&dest, data).map_err(err)?;
        let dest_str = dest.to_string_lossy().to_string();

        // Pre-create the row with ownership so the indexer's dedup path reuses
        // it (never inserting an unowned row for this file). The hash is stamped
        // HERE, at creation: the indexer's re-index branch updates status but
        // never backfills `file_hash`, so a row created blank stayed blank
        // forever and could never be matched by content afterwards.
        let existing = crate::db::repositories::sources::get_by_path(&st.pool, &dest_str)
            .await
            .map_err(err)?;
        let source_id = match existing {
            Some(s) => s.id,
            None => {
                let meta_len = data.len() as i64;
                crate::db::repositories::sources::insert_scoped(
                    // B2 — canonical format, not the raw extension. `as_str()`
                    // returns exactly what `sources_format_check` permits.
                    &st.pool, &dest_str, base, fmt.as_str(),
                    meta_len, hash, now, &scope,
                )
                .await
                .map_err(err)?
                .id
            }
        };
        crate::db::repositories::sources::link_to_conversation(
            &st.pool, &source_id, &conv_id, &user.id,
        )
        .await
        .map_err(err)?;
        paths.push(dest_str);
        out.push(serde_json::json!({
            "sourceId": source_id,
            "fileName": base,
            "status": "processing",
            "reused": false,
        }));
    }

    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&user.id), "attach", Some(&conv_id), None,
    )
    .await;

    // Every file may have been a link, in which case there is nothing to index.
    if !paths.is_empty() {
        index_sources(&st.pool, &st.embedder, &st.tier, &nb, &paths, None, None, None)
            .await
            .map_err(err)?;
    }
    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separator_and_case_differences_are_the_same_title() {
        // The corpus already carries all three spellings of the same instrument;
        // none of them is a different document.
        assert_eq!(
            title_tokens("Sindh_Civil_Servants_Act_1973.pdf"),
            title_tokens("Sindh Civil Servants Act 1973.pdf"),
        );
        assert_eq!(
            title_tokens("sindh-civil-servants-act-1973.pdf"),
            title_tokens("Sindh Civil Servants Act 1973.PDF"),
        );
    }

    #[test]
    fn browser_download_counter_does_not_make_a_new_document() {
        // "(1)" is what a second download of the same file is called.
        assert_eq!(
            title_tokens("Constitution (1).pdf"),
            title_tokens("Constitution.pdf"),
        );
        assert!(title_similarity("Constitution (1).pdf", "Constitution.pdf") > 0.99);
    }

    #[test]
    fn a_year_is_content_and_must_survive_tokenising() {
        // The 2009 Act vs the 2010 Rules is a distinction this corpus has already
        // been burned by — the year must never be filtered out as a counter.
        assert!(title_tokens("SPPRA Rules 2010.pdf").contains(&"2010".to_string()));
        assert!(
            title_similarity("Sindh Public Procurement Act 2009.pdf", "SPPRA Rules 2010.pdf")
                < SIMILAR_TITLE_THRESHOLD
        );
    }

    #[test]
    fn an_amended_instrument_warns_against_its_base_text() {
        // Both must exist, but the officer should be told they now hold two.
        assert!(
            title_similarity(
                "SPPRA Rules 2010 Amended 15Jul2022.pdf",
                "SPPRA Rules 2010.pdf",
            ) >= SIMILAR_TITLE_THRESHOLD
        );
    }

    #[test]
    fn merely_sharing_a_subject_does_not_warn() {
        // Two genuinely different statutes that share "Sindh" and "Act" must not
        // trip the warning, or it becomes noise the admin learns to ignore.
        assert!(
            title_similarity(
                "Sindh Local Government Act 2013.pdf",
                "Sindh Civil Servants Act 1973.pdf",
            ) < SIMILAR_TITLE_THRESHOLD
        );
    }

    #[test]
    fn empty_or_extensionless_titles_never_panic_or_match() {
        assert_eq!(title_similarity("", "Constitution.pdf"), 0.0);
        assert_eq!(title_similarity(".pdf", "Constitution.pdf"), 0.0);
    }

    #[test]
    fn page_count_is_none_for_non_pdf_so_byte_cap_governs() {
        // A non-PDF name never triggers the page cap — only the byte cap applies.
        assert_eq!(pdf_page_count("notice.docx", b"whatever"), None);
        assert_eq!(pdf_page_count("scan.PNG", b"\x89PNG"), None);
    }

    #[test]
    fn page_count_is_none_for_unreadable_pdf() {
        // An unparseable/encrypted .pdf must not panic and must fall through to
        // the byte cap (None), never a fabricated page count.
        assert_eq!(pdf_page_count("broken.pdf", b"%PDF-1.7 not really a pdf"), None);
    }

    #[test]
    fn default_attach_limits_are_15_pages_8_mb() {
        // Guards the coupling with extract::MAX_DOC_CHARS (must read ~15 pages whole).
        // Only asserts defaults when the env overrides are unset.
        if std::env::var("NOVIQ_ATTACH_MAX_PAGES").is_err()
            && std::env::var("NOVIQ_ATTACH_MAX_MB").is_err()
        {
            assert_eq!(attach_limits(), (15, 8 * 1024 * 1024));
        }
    }
}
