//! T013 — conversation memory base for the context allocator (FR-017/FR-018).
//!
//! Provides the allocator its history inputs for a turn: the running summary,
//! the most-recent turns (most-recent-first, as the allocator expects), and the
//! always-present pinned-fact slot.
//!
//! - **T043** — deterministic pinned-fact extraction (grade/BPS, length of
//!   service, leave type) over user turns into `pinned_case_fact` rows; a later
//!   restatement supersedes the earlier value (research R5, FR-018). Pinned facts
//!   are kept verbatim and never summarized.
//! - **T044** — summarize-old-turns eviction: when the verbatim history grows past
//!   the keep-recent window, the oldest turns are folded into the running
//!   `conversation_summary` and dropped from the verbatim set (FR-017). Pinned
//!   facts are exempt (separate table, never touched).
//!
//! F015 keys memory on `conversation_id`. A native F015 chat sets
//! `chat_message.draft_session_id == conversation_id` (the column is NOT NULL,
//! and migration 004 backfilled `conversation_id` from it for legacy rows).

use crate::db::Db;
use anyhow::Result;
use regex_lite::Regex;
use std::sync::OnceLock;

/// Everything memory contributes to one turn's context allocation.
#[derive(Debug, Clone, Default)]
pub struct MemoryContext {
    /// Pinned case facts — an always-present allocator slot (FR-018). Stubbed
    /// empty until Phase E (T043).
    pub pinned_facts: Vec<String>,
    /// Recent turns, MOST-RECENT-FIRST, formatted `"role: content"`.
    pub recent_turns: Vec<String>,
    /// Running summary of evicted older turns (FR-017), if any.
    pub summary: Option<String>,
}

/// Load memory for `conversation_id`: the always-present pinned facts, the last
/// `max_turns` **non-summarized** messages (most-recent-first), and the running
/// summary of the turns already evicted (FR-017/FR-018).
pub async fn load_for_turn(
    pool: &Db,
    conversation_id: &str,
    max_turns: usize,
) -> Result<MemoryContext> {
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT seq, role, content FROM chat_message
         WHERE conversation_id = $1 AND summarized = 0 ORDER BY seq DESC LIMIT $2",
    )
    .bind(conversation_id)
    .bind(max_turns as i64)
    .fetch_all(pool)
    .await?;
    let recent_turns = rows
        .into_iter()
        .map(|(_, role, content)| format!("{role}: {content}"))
        .collect();

    let summary: Option<String> = sqlx::query_scalar(
        "SELECT summary_text FROM conversation_summary WHERE draft_session_id = $1",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?;

    let pinned_facts = load_pinned_facts(pool, conversation_id)
        .await?
        .into_iter()
        .map(|(k, v)| fact_line(&k, &v))
        .collect();

    Ok(MemoryContext { pinned_facts, recent_turns, summary })
}

// ─── Ordinal turn recall — the "numbered turn" graph-memory tier ─────────────
//
// An ordinal reference ("what was my FIRST question?", "compare the first one
// with the THIRD one") addresses a turn by its POSITION in the conversation, not
// its content. Two consequences the verbatim window and vector search both miss:
//   1. The position has no content to embed — "my first question" shares no words
//      with "grounds for penalty", so semantic recall matches nothing.
//   2. The referenced turn is almost always OLDER than the verbatim window, so it
//      is not in `recent_turns` — the model names the oldest turn it can still
//      see instead (prod audit 2026-07-16: "what was my first question?" answered
//      Q3, the oldest of the 4 pairs left in the window).
// The fix is a deterministic `seq` lookup over the FULL turn history — including
// turns already folded into the summary (`summarized = 1`, still present in
// `chat_message`) — returning the exact earlier turn(s) for the caller to inject
// as non-citable recall. This is tier 2 of the architect's graph design: numbered
// nodes 1..n, each holding its question/answer, retrieved on demand.

/// Cap per quoted earlier turn (chars) so recall can't crowd retrieved law out of
/// the window. Injected verbatim otherwise.
const RECALL_TURN_CHAR_CAP: usize = 300;

/// Map an ordinal token to its 1-based position, or `None` for "last"-style words
/// (resolved against the actual turn count by the caller). Accepts number words
/// (`first`..`tenth`) and digit forms (`1st`..`10th`).
fn ordinal_to_index(tok: &str) -> Option<Option<usize>> {
    let t = tok.to_ascii_lowercase();
    let n = match t.as_str() {
        "first" | "1st" => 1,
        "second" | "2nd" => 2,
        "third" | "3rd" => 3,
        "fourth" | "4th" => 4,
        "fifth" | "5th" => 5,
        "sixth" | "6th" => 6,
        "seventh" | "7th" => 7,
        "eighth" | "8th" => 8,
        "ninth" | "9th" => 9,
        "tenth" | "10th" => 10,
        "last" | "previous" | "latest" => return Some(None), // resolve to the last turn
        _ => return None,
    };
    Some(Some(n))
}

