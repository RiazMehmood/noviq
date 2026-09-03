//! Feature 010 US1 acceptance (T018) — full pipeline, canned writer, stub
//! embedder, fixture corpus. NO live LLM (tasks.md rule).
//!
//! Covers SC-001/SC-002 shape and US1 acceptance scenarios 1–3: one sentence
//! in → 5 verified numbered paragraphs out, zero unflagged foreign numbers,
//! officer figure verbatim, no boilerplate.

use echo_ai_lib::services::drafting::one_ask::compose::SectionWriter;
use echo_ai_lib::services::drafting::one_ask::state::{Run, Status};
use echo_ai_lib::services::drafting::one_ask::{intent, run_pipeline, state, Stage};
use regex_lite::Regex;
use echo_ai_lib::db::Db;
use std::path::Path;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

fn fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/one_ask")
}

fn unit_vec(dim: usize, seed: u64) -> Vec<f32> {
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut v: Vec<f32> = (0..dim)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            ((x % 2000) as f32 / 1000.0) - 1.0
        })
        .collect();
    let norm = v.iter().map(|a| a * a).sum::<f32>().sqrt().max(1e-9);
    v.iter_mut().for_each(|a| *a /= norm);
    v
}

/// Parse the fixture corpus: (file_name, Vec<chunk_text>).
fn parse_corpus() -> Vec<(String, Vec<String>)> {
    let text = std::fs::read_to_string(fixture_dir().join("corpus.md")).unwrap();
    let mut docs: Vec<(String, Vec<String>)> = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;
    let mut buf = String::new();
    let flush_chunk = |buf: &mut String, current: &mut Option<(String, Vec<String>)>| {
        let t = buf.trim().to_string();
        if !t.is_empty() {
            if let Some((_, chunks)) = current.as_mut() {
                chunks.push(t);
            }
        }
        buf.clear();
    };
    for line in text.lines() {
        if line.starts_with('#') && !line.starts_with("## doc:") {
            continue; // comments
        }
        if let Some(name) = line.strip_prefix("## doc: ") {
            flush_chunk(&mut buf, &mut current);
            if let Some(doc) = current.take() {
                docs.push(doc);
            }
            current = Some((name.trim().to_string(), Vec::new()));
        } else if line.trim() == "---" {
            flush_chunk(&mut buf, &mut current);
        } else {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    flush_chunk(&mut buf, &mut current);
    if let Some(doc) = current.take() {
        docs.push(doc);
    }
    docs
}

async fn build_db() -> (Db, usize) {
    let pool = echo_ai_lib::db::testing::test_pool().await;
    let (dim_s,): (String,) =
        sqlx::query_as("SELECT value FROM db_config WHERE key = 'embedding_dim'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let dim: usize = dim_s.parse().unwrap();

    sqlx::query(
        "INSERT INTO indexed_folders (id, folder_path, indexing_status, file_count, created_at) \
         VALUES ('f1', '/corpus/', 'indexed', 3, 0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let mut seed = 0u64;
    for (di, (file_name, chunks)) in parse_corpus().into_iter().enumerate() {
        let source_id = format!("s{di}");
        sqlx::query(
            "INSERT INTO sources (id, file_path, file_name, format, file_size_bytes, \
             file_hash, indexing_status, last_modified, created_at) \
             VALUES ($1, $2, $3, 'md', 1, $1, 'indexed', 0, 0)",
        )
        .bind(&source_id)
        .bind(format!("/corpus/{file_name}"))
        .bind(&file_name)
        .execute(&pool)
        .await
        .unwrap();
        for (ci, content) in chunks.into_iter().enumerate() {
            seed += 1;
            let chunk_id = format!("c{di}_{ci}");
            sqlx::query(
                "INSERT INTO chunks (id, source_id, chunk_index, content, char_start, \
                 char_end, token_count, page_number, created_at) \
                 VALUES ($1, $2, $3, $4, 0, 10, 5, $5, 0)",
            )
            .bind(&chunk_id)
            .bind(&source_id)
            .bind(ci as i64)
            .bind(&content)
            .bind((ci + 1) as i64)
            .execute(&pool)
            .await
            .unwrap();
            let emb = serde_json::to_string(&unit_vec(dim, seed)).unwrap();
            sqlx::query("INSERT INTO chunk_embeddings (chunk_id, embedding) VALUES ($1, $2::vector)")
                .bind(&chunk_id)
                .bind(emb)
                .execute(&pool)
                .await
                .unwrap();
        }
    }
    (pool, dim)
}

/// Canned writer: section title in the prompt → fixture file.
struct CannedWriter {
    /// Optional override: section title → fixture file name.
    overrides: Vec<(&'static str, &'static str)>,
}

impl SectionWriter for CannedWriter {
    fn write(&self, prompt: &str) -> Result<String, String> {
        let file = self
            .overrides
            .iter()
            .find(|(title, _)| prompt.contains(title))
            .map(|(_, f)| *f)
            .or_else(|| {
                [
                    // US10 skeleton — one move per section. "Stakeholder Legal
                    // Status" is listed before "Stakeholders" so the longer
                    // title matches first (both prompts carry a "Stakeholders" line).
                    ("Urgency", "background_purpose.txt"),
                    ("Legal Validation", "legal_procedural_position.txt"),
                    ("Stakeholder Legal Status", "legal_procedural_position.txt"),
                    ("Stakeholders", "marker_free.txt"),
                    ("Procedure", "legal_procedural_position.txt"),
                    ("Financial Aspect", "financial_aspect.txt"),
                    ("Prayer for Approval", "prayer_for_approval.txt"),
                ]
                .iter()
                .find(|(title, _)| prompt.contains(title))
                .map(|(_, f)| *f)
            })
            .ok_or("no canned output for prompt")?;
        std::fs::read_to_string(fixture_dir().join("canned_writer").join(file))
            .map_err(|e| e.to_string())
    }
}

const ASK: &str = "I want to get 4 billion to develop schools/IT institutes for youngsters";

fn stub_embed(dim: usize) -> impl Fn(&str) -> Result<Vec<f32>, String> + Send + Sync {
    move |query: &str| {
        // Deterministic query→direction mapping: nudge towards a chunk family
        // per role-hint keyword so each section retrieves sensibly.
        let seed = if query.contains("financial powers") {
            3
        } else if query.contains("PC-I") || query.contains("development project") {
            5
        } else if query.contains("routing") || query.contains("rules of business") {
            1
        } else {
            7
        };
        Ok(unit_vec(dim, seed))
    }
}

#[tokio::test]
async fn one_ask_produces_verified_seven_section_draft() {
    let (pool, dim) = build_db().await;
    let parsed = intent::parse_intent(ASK).unwrap();
    let st = state::OneAskState::new(parsed);
    state::save(&pool, "d1", &st).await.unwrap();

    let stages: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let embed = stub_embed(dim);
    let writer = CannedWriter { overrides: vec![] };
    run_pipeline(&pool, &embed, &writer, "d1", false, &CancellationToken::new(), &|s: Stage| {
        stages.lock().unwrap().push(format!("{s:?}"));
    })
    .await
    .unwrap();

    let st = state::load(&pool, "d1").await.unwrap().unwrap();
    assert_eq!(st.status, Status::Ready);
    assert!(!st.cards.is_empty());

    // US10/FR-019: 7 single-move paragraphs (financial included — the ASK has a
    // figure), continuous numbering.
    assert_eq!(st.paragraphs.len(), 7);
    assert_eq!(
        st.paragraphs.iter().map(|p| p.number).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6, 7]
    );

    let provision_re = Regex::new(r"(?i)\b(article|rule|section|regulation)[-\s]+\d").unwrap();
    let mut citation_count = 0usize;
    let mut figure_found = false;
    for p in &st.paragraphs {
        for r in &p.runs {
            match r {
                Run::Citation { verified, rendered_label, quoted, .. } => {
                    citation_count += 1;
                    assert!(*verified, "unverified citation shipped: {rendered_label}");
                    assert!(quoted.is_some());
                }
                Run::Prose { text } => {
                    // SC-002 surface: no foreign provision numbers in prose.
                    assert!(
                        !provision_re.is_match(text),
                        "provision number leaked into prose: {text}"
                    );
                    assert!(!text.contains("Rs."), "money figure leaked into prose: {text}");
                }
                Run::Figure { text, .. } => {
                    if text == "4 billion" {
                        figure_found = true;
                    }
                }
                Run::Flag { .. } => {}
            }
        }
    }
    // US10: citation-bearing moves (legal validation / status / procedure ≤2 each,
    // financial ≤1) bound the draft; at least one verified citation must render,
    // and the per-section caps keep it within the budget.
    assert!(
        (1..=7).contains(&citation_count),
        "expected 1..=7 verified citations under the per-move caps, got {citation_count}"
    );
    assert!(figure_found, "officer figure '4 billion' must appear verbatim (FR-003)");

    // FR-002: no boilerplate anywhere.
    let all_text = serde_json::to_string(&st.paragraphs).unwrap();
    for forbidden in ["letterhead", "Signature", "C.C.", "REFERENCE NO"] {
        assert!(!all_text.contains(forbidden), "boilerplate leaked: {forbidden}");
    }

    // FR-013 stage flow.
    let log = stages.lock().unwrap().join(" → ");
    assert!(log.starts_with("Researching"));
    assert!(log.contains("Composing { index: 1, total: 7"));
    assert!(log.contains("Composing { index: 7, total: 7"));
    assert!(log.contains("Verifying"));
    assert!(log.ends_with("Ready"));
}

/// US4/D7: a writer that panics if called — proves deterministic drafting
/// authors prose itself and never invokes the LLM.
struct PanicWriter;
impl SectionWriter for PanicWriter {
    fn write(&self, _prompt: &str) -> Result<String, String> {
        panic!("US4/D7 violated: deterministic drafting must not call the writer");
    }
}

#[tokio::test]
async fn deterministic_draft_never_calls_writer_and_verifies() {
    let (pool, dim) = build_db().await;
    let st = state::OneAskState::new(intent::parse_intent(ASK).unwrap());
    state::save(&pool, "d5", &st).await.unwrap();

    let embed = stub_embed(dim);
    let writer = PanicWriter; // would panic if the deterministic lane called it
    run_pipeline(&pool, &embed, &writer, "d5", true, &CancellationToken::new(), &|_| {})
        .await
        .unwrap();

    let st = state::load(&pool, "d5").await.unwrap().unwrap();
    assert_eq!(st.status, Status::Ready);
    // 7 single-move paragraphs (ASK carries a figure → Financial renders).
    assert_eq!(st.paragraphs.len(), 7);

    let provision_re = Regex::new(r"(?i)\b(article|rule|section|regulation)[-\s]+\d").unwrap();
    let mut verified_cites = 0usize;
    let mut figure_found = false;
    for p in &st.paragraphs {
        for r in &p.runs {
            match r {
                Run::Citation { verified, quoted, .. } => {
                    assert!(*verified, "deterministic lane shipped an unverified citation");
                    assert!(quoted.is_some());
                    verified_cites += 1;
                }
                Run::Prose { text } => {
                    assert!(!provision_re.is_match(text), "provision number in prose: {text}");
                    assert!(!text.contains("Rs."), "money figure in prose: {text}");
                }
                Run::Figure { text, .. } => {
                    if text == "4 billion" {
                        figure_found = true;
                    }
                }
                Run::Flag { .. } => {}
            }
        }
    }
    assert!(verified_cites >= 1, "deterministic draft must splice >=1 verified citation");
    assert!(figure_found, "officer figure '4 billion' must appear verbatim via marker");
}

#[tokio::test]
async fn malicious_writer_output_is_flagged_never_cited() {
    // The writer "misbehaves": invents Article 99 + Rs. 9 crore in prose.
    let (pool, dim) = build_db().await;
    let st = state::OneAskState::new(intent::parse_intent(ASK).unwrap());
    state::save(&pool, "d2", &st).await.unwrap();

    let embed = stub_embed(dim);
    let writer = CannedWriter {
        overrides: vec![("Legal Validation", "malformed_strays.txt")],
    };
    run_pipeline(&pool, &embed, &writer, "d2", false, &CancellationToken::new(), &|_| {})
        .await
        .unwrap();

    let st = state::load(&pool, "d2").await.unwrap().unwrap();
    assert_eq!(st.status, Status::Ready);
    let legal = &st.paragraphs[1];
    let has_flag = legal.runs.iter().any(|r| matches!(
        r,
        Run::Flag { reason, .. } if reason.contains("Article 99") || reason.contains("Rs. 9 crore")
    ));
    assert!(has_flag, "invented references must surface as flags: {:?}", legal.runs);
    // And the invented text never appears as Prose or Citation.
    for r in &legal.runs {
        if let Run::Prose { text } = r {
            assert!(!text.contains("Article 99") && !text.contains("Rs. 9 crore"));
        }
        if let Run::Citation { quoted, .. } = r {
            assert!(!quoted.as_deref().unwrap_or("").contains("Article 99"));
        }
    }
}

#[tokio::test]
async fn cancellation_is_clean_and_marker_free_output_tolerated() {
    let (pool, dim) = build_db().await;
    let st = state::OneAskState::new(intent::parse_intent(ASK).unwrap());
    state::save(&pool, "d3", &st).await.unwrap();

    // Pre-cancelled token → pipeline exits Cancelled after research.
    let token = CancellationToken::new();
    token.cancel();
    let embed = stub_embed(dim);
    let writer = CannedWriter { overrides: vec![] };
    run_pipeline(&pool, &embed, &writer, "d3", false, &token, &|_| {}).await.unwrap();
    let st = state::load(&pool, "d3").await.unwrap().unwrap();
    assert_eq!(st.status, Status::Cancelled);

    // Marker-free writer output (prayer fixture is naturally marker-light) —
    // a fully marker-free section yields a single prose run, still verified.
    let st4 = state::OneAskState::new(intent::parse_intent(ASK).unwrap());
    state::save(&pool, "d4", &st4).await.unwrap();
    let writer = CannedWriter {
        overrides: vec![("Urgency", "marker_free.txt")],
    };
    run_pipeline(&pool, &embed, &writer, "d4", false, &CancellationToken::new(), &|_| {})
        .await
        .unwrap();
    let st4 = state::load(&pool, "d4").await.unwrap().unwrap();
    assert!(matches!(st4.paragraphs[0].runs[0], Run::Prose { .. }));
}
