//! Repository: `answer_feedback` table (Feature 018 — Answer Feedback Loop).
//!
//! One CURRENT row per `(message_id, user)`; a re-tap SUPERSEDES the prior row
//! (history retained). The two citation questions are answered independently, so
//! either column may be NULL and a later tap MERGES into a fresh current row.
//!
//! Anchor is `chat_message.id` — the assistant turn the client already holds as
//! `TurnResponse.message_id`. Writes are best-effort at the call site: a feedback
//! failure must never break the user's chat turn.

use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::db::Db;

/// The current (accuracy, completeness) selection for one message+user, used to
/// re-hydrate the UI. Codes: accuracy ∈ {all,some,wrong,unsure};
/// completeness ∈ {yes,missing,na}.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CurrentFeedback {
    pub citation_accuracy: Option<String>,
    pub citation_completeness: Option<String>,
}

/// One `(value, count)` bucket in an aggregate distribution (FR-009).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CategoryCount {
    pub value: String,
    pub count: i64,
}

/// Aggregate distribution of both citation metrics over CURRENT rows.
#[derive(Debug, Clone, Serialize)]
pub struct FeedbackSummary {
    pub accuracy: Vec<CategoryCount>,
    pub completeness: Vec<CategoryCount>,
    pub total_current: i64,
}

/// Insert or update citation feedback for one answered turn.
///
/// Latest-write-wins with history: if a current row exists for
/// `(message_id, user_id)`, the still-set other column is carried forward, the
/// prior row is stamped `superseded_at`, and a new current row is inserted with
/// `supersedes` pointing at it. At least one of `accuracy`/`completeness` MUST be
/// `Some` (the caller validates; this guards the invariant too).
pub async fn upsert(
    pool: &Db,
    message_id: &str,
    conversation_id: Option<&str>,
    user_id: Option<&str>,
    accuracy: Option<&str>,
    completeness: Option<&str>,
) -> Result<()> {
    if accuracy.is_none() && completeness.is_none() {
        anyhow::bail!("answer_feedback: at least one of accuracy/completeness required");
    }

    let mut tx = pool.begin().await?;

    // Lock the current row (if any) so a concurrent second tap can't race us into
    // two current rows (the partial unique index would also reject that).
    let existing: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, citation_accuracy, citation_completeness \
         FROM answer_feedback \
         WHERE message_id = $1 AND COALESCE(user_id, '') = COALESCE($2, '') \
           AND superseded_at IS NULL \
         FOR UPDATE",
    )
    .bind(message_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;

    let now = Utc::now().timestamp_millis();
    let (merged_acc, merged_comp, supersedes) = match existing {
        Some((old_id, old_acc, old_comp)) => {
            let acc = accuracy.map(str::to_string).or(old_acc);
            let comp = completeness.map(str::to_string).or(old_comp);
            // Retire the prior current row BEFORE inserting the new one so the
            // partial unique index (over superseded_at IS NULL) never conflicts.
            sqlx::query("UPDATE answer_feedback SET superseded_at = $1 WHERE id = $2")
                .bind(now)
                .bind(&old_id)
                .execute(&mut *tx)
                .await?;
            (acc, comp, Some(old_id))
        }
        None => (
            accuracy.map(str::to_string),
            completeness.map(str::to_string),
            None,
        ),
    };

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO answer_feedback \
         (id, message_id, conversation_id, user_id, citation_accuracy, \
          citation_completeness, supersedes, superseded_at, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, $8)",
    )
    .bind(&id)
    .bind(message_id)
    .bind(conversation_id)
    .bind(user_id)
    .bind(&merged_acc)
    .bind(&merged_comp)
    .bind(&supersedes)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// The current feedback selection for one message+user (for UI re-hydration).
