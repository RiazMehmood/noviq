//! Multi-model evaluation harness.
//!
//! Runs the same 5 US1 queries through `intent_router::dispatch` (the
//! exact pipeline the chat UI hits) against each LOCAL model by
//! hot-swapping the GGUF the discover_local_gguf() resolver sees.
//!
//! Cloud (Groq llama-3.3-70b) is NOT exercised here — the cloud routing
//! in `route_scoped_with_override` requires `app: Some(AppHandle)`,
//! which can't be constructed outside Tauri's runtime. The test does
//! write the cloud_settings.json with the correct config at the end so
//! the user can flip the toggle in Settings, restart the app, and
//! re-run the 5 queries via the chat UI to get the cloud numbers.
//!
//! Run: `cargo test --test multimodel_eval -- --ignored --nocapture`
//! Time: ~30-60 minutes wall (model loads + 5 queries × 3 models).
//! Output: /tmp/echoai_eval_results.md

use echo_ai_lib::services::{
    embedder::EmbedderState,
    hardware_detector::HardwareTier,
    intent_router::{self, types::ChatQuery, types::Scope},
    llm::ModelSlot,
    reranker::RerankerSlot,
};
use std::sync::Arc;
use std::time::Instant;

const QUERIES: &[(&str, &str)] = &[
    ("Q1",       "What are the Principles of Policy listed in Chapter 2 of Part II?"),
    ("Q11",      "Which Articles fall under Chapter 1 of Part II (Fundamental Rights)? List them with titles."),
    ("Q1-fake",  "What are the duties listed in Chapter 3 of Part XYZ?"),
    ("Q3",       "What rights of an arrested person are guaranteed across Articles 9, 10, and 10A taken together?"),
    ("Q2",       "Summarise Article 19 on freedom of speech, including its stated restrictions."),
];

struct ModelDef {
    name: &'static str,
    gguf_src: &'static str,
}

const MODELS: &[ModelDef] = &[
    ModelDef {
        name: "0.5B-sindh-qwen",
        gguf_src: "/media/riaz/New Volume/echo-AI/models/sindh_qwen2.5-0.5b-instruct.Q4_K_M.gguf",
    },
    ModelDef {
        name: "3B-llama-3.2",
        gguf_src: "/media/riaz/New Volume/ollama/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
    },
    ModelDef {
        name: "7B-mistral-v0.3",
        gguf_src: "/media/riaz/New Volume/echo-AI/models/ir_builder/Mistral-7B-Instruct-v0.3-Q4_K_M.gguf",
    },
];

/// PG connection URL of the RUNNING app instance (bundled engine). Read it
/// from `{app_data_dir}/pg-instance.json` and export as ECHO_PG_URL.
fn db_url() -> String {
    std::env::var("ECHO_PG_URL").expect("set ECHO_PG_URL to the app's bundled-PG connection URL")
}
const NOTEBOOK_ID: &str = "c51eee88-b775-4fa6-912d-73244911110d";
const SESSION_ID: &str = "multimodel-eval-session-001";
const RESULTS_FILE: &str = "/tmp/echoai_eval_results.md";

