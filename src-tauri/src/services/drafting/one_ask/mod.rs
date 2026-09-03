//! Feature 010 — One-Ask Drafting (ADR-0013).
//!
//! Officer types one sentence; the pipeline researches the indexed corpus,
//! composes the middle content of a government summary over a built-in
//! 5-section skeleton, and verifies every citation deterministically.
//!
//! Two-lane invariant: the model proposes *placement* ([CITE:n]/[FIG:n]
//! markers); deterministic code owns *content* (verbatim splice + gate). A
//! fabricated number can only ever appear as a flagged run, never a citation.

pub mod brevity;
pub mod compose;
pub mod doc_type;
pub mod intent;
pub mod research;
pub mod skeleton;
pub mod stakeholders;
pub mod state;
pub mod urgency;
pub mod verify;

use compose::{RawRun, SectionWriter};
use research::EmbedFn;
use skeleton::{SectionId, SKELETON};
use state::ProvisionCard;
use crate::db::Db;
use state::{OneAskState, Status};
use tokio_util::sync::CancellationToken;

/// Stage notifications for `drafting://one-ask-progress` (FR-013).
#[derive(Debug, Clone)]
pub enum Stage {
    Researching,
    /// Granular running commentary inside a long stage (research per-document
    /// findings) with the overall pipeline percent — drives the live activity
    /// feed so the officer sees the work, not a static label (FR-013 UX).
    Note { percent: u8, detail: String },
    /// 1-based section index out of `total`.
    Composing {
        index: usize,
        total: usize,
        title: &'static str,
    },
    Verifying,
    Ready,
    Failed(String),
    Cancelled,
}

