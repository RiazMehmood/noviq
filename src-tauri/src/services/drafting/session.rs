//! Feature 009 (US5, T044) — persistent, revisable draft session.
//!
//! A draft is a living working state (FR-023): selected template, intent,
//! candidate + confirmed document sets, the current rendered blocks, per-prose
//! officer edits, gap resolutions, and a `finalized` flag. It survives multiple
//! revision turns and can be resumed after restart (persisted to `draft_session`,
//! migration 022, as one JSON blob). The in-memory copy is the live working set;
//! every mutation is written through to the DB so a resumed draft is faithful.

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db::Db;

use super::{Block, CorpusDocRef, GapResolution, IntentAnswers};

/// The full per-draft working state (serialised 1:1 into `draft_session`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftState {
    pub template_id: i64,
    pub intent: IntentAnswers,
    pub candidate_docs: Vec<CorpusDocRef>,
    pub confirmed_docs: Vec<CorpusDocRef>,
    /// The current rendered blocks (empty until first render). Hand-edited prose
    /// is captured here AND in `prose_edits` so it survives unrelated re-renders.
    #[serde(default)]
    pub blocks: Vec<Block>,
    /// Officer free-text prose edits, keyed by slot `ordering` (FR-025: prose
    /// only). A revision that re-renders a prose slot keeps the edit unless the
    /// revision IS a regenerate/edit of that same slot.
    #[serde(default)]
    pub prose_edits: HashMap<i64, String>,
    /// Per-gapped-slot officer decisions, keyed by slot `ordering` (US4).
    #[serde(default)]
    pub gap_resolutions: HashMap<i64, GapResolution>,
    /// Once finalized the draft is locked: no further revision, export-only.
    #[serde(default)]
    pub finalized: bool,
}

impl DraftState {
    /// A fresh session from a template + intent + proposed candidates (start_draft).
    pub fn new(template_id: i64, intent: IntentAnswers, candidate_docs: Vec<CorpusDocRef>) -> Self {
        Self {
            template_id,
            intent,
            candidate_docs,
            confirmed_docs: Vec::new(),
            blocks: Vec::new(),
            prose_edits: HashMap::new(),
            gap_resolutions: HashMap::new(),
            finalized: false,
        }
    }
}

/// Write-through persist (insert or replace). Called after every mutation so an
/// unfinished draft can be resumed (FR-023).
pub async fn save(pool: &Db, draft_id: &str, state: &DraftState) -> Result<()> {
    let state_json = serde_json::to_string(state)?;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO draft_session (draft_id, state_json, finalized, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $4)
         ON CONFLICT(draft_id) DO UPDATE SET
            state_json = excluded.state_json,
            finalized  = excluded.finalized,
            updated_at = excluded.updated_at",
    )
    .bind(draft_id)
    .bind(&state_json)
    .bind(state.finalized as i64)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Load a persisted draft (for resume / cache-miss). `None` if unknown.
pub async fn load(pool: &Db, draft_id: &str) -> Result<Option<DraftState>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT state_json FROM draft_session WHERE draft_id = $1")
            .bind(draft_id)
            .fetch_optional(pool)
            .await?;
    match row {
        Some((json,)) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

/// Remove a persisted draft (e.g. discard). Idempotent.
#[allow(dead_code)]
pub async fn delete(pool: &Db, draft_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM draft_session WHERE draft_id = $1")
        .bind(draft_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;
    use crate::services::drafting::DocRole;

    fn sample() -> DraftState {
        let mut s = DraftState::new(
            7,
            IntentAnswers {
                instrument_type: "summary".into(),
                addressee: "Chief Secretary".into(),
                purpose: "release ADP funds".into(),
                scheme: "Water Scheme".into(),
            },
            vec![CorpusDocRef {
                doc_id: "rob".into(),
                title: "Rules of Business".into(),
                source_path: "/x/rob.pdf".into(),
                role: DocRole::Rules,
            }],
        );
        s.blocks = vec![Block::Prose { label: None, text: "edited narrative".into() }];
        s.prose_edits.insert(2, "edited narrative".into());
        s
    }

    #[tokio::test]
    async fn save_load_roundtrip_preserves_blocks_and_edits() {
        let pool = test_pool().await;
        let state = sample();
        save(&pool, "d1", &state).await.unwrap();
        let loaded = load(&pool, "d1").await.unwrap().expect("present");
        assert_eq!(loaded.template_id, 7);
        assert_eq!(loaded.blocks.len(), 1);
        assert_eq!(loaded.prose_edits.get(&2).unwrap(), "edited narrative");
        assert!(!loaded.finalized);
    }

    #[tokio::test]
    async fn save_is_upsert_and_tracks_finalized() {
        let pool = test_pool().await;
        let mut state = sample();
        save(&pool, "d1", &state).await.unwrap();
        state.finalized = true;
        save(&pool, "d1", &state).await.unwrap(); // overwrite
        let loaded = load(&pool, "d1").await.unwrap().unwrap();
        assert!(loaded.finalized);
        let (fin,): (i64,) = sqlx::query_as("SELECT finalized FROM draft_session WHERE draft_id='d1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(fin, 1);
    }

    #[tokio::test]
    async fn load_unknown_is_none() {
        let pool = test_pool().await;
        assert!(load(&pool, "nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn officer_supplied_and_suggestion_resolutions_roundtrip() {
        // Phase E (FR-008a / FR-019): the new GapResolution variants must survive
        // save/load so an officer-typed value is resume-safe (FR-032).
        let pool = test_pool().await;
        let mut state = sample();
        state.gap_resolutions.insert(
            5,
            GapResolution::OfficerSupplied {
                value: "Rs. 500 billion for 100 schools".into(),
            },
        );
        state.gap_resolutions.insert(
            7,
            GapResolution::AcceptedSuggestion { doc_ids: vec!["rob".into()] },
        );
        save(&pool, "d2", &state).await.unwrap();
        let loaded = load(&pool, "d2").await.unwrap().expect("present");
        match loaded.gap_resolutions.get(&5) {
            Some(GapResolution::OfficerSupplied { value }) => {
                assert_eq!(value, "Rs. 500 billion for 100 schools");
            }
            other => panic!("expected OfficerSupplied, got {other:?}"),
        }
        assert!(matches!(
            loaded.gap_resolutions.get(&7),
            Some(GapResolution::AcceptedSuggestion { .. })
        ));
    }
}
