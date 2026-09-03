# Reviewer Agent + Answer Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an LLM judge that detects non-responsive chat answers, retries with progressively richer context (up to 3 times), and improves answer quality by removing the short-answer constraint and adding a summary mode for document overview requests.

**Architecture:** Seven tasks, all changes confined to three files. `query_router.rs` gets `classify_query()`, `judge_answer()`, and a retry loop inside `route()`. `llm.rs` gets an updated system prompt, a new summary prompt, and two new parameters on `infer_with_cancel`. `MessageBubble.tsx` / `QualityNoticeBanner.tsx` get a second badge style for summary notices.

**Tech Stack:** Rust 1.87 (llama-cpp-2 via feature flag), SolidJS + TypeScript frontend, SQLite (sqlx), Tauri 2.0.

---

## File Map

| File | What changes |
|------|-------------|
| `src-tauri/src/services/query_router.rs` | Add `QueryType` enum, `classify_query()`, `judge_answer()`, retry loop in `route()`, update `try_llm_infer_streaming` signature |
| `src-tauri/src/services/llm.rs` | Update `GROUNDING_SYSTEM_PROMPT` (remove 1-3 sentence constraint), add `SUMMARY_SYSTEM_PROMPT` + `SUMMARY_SYSTEM_PROMPT_VERSION`, add `n_ctx` + `max_prompt_tokens` params to `infer_with_cancel` (both feature and stub) |
| `src/components/chat/QualityNoticeBanner.tsx` | Handle `constrained=false` (blue info badge) alongside `constrained=true` (yellow hardware badge) |
| `src/components/chat/MessageBubble.tsx` | Show banner for all `qualityNotice` values, not just `constrained=true` |

---

## Task 1: classify_query() — query type classifier

**Files:**
- Modify: `src-tauri/src/services/query_router.rs` (add after the `GROUNDING_FALLBACK` const, before the `route` function)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block at the bottom of `query_router.rs`:

```rust
// ── classify_query tests ─────────────────────────────────────────────────
#[test]
fn classify_summary_keywords() {
    assert!(matches!(classify_query("summarize this book"), QueryType::Summary));
    assert!(matches!(classify_query("give me an overview"), QueryType::Summary));
    assert!(matches!(classify_query("what is the book about"), QueryType::Summary));
    assert!(matches!(classify_query("outline the document"), QueryType::Summary));
    assert!(matches!(classify_query("Summarise the report"), QueryType::Summary));
    assert!(matches!(classify_query("brief me on the contract"), QueryType::Summary));
    assert!(matches!(classify_query("list all chapters"), QueryType::Summary));
    assert!(matches!(classify_query("table of contents"), QueryType::Summary));
    assert!(matches!(classify_query("what topics does this cover"), QueryType::Summary));
}

#[test]
fn classify_regular_queries() {
    assert!(matches!(classify_query("what is article 17?"), QueryType::Regular));
    assert!(matches!(classify_query("who signed the agreement?"), QueryType::Regular));
    assert!(matches!(classify_query("define constitutional position"), QueryType::Regular));
    assert!(matches!(classify_query(""), QueryType::Regular));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /media/riaz/New\ Volume/echo-AI/src-tauri
CARGO_HOME="/media/riaz/New Volume/.cargo" \
CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
cargo test classify_ --no-default-features --features llm 2>&1 | tail -20
```

Expected: FAIL — `QueryType` and `classify_query` not defined.

- [ ] **Step 3: Add QueryType enum and classify_query() to query_router.rs**

Add immediately after the `pub const GROUNDING_FALLBACK` constant (around line 39):

```rust
/// Discriminates between a factual lookup and a broad summary/outline request.
/// Used to select the system prompt and max_tokens budget for LLM synthesis.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum QueryType {
    Regular,
    Summary,
}

/// Classify a query as Summary if it matches broad-overview trigger words.
/// Pure function — no LLM call, no allocation beyond the lowercase copy.
pub(crate) fn classify_query(query: &str) -> QueryType {
    let lower = query.to_lowercase();
    let triggers = [
        "summarize", "summarise", "summary",
        "outline", "overview",
        "what is the book about", "what does the book",
        "what does this document cover", "what does the document",
        "list all chapters", "table of contents",
        "give me an overview", "brief me", "brief on",
        "what topics", "what subjects",
    ];
    if triggers.iter().any(|t| lower.contains(t)) {
        QueryType::Summary
    } else {
        QueryType::Regular
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /media/riaz/New\ Volume/echo-AI/src-tauri
CARGO_HOME="/media/riaz/New Volume/.cargo" \
CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
cargo test classify_ --no-default-features --features llm 2>&1 | tail -10
```