pub async fn current_for(
    pool: &Db,
    message_id: &str,
    user_id: Option<&str>,
) -> Result<Option<CurrentFeedback>> {
    Ok(sqlx::query_as::<_, CurrentFeedback>(
        "SELECT citation_accuracy, citation_completeness \
         FROM answer_feedback \
         WHERE message_id = $1 AND COALESCE(user_id, '') = COALESCE($2, '') \
           AND superseded_at IS NULL \
         LIMIT 1",
    )
    .bind(message_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?)
}

/// Aggregate distribution of both citation metrics over CURRENT rows, optionally
/// bounded to `[from, to]` epoch-ms on `created_at` (FR-009).
pub async fn summary(pool: &Db, from: Option<i64>, to: Option<i64>) -> Result<FeedbackSummary> {
    let accuracy = sqlx::query_as::<_, CategoryCount>(
        "SELECT citation_accuracy AS value, COUNT(*)::int8 AS count \
         FROM answer_feedback \
         WHERE superseded_at IS NULL AND citation_accuracy IS NOT NULL \
           AND ($1::int8 IS NULL OR created_at >= $1) \
           AND ($2::int8 IS NULL OR created_at <= $2) \
         GROUP BY citation_accuracy",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;

    let completeness = sqlx::query_as::<_, CategoryCount>(
        "SELECT citation_completeness AS value, COUNT(*)::int8 AS count \
         FROM answer_feedback \
         WHERE superseded_at IS NULL AND citation_completeness IS NOT NULL \
           AND ($1::int8 IS NULL OR created_at >= $1) \
           AND ($2::int8 IS NULL OR created_at <= $2) \
         GROUP BY citation_completeness",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;

    let total_current: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::int8 FROM answer_feedback \
         WHERE superseded_at IS NULL \
           AND ($1::int8 IS NULL OR created_at >= $1) \
           AND ($2::int8 IS NULL OR created_at <= $2)",
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;

    Ok(FeedbackSummary {
        accuracy,
        completeness,
        total_current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    async fn pool() -> Db {
        crate::db::test_pool().await
    }

    /// How many CURRENT (non-superseded) rows exist for a message+user.
    async fn current_count(p: &Db, message_id: &str, user_id: Option<&str>) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)::int8 FROM answer_feedback \
             WHERE message_id = $1 AND COALESCE(user_id, '') = COALESCE($2, '') \
               AND superseded_at IS NULL",
        )
        .bind(message_id)
        .bind(user_id)
        .fetch_one(p)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn upsert_requires_at_least_one_value() {
        let p = pool().await;
        let err = upsert(&p, "m1", None, Some("u1"), None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("at least one"));
    }

    #[tokio::test]
    async fn insert_single_metric_creates_current_row() {
        let p = pool().await;
        upsert(&p, "m1", None, Some("u1"), Some("all"), None)
            .await
            .unwrap();

        let cur = current_for(&p, "m1", Some("u1")).await.unwrap().unwrap();
        assert_eq!(cur.citation_accuracy.as_deref(), Some("all"));
        assert_eq!(cur.citation_completeness, None);
        assert_eq!(current_count(&p, "m1", Some("u1")).await, 1);
    }

    #[tokio::test]
    async fn re_tap_supersedes_and_keeps_one_current() {
        let p = pool().await;
        upsert(&p, "m1", None, Some("u1"), Some("all"), None)
            .await
            .unwrap();
        // Re-tap the SAME metric with a new value.
        upsert(&p, "m1", None, Some("u1"), Some("wrong"), None)
            .await
            .unwrap();

        let cur = current_for(&p, "m1", Some("u1")).await.unwrap().unwrap();
        assert_eq!(cur.citation_accuracy.as_deref(), Some("wrong"));
        // Exactly one CURRENT row; the prior "all" row is retained but superseded.
        assert_eq!(current_count(&p, "m1", Some("u1")).await, 1);
        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*)::int8 FROM answer_feedback WHERE message_id = 'm1'")
                .fetch_one(&p)
                .await
                .unwrap();
        assert_eq!(total, 2, "history retained (1 current + 1 superseded)");
    }

    #[tokio::test]
    async fn two_metric_taps_merge_into_one_current_row() {
        let p = pool().await;
        // First tap: accuracy only.
        upsert(&p, "m1", None, Some("u1"), Some("some"), None)
            .await
            .unwrap();
        // Second tap: completeness only — must carry forward the accuracy value.
        upsert(&p, "m1", None, Some("u1"), None, Some("missing"))
            .await
            .unwrap();

        let cur = current_for(&p, "m1", Some("u1")).await.unwrap().unwrap();
        assert_eq!(cur.citation_accuracy.as_deref(), Some("some"));
        assert_eq!(cur.citation_completeness.as_deref(), Some("missing"));
        assert_eq!(current_count(&p, "m1", Some("u1")).await, 1);
    }

    #[tokio::test]
    async fn summary_counts_current_rows_only() {
        let p = pool().await;
        // u1: all + yes
        upsert(&p, "m1", None, Some("u1"), Some("all"), Some("yes"))
            .await
            .unwrap();
        // u2: some (then superseded by wrong)
        upsert(&p, "m2", None, Some("u2"), Some("some"), None)
            .await
            .unwrap();
        upsert(&p, "m2", None, Some("u2"), Some("wrong"), None)
            .await
            .unwrap();

        let s = summary(&p, None, None).await.unwrap();
        assert_eq!(s.total_current, 2);

        let acc: std::collections::HashMap<_, _> = s
            .accuracy
            .iter()
            .map(|c| (c.value.clone(), c.count))
            .collect();
        // "some" was superseded → not counted; "all" and "wrong" each once.
        assert_eq!(acc.get("all"), Some(&1));
        assert_eq!(acc.get("wrong"), Some(&1));
        assert_eq!(acc.get("some"), None);

        let comp: std::collections::HashMap<_, _> = s
            .completeness
            .iter()
            .map(|c| (c.value.clone(), c.count))
            .collect();
        assert_eq!(comp.get("yes"), Some(&1));
    }
}
