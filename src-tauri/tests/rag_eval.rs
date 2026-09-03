//! Faithful RAG eval harness (Phase-0 fix 0d).
//!
//! Runs `data/eval/sindh_eval_v1.json` through the REAL pipeline —
//! `intent_router::dispatch` → `query_router::route_scoped` — against the LIVE
//! indexed corpus. Retrieval is 100% production: FTS word-search + pgvector KNN
//! + RRF, the real fastembed embedder, and the real local 0.5B model. The
//! reranker stays DISABLED (production default — ECHO_RERANK unset).
//!
//! Scope = `Both` (whole corpus). `app: None` forces local routing (no cloud).
//!
//! Run (keep the app running so its bundled PG is up; reads are concurrent-safe):
//!   ECHO_PG_URL=postgres://postgres:<pw>@127.0.0.1:40371/echo \
//!   cargo test --test rag_eval -- --ignored --nocapture
//!
//! Output: stdout summary + /tmp/sindh_eval_report.md
//! Time: model load + ~15 questions × ~45 s ≈ 12–15 min.

use echo_ai_lib::db::pg::settings::tuning_for;
use echo_ai_lib::services::{
    embedder::EmbedderState,
    hardware_detector::HardwareTier,
    intent_router::{self, types::ChatQuery, types::Scope},
    llm::ModelSlot,
    reranker::RerankerSlot,
};
use serde::Deserialize;
use sqlx::Executor;
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Instant;

const NOTEBOOK_ID: &str = "rag-eval-notebook-0001";
const SESSION_ID: &str = "rag-eval-session-0001";
const REPORT_FILE: &str = "/tmp/sindh_eval_report.md";

#[derive(Deserialize)]
struct EvalSet {
    questions: Vec<Question>,
}

#[derive(Deserialize)]
struct Question {
    id: String,
    #[serde(default)]
    risk: String,
    question: String,
    #[serde(default)]
    expect_facts: Vec<String>,
    #[serde(default)]
    expect_source: String,
    #[serde(default)]
    expect_provision: Option<String>,
}

struct Scored {
    id: String,
    risk: String,
    question: String,
    elapsed_s: f32,
    answer: String,
    citation_count: usize,
    factual: bool,
    source_ok: bool,
    provision_ok: bool,
    error: Option<String>,
}

