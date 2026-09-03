//! One-ask session state (FR-010, research R7, ADR-0014).
//!
//! ALL access to `one_ask_session` lives here (repository-shaped seam). The
//! state is a versioned serde JSON blob — maps 1:1 to Postgres JSONB later.
//! SQL stays ANSI-portable: TEXT ISO-8601 timestamps, TEXT uuid ids,
//! `ON CONFLICT` upsert (not SQLite `INSERT OR REPLACE`).

use super::skeleton::SectionId;
use serde::{Deserialize, Serialize};
use crate::db::Db;

/// Pipeline status (data-model state transitions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "stage")]
pub enum Status {
    Researching,
    Composing { section: usize },
    Verifying,
    Ready,
    Failed { reason: String },
    Cancelled,
}

/// Officer-stated figure, verbatim from the ask (FR-003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Figure {
    pub text: String,
    #[serde(default)]
    pub normalized: Option<String>,
}

/// Parsed one-sentence ask (research R6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OneAskIntent {
    pub raw: String,
    #[serde(default)]
    pub figures: Vec<Figure>,
    #[serde(default)]
    pub action: Option<String>,
}

/// Verbatim provision extract — the ONLY permitted citation source (FR-004/005).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionCard {
    pub chunk_id: String,
    pub source_id: String,
    pub source_title: String,
    /// Verbatim chunk text (or sub-span) — gate-checked at construction.
    pub excerpt: String,
    /// Deterministically resolved provision identifier; None ⇒ fast-tier card
    /// (renders with the §XI "hierarchical detail unavailable" disclosure).
    #[serde(default)]
    pub label: Option<String>,
    /// Page reference for the §XI fast-tier citation form, when known.
    #[serde(default)]
    pub page: Option<i64>,
    /// Which skeleton sections retrieved this card (deduped across sections).
    #[serde(default)]
    pub section_ids: Vec<SectionId>,
}

/// One inline run of a paragraph (data-model tagged enum).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum Run {
    /// Model narrative, post-gate (contains no foreign numbers).
    Prose { text: String },
    /// Spliced from `cards[card_idx]`; `verified` MUST be true to render.
    Citation {
        card_idx: usize,
        rendered_label: String,
        quoted: Option<String>,
        verified: bool,
    },
    /// Officer-stated figure, verbatim from the intent.
    Figure { fig_idx: usize, text: String },
    /// Unverifiable content — visually distinct, never silent (FR-007).
    Flag {
        reason: String,
        original: Option<String>,
    },
}

/// One numbered paragraph of the draft body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paragraph {
    pub number: u32,
    pub section: SectionId,
    pub runs: Vec<Run>,
}

/// Root aggregate persisted as `one_ask_session.state_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OneAskState {
    #[serde(default = "default_version")]
    pub version: u32,
    pub status: Status,
    pub intent: OneAskIntent,
    #[serde(default)]
    pub cards: Vec<ProvisionCard>,
    #[serde(default)]
    pub paragraphs: Vec<Paragraph>,
    #[serde(default)]
    pub finalized: bool,
}

fn default_version() -> u32 {
    1
}

impl OneAskState {
    pub fn new(intent: OneAskIntent) -> Self {
        Self {
            version: 1,
            status: Status::Researching,
            intent,
            cards: Vec::new(),
            paragraphs: Vec::new(),
            finalized: false,
        }
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Upsert the session state (ANSI `ON CONFLICT`, ADR-0014).
pub async fn save(pool: &Db, draft_id: &str, state: &OneAskState) -> Result<(), String> {
    let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO one_ask_session (draft_id, state_json, updated_at) \
         VALUES ($1, $2, $3) \
         ON CONFLICT(draft_id) DO UPDATE SET state_json = $2, updated_at = $3",
    )
    .bind(draft_id)
    .bind(&json)
    .bind(now_iso())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Load a session; `Ok(None)` when the draft id is unknown.
pub async fn load(pool: &Db, draft_id: &str) -> Result<Option<OneAskState>, String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT state_json FROM one_ask_session WHERE draft_id = $1")
            .bind(draft_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    match row {
        None => Ok(None),
        Some((json,)) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| format!("corrupt one_ask_session state for {draft_id}: {e}")),
    }
}

/// List sessions for the resume picker (newest first).
pub async fn list(pool: &Db) -> Result<Vec<(String, OneAskState, String)>, String> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT draft_id, state_json, updated_at FROM one_ask_session \
         ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, json, updated) in rows {
        if let Ok(state) = serde_json::from_str::<OneAskState>(&json) {
            out.push((id, state, updated));
        }
    }
    Ok(out)
}

pub async fn delete(pool: &Db, draft_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM one_ask_session WHERE draft_id = $1")
        .bind(draft_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    fn sample_state() -> OneAskState {
        let mut s = OneAskState::new(OneAskIntent {
            raw: "I want to get 4 billion to develop schools".into(),
            figures: vec![Figure {
                text: "4 billion".into(),
                normalized: None,
            }],
            action: Some("get".into()),
        });
        s.status = Status::Composing { section: 2 };
        s.cards.push(ProvisionCard {
            chunk_id: "c1".into(),
            source_id: "s1".into(),
            source_title: "Delegation of Financial Powers 2019".into(),
            excerpt: "Rule 7(2) The competent authority may sanction…".into(),
            label: Some("Rule 7(2), Delegation of Financial Powers 2019".into()),
            page: Some(12),
            section_ids: vec![SectionId::FinancialAspect],
        });
        s.paragraphs.push(Paragraph {
            number: 1,
            section: SectionId::Urgency,
            runs: vec![
                Run::Prose {
                    text: "The matter pertains to…".into(),
                },
                Run::Figure {
                    fig_idx: 0,
                    text: "4 billion".into(),
                },
            ],
        });
        s
    }

    #[tokio::test]
    async fn save_load_roundtrip_preserves_state() {
        let pool = test_pool().await;
        let s = sample_state();
        save(&pool, "draft-1", &s).await.unwrap();
        let loaded = load(&pool, "draft-1").await.unwrap().unwrap();
        assert_eq!(loaded.status, s.status);
        assert_eq!(loaded.intent, s.intent);
        assert_eq!(loaded.cards, s.cards);
        assert_eq!(loaded.paragraphs, s.paragraphs);
        assert!(!loaded.finalized);

        // upsert path
        let mut s2 = s.clone();
        s2.status = Status::Ready;
        save(&pool, "draft-1", &s2).await.unwrap();
        let again = load(&pool, "draft-1").await.unwrap().unwrap();
        assert_eq!(again.status, Status::Ready);

        // list + delete
        assert_eq!(list(&pool).await.unwrap().len(), 1);
        delete(&pool, "draft-1").await.unwrap();
        assert!(load(&pool, "draft-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn unknown_draft_loads_none() {
        let pool = test_pool().await;
        assert!(load(&pool, "nope").await.unwrap().is_none());
    }

    #[test]
    fn tolerates_future_fields_and_missing_defaults() {
        // Future writer added a field + omitted optional ones — must still load.
        let json = r#"{
            "version": 2,
            "status": {"stage":"ready"},
            "intent": {"raw":"x"},
            "futureField": {"anything": true}
        }"#;
        let s: OneAskState = serde_json::from_str(json).unwrap();
        assert_eq!(s.version, 2);
        assert_eq!(s.status, Status::Ready);
        assert!(s.cards.is_empty() && s.paragraphs.is_empty() && !s.finalized);
    }
}
