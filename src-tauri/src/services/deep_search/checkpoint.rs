//! Feature 026 (Part 1) — durable checkpoints for the deep-search loop.
//!
//! ALL access to `deep_checkpoint` lives here (repository seam, mirrors
//! `drafting/one_ask/state.rs`). One row per completed node; `parent_id` chains
//! them into the branch DAG used for time-travel. Reads power RESUME (`latest`)
//! and the time-travel picker (`history` / `load`). Writes are best-effort at the
//! call site — a failed checkpoint must never fail the user's turn.

use serde::{Deserialize, Serialize};

use crate::db::Db;

use super::state::DeepState;

/// Feature flag (mirrors `agentic_enabled` / `graph_expand_enabled`). When OFF
/// (default) the loop runs exactly as before and writes NO checkpoints; resume /
/// time-travel are dormant. Turn on to make deep turns durable + resumable.
pub fn checkpoint_enabled() -> bool {
    matches!(
        std::env::var("ECHO_DEEP_CHECKPOINT")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "on" | "true" | "yes"
    )
}

/// One persisted checkpoint row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub thread_id: String,
    pub seq: i64,
    pub node: String,
    pub parent_id: Option<String>,
    pub state: DeepState,
    pub status: String,
    pub created_at: i64,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Append a checkpoint after a node completes. Returns the new checkpoint id so