fn build_temp_models_dir(gguf_src: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("create temp dir");
    let target = temp.path().join("model.gguf");
    // Symlink keeps the test fast — no 4 GB copy of Mistral per model swap.
    std::os::unix::fs::symlink(gguf_src, &target).expect("symlink gguf");
    temp
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn multimodel_eval_5_queries_across_local_models() {
    // ── Connect to the LIVE app DB ───────────────────────────────────────
    // Caller MUST stop the running app before kicking this off.
    let pool = sqlx::postgres::PgPool::connect(&db_url())
        .await
        .expect("connect to the app's bundled PostgreSQL (ECHO_PG_URL)");

    // Ensure the session row exists so route_scoped's session_repo calls
    // succeed if any sneak past dispatch.
    sqlx::query(
        "INSERT INTO query_sessions (id, notebook_id, created_at, updated_at) \
         VALUES ($1, $2, 0, 0) ON CONFLICT (id) DO NOTHING",
    )
    .bind(SESSION_ID)
    .bind(NOTEBOOK_ID)
    .execute(&pool)
    .await
    .expect("create eval session");

    let tier = HardwareTier::detect();
    eprintln!("Hardware tier: max_model_tier={} ram_gb={}", tier.max_model_tier, tier.ram_gb);

    let embedder = Arc::new(EmbedderState::new(&tier));
    let model_slot = ModelSlot::new().expect("init model_slot");
    let reranker_slot = RerankerSlot::new("/dev/null", "/dev/null");

    let history = Vec::new();
    let mut results: Vec<ModelResult> = Vec::new();

    for model in MODELS {
        eprintln!("\n──── Testing model: {} ────", model.name);
        let _temp = build_temp_models_dir(model.gguf_src);
        std::env::set_var("ECHO_MODELS_DIR", _temp.path());

        let mut per_query: Vec<QueryResult> = Vec::new();

        for (qid, qtext) in QUERIES {
            eprintln!("  → {qid}");
            let t0 = Instant::now();

            let query = ChatQuery {
                text: qtext.to_string(),
                scope: Scope::Both,
                use_cloud: false,
                thread_id: None,
            };

            let outcome = match intent_router::dispatch(
                &pool,
                &embedder,
                &model_slot,
                &reranker_slot,
                &tier,
                NOTEBOOK_ID,
                SESSION_ID,
                &history,
                query,
                None, // cancel
                None, // app handle — forces local routing
            )
            .await
            {
                Ok(resp) => {
                    let bubble = resp.bubbles.first().cloned().unwrap_or_default_bubble();
                    let cites = resp.bubbles.first().map(|b| b.citations.len()).unwrap_or(0);
                    QueryOutcome::Ok {
                        intent: format!("{:?}", resp.intent),
                        confidence: resp.intent_confidence,
                        headline: bubble.headline.clone(),
                        body: bubble.body.clone(),
                        citation_count: cites,
                        upgrade_prompt: resp.upgrade_prompt.is_some(),
                    }
                }
                Err(e) => QueryOutcome::Err(e.to_string()),
            };

            let elapsed = t0.elapsed().as_secs_f32();
            eprintln!("     ({elapsed:.1}s)");
            per_query.push(QueryResult {
                qid: qid.to_string(),
                question: qtext.to_string(),
                elapsed_s: elapsed,
                outcome,
            });
        }
        results.push(ModelResult {
            name: model.name.to_string(),
            queries: per_query,
        });
    }

    // ── Stage cloud_settings.json so the user can flip the toggle and re-run manually ──
    // Key comes from the environment — never hardcode a credential in the tree.
    // Export GROQ_API_KEY before running; without it the staged config is inert
    // and the user pastes their own key in Settings.
    let groq_api_key = std::env::var("GROQ_API_KEY").unwrap_or_default();
    if groq_api_key.is_empty() {
        eprintln!("\nGROQ_API_KEY not set — staging cloud_settings.json without a key.");
    }
    let groq_settings = serde_json::json!({
        "enabled": false, // user must toggle ON in Settings UI
        "provider": "groq",
        "api_key": groq_api_key,
        "model": "llama-3.3-70b-versatile",
        "endpoint_url": "",
    });
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/riaz".to_string());
    {
        let dst = std::path::PathBuf::from(&home).join(".local/share/com.echoai.app/cloud_settings.json");
        // Backup the current one so we don't clobber custom config.
        let backup = dst.with_extension("json.bak-pre-eval");
        if dst.exists() {
            let _ = std::fs::copy(&dst, &backup);
        }
        let _ = std::fs::write(&dst, serde_json::to_string_pretty(&groq_settings).unwrap());
        eprintln!("\nGroq config staged at {} (backup at {})", dst.display(), backup.display());
    }

    write_markdown(&results);
    eprintln!("\nResults written to {RESULTS_FILE}");
}

// ─── support types ──────────────────────────────────────────────────────

struct ModelResult {
    name: String,
    queries: Vec<QueryResult>,
}

struct QueryResult {
    qid: String,
    question: String,
    elapsed_s: f32,
    outcome: QueryOutcome,
}

enum QueryOutcome {
    Ok {
        intent: String,
        confidence: f32,
        headline: String,
        body: String,
        citation_count: usize,
        upgrade_prompt: bool,
    },
    Err(String),
}

trait BubbleExt {
    fn unwrap_or_default_bubble(self) -> echo_ai_lib::services::intent_router::types::ChatBubble;
}
impl BubbleExt for Option<echo_ai_lib::services::intent_router::types::ChatBubble> {
    fn unwrap_or_default_bubble(self) -> echo_ai_lib::services::intent_router::types::ChatBubble {
        self.unwrap_or_else(|| {
            echo_ai_lib::services::intent_router::types::ChatBubble {
                headline: String::new(),
                body: "(no bubble returned)".to_string(),
                citations: vec![],
                see_also: vec![],
                flow: None,
            }
        })
    }
}

fn write_markdown(results: &[ModelResult]) {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "# EchoAI multi-model eval — 5 US1 queries").unwrap();
    writeln!(out, "\nDate: 2026-06-04\n").unwrap();
    writeln!(out, "Models tested through the live `intent_router::dispatch` pipeline.\n").unwrap();
    writeln!(out, "Cloud (Groq llama-3.3-70b) requires manual UI toggle — see instructions at the bottom.\n").unwrap();

    for model in results {
        writeln!(out, "\n---\n\n## Model: `{}`\n", model.name).unwrap();
        for q in &model.queries {
            writeln!(out, "### {} — `{}`", q.qid, q.question).unwrap();
            writeln!(out, "_Elapsed: {:.1}s_\n", q.elapsed_s).unwrap();
            match &q.outcome {
                QueryOutcome::Ok { intent, confidence, headline, body, citation_count, upgrade_prompt } => {
                    writeln!(out, "- Intent: `{}` (conf {:.2})", intent, confidence).unwrap();
                    writeln!(out, "- Headline: **{}**", headline).unwrap();
                    writeln!(out, "- Citations returned: **{}**", citation_count).unwrap();
                    if *upgrade_prompt {
                        writeln!(out, "- ⚠ UpgradePrompt fired").unwrap();
                    }
                    writeln!(out, "\n**Answer:**\n").unwrap();
                    writeln!(out, "```\n{}\n```\n", body).unwrap();
                }
                QueryOutcome::Err(e) => {
                    writeln!(out, "- **ERROR**: {}", e).unwrap();
                }
            }
        }
    }

    writeln!(out, "\n---\n\n## Cloud (Groq llama-3.3-70b-versatile)\n").unwrap();
    writeln!(out, "Cloud routing is gated on `app: Option<AppHandle>` being `Some`, which can't be").unwrap();
    writeln!(out, "constructed outside Tauri's runtime. The test has staged the cloud config at").unwrap();
    writeln!(out, "`~/.local/share/com.echoai.app/cloud_settings.json` (enabled=false; toggle ON in Settings).\n").unwrap();
    writeln!(out, "To capture cloud numbers:").unwrap();
    writeln!(out, "1. Start the app: `npm run tauri dev`").unwrap();
    writeln!(out, "2. Open Settings → enable the cloud LLM toggle").unwrap();
    writeln!(out, "3. Run the 5 queries above via chat").unwrap();
    writeln!(out, "4. Paste the answers back\n").unwrap();

    std::fs::write(RESULTS_FILE, out).expect("write results");
}