/// Which side of an exchange an ordinal noun refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecallTarget {
    Question, // "my first question"
    Answer,   // "the third answer/reply/response"
    Exchange, // "the first one/thing" — both sides
}

/// One parsed ordinal reference: `pos = None` means "last".
#[derive(Debug, Clone, PartialEq, Eq)]
struct OrdinalRef {
    label: String, // "1st", "3rd", "last" — for the rendered recall line
    pos: Option<usize>,
    target: RecallTarget,
}

/// Parse every ordinal turn-reference in `query` (deterministic, no LLM). Matches
/// an ordinal token immediately followed by a turn noun: `question` → the user's
/// turn, `answer`/`reply`/`response` → the assistant's, `one`/`thing` → the whole
/// exchange. A legal ordinal with a non-turn noun ("first appeal", "first
/// schedule") does not match. Deduped, in first-seen order.
fn parse_ordinal_refs(query: &str) -> Vec<OrdinalRef> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(first|second|third|fourth|fifth|sixth|seventh|eighth|ninth|tenth|last|previous|latest|\d{1,2}(?:st|nd|rd|th))\s+(questions?|answers?|repl(?:y|ies)|responses?|ones?|things?)\b",
        )
        .unwrap()
    });
    let mut out: Vec<OrdinalRef> = Vec::new();
    for c in re.captures_iter(query) {
        let Some(pos) = ordinal_to_index(&c[1]) else { continue };
        let noun = c[2].to_ascii_lowercase();
        let target = if noun.starts_with("question") {
            RecallTarget::Question
        } else if noun.starts_with("answer")
            || noun.starts_with("repl")
            || noun.starts_with("response")
        {
            RecallTarget::Answer
        } else {
            RecallTarget::Exchange
        };
        let label = match pos {
            None => "last".to_string(),
            Some(n) => nth_label(n),
        };
        let r = OrdinalRef { label, pos, target };
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

/// "1st", "2nd", "3rd", "11th"… for the rendered recall line.
fn nth_label(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// A quoted earlier turn, capped and whitespace-collapsed for injection.
fn quote_turn(content: &str) -> String {
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let snippet: String = flat.chars().take(RECALL_TURN_CHAR_CAP).collect();
    let ell = if flat.chars().count() > RECALL_TURN_CHAR_CAP { "…" } else { "" };
    format!("{snippet}{ell}")
}

/// Resolve every ordinal turn-reference in `query` against the FULL turn history
/// for `conversation_id` and render a non-citable recall block, or `""` when the
/// query references no earlier turn (the common case — zero cost).
///
/// The current in-flight user turn (already appended by the orchestrator before
/// this runs) is excluded from the QUESTION list, so "my first question" means the
/// first real question and "my last question" means the one before this one.
///
/// The returned block is appended to the system prompt by the caller, which
/// `route_scoped`'s `law_overhead` MEASURES (`est_tokens(system_prompt)`) — so the
/// law budget shrinks to make room and the prompt cannot overflow (the reason the
/// earlier unmeasured `memory_block` injection was reverted in `80048b37`).
pub async fn ordinal_recall_block(
    pool: &Db,
    conversation_id: &str,
    query: &str,
) -> Result<String> {
    let refs = parse_ordinal_refs(query);
    if refs.is_empty() {
        return Ok(String::new());
    }

    let (questions, answers) = fetch_turn_lists(pool, conversation_id).await?;
    let questions: Vec<&str> = questions.iter().map(String::as_str).collect();
    let answers: Vec<&str> = answers.iter().map(String::as_str).collect();

    let pick = |v: &[&str], pos: Option<usize>| -> Option<String> {
        let idx = match pos {
            None => v.len().checked_sub(1)?, // "last"
            Some(n) => n.checked_sub(1)?,
        };
        v.get(idx).map(|s| quote_turn(s))
    };

    let mut lines: Vec<String> = Vec::new();
    for r in &refs {
        match r.target {
            RecallTarget::Question => {
                if let Some(q) = pick(&questions, r.pos) {
                    lines.push(format!("- The officer's {} question was: \"{}\"", r.label, q));
                }
            }
            RecallTarget::Answer => {
                if let Some(a) = pick(&answers, r.pos) {
                    lines.push(format!("- Your {} answer was: \"{}\"", r.label, a));
                }
            }
            RecallTarget::Exchange => {
                if let Some(q) = pick(&questions, r.pos) {
                    lines.push(format!("- The officer's {} question was: \"{}\"", r.label, q));
                }
                if let Some(a) = pick(&answers, r.pos) {
                    lines.push(format!("- Your {} answer was: \"{}\"", r.label, a));
                }
            }
        }
    }
    if lines.is_empty() {
        return Ok(String::new());
    }

    Ok(format!(
        "\n\n=== EARLIER TURNS IN THIS CONVERSATION (recall) ===\n\
The officer is referring to specific earlier turn(s) of THIS conversation by their \
position. Answer which turn was asked from these EXACT records — do NOT use the \
retrieved documents to answer it, and never attach a citation marker to them.\n\
{}\n",
        lines.join("\n")
    ))
}

/// Ordered turn text for `conversation_id`, split by role. The current in-flight
/// user turn (already appended by the orchestrator before recall runs) is the LAST
/// user row; it is dropped from `questions` so ordinals count the officer's PRIOR
/// questions ("first" = the real Q1, "last" = the one before this). Includes turns
/// folded into the summary (`summarized = 1`) — the whole point of the lookup.
async fn fetch_turn_lists(pool: &Db, conversation_id: &str) -> Result<(Vec<String>, Vec<String>)> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT role, content FROM chat_message
         WHERE conversation_id = $1 ORDER BY seq ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    let mut questions: Vec<String> =
        rows.iter().filter(|(r, _)| r == "user").map(|(_, c)| c.clone()).collect();
    questions.pop(); // exclude the current in-flight turn
    let answers: Vec<String> =
        rows.iter().filter(|(r, _)| r == "assistant").map(|(_, c)| c.clone()).collect();
    Ok((questions, answers))
}

/// True when `query` is a PURE conversation-recall question — asking only about a
/// turn of THIS chat by position ("what was my first question?", "read me your
/// last answer") — as opposed to a legal question that merely contains an ordinal
/// ("what is the first schedule?") or a compound task that needs reasoning over the
/// corpus ("compare the first one with the third one"). Requires (a) an ordinal
/// turn-reference, (b) a first/second-person anchor (`my`/`your`/`our`) — so a
/// legal "the first schedule" and the anchorless "compare the first one" both fall
/// through to normal retrieval — and (c) no comparison/analysis verb.
fn is_pure_recall(query: &str, refs: &[OrdinalRef]) -> bool {
    if refs.is_empty() {
        return false;
    }
    static ANCHOR: OnceLock<Regex> = OnceLock::new();
    let anchor = ANCHOR.get_or_init(|| Regex::new(r"(?i)\b(my|your|our)\b").unwrap());
    if !anchor.is_match(query) {
        return false;
    }
    let low = format!(" {} ", query.to_lowercase());
    const EXCLUDE: &[&str] = &[
        "compar", "contrast", "differ", "versus", " vs ", "relation",
        "connection", "between", "combine", "summar",
    ];
    !EXCLUDE.iter().any(|e| low.contains(e))
}

/// A ready-to-send answer for a PURE conversation-recall question, or `None` when
/// the query is not pure recall (falls through to normal retrieval). Deterministic
/// — quotes the exact earlier turn(s) from `chat_message`, no model involved.
///
/// This is the graph-memory tier-2 short-circuit. Injecting the recalled turn into
/// the grounded RAG prompt (see [`ordinal_recall_block`]) proved too weak: a
/// strongly-grounded model REFUSED "what was my first question?" (no legal passage
/// supports it) and read "the first one" as a retrieved DOCUMENT (prod 2026-07-17).
/// A conversation-structure question must not be routed through corpus generation at
/// all — the same reasoning as the deterministic Count capability (ADR-0019).
pub async fn direct_recall_answer(
    pool: &Db,
    conversation_id: &str,
    query: &str,
) -> Result<Option<String>> {
    let refs = parse_ordinal_refs(query);
    if !is_pure_recall(query, &refs) {
        return Ok(None);
    }
    let (questions, answers) = fetch_turn_lists(pool, conversation_id).await?;
    let questions: Vec<&str> = questions.iter().map(String::as_str).collect();
    let answers: Vec<&str> = answers.iter().map(String::as_str).collect();

    let pick = |v: &[&str], pos: Option<usize>| -> Option<String> {
        let idx = match pos {
            None => v.len().checked_sub(1)?,
            Some(n) => n.checked_sub(1)?,
        };
        v.get(idx).map(|s| quote_turn(s))
    };

    let mut parts: Vec<String> = Vec::new();
    for r in &refs {
        match r.target {
            RecallTarget::Question | RecallTarget::Exchange => {
                if let Some(q) = pick(&questions, r.pos) {
                    parts.push(format!("Your {} question in this conversation was:\n\n\"{}\"", r.label, q));
                }
                if r.target == RecallTarget::Exchange {
                    if let Some(a) = pick(&answers, r.pos) {
                        parts.push(format!("and my {} answer was:\n\n\"{}\"", r.label, a));
                    }
                }
            }
            RecallTarget::Answer => {
                if let Some(a) = pick(&answers, r.pos) {
                    parts.push(format!("My {} answer in this conversation was:\n\n\"{}\"", r.label, a));
                }
            }
        }
    }
    if parts.is_empty() {
        return Ok(None);
    }
    Ok(Some(parts.join("\n\n")))
}

// ─── T043: deterministic pinned-fact extraction (FR-018, research R5) ─────────

/// Render a pinned fact for the allocator's always-present slot.
fn fact_line(key: &str, value: &str) -> String {
    let label = match key {
        "grade" => "Grade",
        "length_of_service" => "Length of service",
        "leave_type" => "Leave type",
        _ => key,
    };
    format!("{label}: {value}")
}

/// Extract load-bearing situational facts from a user turn via labelled patterns
/// (deterministic — no LLM, no confabulation). Returns canonical `(key, value)`
/// pairs in a stable order: `grade`, `length_of_service`, `leave_type`. A turn
/// that states none returns an empty vec.
pub fn extract_facts(text: &str) -> Vec<(String, String)> {
    let mut facts: Vec<(String, String)> = Vec::new();

    // Grade / pay scale: "BS-17", "BPS 18", "grade 17", "scale 19".
    static GRADE: OnceLock<Regex> = OnceLock::new();
    let grade = GRADE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:bps|bs|grade|scale)\s*[-#]?\s*(\d{1,2})\b").unwrap()
    });
    if let Some(c) = grade.captures(text) {
        facts.push(("grade".into(), format!("BS-{}", &c[1])));
    }

    // Length of service: "8 years service", "8 years of service", "8+ years' service".
    static SERVICE: OnceLock<Regex> = OnceLock::new();
    let service = SERVICE.get_or_init(|| {
        Regex::new(r"(?i)\b(\d{1,2})\s*\+?\s*years?[’'\s]*(?:of\s+)?service\b").unwrap()
    });
    if let Some(c) = service.captures(text) {
        facts.push(("length_of_service".into(), format!("{} years", &c[1])));
    }

    // Leave type: "casual leave", "earned leave", "LPR", "ex-Pakistan leave", etc.
    static LEAVE: OnceLock<Regex> = OnceLock::new();
    let leave = LEAVE.get_or_init(|| {
        Regex::new(
            r"(?i)\b((?:casual|earned|medical|maternity|paternity|study|extraordinary|ex[-\s]?pakistan|leave preparatory to retirement)\s+leave|lpr)\b",
        )
        .unwrap()
    });
    if let Some(c) = leave.captures(text) {
        facts.push(("leave_type".into(), c[1].trim().to_string()));
    }

    facts
}