Expected: `test classify_summary_keywords ... ok` and `test classify_regular_queries ... ok`

- [ ] **Step 5: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/services/query_router.rs
git commit -m "feat(reviewer): classify_query() — detect summary/outline requests"
```

---

## Task 2: Update system prompts in llm.rs

**Files:**
- Modify: `src-tauri/src/services/llm.rs` (constants section, lines 57–80)

- [ ] **Step 1: Replace GROUNDING_SYSTEM_PROMPT and add SUMMARY_SYSTEM_PROMPT**

Find the block starting at `pub const GROUNDING_SYSTEM_PROMPT_VERSION` and replace the entire prompts section (lines 59–80) with:

```rust
pub const GROUNDING_SYSTEM_PROMPT_VERSION: &str = "3.0.0";

/// Regular Q&A system prompt — answers as long as the content warrants.
/// v3.0.0: Removed "1-3 sentences" cap; model answers short for simple facts,
/// long for complex questions.
pub const GROUNDING_SYSTEM_PROMPT: &str = "\
You are a document Q&A assistant. The passages provided contain the answer — read them and respond directly.\n\n\
Rules:\n\
1. Answer using ONLY the text in the passages below.\n\
2. Copy the source label exactly — end EVERY factual sentence with [filename, page N].\n\
3. Answer as thoroughly as the content warrants — concise for simple facts, detailed for complex questions.\n\
4. Never use outside knowledge.\n\n\
Worked example:\n\
Passage: [1] constitution.pdf, page 17\n\
\"the State\" means the Federal Government, Parliament, a Provincial Government...\n\
Passage: [2] constitution.pdf, page 18\n\
The State shall take steps to secure economic advancement of all citizens.\n\
Question: What is the State and what must it do?\n\
Answer: The State means the Federal Government, Parliament, and Provincial Governments [constitution.pdf, page 17]. It must take steps to secure the economic advancement of all citizens [constitution.pdf, page 18].\n\n\
Now answer the question from the passages.";

pub const SUMMARY_SYSTEM_PROMPT_VERSION: &str = "1.0.0";

/// Summary/outline system prompt — used when classify_query() returns Summary.
/// Instructs a detailed structured response; citations optional but welcome.
pub const SUMMARY_SYSTEM_PROMPT: &str = "\
You are a document assistant. The user wants a thorough summary or overview.\n\n\
Rules:\n\
1. Synthesise information across ALL provided passages.\n\
2. Structure your response with headings or bullet points where appropriate.\n\
3. Be comprehensive — cover all major themes, sections, and key points found in the passages.\n\
4. You may cite sources as [filename, page N] when helpful, but it is not required for every sentence.\n\
5. Never use outside knowledge — base the summary entirely on the passages.\n\n\
Write a detailed, well-organised summary now.";
```

- [ ] **Step 2: Verify it compiles**

```bash
cd /media/riaz/New\ Volume/echo-AI/src-tauri
CARGO_HOME="/media/riaz/New Volume/.cargo" \
CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
cargo check --no-default-features --features llm 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 3: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/services/llm.rs
git commit -m "feat(reviewer): update GROUNDING_SYSTEM_PROMPT v3.0 + add SUMMARY_SYSTEM_PROMPT"
```

---

## Task 3: Parameterise infer_with_cancel (n_ctx + max_prompt_tokens)

Currently `infer_with_cancel` hardcodes `n_ctx=1024` and `MAX_PROMPT_TOKENS=928`. Judge calls need `n_ctx=512`; summary calls need `n_ctx=2048`. This task adds two parameters.

**Files:**
- Modify: `src-tauri/src/services/llm.rs` (both the `#[cfg(not(feature = "llm"))]` stub and the `#[cfg(feature = "llm")] mod full` implementation)

- [ ] **Step 1: Update stub infer_with_cancel (around line 138)**

Find and replace the stub `infer_with_cancel`:

```rust
// OLD:
pub fn infer_with_cancel(
    &self,
    _model_path: &str,
    _prompt: &str,
    _max_tokens: u32,
    _tier: &HardwareTier,
    _cancel: Option<tokio_util::sync::CancellationToken>,
    _app: Option<tauri::AppHandle>,
) -> Result<(String, bool)> {
    Ok(("[LLM feature not compiled]".to_string(), false))
}
```

Replace with:

```rust
pub fn infer_with_cancel(
    &self,
    _model_path: &str,
    _prompt: &str,
    _max_tokens: u32,
    _n_ctx: u32,
    _max_prompt_tokens: usize,
    _tier: &HardwareTier,
    _cancel: Option<tokio_util::sync::CancellationToken>,
    _app: Option<tauri::AppHandle>,
) -> Result<(String, bool)> {
    Ok(("[LLM feature not compiled]".to_string(), false))
}
```

- [ ] **Step 2: Update full infer_with_cancel (inside mod full, around line 313)**

Find and replace the real `infer_with_cancel` signature and its two hardcoded constants:

```rust
// OLD signature:
pub fn infer_with_cancel(
    &self,
    model_path: &str,
    prompt: &str,
    max_tokens: u32,
    tier: &HardwareTier,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<tauri::AppHandle>,
) -> Result<(String, bool)> {
    ...
    // n_ctx=1024 for chat: prompt ~400-700 tokens + response fits comfortably.
    // Kept at 1024 (not 1536) to preserve chat generation speed.
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(Some(std::num::NonZeroU32::new(1024).unwrap()))
        .with_n_batch(1024)
        ...
    let tokens = loaded.model.str_to_token(prompt, llama_cpp_2::model::AddBos::Always)?;
    const MAX_PROMPT_TOKENS: usize = 928;
    let n_prompt = tokens.len().min(MAX_PROMPT_TOKENS);
```

Replace with:

```rust
pub fn infer_with_cancel(
    &self,
    model_path: &str,
    prompt: &str,
    max_tokens: u32,
    n_ctx: u32,
    max_prompt_tokens: usize,
    tier: &HardwareTier,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<tauri::AppHandle>,
) -> Result<(String, bool)> {
    if let Some(t) = &cancel {
        if t.is_cancelled() {
            return Ok(("".to_string(), true));
        }
    }
    self.load(model_path, tier)?;
    let g = self.inner.lock().map_err(|_| anyhow::anyhow!("lock"))?;
    let loaded = g.current.as_ref().ok_or_else(|| anyhow::anyhow!("no model"))?;
    let n_ctx_nz = std::num::NonZeroU32::new(n_ctx.max(64)).unwrap();
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(Some(n_ctx_nz))
        .with_n_batch(n_ctx)
        .with_n_threads(4)
        .with_n_threads_batch(4);
    let mut ctx = loaded.model.new_context(&self.backend, ctx_params)?;
    let tokens = loaded.model.str_to_token(prompt, llama_cpp_2::model::AddBos::Always)?;
    let n_prompt = tokens.len().min(max_prompt_tokens);
    let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(n_prompt.max(1), 1);
    for (i, t) in tokens.iter().take(n_prompt).enumerate() {
        let is_last = i == n_prompt - 1;
        batch.add(*t, i as i32, &[0], is_last)?;
    }
    ctx.decode(&mut batch)?;
    let mut output = String::new();
    let mut i = n_prompt;
    let mut n_generated: u32 = 0;
    let mut decoder = UTF_8.new_decoder();
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::penalties(64, 1.3, 0.0, 0.0),
        LlamaSampler::temp(0.1),
        LlamaSampler::greedy(),
    ]);
    while n_generated < max_tokens {
        if let Some(t) = &cancel {
            if t.is_cancelled() {
                return Ok((output, true));
            }
        }
        let token = sampler.sample(&ctx, -1);
        if loaded.model.is_eog_token(token) { break; }
        let token_str = loaded.model.token_to_piece(token, &mut decoder, false, None)?;
        if let Some(a) = &app {
            use tauri::Emitter;
            let _ = a.emit("chat://token", serde_json::json!({ "token": &token_str }));
        }
        output.push_str(&token_str);
        batch.clear();
        batch.add(token, i as i32, &[0], true)?;
        ctx.decode(&mut batch)?;
        i += 1;
        n_generated += 1;
    }
    Ok((output, false))
}
```

- [ ] **Step 3: Update try_llm_infer_streaming in query_router.rs to pass the new params**

Find `try_llm_infer_streaming` (around line 741) and update its signature and body:

```rust
// OLD:
async fn try_llm_infer_streaming(
    model_slot: &ModelSlot,
    model_path: &str,
    prompt: &str,
    max_tokens: u32,
    tier: &HardwareTier,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<tauri::AppHandle>,
) -> Result<(String, bool)> {
    tokio::task::spawn_blocking({
        let model_slot = unsafe {
            &*(model_slot as *const ModelSlot)
        };
        let model_path = model_path.to_string();
        let tier = tier.clone();
        let prompt = prompt.to_string();
        move || model_slot.infer_with_cancel(&model_path, &prompt, max_tokens, &tier, cancel, app)
    })
    .await?
}
```