/// Run the full pipeline for a session whose state (with parsed intent) is
/// already persisted. Saves state at every transition so a crash resumes as
/// `Failed{interrupted}` (the loader, not this fn, applies that mapping).
/// `deterministic` (US4/D7): when true, sections are composed from fixed
/// templates + verbatim slots and the `writer` is NOT called — drafting cannot
/// fabricate prose. When false, the model authors connective prose (legacy lane,
/// retained for A/B and the verify-machinery acceptance tests).
pub async fn run_pipeline(
    pool: &Db,
    embed: EmbedFn<'_>,
    writer: &dyn SectionWriter,
    draft_id: &str,
    deterministic: bool,
    cancel: &CancellationToken,
    on_stage: &(dyn Fn(Stage) + Send + Sync),
) -> Result<(), String> {
    let mut st = state::load(pool, draft_id)
        .await?
        .ok_or_else(|| format!("unknown draft {draft_id}"))?;

    // ── Research ─────────────────────────────────────────────────────────────
    st.status = Status::Researching;
    state::save(pool, draft_id, &st).await?;
    on_stage(Stage::Researching);

    // Research spans 3–23% of the overall run; notes carry per-document detail.
    let note = |frac: f32, detail: String| {
        let percent = 3 + (frac.clamp(0.0, 1.0) * 20.0).round() as u8;
        on_stage(Stage::Note { percent, detail });
    };
    // US1: classify the request → retrieval profile (modulates per-section
    // role-hints so retrieval emphasis fits the document type).
    let doc_type = doc_type::classify(&st.intent);
    log::info!("one_ask doc_type = {} ({:?})", doc_type.label(), doc_type);
    let cards = research::research(pool, embed, &st.intent, &SKELETON, doc_type, &note).await?;
    st.cards = cards;
    state::save(pool, draft_id, &st).await?;

    if cancel.is_cancelled() {
        return finish_cancelled(pool, draft_id, &mut st, on_stage).await;
    }

    // Whole-ask refusal: nothing in the corpus supports any section (FR-007).
    if st.cards.is_empty() {
        let reason = "The indexed corpus contains no provisions relevant to this \
                      request. Draft cannot be grounded — index the governing \
                      documents (rules, manuals, delegations) and retry."
            .to_string();
        st.status = Status::Failed {
            reason: reason.clone(),
        };
        state::save(pool, draft_id, &st).await?;
        on_stage(Stage::Failed(reason.clone()));
        return Err(reason);
    }

    // ── Compose, one section at a time ───────────────────────────────────────
    let total = SKELETON.len();
    // FR-014: the officer's request is the subject of every section (figures
    // neutralised so no raw number leaks into prose). Compute once.
    let subject = compose::sanitized_subject(&st.intent);
    // US9: factual urgency from the officer's own words (None when absent), and
    // stakeholders auto-extracted from the officer's sentence ∪ the role-holders
    // named in the Legal cards. Both computed once, deterministically.
    let urgency = urgency::detect_urgency(&st.intent);
    let stakeholders = {
        let legal_cards: Vec<&ProvisionCard> = st
            .cards
            .iter()
            .filter(|c| {
                c.section_ids.contains(&SectionId::LegalValidation)
                    || c.section_ids.contains(&SectionId::StakeholderStatus)
                    || c.section_ids.contains(&SectionId::Procedure)
            })
            .collect();
        stakeholders::extract_stakeholders(&st.intent, &legal_cards)
    };
    if !stakeholders.is_empty() || urgency.is_some() {
        log::info!(
            "one_ask US9: urgency={:?} stakeholders={:?}",
            urgency.as_deref(),
            stakeholders
        );
    }
    let mut sections_raw: Vec<(SectionId, Vec<RawRun>)> = Vec::with_capacity(total);
    for (i, section) in SKELETON.iter().enumerate() {
        if cancel.is_cancelled() {
            return finish_cancelled(pool, draft_id, &mut st, on_stage).await;
        }
        // US10/FR-022: the Financial move renders only when the officer states a
        // figure; otherwise it is omitted entirely (no empty financial paragraph).
        if section.id == SectionId::FinancialAspect && st.intent.figures.is_empty() {
            continue;
        }
        st.status = Status::Composing { section: i + 1 };
        state::save(pool, draft_id, &st).await?;
        on_stage(Stage::Composing {
            index: i + 1,
            total,
            title: section.title,
        });

        let cards = research::cards_for_section(&st.cards, section);
        let figures = if section.wants_figures {
            st.intent.figures.clone()
        } else {
            Vec::new()
        };
        // US4/D7: deterministic lane authors no prose — emit the section's runs
        // from fixed templates + verbatim slots; the legacy model lane remains
        // for A/B. Both feed the same verify/splice gate.
        let runs = if deterministic {
            compose::compose_deterministic(
                section,
                &subject,
                &cards,
                &figures,
                urgency.as_deref(),
                &stakeholders,
            )
        } else {
            let prompt = compose::build_section_prompt(
                section,
                &subject,
                &cards,
                &figures,
                urgency.as_deref(),
                &stakeholders,
            );
            compose::parse_markers(&writer.write(&prompt)?)
        };
        sections_raw.push((section.id, runs));
    }

    // ── Verify ───────────────────────────────────────────────────────────────
    st.status = Status::Verifying;
    state::save(pool, draft_id, &st).await?;
    on_stage(Stage::Verifying);

    st.paragraphs = verify::assemble_and_verify(&sections_raw, &st.cards, &st.intent);
    st.status = Status::Ready;
    state::save(pool, draft_id, &st).await?;
    on_stage(Stage::Ready);
    Ok(())
}

async fn finish_cancelled(
    pool: &Db,
    draft_id: &str,
    st: &mut OneAskState,
    on_stage: &(dyn Fn(Stage) + Send + Sync),
) -> Result<(), String> {
    st.status = Status::Cancelled;
    state::save(pool, draft_id, st).await?;
    on_stage(Stage::Cancelled);
    Ok(())
}

/// Map an interrupted session (app died mid-generation) to a re-runnable
/// failed state on load (data-model edge case: never present a half-draft as
/// finished).
pub fn normalize_interrupted(status: &Status) -> Status {
    match status {
        Status::Researching | Status::Composing { .. } | Status::Verifying => Status::Failed {
            reason: "Generation was interrupted (app closed). Retry to re-run this draft."
                .to_string(),
        },
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_statuses_normalize_to_retryable_failed() {
        for s in [
            Status::Researching,
            Status::Composing { section: 3 },
            Status::Verifying,
        ] {
            match normalize_interrupted(&s) {
                Status::Failed { reason } => assert!(reason.contains("interrupted")),
                other => panic!("expected Failed, got {other:?}"),
            }
        }
        assert_eq!(normalize_interrupted(&Status::Ready), Status::Ready);
        assert_eq!(normalize_interrupted(&Status::Cancelled), Status::Cancelled);
    }
}