/// Load the non-superseded pinned facts for a conversation (the always-present
/// allocator slot), oldest-first for stable display.
pub async fn load_pinned_facts(
    pool: &Db,
    conversation_id: &str,
) -> Result<Vec<(String, String)>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT key, value FROM pinned_case_fact
         WHERE conversation_id = $1 AND superseded = FALSE
         ORDER BY created_at ASC, key ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Record extracted facts for a conversation, superseding any earlier value for
/// the same key (FR-018: "a later restatement supersedes"). A restatement of the
/// SAME value is a no-op. Returns the number of newly-pinned facts.
pub async fn record_pinned_facts(
    pool: &Db,
    conversation_id: &str,
    source_seq: i64,
    facts: &[(String, String)],
) -> Result<usize> {
    let mut pinned = 0usize;
    for (key, value) in facts {
        // Already pinned with the same value? Nothing to do.
        let current: Option<String> = sqlx::query_scalar(
            "SELECT value FROM pinned_case_fact
             WHERE conversation_id = $1 AND key = $2 AND superseded = FALSE",
        )
        .bind(conversation_id)
        .bind(key)
        .fetch_optional(pool)
        .await?;
        if current.as_deref() == Some(value.as_str()) {
            continue;
        }
        // Supersede any earlier non-superseded value for this key.
        sqlx::query(
            "UPDATE pinned_case_fact SET superseded = TRUE
             WHERE conversation_id = $1 AND key = $2 AND superseded = FALSE",
        )
        .bind(conversation_id)
        .bind(key)
        .execute(pool)
        .await?;
        sqlx::query(
            "INSERT INTO pinned_case_fact (conversation_id, key, value, source_seq)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(conversation_id)
        .bind(key)
        .bind(value)
        .bind(source_seq)
        .execute(pool)
        .await?;
        pinned += 1;
    }
    Ok(pinned)
}

/// Extract + record a user turn's pinned facts in one step (called per user turn
/// by the orchestrator). Returns the number of newly-pinned facts.
pub async fn capture_pinned_facts(
    pool: &Db,
    conversation_id: &str,
    source_seq: i64,
    user_text: &str,
) -> Result<usize> {
    let facts = extract_facts(user_text);
    if facts.is_empty() {
        return Ok(0);
    }
    record_pinned_facts(pool, conversation_id, source_seq, &facts).await
}

// ─── T044: summarize-old-turns eviction (FR-017) ─────────────────────────────

/// A compact, faithful one-line extract of a turn for the running summary
/// (deterministic; an LLM-assisted summary can replace this without changing the
/// eviction mechanics). Collapses whitespace and caps length.
fn compact_turn(role: &str, content: &str) -> String {
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let snippet: String = flat.chars().take(140).collect();
    let ell = if flat.chars().count() > 140 { "…" } else { "" };
    format!("{role}: {snippet}{ell}")
}

/// Fold the oldest verbatim turns into the running summary when the
/// non-summarized history grows past `keep_recent` (FR-017). The most-recent
/// `keep_recent` turns stay verbatim; older ones are appended to
/// `conversation_summary` and marked `summarized` so they leave the verbatim set.
/// Pinned facts are untouched (separate table). Returns the number evicted.
pub async fn evict_old_turns(
    pool: &Db,
    conversation_id: &str,
    keep_recent: usize,
) -> Result<usize> {
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT seq, role, content FROM chat_message
         WHERE conversation_id = $1 AND summarized = 0 ORDER BY seq ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    if rows.len() <= keep_recent {
        return Ok(0);
    }
    let evict_count = rows.len() - keep_recent;
    let to_evict = &rows[..evict_count];
    let covers_upto = to_evict.last().map(|(seq, _, _)| *seq).unwrap_or(0);

    let extract = to_evict
        .iter()
        .map(|(_, role, content)| compact_turn(role, content))
        .collect::<Vec<_>>()
        .join(" ");

    let existing: Option<String> = sqlx::query_scalar(
        "SELECT summary_text FROM conversation_summary WHERE draft_session_id = $1",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?;
    let merged = match existing {
        Some(prev) if !prev.trim().is_empty() => format!("{prev} {extract}"),
        _ => extract,
    };

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO conversation_summary (draft_session_id, summary_text, covers_upto_seq, updated_at)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (draft_session_id)
         DO UPDATE SET summary_text = EXCLUDED.summary_text,
                       covers_upto_seq = EXCLUDED.covers_upto_seq,
                       updated_at = EXCLUDED.updated_at",
    )
    .bind(conversation_id)
    .bind(&merged)
    .bind(covers_upto)
    .bind(&now)
    .execute(pool)
    .await?;

    // Drop the evicted turns from the verbatim set (all non-summarized turns with
    // seq ≤ covers_upto are exactly `to_evict`; kept turns have larger seq).
    sqlx::query(
        "UPDATE chat_message SET summarized = 1
         WHERE conversation_id = $1 AND summarized = 0 AND seq <= $2",
    )
    .bind(conversation_id)
    .bind(covers_upto)
    .execute(pool)
    .await?;

    Ok(evict_count)
}

/// Append a turn keyed on `conversation_id`, returning its dense, monotonic
/// `seq` **and** its `chat_message.id` (the export handle for draft turns, T027).
/// Sets both `draft_session_id` and `conversation_id` to the same id so the row
/// satisfies the legacy NOT-NULL column and the F015 index alike.
pub async fn append_turn(
    pool: &Db,
    conversation_id: &str,
    role: &str,
    content: &str,
) -> Result<(i64, String)> {
    let seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq) + 1, 0) FROM chat_message WHERE conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_one(pool)
    .await?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let token_count = (content.len() / 4) as i64; // coarse; refined in Phase E
    sqlx::query(
        "INSERT INTO chat_message
            (id, draft_session_id, conversation_id, seq, role, content, token_count, summarized, created_at)
         VALUES ($1, $2, $2, $3, $4, $5, $6, 0, $7)",
    )
    .bind(&id)
    .bind(conversation_id)
    .bind(seq)
    .bind(role)
    .bind(content)
    .bind(token_count)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok((seq, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    #[tokio::test]
    async fn recent_turns_are_most_recent_first_and_capped() {
        let pool = test_pool().await;
        let conv = "c1";
        for i in 0..5 {
            append_turn(&pool, conv, "user", &format!("q{i}")).await.unwrap();
            append_turn(&pool, conv, "assistant", &format!("a{i}")).await.unwrap();
        }
        let mem = load_for_turn(&pool, conv, 3).await.unwrap();
        assert_eq!(mem.recent_turns.len(), 3, "capped at max_turns");
        // most-recent-first: the latest assistant turn leads.
        assert_eq!(mem.recent_turns[0], "assistant: a4");
        assert!(mem.pinned_facts.is_empty(), "pinned facts stubbed in Phase 2");
        assert!(mem.summary.is_none());
    }

    #[tokio::test]
    async fn summary_is_loaded_when_present() {
        let pool = test_pool().await;
        let conv = "c2";
        append_turn(&pool, conv, "user", "hello").await.unwrap();
        sqlx::query(
            "INSERT INTO conversation_summary (draft_session_id, summary_text, covers_upto_seq, updated_at)
             VALUES ($1, $2, 0, $3)",
        )
        .bind(conv)
        .bind("earlier: officer asked about leave rules")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();
        let mem = load_for_turn(&pool, conv, 6).await.unwrap();
        assert_eq!(mem.summary.as_deref(), Some("earlier: officer asked about leave rules"));
    }

    #[tokio::test]
    async fn conversations_are_isolated() {
        let pool = test_pool().await;
        append_turn(&pool, "a", "user", "x").await.unwrap();
        append_turn(&pool, "b", "user", "y").await.unwrap();
        assert_eq!(load_for_turn(&pool, "a", 6).await.unwrap().recent_turns.len(), 1);
        assert_eq!(load_for_turn(&pool, "b", 6).await.unwrap().recent_turns.len(), 1);
    }

    #[tokio::test]
    async fn seq_is_dense_and_monotonic() {
        let pool = test_pool().await;
        let (s0, id0) = append_turn(&pool, "c", "user", "1").await.unwrap();
        let (s1, id1) = append_turn(&pool, "c", "assistant", "2").await.unwrap();
        assert_eq!((s0, s1), (0, 1));
        assert_ne!(id0, id1, "each turn gets a distinct message id");
    }

    // ── T043: pinned-fact extraction ─────────────────────────────────────────

    #[test]
    fn extracts_grade_service_and_leave_type() {
        let facts = extract_facts("I am a BS-17 officer with 8 years of service applying for earned leave");
        assert_eq!(facts, vec![
            ("grade".to_string(), "BS-17".to_string()),
            ("length_of_service".to_string(), "8 years".to_string()),
            ("leave_type".to_string(), "earned leave".to_string()),
        ]);
    }

    #[test]
    fn grade_variants_normalize_and_no_facts_is_empty() {
        assert_eq!(extract_facts("grade 18 here")[0], ("grade".into(), "BS-18".into()));
        assert_eq!(extract_facts("BPS-19")[0], ("grade".into(), "BS-19".into()));
        assert!(extract_facts("what is the procedure for long leave").is_empty());
    }

    #[tokio::test]
    async fn pinned_facts_persist_and_restatement_supersedes() {
        let pool = test_pool().await;
        let conv = "pf1";
        // Initially BS-17.
        let n = capture_pinned_facts(&pool, conv, 0, "I'm a BS-17 officer").await.unwrap();
        assert_eq!(n, 1);
        let facts = load_pinned_facts(&pool, conv).await.unwrap();
        assert_eq!(facts, vec![("grade".into(), "BS-17".into())]);

        // Same value restated → no new row.
        assert_eq!(capture_pinned_facts(&pool, conv, 1, "still BS-17").await.unwrap(), 0);

        // Promotion to BS-18 supersedes the earlier value.
        assert_eq!(capture_pinned_facts(&pool, conv, 2, "now I'm BS-18").await.unwrap(), 1);
        let facts = load_pinned_facts(&pool, conv).await.unwrap();
        assert_eq!(facts, vec![("grade".into(), "BS-18".into())], "only the current value is live");
    }

    #[tokio::test]
    async fn pinned_facts_surface_in_memory_context() {
        let pool = test_pool().await;
        let conv = "pf2";
        capture_pinned_facts(&pool, conv, 0, "BS-17 with 8 years service").await.unwrap();
        let mem = load_for_turn(&pool, conv, 6).await.unwrap();
        assert!(mem.pinned_facts.iter().any(|f| f == "Grade: BS-17"));
        assert!(mem.pinned_facts.iter().any(|f| f == "Length of service: 8 years"));
    }

    // ── Ordinal turn recall ──────────────────────────────────────────────────

    #[test]
    fn parse_ordinal_refs_maps_words_digits_and_last() {
        let refs = parse_ordinal_refs("compare the first one with the third one");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], OrdinalRef { label: "1st".into(), pos: Some(1), target: RecallTarget::Exchange });
        assert_eq!(refs[1], OrdinalRef { label: "3rd".into(), pos: Some(3), target: RecallTarget::Exchange });

        assert_eq!(
            parse_ordinal_refs("what was my first question?"),
            vec![OrdinalRef { label: "1st".into(), pos: Some(1), target: RecallTarget::Question }],
        );
        assert_eq!(
            parse_ordinal_refs("read me your 2nd answer")[0],
            OrdinalRef { label: "2nd".into(), pos: Some(2), target: RecallTarget::Answer },
        );
        assert_eq!(
            parse_ordinal_refs("what was my last question")[0],
            OrdinalRef { label: "last".into(), pos: None, target: RecallTarget::Question },
        );
    }

    #[test]
    fn parse_ordinal_refs_ignores_legal_ordinals_without_a_turn_noun() {
        // "first appeal" / "first schedule" are legal phrases, not turn references.
        assert!(parse_ordinal_refs("what is the first appeal timeline?").is_empty());
        assert!(parse_ordinal_refs("penalties under the first schedule").is_empty());
        assert!(parse_ordinal_refs("who is the competent authority?").is_empty());
    }

    #[tokio::test]
    async fn ordinal_recall_finds_the_first_question_even_after_eviction() {
        let pool = test_pool().await;
        let conv = "or1";
        // An 8-message (4-pair) chat, then the current in-flight question appended.
        let turns = [
            ("user", "What penalties can be imposed under the E&D Rules?"),
            ("assistant", "Several penalties apply [1]."),
            ("user", "and for bs-17 officer"),
            ("assistant", "For BS-17, minor and major penalties [2]."),
            ("user", "who is competent authority for that?"),
            ("assistant", "The competent authority is the appointing authority [3]."),
            ("user", "what about the appeal timeline?"),
            ("assistant", "An appeal must be filed within 30 days [4]."),
        ];
        for (role, content) in turns {
            append_turn(&pool, conv, role, content).await.unwrap();
        }
        // Evict the oldest pair so Q1 leaves the verbatim window (still in the table).
        evict_old_turns(&pool, conv, 6).await.unwrap();
        // The current in-flight user turn.
        append_turn(&pool, conv, "user", "what was my first question?").await.unwrap();

        let block = ordinal_recall_block(&pool, conv, "what was my first question?").await.unwrap();
        assert!(
            block.contains("What penalties can be imposed under the E&D Rules?"),
            "recall must reach the evicted Q1, not the oldest visible turn: {block}"
        );
        assert!(block.contains("1st question"), "labels the ordinal: {block}");
    }

    #[tokio::test]
    async fn ordinal_recall_compares_first_and_third_exchanges() {
        let pool = test_pool().await;
        let conv = "or2";
        for i in 1..=3 {
            append_turn(&pool, conv, "user", &format!("question {i} text")).await.unwrap();
            append_turn(&pool, conv, "assistant", &format!("answer {i} text")).await.unwrap();
        }
        append_turn(&pool, conv, "user", "compare the first one with the third one").await.unwrap();

        let block = ordinal_recall_block(&pool, conv, "compare the first one with the third one")
            .await
            .unwrap();
        assert!(block.contains("question 1 text") && block.contains("answer 1 text"), "1st exchange: {block}");
        assert!(block.contains("question 3 text") && block.contains("answer 3 text"), "3rd exchange: {block}");
        assert!(!block.contains("question 2 text"), "the 2nd exchange is not referenced: {block}");
    }

    #[tokio::test]
    async fn ordinal_recall_last_question_excludes_the_current_turn() {
        let pool = test_pool().await;
        let conv = "or3";
        append_turn(&pool, conv, "user", "real prior question").await.unwrap();
        append_turn(&pool, conv, "assistant", "an answer").await.unwrap();
        append_turn(&pool, conv, "user", "what was my last question?").await.unwrap();

        let block = ordinal_recall_block(&pool, conv, "what was my last question?").await.unwrap();
        assert!(block.contains("real prior question"), "last question = the one before the current: {block}");
        assert!(!block.contains("what was my last question?"), "must not echo the current in-flight turn: {block}");
    }

    #[tokio::test]
    async fn ordinal_recall_is_empty_without_a_turn_reference() {
        let pool = test_pool().await;
        let conv = "or4";
        append_turn(&pool, conv, "user", "some question").await.unwrap();
        append_turn(&pool, conv, "assistant", "some answer").await.unwrap();
        append_turn(&pool, conv, "user", "what is the appeal procedure?").await.unwrap();
        let block = ordinal_recall_block(&pool, conv, "what is the appeal procedure?").await.unwrap();
        assert!(block.is_empty(), "no ordinal reference → zero-cost empty block: {block:?}");
    }

    #[test]
    fn is_pure_recall_gates_on_anchor_and_excludes_compare_and_legal() {
        let pr = |q: &str| is_pure_recall(q, &parse_ordinal_refs(q));
        assert!(pr("what was my first question?"), "canonical pure recall");
        assert!(pr("read me your last answer"), "assistant-turn recall");
        // Compound comparison — needs reasoning, not a raw dump → not pure recall.
        assert!(!pr("compare the first one with the third one"), "no anchor + compare verb");
        // Legal ordinal with a possessive but no turn noun matched → no refs.
        assert!(!pr("what does my first schedule say?"), "legal, not a turn noun");
        // Anchorless ordinal → falls through to retrieval.
        assert!(!pr("what is the first question a tribunal asks?"), "no my/your anchor");
    }

    #[tokio::test]
    async fn direct_recall_answers_first_question_deterministically() {
        let pool = test_pool().await;
        let conv = "dr1";
        append_turn(&pool, conv, "user", "What penalties can be imposed under the E&D Rules?").await.unwrap();
        append_turn(&pool, conv, "assistant", "Several penalties apply.").await.unwrap();
        append_turn(&pool, conv, "user", "and for bs-17 officer").await.unwrap();
        append_turn(&pool, conv, "assistant", "For BS-17...").await.unwrap();
        evict_old_turns(&pool, conv, 2).await.unwrap(); // evict Q1 from the window
        append_turn(&pool, conv, "user", "what was my first question?").await.unwrap();

        let ans = direct_recall_answer(&pool, conv, "what was my first question?").await.unwrap();
        let ans = ans.expect("pure recall → a deterministic answer");
        assert!(ans.contains("What penalties can be imposed under the E&D Rules?"), "quotes the evicted Q1: {ans}");
        assert!(!ans.contains("do not contain enough"), "never a refusal");
    }

    #[tokio::test]
    async fn direct_recall_returns_none_for_non_pure_queries() {
        let pool = test_pool().await;
        let conv = "dr2";
        append_turn(&pool, conv, "user", "q1").await.unwrap();
        append_turn(&pool, conv, "assistant", "a1").await.unwrap();
        append_turn(&pool, conv, "user", "compare the first one with the third one").await.unwrap();
        // Compound compare → falls through to retrieval, not the short-circuit.
        assert!(direct_recall_answer(&pool, conv, "compare the first one with the third one").await.unwrap().is_none());
        // Ordinary legal question → no short-circuit.
        assert!(direct_recall_answer(&pool, conv, "what is the appeal procedure?").await.unwrap().is_none());
    }

    // ── T044: summarize-old-turns eviction ───────────────────────────────────

    #[tokio::test]
    async fn eviction_folds_oldest_turns_into_summary_keeping_recent_verbatim() {
        let pool = test_pool().await;
        let conv = "ev1";
        for i in 0..10 {
            append_turn(&pool, conv, "user", &format!("question number {i} about leave")).await.unwrap();
        }
        // Keep the 4 most recent verbatim; fold the other 6 into the summary.
        let evicted = evict_old_turns(&pool, conv, 4).await.unwrap();
        assert_eq!(evicted, 6);

        let mem = load_for_turn(&pool, conv, 100).await.unwrap();
        assert_eq!(mem.recent_turns.len(), 4, "only the keep-recent set stays verbatim");
        // most-recent-first: the latest turn leads.
        assert!(mem.recent_turns[0].contains("question number 9"));
        let summary = mem.summary.expect("evicted turns are summarized, not dropped");
        assert!(summary.contains("question number 0"), "oldest turn folded into summary: {summary}");
        assert!(summary.contains("question number 5"), "last evicted turn folded in: {summary}");
        assert!(!summary.contains("question number 6"), "kept turns are not in the summary");
    }

    #[tokio::test]
    async fn eviction_is_a_noop_when_within_keep_window() {
        let pool = test_pool().await;
        let conv = "ev2";
        append_turn(&pool, conv, "user", "one").await.unwrap();
        append_turn(&pool, conv, "user", "two").await.unwrap();
        assert_eq!(evict_old_turns(&pool, conv, 6).await.unwrap(), 0);
        assert!(load_for_turn(&pool, conv, 6).await.unwrap().summary.is_none());
    }

    #[tokio::test]
    async fn eviction_is_incremental_and_pinned_facts_are_exempt() {
        let pool = test_pool().await;
        let conv = "ev3";
        capture_pinned_facts(&pool, conv, 0, "BS-19 officer").await.unwrap();
        for i in 0..6 {
            append_turn(&pool, conv, "user", &format!("turn {i}")).await.unwrap();
        }
        evict_old_turns(&pool, conv, 2).await.unwrap();
        // More turns arrive; a second eviction extends the same summary.
        for i in 6..10 {
            append_turn(&pool, conv, "user", &format!("turn {i}")).await.unwrap();
        }
        evict_old_turns(&pool, conv, 2).await.unwrap();

        let mem = load_for_turn(&pool, conv, 100).await.unwrap();
        assert_eq!(mem.recent_turns.len(), 2);
        let summary = mem.summary.unwrap();
        assert!(summary.contains("turn 0") && summary.contains("turn 7"), "summary accretes: {summary}");
        // Pinned fact survives every eviction (exempt).
        assert!(mem.pinned_facts.iter().any(|f| f == "Grade: BS-19"));
    }
}