/// the driver can thread it as the next row's `parent_id`. `status` is one of
/// `running` | `interrupted` | `done` | `error`.
pub async fn append(
    pool: &Db,
    seq: i64,
    parent_id: Option<&str>,
    state: &DeepState,
    status: &str,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO deep_checkpoint \
             (id, thread_id, seq, node, parent_id, state_json, status, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(&state.thread_id)
    .bind(seq)
    .bind(state.node.as_str())
    .bind(parent_id)
    .bind(&json)
    .bind(status)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(id)
}

fn decode(
    id: String,
    thread_id: String,
    seq: i64,
    node: String,
    parent_id: Option<String>,
    json: String,
    status: String,
    created_at: i64,
) -> Result<Checkpoint, String> {
    let state: DeepState = serde_json::from_str(&json)
        .map_err(|e| format!("corrupt deep_checkpoint state for {id}: {e}"))?;
    Ok(Checkpoint { id, thread_id, seq, node, parent_id, state, status, created_at })
}

/// The most-recent checkpoint for a thread (highest `seq`) — whatever its status.
/// `Ok(None)` when the thread has no checkpoints. Used for history/inspection; the
/// resume path wants [`latest_resumable`], not this.
pub async fn latest(pool: &Db, thread_id: &str) -> Result<Option<Checkpoint>, String> {
    let row: Option<(String, String, i64, String, Option<String>, String, String, i64)> =
        sqlx::query_as(
            "SELECT id, thread_id, seq, node, parent_id, state_json, status, created_at \
             FROM deep_checkpoint WHERE thread_id = $1 ORDER BY seq DESC LIMIT 1",
        )
        .bind(thread_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    match row {
        None => Ok(None),
        Some((id, t, s, n, p, j, st, c)) => decode(id, t, s, n, p, j, st, c).map(Some),
    }
}

/// The resume anchor: the most-recent checkpoint for a thread **only if it is
/// `interrupted`** — i.e. a turn that was actually stopped part-way and has work
/// worth continuing.
///
/// The status filter is load-bearing. Without it, re-asking a question whose thread
/// already ran to completion would "resume" that `done` checkpoint and instantly
/// replay the stored answer — an accidental cache, not a resume. A finished thread
/// must re-run fresh; only an interrupted one continues.
pub async fn latest_resumable(pool: &Db, thread_id: &str) -> Result<Option<Checkpoint>, String> {
    let row: Option<(String, String, i64, String, Option<String>, String, String, i64)> =
        sqlx::query_as(
            "SELECT id, thread_id, seq, node, parent_id, state_json, status, created_at \
             FROM deep_checkpoint WHERE thread_id = $1 ORDER BY seq DESC LIMIT 1",
        )
        .bind(thread_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    match row {
        None => Ok(None),
        // Only the LATEST row decides: if the thread's newest checkpoint is `done`,
        // the thread is finished — an older `interrupted` row behind it is history,
        // not a resume anchor.
        Some((_, _, _, _, _, _, ref st, _)) if st != "interrupted" => Ok(None),
        Some((id, t, s, n, p, j, st, c)) => decode(id, t, s, n, p, j, st, c).map(Some),
    }
}

/// One checkpoint by id — the time-travel entry point (`resume_from`).
pub async fn load(pool: &Db, checkpoint_id: &str) -> Result<Option<Checkpoint>, String> {
    let row: Option<(String, String, i64, String, Option<String>, String, String, i64)> =
        sqlx::query_as(
            "SELECT id, thread_id, seq, node, parent_id, state_json, status, created_at \
             FROM deep_checkpoint WHERE id = $1",
        )
        .bind(checkpoint_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    match row {
        None => Ok(None),
        Some((id, t, s, n, p, j, st, c)) => decode(id, t, s, n, p, j, st, c).map(Some),
    }
}

/// Every checkpoint for a thread, oldest→newest — the time-travel picker feed.
pub async fn history(pool: &Db, thread_id: &str) -> Result<Vec<Checkpoint>, String> {
    let rows: Vec<(String, String, i64, String, Option<String>, String, String, i64)> =
        sqlx::query_as(
            "SELECT id, thread_id, seq, node, parent_id, state_json, status, created_at \
             FROM deep_checkpoint WHERE thread_id = $1 ORDER BY seq ASC",
        )
        .bind(thread_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, t, s, n, p, j, st, c) in rows {
        if let Ok(ckpt) = decode(id, t, s, n, p, j, st, c) {
            out.push(ckpt);
        }
    }
    Ok(out)
}

/// Next free `seq` for a thread = max(seq)+1 (so a time-travel branch appends
/// after the existing rows rather than colliding). 0 for a fresh thread.
pub async fn next_seq(pool: &Db, thread_id: &str) -> Result<i64, String> {
    let max: Option<i64> =
        sqlx::query_scalar("SELECT MAX(seq) FROM deep_checkpoint WHERE thread_id = $1")
            .bind(thread_id)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(max.map(|m| m + 1).unwrap_or(0))
}

/// Drop every checkpoint for a thread (housekeeping once a turn is truly done and
/// resume is no longer wanted; not called on the happy path so history survives
/// for time-travel).
pub async fn delete_thread(pool: &Db, thread_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM deep_checkpoint WHERE thread_id = $1")
        .bind(thread_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;
    use crate::services::deep_search::state::Node;

    fn sample(thread: &str, node: Node) -> DeepState {
        let mut s = DeepState::fresh(
            thread.to_string(),
            "grounds, penalties, and appeal under E&D Rules".into(),
            "system".into(),
            Some(vec!["src-1".into()]),
        );
        s.node = node;
        s.answer = "partial".into();
        s
    }

    #[tokio::test]
    async fn append_latest_roundtrip_and_ordering() {
        let pool = test_pool().await;
        let t = "conv-a:1";
        let p0 = append(&pool, 0, None, &sample(t, Node::Expanded), "running").await.unwrap();
        let _p1 = append(&pool, 1, Some(&p0), &sample(t, Node::RetrievedR1), "running").await.unwrap();

        let latest = latest(&pool, t).await.unwrap().unwrap();
        assert_eq!(latest.seq, 1);
        assert_eq!(latest.node, "retrieve_r1");
        assert_eq!(latest.state.node, Node::RetrievedR1);
        // state_json round-trips the inputs needed to resume.
        assert_eq!(latest.state.scope_source_ids.as_deref(), Some(&["src-1".to_string()][..]));

        let hist = history(&pool, t).await.unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].seq, 0);
        assert_eq!(hist[1].parent_id.as_deref(), Some(p0.as_str()));

        assert_eq!(next_seq(&pool, t).await.unwrap(), 2);
        assert_eq!(next_seq(&pool, "conv-unknown:9").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn latest_resumable_only_resumes_an_interrupted_thread() {
        let pool = test_pool().await;

        // Nothing checkpointed → nothing to resume.
        assert!(latest_resumable(&pool, "conv-r:none").await.unwrap().is_none());

        // A thread stopped part-way → resumable, anchored at the interrupted node.
        let t_int = "conv-r:interrupted";
        let p0 = append(&pool, 0, None, &sample(t_int, Node::Expanded), "running").await.unwrap();
        append(&pool, 1, Some(&p0), &sample(t_int, Node::RetrievedR1), "interrupted").await.unwrap();
        let r = latest_resumable(&pool, t_int).await.unwrap().expect("interrupted thread resumes");
        assert_eq!(r.seq, 1);
        assert_eq!(r.state.node, Node::RetrievedR1);

        // A thread that RAN TO COMPLETION must NOT resume — otherwise re-asking the
        // question would replay the stored answer (accidental cache, not resume).
        let t_done = "conv-r:done";
        let d0 = append(&pool, 0, None, &sample(t_done, Node::Synthesized), "interrupted").await.unwrap();
        append(&pool, 1, Some(&d0), &sample(t_done, Node::Done), "done").await.unwrap();
        assert!(
            latest_resumable(&pool, t_done).await.unwrap().is_none(),
            "a completed thread must re-run fresh, even though an older interrupted row exists behind it",
        );
        // …while plain `latest` still sees it (history/inspection is unaffected).
        assert_eq!(latest(&pool, t_done).await.unwrap().unwrap().status, "done");
    }

    #[tokio::test]
    async fn load_by_id_powers_time_travel_and_threads_are_isolated() {
        let pool = test_pool().await;
        let a = append(&pool, 0, None, &sample("conv-x:1", Node::CoverageChecked), "running").await.unwrap();
        let _b = append(&pool, 0, None, &sample("conv-y:1", Node::Expanded), "running").await.unwrap();

        let picked = load(&pool, &a).await.unwrap().unwrap();
        assert_eq!(picked.thread_id, "conv-x:1");
        assert_eq!(picked.state.node, Node::CoverageChecked);
        assert!(load(&pool, "no-such-id").await.unwrap().is_none());

        // Threads don't leak into each other.
        assert_eq!(history(&pool, "conv-x:1").await.unwrap().len(), 1);
        delete_thread(&pool, "conv-x:1").await.unwrap();
        assert!(latest(&pool, "conv-x:1").await.unwrap().is_none());
        assert!(latest(&pool, "conv-y:1").await.unwrap().is_some());
    }
}