Replace with:

```rust
async fn try_llm_infer_streaming(
    model_slot: &ModelSlot,
    model_path: &str,
    prompt: &str,
    max_tokens: u32,
    n_ctx: u32,
    max_prompt_tokens: usize,
    tier: &HardwareTier,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<tauri::AppHandle>,
) -> Result<(String, bool)> {
    tokio::task::spawn_blocking({
        let model_slot = unsafe {
            &*(model_slot as *const ModelSlot)
        };
        let model_path = model_path.to_string();
        let tier = tier.clone();
        let prompt = prompt.to_string();
        move || model_slot.infer_with_cancel(
            &model_path, &prompt, max_tokens, n_ctx, max_prompt_tokens, &tier, cancel, app,
        )
    })
    .await?
}
```

- [ ] **Step 4: Update the call site in route() to pass n_ctx=1024, max_prompt_tokens=768**

Find the `try_llm_infer_streaming` call in `route()` (around line 204):

```rust
// OLD:
let (answer, tier_used, was_cancelled) =
    match try_llm_infer_streaming(model_slot, &model_path, &prompt, 256, tier, cancel, app).await {
```

Replace with:

```rust
let (answer, tier_used, was_cancelled) =
    match try_llm_infer_streaming(
        model_slot, &model_path, &prompt,
        256, 1024, 768,
        tier, cancel, app,
    ).await {
```

- [ ] **Step 5: Verify cargo check passes**

```bash
cd /media/riaz/New\ Volume/echo-AI/src-tauri
CARGO_HOME="/media/riaz/New Volume/.cargo" \
CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
cargo check --no-default-features --features llm 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 6: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/services/llm.rs src-tauri/src/services/query_router.rs
git commit -m "refactor(llm): parameterise infer_with_cancel n_ctx + max_prompt_tokens"
```

---

## Task 4: Wire summary mode into route()

Use `classify_query()` to select the correct prompt, max_tokens, n_ctx, and quality notice for each query.

**Files:**
- Modify: `src-tauri/src/services/query_router.rs`

First, add the import for `SUMMARY_SYSTEM_PROMPT` at the top of the file. Find the existing import line:

```rust
use crate::services::llm::{ModelSlot, GROUNDING_SYSTEM_PROMPT, GROUNDING_SYSTEM_PROMPT_VERSION};
```

Replace with:

```rust
use crate::services::llm::{
    ModelSlot,
    GROUNDING_SYSTEM_PROMPT, GROUNDING_SYSTEM_PROMPT_VERSION,
    SUMMARY_SYSTEM_PROMPT, SUMMARY_SYSTEM_PROMPT_VERSION,
};
```

- [ ] **Step 1: Classify the query early in route() — before retrieval**

Find in `route()` the line `let t_route = std::time::Instant::now();` and add the classify call immediately after it:

```rust
    let t_route = std::time::Instant::now();
    // Classify before retrieval so ctx_top_k / ctx_budget affect the first fetch.
    let query_type = classify_query(query_text);
    let (system_prompt, gen_max_tokens, gen_n_ctx, gen_max_prompt, ctx_top_k, ctx_budget) = match query_type {
        QueryType::Summary => (SUMMARY_SYSTEM_PROMPT, 512u32, 2048u32, 1400usize, 12usize, 640usize),
        QueryType::Regular => (GROUNDING_SYSTEM_PROMPT, 256u32, 1024u32, 768usize, CONTEXT_TOP_K, CONTEXT_TOKEN_BUDGET),
    };
    log::info!(
        "Query type: {:?}, max_tokens={gen_max_tokens}, n_ctx={gen_n_ctx}, ctx_k={ctx_top_k}",
        query_type
    );
```

Then update `fetch_limit` to use `ctx_top_k` instead of `CONTEXT_TOP_K`. Find:

```rust
    let fetch_limit = (CONTEXT_TOP_K * 2) as i64;
```

Replace with:

```rust
    let fetch_limit = (ctx_top_k * 2) as i64;
```

Then update `build_context` call to use `ctx_top_k` and `ctx_budget`. Find:

```rust
    let chunks = build_context(&raw_chunks, CONTEXT_TOP_K, CONTEXT_TOKEN_BUDGET);
```

Replace with:

```rust
    let chunks = build_context(&raw_chunks, ctx_top_k, ctx_budget);
```