/// Lowercase, drop commas/newlines, collapse whitespace — tolerant to OCR
/// spacing ("Rs . 10,000" → "rs . 10000") so fact matching is robust.
fn norm(s: &str) -> String {
    let lowered: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .collect();
    lowered.replace(',', "").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn all_facts_present(answer: &str, facts: &[String]) -> bool {
    let h = norm(answer);
    !facts.is_empty() && facts.iter().all(|f| h.contains(&norm(f)))
}

/// `;`-separated expected sources/provisions — every one must appear in the
/// haystack (answer body + citation labels). Source match is lenient: full
/// normalised stem OR its distinctive 10-char prefix (e.g. "06_manual_").
fn all_present(haystack: &str, expected: &str) -> bool {
    let h = norm(haystack);
    expected.split(';').filter(|s| !s.trim().is_empty()).all(|item| {
        let n = norm(item);
        let key: String = n.chars().take(10).collect();
        h.contains(&n) || (key.len() >= 6 && h.contains(&key))
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn rag_eval_sindh_v1() {
    // ── Load the eval set ────────────────────────────────────────────────
    let set_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../data/eval/sindh_eval_v1.json");
    let raw = std::fs::read_to_string(&set_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", set_path.display()));
    let set: EvalSet = serde_json::from_str(&raw).expect("parse eval set");
    eprintln!("Loaded {} eval questions", set.questions.len());

    // ── Connect to the LIVE app PG; replicate hnsw.ef_search per tier ────
    let url = std::env::var("ECHO_PG_URL")
        .expect("set ECHO_PG_URL to the app's bundled-PG URL (postgres://postgres:<pw>@127.0.0.1:40371/echo)");
    let tier = HardwareTier::detect();
    let ef = tuning_for(&tier).hnsw_ef_search;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .after_connect(move |conn, _| {
            Box::pin(async move {
                conn.execute(format!("SET hnsw.ef_search = {ef}").as_str()).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("connect to app PG (ECHO_PG_URL)");
    eprintln!("Tier max={} ram_gb={} ef_search={ef}", tier.max_model_tier, tier.ram_gb);

    // Session row so any session_repo writes inside the pipeline succeed.
    sqlx::query(
        "INSERT INTO query_sessions (id, notebook_id, created_at, updated_at) \
         VALUES ($1, $2, 0, 0) ON CONFLICT (id) DO NOTHING",
    )
    .bind(SESSION_ID)
    .bind(NOTEBOOK_ID)
    .execute(&pool)
    .await
    .ok();
    // Clear the semantic cache so every question really hits retrieval + LLM.
    let _ = sqlx::query("DELETE FROM query_cache").execute(&pool).await;

    // ── Real pipeline deps (reranker disabled => paths unused) ───────────
    let embedder = Arc::new(EmbedderState::new(&tier));
    let model_slot = ModelSlot::new().expect("init model_slot");
    let reranker_slot = RerankerSlot::new("/dev/null", "/dev/null");
    let history = Vec::new();

    let mut results: Vec<Scored> = Vec::new();

    for q in &set.questions {
        eprintln!("→ {} ({})", q.id, q.risk);
        let t0 = Instant::now();
        let query = ChatQuery {
            text: q.question.clone(),
            scope: Scope::Both,
            use_cloud: false,
            thread_id: None,
        };
        let outcome = intent_router::dispatch(
            &pool, &embedder, &model_slot, &reranker_slot, &tier,
            NOTEBOOK_ID, SESSION_ID, &history, query, None, None,
        )
        .await;
        let elapsed = t0.elapsed().as_secs_f32();

        let scored = match outcome {
            Ok(resp) => {
                // Concatenate all bubble bodies + every citation label into one
                // answer/haystack (covers LLM prose AND extractive verbatim).
                let mut answer = String::new();
                let mut cite_haystack = String::new();
                let mut cite_count = 0usize;
                for b in &resp.bubbles {
                    answer.push_str(&b.headline);
                    answer.push(' ');
                    answer.push_str(&b.body);
                    answer.push('\n');
                    for c in &b.citations {
                        cite_count += 1;
                        cite_haystack.push_str(&c.officer_format);
                        cite_haystack.push(' ');
                        cite_haystack.push_str(&c.short_format);
                        cite_haystack.push(' ');
                    }
                }
                let full = format!("{answer}\n{cite_haystack}");
                Scored {
                    id: q.id.clone(),
                    risk: q.risk.clone(),
                    question: q.question.clone(),
                    elapsed_s: elapsed,
                    factual: all_facts_present(&answer, &q.expect_facts),
                    source_ok: all_present(&full, &q.expect_source),
                    provision_ok: q
                        .expect_provision
                        .as_deref()
                        .map(|p| all_present(&full, p))
                        .unwrap_or(true),
                    citation_count: cite_count,
                    answer: answer.trim().to_string(),
                    error: None,
                }
            }
            Err(e) => Scored {
                id: q.id.clone(),
                risk: q.risk.clone(),
                question: q.question.clone(),
                elapsed_s: elapsed,
                answer: String::new(),
                citation_count: 0,
                factual: false,
                source_ok: false,
                provision_ok: false,
                error: Some(e.to_string()),
            },
        };
        eprintln!(
            "   factual={} source={} provision={} cites={} ({:.1}s)",
            scored.factual, scored.source_ok, scored.provision_ok,
            scored.citation_count, scored.elapsed_s
        );
        results.push(scored);
    }

    write_report(&results);
    print_summary(&results);
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 { "—".into() } else { format!("{:.0}%", 100.0 * n as f32 / d as f32) }
}

fn print_summary(r: &[Scored]) {
    let total = r.len();
    let f = r.iter().filter(|x| x.factual).count();
    let s = r.iter().filter(|x| x.source_ok).count();
    let p = r.iter().filter(|x| x.provision_ok).count();
    let frag: Vec<_> = r.iter().filter(|x| x.risk == "fragmented").collect();
    let clean: Vec<_> = r.iter().filter(|x| x.risk == "clean").collect();
    eprintln!("\n══════════ SINDH EVAL v1 — SUMMARY ══════════");
    eprintln!("Questions: {total}");
    eprintln!("Factual:   {f}/{total} ({})", pct(f, total));
    eprintln!("Source:    {s}/{total} ({})", pct(s, total));
    eprintln!("Provision: {p}/{total} ({})", pct(p, total));
    eprintln!(
        "  clean docs  factual: {}/{} ({})",
        clean.iter().filter(|x| x.factual).count(), clean.len(),
        pct(clean.iter().filter(|x| x.factual).count(), clean.len())
    );
    eprintln!(
        "  fragmented  factual: {}/{} ({})  <- Phase-2 signal",
        frag.iter().filter(|x| x.factual).count(), frag.len(),
        pct(frag.iter().filter(|x| x.factual).count(), frag.len())
    );
    eprintln!("Full report: {REPORT_FILE}");
}

fn write_report(r: &[Scored]) {
    let mut out = String::new();
    writeln!(out, "# Sindh eval v1 — faithful pipeline run\n").unwrap();
    writeln!(out, "Real `intent_router::dispatch` (FTS + pgvector KNN + RRF + local 0.5B), reranker off, Scope::Both.\n").unwrap();
    writeln!(out, "| id | risk | factual | source | provision | cites | s | answer (truncated) |").unwrap();
    writeln!(out, "|----|------|---------|--------|-----------|-------|---|--------------------|").unwrap();
    for x in r {
        let a = x.answer.replace('\n', " ").replace('|', "\\|");
        let a: String = a.chars().take(160).collect();
        let cell = |b: bool| if b { "✅" } else { "❌" };
        let factual = x.error.as_ref().map(|e| format!("ERR: {e}")).unwrap_or_else(|| cell(x.factual).into());
        writeln!(
            out, "| {} | {} | {} | {} | {} | {} | {:.0} | {} |",
            x.id, x.risk, factual, cell(x.source_ok), cell(x.provision_ok),
            x.citation_count, x.elapsed_s, a
        ).unwrap();
    }
    std::fs::write(REPORT_FILE, out).expect("write report");
}
