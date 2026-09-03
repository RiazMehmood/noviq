//! Feature 009 (Conversational session, Phase A) — durable chat transcript.
//!
//! Append-only persistence of every officer/system turn (FR-031) with ordered
//! retrieval for resume (FR-032). No compaction here — the rolling summary
//! (`conversation_summary`, FR-033/034) is Phase C. Rows are never deleted.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db::Db;

/// One stored conversational turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub seq: i64,
    pub role: String, // "officer" | "system"
    pub content: String,
    pub created_at: String,
}

/// Append one turn; returns its assigned `seq`. Per-session sequence is dense and
/// monotonic (MAX(seq)+1, 0 for the first turn). Rows are never deleted (FR-031);
/// a rough token estimate is stored for the Phase-C compaction accounting.
pub async fn append_message(
    pool: &Db,
    draft_id: &str,
    role: &str,
    content: &str,
) -> Result<i64> {
    let seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq) + 1, 0) FROM chat_message WHERE draft_session_id = $1",
    )
    .bind(draft_id)
    .fetch_one(pool)
    .await?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let token_count = (content.len() / 4) as i64; // coarse estimate; refined in Phase C
    sqlx::query(
        "INSERT INTO chat_message
            (id, draft_session_id, seq, role, content, token_count, summarized, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, 0, $7)",
    )
    .bind(&id)
    .bind(draft_id)
    .bind(seq)
    .bind(role)
    .bind(content)
    .bind(token_count)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(seq)
}

/// Full transcript for a draft, ordered by `seq` (FR-032). All rows are returned
/// regardless of `summarized` — the officer always sees the complete history.
pub async fn load_conversation(pool: &Db, draft_id: &str) -> Result<Vec<ChatMessage>> {
    let rows = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT seq, role, content, created_at FROM chat_message
         WHERE draft_session_id = $1 ORDER BY seq ASC",
    )
    .bind(draft_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(seq, role, content, created_at)| ChatMessage {
            seq,
            role,
            content,
            created_at,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    #[tokio::test]
    async fn append_assigns_monotonic_seq() {
        let pool = test_pool().await;
        let s0 = append_message(&pool, "d1", "system", "Who is the addressee?").await.unwrap();
        let s1 = append_message(&pool, "d1", "officer", "Chief Secretary").await.unwrap();
        let s2 = append_message(&pool, "d1", "system", "What is the purpose?").await.unwrap();
        assert_eq!((s0, s1, s2), (0, 1, 2));
    }

    #[tokio::test]
    async fn load_returns_messages_in_order() {
        let pool = test_pool().await;
        append_message(&pool, "d1", "system", "Q1").await.unwrap();
        append_message(&pool, "d1", "officer", "A1").await.unwrap();
        append_message(&pool, "d1", "system", "Q2").await.unwrap();
        let msgs = load_conversation(&pool, "d1").await.unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "Q1");
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[1].content, "A1");
        assert_eq!(msgs[2].content, "Q2");
        assert_eq!(msgs[2].seq, 2);
    }

    #[tokio::test]
    async fn sessions_are_isolated() {
        let pool = test_pool().await;
        append_message(&pool, "d1", "officer", "x").await.unwrap();
        append_message(&pool, "d2", "officer", "y").await.unwrap();
        assert_eq!(load_conversation(&pool, "d1").await.unwrap().len(), 1);
        assert_eq!(load_conversation(&pool, "d2").await.unwrap().len(), 1);
    }

    /// T052 — driving the three intent answers through the same parse→apply loop
    /// the `send_chat_turn` command uses yields the complete intent that a direct
    /// `start_draft` call would receive, and every turn is persisted (FR-027/031).
    /// (Full retrieval/candidate-doc parity is the manual quickstart check, T062,
    /// since it needs an embedder + indexed corpus.)
    #[tokio::test]
    async fn intent_loop_completes_and_persists() {
        use crate::services::drafting::chat::{self, DraftAction};
        use crate::services::drafting::IntentAnswers;

        let pool = test_pool().await;
        let draft_id = "conv1";
        let mut intent = IntentAnswers {
            instrument_type: "summary".into(),
            addressee: String::new(),
            purpose: String::new(),
            scheme: String::new(),
        };
        let answers = ["Chief Secretary, Sindh", "Approval to use ADP funds", "Karachi Water Supply"];
        for ans in answers {
            // system asks the current question
            let field = chat::next_field(&intent).expect("a question remains");
            append_message(&pool, draft_id, "system", chat::question_for(field)).await.unwrap();
            // officer answers
            append_message(&pool, draft_id, "officer", ans).await.unwrap();
            match chat::parse_turn(&intent, ans) {
                DraftAction::SetIntent { field, value } => chat::apply(&mut intent, field, value),
                DraftAction::Clarify => panic!("answer should map to an intent field"),
            }
        }
        assert!(chat::intent_complete(&intent));
        assert_eq!(intent.addressee, "Chief Secretary, Sindh");
        assert_eq!(intent.purpose, "Approval to use ADP funds");
        assert_eq!(intent.scheme, "Karachi Water Supply");

        let msgs = load_conversation(&pool, draft_id).await.unwrap();
        assert_eq!(msgs.len(), 6, "3 system questions + 3 officer answers persisted");
        assert_eq!(msgs.iter().filter(|m| m.role == "officer").count(), 3);
    }
}