- [ ] **Step 2: Update build_rag_prompt to accept system_prompt parameter**

Find `fn build_rag_prompt` (around line 593) and update its signature:

```rust
// OLD:
fn build_rag_prompt(query: &str, context: &str, history: &[HistoryTurn]) -> String {
    log::debug!(
        "LLM prompt uses grounding system prompt v{}",
        GROUNDING_SYSTEM_PROMPT_VERSION
    );
    // ... prior_turns building ...
    format!(
        "<|im_start|>system\n\
{system}\
<|im_end|>\n\
{prior}\
<|im_start|>user\n\
Context passages:\n\
{context}\n\n\
Question: {query}<|im_end|>\n\
<|im_start|>assistant\n",
        system = GROUNDING_SYSTEM_PROMPT,
        prior = prior_turns,
        context = context,
        query = query,
    )
}
```

Replace with:

```rust
fn build_rag_prompt(
    query: &str,
    context: &str,
    history: &[HistoryTurn],
    system_prompt: &str,
    focus_hint: bool,
) -> String {
    log::debug!(
        "LLM prompt uses system prompt v{}/v{}",
        GROUNDING_SYSTEM_PROMPT_VERSION,
        SUMMARY_SYSTEM_PROMPT_VERSION,
    );

    let mut prior_turns = String::new();
    let pairs: Vec<&[HistoryTurn]> = history
        .chunks(2)
        .filter(|c| c.len() == 2 && c[0].role == "user" && c[1].role == "assistant")
        .collect();
    for pair in pairs.iter().rev().take(2).rev() {
        let q = pair[0].content.trim();
        let a = truncate(pair[1].content.trim(), 150);
        prior_turns.push_str(&format!(
            "<|im_start|>user\n{q}<|im_end|>\n<|im_start|>assistant\n{a}<|im_end|>\n"
        ));
    }

    let question_line = if focus_hint {
        format!("Focus specifically on answering this question: {query}")
    } else {
        format!("Question: {query}")
    };

    format!(
        "<|im_start|>system\n\
{system}\
<|im_end|>\n\
{prior}\
<|im_start|>user\n\
Context passages:\n\
{context}\n\n\
{question_line}<|im_end|>\n\
<|im_start|>assistant\n",
        system = system_prompt,
        prior = prior_turns,
        context = context,
        question_line = question_line,
    )
}
```

- [ ] **Step 3: Update the call to build_rag_prompt in route()**

Find `let prompt = build_rag_prompt(query_text, &context, history);` and replace with:

```rust
    let prompt = build_rag_prompt(query_text, &context, history, system_prompt, false);
```

- [ ] **Step 4: Update try_llm_infer_streaming call to use gen_* vars**

Find the `try_llm_infer_streaming` call you updated in Task 3 Step 4:

```rust
    let (answer, tier_used, was_cancelled) =
        match try_llm_infer_streaming(
            model_slot, &model_path, &prompt,
            256, 1024, 768,
            tier, cancel, app,
        ).await {
```

Replace with:

```rust
    let (answer, tier_used, was_cancelled) =
        match try_llm_infer_streaming(
            model_slot, &model_path, &prompt,
            gen_max_tokens, gen_n_ctx, gen_max_prompt,
            tier, cancel, app,
        ).await {
```

- [ ] **Step 5: Add summary quality notice**

Find the `effective_notice` block (around line 342). After the closing `};` of that block and before `let response = QueryResponse {`, add:

```rust
    // For summary queries, always attach the "not cited" info notice.
    let effective_notice = if matches!(query_type, QueryType::Summary) && tier_used != "error" {
        Some(QualityNotice {
            constrained: false,
            message: "General summary — content synthesised from document; specific claims may not be individually cited.".to_string(),
        })
    } else {
        effective_notice
    };
```

- [ ] **Step 6: cargo check**

```bash
cd /media/riaz/New\ Volume/echo-AI/src-tauri
CARGO_HOME="/media/riaz/New Volume/.cargo" \
CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
cargo check --no-default-features --features llm 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 7: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/services/query_router.rs src-tauri/src/services/llm.rs
git commit -m "feat(reviewer): summary mode — SUMMARY_SYSTEM_PROMPT, n_ctx=2048, 512 tokens"
```

---

## Task 5: judge_answer() function + tests

**Files:**
- Modify: `src-tauri/src/services/query_router.rs`

- [ ] **Step 1: Write failing tests for judge output parsing**

Add to the test module in `query_router.rs`:

```rust
// ── judge output parsing tests ───────────────────────────────────────────
#[test]
fn judge_yes_response_passes() {
    assert!(parse_judge_output("YES"));
    assert!(parse_judge_output("Yes, it does."));
    assert!(parse_judge_output("yes"));
    assert!(parse_judge_output("  YES  "));
}

#[test]
fn judge_no_response_fails() {
    assert!(!parse_judge_output("NO"));
    assert!(!parse_judge_output("No, it does not."));
    assert!(!parse_judge_output("no"));
}

#[test]
fn judge_empty_or_unclear_fails() {
    assert!(!parse_judge_output(""));
    assert!(!parse_judge_output("   "));
    assert!(!parse_judge_output("Maybe"));
    assert!(!parse_judge_output("unclear"));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /media/riaz/New\ Volume/echo-AI/src-tauri
CARGO_HOME="/media/riaz/New Volume/.cargo" \
CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
cargo test judge_ --no-default-features --features llm 2>&1 | tail -10
```

Expected: FAIL — `parse_judge_output` not defined.

- [ ] **Step 3: Add parse_judge_output() and judge_answer() to query_router.rs**

Add these two functions after `try_llm_infer_streaming` (after the `truncate` helper, before the grounding validator section):

```rust
/// Parse YES/NO output from the LLM judge.
/// Returns true (answer is responsive) only when output explicitly contains "YES".
/// Any unclear, empty, or "NO" output is treated as a fail → trigger retry.
pub(crate) fn parse_judge_output(output: &str) -> bool {
    output.to_uppercase().contains("YES")
}

/// Build the judge prompt: minimal ChatML, YES/NO classification only.
fn build_judge_prompt(query: &str, answer: &str) -> String {
    format!(
        "<|im_start|>system\n\
You are a relevance judge. Reply only YES or NO.\
<|im_end|>\n\
<|im_start|>user\n\
Question: {query}\n\
Answer: {answer}\n\
Does this answer directly address the question? Reply YES or NO only.\
<|im_end|>\n\
<|im_start|>assistant\n"
    )
}

/// Run the LLM judge synchronously (no streaming, no app handle, n_ctx=512).
/// Returns true if the answer is responsive to the query.
/// On any inference error, returns true (fail-open — don't block on judge errors).
async fn judge_answer(
    model_slot: &ModelSlot,
    model_path: &str,
    query: &str,
    answer: &str,
    tier: &HardwareTier,
) -> bool {
    let prompt = build_judge_prompt(query, answer);
    let result = tokio::task::spawn_blocking({
        let model_slot = unsafe { &*(model_slot as *const ModelSlot) };
        let model_path = model_path.to_string();
        let tier = tier.clone();
        move || model_slot.infer_with_cancel(
            &model_path, &prompt,
            5,       // max_tokens — just need YES or NO
            512,     // n_ctx — very small, judge prompt is ~100 tokens
            400,     // max_prompt_tokens
            &tier,
            None,    // no cancellation for judge
            None,    // no streaming
        )
    })
    .await;

    match result {
        Ok(Ok((output, _cancelled))) => {
            let verdict = parse_judge_output(&output);
            log::info!("Judge verdict: {} (raw={:?})", if verdict { "PASS" } else { "FAIL" }, &output[..output.len().min(20)]);
            verdict
        }
        Ok(Err(e)) => {
            log::warn!("Judge inference error — failing open: {e}");
            true // fail-open: don't block on judge errors
        }
        Err(e) => {
            log::warn!("Judge spawn_blocking panicked — failing open: {e}");
            true
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /media/riaz/New\ Volume/echo-AI/src-tauri
CARGO_HOME="/media/riaz/New Volume/.cargo" \
CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
cargo test judge_ --no-default-features --features llm 2>&1 | tail -10
```

Expected: all 3 judge tests PASS.

- [ ] **Step 5: cargo check**

```bash
cargo check --no-default-features --features llm 2>&1 | tail -5
```

Expected: `Finished`.

- [ ] **Step 6: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/services/query_router.rs
git commit -m "feat(reviewer): judge_answer() + parse_judge_output() — LLM YES/NO classification"
```

---

## Task 6: Retry loop in route()

Wire `judge_answer()` into `route()` with up to 3 retries using escalating context.

**Files:**
- Modify: `src-tauri/src/services/query_router.rs`

The retry loop goes immediately after the grounding validation block (after the closing `};` on the `let answer = if tier_used != "error" { ... } else { answer };` block, around line 276).

- [ ] **Step 1: Add the retry constants at the top of the module (after CONTEXT_TOKEN_BUDGET)**

Find `const CONTEXT_TOKEN_BUDGET: usize = 480;` and add after it:

```rust
/// Retry escalation table: (top_k, token_budget) per retry attempt (0-indexed).
/// Attempt 0 uses the original CONTEXT_TOP_K / CONTEXT_TOKEN_BUDGET.
const RETRY_PARAMS: [(usize, usize); 3] = [
    (10, 560), // retry 1
    (12, 640), // retry 2
    (12, 640), // retry 3 (final)
];
```

- [ ] **Step 2: Add the retry loop after grounding validation in route()**

Find the closing of the grounding validation block. It ends with:
```rust
    } else {
        answer
    };
```

Immediately after that closing `};`, add the retry loop. The full block to add:

```rust
    // ── Reviewer: judge + retry loop (skip on cancel, error, or fallback) ──────
    let (answer, tier_used) = if !was_cancelled
        && tier_used != "error"
        && answer != GROUNDING_FALLBACK
    {
        let mut current_answer = answer;
        let mut current_tier = tier_used;
        let mut attempt = 0usize;

        loop {
            let pass = judge_answer(model_slot, &model_path, query_text, &current_answer, tier).await;
            if pass || attempt >= RETRY_PARAMS.len() {
                if !pass {
                    // 3 retries exhausted — return error response immediately.
                    log::warn!("Reviewer: 3 retries exhausted for query {:?}", &query_text[..query_text.len().min(60)]);
                    return Ok((QueryResponse {
                        answer: "The model was unable to generate a relevant answer. Please rephrase your question and try again.".to_string(),
                        citations: vec![],
                        tier_used: "error".to_string(),
                        quality_notice: None,
                    }, false));
                }
                break;
            }

            // Retry with escalated context params.
            let (retry_k, retry_budget) = RETRY_PARAMS[attempt];
            attempt += 1;
            log::info!("Reviewer retry {attempt}: k={retry_k} budget={retry_budget}");

            let retry_fetch_limit = (retry_k * 2) as i64;
            let retry_vec = get_context_chunks(pool, notebook_id, &query_embedding, retry_fetch_limit).await.unwrap_or_default();
            let retry_fts = if is_entity_query(query_text) || retry_vec.len() < 2 || is_short_query {
                fts5_retrieve(pool, notebook_id, query_text).await.unwrap_or_default()
            } else {
                vec![]
            };
            let retry_raw: Vec<(String, String, String, Option<i64>, String)> =
                match (retry_fts.is_empty(), retry_vec.is_empty()) {
                    (false, false) => rrf_merge(&retry_fts, &retry_vec, 60),
                    (false, true)  => retry_fts,
                    _              => retry_vec,
                };
            let retry_chunks = build_context(&retry_raw, retry_k, retry_budget);
            if retry_chunks.is_empty() {
                break;
            }
            let retry_context = build_context_string(&retry_chunks);
            let retry_prompt = build_rag_prompt(query_text, &retry_context, history, system_prompt, true);

            match try_llm_infer_streaming(
                model_slot, &model_path, &retry_prompt,
                gen_max_tokens, gen_n_ctx, gen_max_prompt,
                tier, None, None,
            ).await {
                Ok((text, false)) if !text.trim().is_empty() => {
                    let cleaned = clean_llm_output(&text);
                    let retry_grounding = validate_grounding(&cleaned, &retry_chunks);
                    current_answer = if retry_grounding.fallback_triggered && !retry_grounding.uncited_only {
                        GROUNDING_FALLBACK.to_string()
                    } else {
                        cleaned
                    };
                    current_tier = llm_tier.clone();
                }
                _ => break, // inference error or cancel → stop retrying
            }
        }

        (current_answer, current_tier)
    } else {
        (answer, tier_used)
    };
```

- [ ] **Step 3: Fix variable binding — move is_short_query before the retry block**

The retry loop uses `is_short_query`, which is currently defined inside the initial retrieval block. Find:

```rust
    let is_short_query = query_text.split_whitespace().count() <= 5;
    let fts_chunks = if is_entity_query(query_text) || vec_chunks.len() < 2 || is_short_query {
```

This binding is already at module scope within `route()`, so it is accessible. No change needed if it was already declared with `let` before the retry loop. Verify by running cargo check.

- [ ] **Step 4: cargo check**

```bash
cd /media/riaz/New\ Volume/echo-AI/src-tauri
CARGO_HOME="/media/riaz/New Volume/.cargo" \
CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
cargo check --no-default-features --features llm 2>&1 | tail -10
```

Expected: `Finished`. Fix any borrow/lifetime errors by cloning strings where needed (e.g., `tier_used.clone()`).

- [ ] **Step 5: Run all backend tests**

```bash
cd /media/riaz/New\ Volume/echo-AI/src-tauri
CARGO_HOME="/media/riaz/New Volume/.cargo" \
CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
cargo test --no-default-features --features llm 2>&1 | grep -E "^test |FAILED|passed|failed"
```

Expected: all existing tests pass + new classify/judge tests pass.

- [ ] **Step 6: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/services/query_router.rs
git commit -m "feat(reviewer): retry loop — up to 3 retries with escalating context + error response"
```

---

## Task 7: Frontend summary notice badge

Currently `QualityNoticeBanner` only shows when `constrained=true` (yellow hardware warning). Summary notices have `constrained=false` and need a distinct blue info style.

**Files:**
- Modify: `src/components/chat/QualityNoticeBanner.tsx`
- Modify: `src/components/chat/MessageBubble.tsx`

- [ ] **Step 1: Update QualityNoticeBanner.tsx to handle both styles**

Replace the entire file content:

```tsx
/**
 * QualityNoticeBanner — hardware constraint warning (yellow) or info badge (blue).
 * constrained=true → yellow (lower model tier used due to RAM).
 * constrained=false → blue (summary mode — content not fully cited).
 */
import { Component } from "solid-js";
import { dismissQualityNotice } from "../../stores/query";
import type { QualityNotice } from "../../api";

interface Props {
  notice: QualityNotice;
}

const QualityNoticeBanner: Component<Props> = (props) => {
  const isWarning = () => props.notice.constrained;

  return (
    <div
      class={`flex items-start gap-2 px-3 py-2 rounded-lg text-sm ${
        isWarning()
          ? "bg-yellow-900/30 border border-yellow-700/40 text-yellow-300"
          : "bg-blue-900/30 border border-blue-700/40 text-blue-300"
      }`}
    >
      <span class="mt-0.5 flex-shrink-0">{isWarning() ? "⚠" : "ℹ"}</span>
      <p class="flex-1">{props.notice.message}</p>
      <button
        class={`flex-shrink-0 transition-colors ${
          isWarning()
            ? "text-yellow-500 hover:text-yellow-200"
            : "text-blue-500 hover:text-blue-200"
        }`}
        onClick={dismissQualityNotice}
        title="Dismiss"
      >
        ×
      </button>
    </div>
  );
};

export default QualityNoticeBanner;
```

- [ ] **Step 2: Update MessageBubble.tsx to show banner for all quality notices**

Find in `MessageBubble.tsx`:

```tsx
      <Show when={!isUser() && props.qualityNotice?.constrained}>
          <QualityNoticeBanner notice={props.qualityNotice!} />
      </Show>
```

Replace with:

```tsx
      <Show when={!isUser() && !!props.qualityNotice}>
          <QualityNoticeBanner notice={props.qualityNotice!} />
      </Show>
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd "/media/riaz/New Volume/echo-AI"
PATH="/home/riaz/.nvm/versions/node/v22.22.2/bin:$PATH" \
npx tsc --noEmit 2>&1 | tail -10
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src/components/chat/QualityNoticeBanner.tsx src/components/chat/MessageBubble.tsx
git commit -m "feat(reviewer): summary info badge — blue QualityNoticeBanner for constrained=false"
```

---

## Task 8: Full test run + PHR

- [ ] **Step 1: Run all Rust tests**

```bash
cd /media/riaz/New\ Volume/echo-AI/src-tauri
CARGO_HOME="/media/riaz/New Volume/.cargo" \
CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
cargo test --no-default-features --features llm 2>&1 | grep -E "^test |FAILED|error\[|passed|failed"
```

Expected: all pre-existing tests pass + new tests:
- `test classify_summary_keywords ... ok`
- `test classify_regular_queries ... ok`
- `test judge_yes_response_passes ... ok`
- `test judge_no_response_fails ... ok`
- `test judge_empty_or_unclear_fails ... ok`

- [ ] **Step 2: Run TypeScript type check**

```bash
cd "/media/riaz/New Volume/echo-AI"
PATH="/home/riaz/.nvm/versions/node/v22.22.2/bin:$PATH" \
npx tsc --noEmit 2>&1 | tail -5
```

Expected: no errors.

- [ ] **Step 3: Final commit if any loose files**

```bash
cd "/media/riaz/New Volume/echo-AI"
git status --short
```

If any staged changes remain, commit them. Otherwise proceed.
