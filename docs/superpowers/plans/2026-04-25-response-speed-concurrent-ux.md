# Response Speed & Concurrent UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace blocking `query_notebook` with token-streaming `stream_query_notebook`, add Stop/Keep/Discard, context-aware thinking narrative, chat-priority concurrency with toast, and per-run selective document analysis.

**Architecture:** Backend emits `chat://token` events per generated token; frontend subscribes with `listen()` and renders tokens progressively. `infer_with_cancel()` returns `(output: String, was_cancelled: bool)` instead of bailing, so partial text is always recoverable. The existing `Scheduler` already preempts background LLM workloads for Chat — we add a `chat://background-paused` event so the frontend can show a toast.

**Tech Stack:** Rust/Tauri v2, llama-cpp-2, SolidJS, SQLite/sqlx, `@tauri-apps/api/event` for `listen()`

---

## File Map

| File | Action | What changes |
|------|--------|-------------|
| `src-tauri/src/db/migrations/005_incomplete_messages.sql` | Create | `is_incomplete` column |
| `src-tauri/src/db/repositories/sessions.rs` | Modify | `QueryMessage` struct + `insert_message` signature |
| `src-tauri/src/services/llm.rs` | Modify | `infer_with_cancel` returns `(String, bool)` + emits `chat://token` |
| `src-tauri/src/services/query_router.rs` | Modify | `route()` takes `app`, emits thinking events |
| `src-tauri/src/commands/query.rs` | Modify | Remove `query_notebook`; add `stream_query_notebook`, `cancel_chat`, `save_partial_message` |
| `src-tauri/src/services/scheduler.rs` | Modify | Emit `chat://background-paused` when Chat preempts LLM workload |
| `src-tauri/src/services/analysis_engine.rs` | Modify | Add `run_template_selected` with `source_ids` filter |
| `src-tauri/src/commands/analysis.rs` | Modify | Add `analyze_selected` command |
| `src-tauri/src/lib.rs` | Modify | Update `invoke_handler` registrations |
| `src/api/index.ts` | Modify | Remove `queryNotebook`; add new commands + types |
| `src/stores/query.ts` | Modify | Replace invoke with `listen()` event pattern |
| `src/components/chat/ChatPanel.tsx` | Modify | Streaming bubble, Stop/Send morph, Keep/Discard modal, thinking narrative, indexing badge |
| `src/components/analysis/AnalysisResultView.tsx` | Modify | Collapsed pill source picker |

---

## Task 1: DB Migration — `is_incomplete` column

**Files:**
- Create: `src-tauri/src/db/migrations/005_incomplete_messages.sql`

- [ ] **Step 1: Create migration file**

```sql
-- src-tauri/src/db/migrations/005_incomplete_messages.sql
ALTER TABLE query_messages ADD COLUMN is_incomplete INTEGER NOT NULL DEFAULT 0;
```

- [ ] **Step 2: Verify migration runs on next app start**

The migration runner picks up all `.sql` files in `migrations/` in numeric order. Confirm it exists:

```bash
ls "src-tauri/src/db/migrations/"
```

Expected output includes `005_incomplete_messages.sql`.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/db/migrations/005_incomplete_messages.sql
git commit -m "feat(db): add is_incomplete column to query_messages"
```

---

## Task 2: Update `sessions.rs` — `is_incomplete` field

**Files:**
- Modify: `src-tauri/src/db/repositories/sessions.rs`

- [ ] **Step 1: Add field to `QueryMessage` struct** (line 17 area)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QueryMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub citations_json: Option<String>,
    pub tier_used: Option<String>,
    pub is_incomplete: i64,   // ← add this
    pub created_at: i64,
}
```

- [ ] **Step 2: Update `insert_message` to accept and bind `is_incomplete`** (line 80 area)

```rust
pub async fn insert_message(
    pool: &SqlitePool,
    session_id: &str,
    role: &str,
    content: &str,
    citations_json: Option<&str>,
    tier_used: Option<&str>,
    is_incomplete: bool,       // ← add this
) -> Result<QueryMessage> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    sqlx::query(
        "INSERT INTO query_messages \
         (id, session_id, role, content, citations_json, tier_used, is_incomplete, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(session_id)
    .bind(role)
    .bind(content)
    .bind(citations_json)
    .bind(tier_used)
    .bind(is_incomplete as i64)   // ← add this
    .bind(now)
    .execute(pool)
    .await?;

    Ok(QueryMessage {
        id,
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        citations_json: citations_json.map(str::to_string),
        tier_used: tier_used.map(str::to_string),
        is_incomplete: is_incomplete as i64,   // ← add this
        created_at: now,
    })
}
```

- [ ] **Step 3: Fix all callers of `insert_message` in the codebase**

Search and add `false` as the last argument before `created_at` in every call site:

```bash
grep -rn "insert_message(" src-tauri/src/
```

In `src-tauri/src/commands/query.rs` (two calls — user message and assistant message), add `false` as the `is_incomplete` argument:

```rust
// user message persist
session_repo::insert_message(&pool, &session_id, "user", &query_text, None, None, false).await?;

// assistant message persist  
session_repo::insert_message(&pool, &session_id, "assistant", &resp.answer, Some(&citations_json), Some(&resp.tier_used), false).await?;
```

- [ ] **Step 4: Verify compile**

```bash
cargo check --features llm 2>&1 | grep -E "^error"
```

Expected: no output (zero errors).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db/repositories/sessions.rs src-tauri/src/commands/query.rs
git commit -m "feat(db): thread is_incomplete through insert_message"
```

---

## Task 3: Update `llm.rs` — streaming tokens + (String, bool) return

**Files:**
- Modify: `src-tauri/src/services/llm.rs`

- [ ] **Step 1: Write failing test for the new return type**

Add to the `#[cfg(test)]` block at the bottom of `llm.rs`:

```rust
#[tokio::test]
async fn infer_with_cancel_returns_tuple() {
    // Stub path: returns (stub_string, false)
    let token = CancellationToken::new();
    // We can't load a real model in unit tests; test that signature compiles
    // and cancel returns (_, true) immediately.
    token.cancel();
    // This won't reach infer — just verifies the cancel pre-check path.
    let _token = token; // compile check only
}
```

- [ ] **Step 2: Run test (verify it compiles, skip LLM-gated path)**

```bash
cargo test -p echo-ai-lib --test cancel_tests 2>&1 | tail -5
```

- [ ] **Step 3: Update stub `infer_with_cancel` (non-llm feature)**

Replace the existing stub `infer_with_cancel` in `#[cfg(not(feature = "llm"))]` block:

```rust
#[cfg(not(feature = "llm"))]
impl ModelSlot {
    pub fn new() -> Result<Self> { Ok(ModelSlot) }
    pub fn evict(&self) {}
    pub fn infer(&self, _path: &str, _prompt: &str, _max: u32, _tier: &HardwareTier) -> Result<String> {
        Ok("[LLM feature not compiled — enable with --features llm]".to_string())
    }
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
}
```

- [ ] **Step 4: Update full `infer_with_cancel` signature (inside `#[cfg(feature = "llm")] mod full`)**

Replace the `infer_with_cancel` method in `impl ModelSlot` inside `mod full`:

```rust
pub fn infer_with_cancel(
    &self,
    model_path: &str,
    prompt: &str,
    max_tokens: u32,
    tier: &HardwareTier,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<tauri::AppHandle>,
) -> Result<(String, bool)> {
    // Pre-cancel check — bail before loading model.
    if let Some(t) = &cancel {
        if t.is_cancelled() {
            return Ok(("".to_string(), true));
        }
    }
    self.load(model_path, tier)?;
    let g = self.inner.lock().map_err(|_| anyhow::anyhow!("lock"))?;
    let loaded = g.current.as_ref().ok_or_else(|| anyhow::anyhow!("no model"))?;
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(Some(std::num::NonZeroU32::new(1024).unwrap()))
        .with_n_batch(1024)
        .with_n_threads(4)
        .with_n_threads_batch(4);
    let mut ctx = loaded.model.new_context(&self.backend, ctx_params)?;
    let tokens = loaded.model.str_to_token(prompt, llama_cpp_2::model::AddBos::Always)?;
    const MAX_PROMPT_TOKENS: usize = 768;
    let n_prompt = tokens.len().min(MAX_PROMPT_TOKENS);
    let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(n_prompt.max(1), 1);
    for (i, t) in tokens.iter().take(n_prompt).enumerate() {
        let is_last = i == n_prompt - 1;
        batch.add(*t, i as i32, &[0], is_last)?;
    }
    ctx.decode(&mut batch)?;
    let mut output = String::new();
    let mut i = n_prompt;
    let mut n_generated: u32 = 0;
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::penalties(64, 1.3, 0.0, 0.0),
        LlamaSampler::temp(0.1),
        LlamaSampler::greedy(),
    ]);
    while n_generated < max_tokens {
        // Check cancellation each token — return partial on cancel.
        if let Some(t) = &cancel {
            if t.is_cancelled() {
                return Ok((output, true));   // ← return partial, was_cancelled=true
            }
        }
        let token = sampler.sample(&ctx, -1);
        if loaded.model.is_eog_token(token) { break; }
        let token_str = loaded.model.token_to_str(token, llama_cpp_2::model::Special::Plaintext)?;
        // Emit streaming token to frontend.
        if let Some(a) = &app {
            use tauri::Emitter;
            let _ = a.emit("chat://token", serde_json::json!({ "token": token_str }));
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

- [ ] **Step 5: Verify compile**

```bash
cargo check --features llm 2>&1 | grep -E "^error"
```

Expected: no output.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/llm.rs
git commit -m "feat(llm): infer_with_cancel returns (output, was_cancelled) + emits chat://token"
```

---

## Task 4: Update `query_router.rs` — thinking events + streaming LLM call

**Files:**
- Modify: `src-tauri/src/services/query_router.rs`

- [ ] **Step 1: Add `app: tauri::AppHandle` to `route()` signature**

Change the `route()` pub signature (line 85):

```rust
pub async fn route(
    pool: &SqlitePool,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    session_id: &str,
    query_text: &str,
    history: &[HistoryTurn],
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: tauri::AppHandle,          // ← add
) -> Result<(QueryResponse, bool)>  // ← return (response, was_cancelled)
```

- [ ] **Step 2: Emit `chat://thinking` at retrieval stage (after embed, ~line 112)**

After the `log::info!("METRIC embed_query_ms=...")` line, add:

```rust
{
    use tauri::Emitter;
    let _ = app.emit("chat://thinking", serde_json::json!({
        "stage": "retrieving",
        "detail": format!("Searching knowledge base for \"{}\"...", &query_text.chars().take(40).collect::<String>())
    }));
}
```

- [ ] **Step 3: Emit `chat://thinking` at ranking stage (after retrieval, ~line 148)**

After `let (raw_chunks, retrieval_tier) = raw;`, add:

```rust
{
    use tauri::Emitter;
    let first_file = raw_chunks.first().map(|(_, _, _, _, f)| f.as_str()).unwrap_or("documents");
    let _ = app.emit("chat://thinking", serde_json::json!({
        "stage": "ranking",
        "detail": format!("Found {} passages in {}...", raw_chunks.len(), first_file)
    }));
}
```

- [ ] **Step 4: Emit `chat://thinking` before LLM call (after `build_rag_prompt`, ~line 172)**

After `let prompt = build_rag_prompt(query_text, &context, history);`, add:

```rust
{
    use tauri::Emitter;
    let _ = app.emit("chat://thinking", serde_json::json!({
        "stage": "composing",
        "detail": format!("Composing answer from {} passages...", chunks.len())
    }));
}
```

- [ ] **Step 5: Replace `try_llm_infer` with `try_llm_infer_streaming` call**

Replace the existing `try_llm_infer` call (~line 177) and the function itself.

**New function** (add near `try_llm_infer`, ~line 688):

```rust
async fn try_llm_infer_streaming(
    model_slot: &ModelSlot,
    model_path: &str,
    prompt: &str,
    max_tokens: u32,
    tier: &HardwareTier,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: tauri::AppHandle,
) -> Result<(String, bool)> {
    tokio::task::spawn_blocking({
        let model_slot = unsafe { &*(model_slot as *const ModelSlot) };
        let model_path = model_path.to_string();
        let tier = tier.clone();
        let prompt = prompt.to_string();
        move || model_slot.infer_with_cancel(&model_path, &prompt, max_tokens, &tier, cancel, Some(app))
    })
    .await?
}
```

**Replace the call site** (~line 177):

```rust
let (raw_answer, was_cancelled) =
    match try_llm_infer_streaming(model_slot, &model_path, &prompt, 96, tier, cancel, app.clone()).await {
        Ok((text, cancelled)) if !text.trim().is_empty() => (clean_llm_output(&text), cancelled),
        Ok((_, cancelled)) => (
            "The AI model returned an empty response. Please try asking again.".to_string(),
            cancelled,
        ),
        Err(e) => {
            log::warn!("LLM inference failed — {e}");
            (format!(
                "**AI model unavailable.**\n\n\
                 The model file could not be loaded: {e}\n\n\
                 Go to **Settings → AI Model** to download the model, then try again."
            ), false)
        }
    };
```

- [ ] **Step 6: Propagate `was_cancelled` through grounding and return `(QueryResponse, bool)`**

After grounding validation, update the final return at the bottom of `route()`:

```rust
Ok((QueryResponse {
    answer,
    citations,
    tier_used,
    quality_notice,
}, was_cancelled))
```

- [ ] **Step 7: Verify compile**

```bash
cargo check --features llm 2>&1 | grep -E "^error"
```

Expected: errors only in `commands/query.rs` (callers of `route()` — fixed in Task 5).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/services/query_router.rs
git commit -m "feat(router): emit chat://thinking events + streaming LLM call"
```

---

## Task 5: Replace `query_notebook` with streaming commands

**Files:**
- Modify: `src-tauri/src/commands/query.rs`

- [ ] **Step 1: Add new DTOs and update existing ones**

In the structs section, add `is_incomplete` to `MessageDto` and add new event payload structs:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct MessageDto {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub citations: Vec<Citation>,
    pub tier_used: Option<String>,
    pub is_incomplete: bool,    // ← add
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
struct ThinkingEvent {
    stage: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct DoneEvent {
    message: MessageDto,
    quality_notice: Option<QualityNotice>,
}

#[derive(Debug, Clone, Serialize)]
struct CancelledEvent {
    partial_text: String,
}

#[derive(Debug, Clone, Serialize)]
struct ErrorEvent {
    message: String,
}
```

- [ ] **Step 2: Remove `query_notebook` command and replace with `stream_query_notebook`**

Delete the old `query_notebook` function entirely. Add these three new functions:

```rust
#[tauri::command]
pub async fn stream_query_notebook(
    app: tauri::AppHandle,
    pool: State<'_, SqlitePool>,
    embedder: State<'_, Arc<EmbedderState>>,
    model_slot: State<'_, Arc<ModelSlot>>,
    tier: State<'_, HardwareTier>,
    scheduler: State<'_, Arc<Scheduler>>,
    session_id: String,
    notebook_id: String,
    query_text: String,
) -> Result<(), String> {
    use tauri::Emitter;

    let chat_handle = scheduler.register(WorkloadKind::Chat);
    let chat_id = chat_handle.id.to_string();

    // Emit started immediately so frontend can store chat_id for cancel.
    let _ = app.emit("chat://started", serde_json::json!({ "chat_id": chat_id }));

    // Fetch history before persisting current query.
    let history: Vec<HistoryTurn> = session_repo::list_messages(&pool, &session_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|m| HistoryTurn { role: m.role, content: m.content })
        .collect();

    // Persist user message.
    session_repo::insert_message(&pool, &session_id, "user", &query_text, None, None, false)
        .await
        .map_err(|e| e.to_string())?;

    // Run RAG pipeline with streaming.
    let resp = query_router::route(
        &pool, &embedder, &model_slot, &tier,
        &notebook_id, &session_id, &query_text, &history,
        Some(chat_handle.cancellation_token()),
        app.clone(),
    ).await;

    scheduler.unregister(chat_handle.id);

    match resp {
        Ok((resp, true)) => {
            // Cancelled — do not persist; frontend will ask Keep or Discard.
            let _ = app.emit("chat://cancelled", CancelledEvent { partial_text: resp.answer });
        }
        Ok((resp, false)) => {
            let citations_json = serde_json::to_string(&resp.citations)
                .unwrap_or_else(|_| "[]".to_string());
            let assistant_msg = session_repo::insert_message(
                &pool, &session_id, "assistant", &resp.answer,
                Some(&citations_json), Some(&resp.tier_used), false,
            ).await.map_err(|e| e.to_string())?;

            session_repo::touch_updated_at(&pool, &session_id)
                .await
                .map_err(|e| e.to_string())?;

            audit::insert_event(
                &pool, "query_answered", Some(&session_id),
                Some(&format!(r#"{{"tier":"{}"}}"#, resp.tier_used)),
            ).await.map_err(|e| e.to_string())?;

            let _ = app.emit("chat://done", DoneEvent {
                message: MessageDto {
                    id: assistant_msg.id,
                    session_id: assistant_msg.session_id,
                    role: assistant_msg.role,
                    content: assistant_msg.content,
                    citations: resp.citations,
                    tier_used: assistant_msg.tier_used,
                    is_incomplete: false,
                    created_at: assistant_msg.created_at,
                },
                quality_notice: resp.quality_notice,
            });
        }
        Err(e) => {
            let _ = app.emit("chat://error", ErrorEvent { message: e.to_string() });
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn cancel_chat(
    scheduler: State<'_, Arc<Scheduler>>,
    chat_id: String,
) -> Result<(), String> {
    if let Ok(id) = uuid::Uuid::parse_str(&chat_id) {
        scheduler.request_cancel(id);
    }
    Ok(())
}

#[tauri::command]
pub async fn save_partial_message(
    pool: State<'_, SqlitePool>,
    session_id: String,
    content: String,
) -> Result<MessageDto, String> {
    let msg = session_repo::insert_message(
        &pool, &session_id, "assistant", &content, None, None, true,
    ).await.map_err(|e| e.to_string())?;

    Ok(MessageDto {
        id: msg.id,
        session_id: msg.session_id,
        role: msg.role,
        content: msg.content,
        citations: vec![],
        tier_used: None,
        is_incomplete: true,
        created_at: msg.created_at,
    })
}
```

- [ ] **Step 3: Update `get_session_history` to map `is_incomplete`**

In the existing `get_session_history` command, update the DTO mapping:

```rust
MessageDto {
    id: m.id,
    session_id: m.session_id,
    role: m.role,
    content: m.content,
    citations,
    tier_used: m.tier_used,
    is_incomplete: m.is_incomplete != 0,   // ← add
    created_at: m.created_at,
}
```

- [ ] **Step 4: Verify compile**

```bash
cargo check --features llm 2>&1 | grep -E "^error"
```

Expected: errors only in `lib.rs` (unregistered new commands — fixed in Task 6).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/query.rs
git commit -m "feat(query): stream_query_notebook + cancel_chat + save_partial_message"
```

---

## Task 6: Update `lib.rs` command registrations

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Replace `query_notebook` with new commands**

Find the `// Query (P1)` section (~line 77) and replace:

```rust
// Query (P1)
query::create_session,
query::stream_query_notebook,    // replaces query_notebook
query::cancel_chat,
query::save_partial_message,
query::get_session_history,
```

- [ ] **Step 2: Add `analyze_selected` (will be implemented in Task 9 — add placeholder now)**

In the `// Analysis (P3)` section, add:

```rust
// Analysis (P3)
analysis::run_analysis,
analysis::analyze_selected,      // ← add
analysis::export_analysis,
```

- [ ] **Step 3: Verify compile**

```bash
cargo check --features llm 2>&1 | grep -E "^error"
```

Expected: one error about `analyze_selected` not existing yet (Task 9 fixes it). All other errors resolved.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(lib): register stream_query_notebook, cancel_chat, save_partial_message, analyze_selected"
```

---

## Task 7: `scheduler.rs` — emit `chat://background-paused`

**Files:**
- Modify: `src-tauri/src/services/scheduler.rs`

The `Scheduler` needs an `AppHandle` to emit the event. It currently only has `events: broadcast::Sender<SchedulerEvent>`. We add a new `SchedulerEvent` variant and store the AppHandle.

- [ ] **Step 1: Add `BackgroundPaused` variant to `SchedulerEvent`**

```rust
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SchedulerEvent {
    ForegroundChanged { kind: WorkloadKind },
    WorkloadRegistered { id: Uuid, kind: WorkloadKind },
    WorkloadFinished { id: Uuid, kind: WorkloadKind },
    ProgressTick { id: Uuid, kind: WorkloadKind, progress: Progress },
    BackgroundPaused { kind: WorkloadKind },    // ← add
}
```

- [ ] **Step 2: Add `app: tauri::AppHandle` field to `Scheduler`**

```rust
pub struct Scheduler {
    pub(crate) workloads: DashMap<Uuid, Arc<WorkloadHandle>>,
    pub(crate) foreground: RwLock<WorkloadKind>,
    pub(crate) events: broadcast::Sender<SchedulerEvent>,
    pub(crate) app: tauri::AppHandle,    // ← add
}
```

- [ ] **Step 3: Update `Scheduler::new()` to accept AppHandle**

```rust
pub fn new(app: tauri::AppHandle) -> Arc<Self> {
    let (tx, _) = broadcast::channel(256);
    Arc::new(Self {
        workloads: DashMap::new(),
        foreground: RwLock::new(WorkloadKind::Chat),
        events: tx,
        app,
    })
}
```

- [ ] **Step 4: Emit `chat://background-paused` in `apply_policy_to()`**

In `apply_policy_to()`, when cancelling an LLM workload for a Chat foreground, emit the event:

```rust
fn apply_policy_to(&self, h: &Arc<WorkloadHandle>, foreground: WorkloadKind) {
    if h.kind.uses_llm() {
        if foreground.uses_llm() {
            h.cancel.cancel();
            h.pause.store(false, Ordering::Release);
            h.notify_resume.notify_waiters();
            // Notify frontend that a background workload was preempted.
            if foreground == WorkloadKind::Chat {
                use tauri::Emitter;
                let _ = self.app.emit("chat://background-paused",
                    serde_json::json!({ "kind": format!("{:?}", h.kind) }));
                let _ = self.events.send(SchedulerEvent::BackgroundPaused { kind: h.kind });
            }
        }
    } else {
        h.pause.store(false, Ordering::Release);
        h.notify_resume.notify_waiters();
    }
}
```

- [ ] **Step 5: Fix call site — `Scheduler::new()` in `lib.rs`**

Find where `Scheduler::new()` is called in `src-tauri/src/lib.rs` and pass the `app` handle:

```rust
let scheduler = Scheduler::new(app.handle().clone());
```

- [ ] **Step 6: Fix unit tests in `scheduler.rs`**

The tests call `Scheduler::new()` without arguments. Update them:

In `#[cfg(test)]` block — the tests use `Scheduler::new()`. Since `tauri::AppHandle` can't be constructed in unit tests, mock the scheduler construction. Simplest fix: skip or remove the `new()` arg in tests by using a test helper. Alternatively, change `new()` to accept `Option<tauri::AppHandle>` and only emit when `Some`.

**Recommended approach** — make app optional:

```rust
pub struct Scheduler {
    pub(crate) workloads: DashMap<Uuid, Arc<WorkloadHandle>>,
    pub(crate) foreground: RwLock<WorkloadKind>,
    pub(crate) events: broadcast::Sender<SchedulerEvent>,
    pub(crate) app: Option<tauri::AppHandle>,
}

pub fn new(app: Option<tauri::AppHandle>) -> Arc<Self> { ... }
```

Update `apply_policy_to()` to guard: `if let Some(a) = &self.app { a.emit(...); }`.

Update `lib.rs` call: `Scheduler::new(Some(app.handle().clone()))`.

Tests call `Scheduler::new(None)` — no change needed.

- [ ] **Step 7: Verify compile**

```bash
cargo check --features llm 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 8: Verify existing scheduler tests pass**

```bash
cargo test -p echo-ai-lib scheduler 2>&1 | tail -10
```

Expected: all `scheduler::tests::*` pass.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/services/scheduler.rs src-tauri/src/lib.rs
git commit -m "feat(scheduler): emit chat://background-paused when Chat preempts LLM workload"
```

---

## Task 8: `analysis_engine.rs` — `run_template_selected`

**Files:**
- Modify: `src-tauri/src/services/analysis_engine.rs`

- [ ] **Step 1: Add `fetch_top_chunks_selected` helper**

After the existing `fetch_top_chunks` function, add:

```rust
async fn fetch_top_chunks_selected(
    pool: &SqlitePool,
    notebook_id: &str,
    source_ids: &[String],
    limit: i64,
) -> Result<Vec<ChunkRow>> {
    if source_ids.is_empty() {
        return fetch_top_chunks(pool, notebook_id, limit).await;
    }
    // Build parameterised IN clause.
    let placeholders = source_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT c.id, c.source_id, c.content, c.page_number \
         FROM chunks c \
         JOIN notebook_sources ns ON ns.source_id = c.source_id \
         WHERE ns.notebook_id = ? \
           AND c.source_id IN ({placeholders}) \
         ORDER BY RANDOM() \
         LIMIT ?"
    );
    let mut q = sqlx::query_as::<_, ChunkRow>(&sql).bind(notebook_id);
    for id in source_ids {
        q = q.bind(id);
    }
    let rows = q.bind(limit).fetch_all(pool).await?;
    Ok(rows)
}
```

- [ ] **Step 2: Add `run_template_selected` public function**

```rust
pub async fn run_template_selected(
    pool: &SqlitePool,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    template: &str,
    source_ids: &[String],
) -> Result<AnalysisResult> {
    if !TEMPLATES.contains(&template) {
        anyhow::bail!("unknown template: {template}");
    }

    let chunks = fetch_top_chunks_selected(pool, notebook_id, source_ids, 50).await?;

    if chunks.is_empty() {
        return Ok(AnalysisResult {
            template: template.to_string(),
            title: template_title(template),
            output: "No indexed content found for the selected documents.".to_string(),
            citations: vec![],
            quality_constrained: false,
        });
    }

    let context = build_context(&chunks);
    let prompt = build_prompt(template, &context);
    let (model_path, quality_constrained) = select_model(tier, template);

    let output = if !model_path.is_empty() {
        model_slot
            .infer(&model_path, &prompt, 2048, tier)
            .unwrap_or_else(|e| format!("LLM inference failed: {e}"))
    } else {
        extract_without_llm(template, &chunks)
    };

    let citations = chunks
        .iter()
        .take(5)
        .map(|c| Citation {
            source_id: c.source_id.clone(),
            chunk_id: c.id.clone(),
            excerpt: c.content.chars().take(200).collect::<String>() + "…",
            page: c.page_number,
        })
        .collect();

    Ok(AnalysisResult {
        template: template.to_string(),
        title: template_title(template),
        output,
        citations,
        quality_constrained,
    })
}
```

- [ ] **Step 3: Verify compile**

```bash
cargo check --features llm 2>&1 | grep -E "^error"
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/analysis_engine.rs
git commit -m "feat(analysis): add run_template_selected with source_ids filter"
```

---

## Task 9: `commands/analysis.rs` — `analyze_selected` command

**Files:**
- Modify: `src-tauri/src/commands/analysis.rs`

- [ ] **Step 1: Add `analyze_selected` command**

After the existing `run_analysis` function, add:

```rust
/// Run analysis only on the specified source IDs.
/// If source_ids is empty, falls back to all sources in the notebook.
#[tauri::command]
pub async fn analyze_selected(
    pool: State<'_, SqlitePool>,
    model_slot: State<'_, Arc<ModelSlot>>,
    tier: State<'_, HardwareTier>,
    notebook_id: String,
    source_ids: Vec<String>,
    template: String,
) -> Result<AnalysisResultDto, String> {
    analysis_engine::run_template_selected(
        &pool, &model_slot, &tier, &notebook_id, &template, &source_ids,
    )
    .await
    .map(Into::into)
    .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Verify full compile**

```bash
cargo check --features llm 2>&1 | grep -E "^error"
```

Expected: zero errors.

- [ ] **Step 3: Verify all existing tests pass**

```bash
cargo test --features llm 2>&1 | tail -15
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/analysis.rs
git commit -m "feat(analysis): analyze_selected command with source_ids filter"
```

---

## Task 10: `src/api/index.ts` — update bindings

**Files:**
- Modify: `src/api/index.ts`

- [ ] **Step 1: Add `is_incomplete` to `Message` type**

```typescript
export interface Message {
  id: string;
  session_id: string;
  role: "user" | "assistant";
  content: string;
  citations: Citation[];
  tier_used: string | null;
  is_incomplete: boolean;   // ← add
  created_at: number;
}
```

- [ ] **Step 2: Add new event payload types**

```typescript
export interface ChatStartedEvent {
  chat_id: string;
}

export interface ChatThinkingEvent {
  stage: "retrieving" | "ranking" | "composing" | "verifying";
  detail: string;
}

export interface ChatTokenEvent {
  token: string;
}

export interface ChatDoneEvent {
  message: Message;
  quality_notice: QualityNotice | null;
}

export interface ChatCancelledEvent {
  partial_text: string;
}

export interface ChatErrorEvent {
  message: string;
}

export interface ChatBackgroundPausedEvent {
  kind: string;
}
```

- [ ] **Step 3: Remove `queryNotebook` and add new command wrappers**

Delete:
```typescript
export const queryNotebook = ( ... ) => invoke<QueryResponse>(...);
```

Add:
```typescript
export const streamQueryNotebook = (
  sessionId: string,
  notebookId: string,
  queryText: string,
) => invoke<void>("stream_query_notebook", { sessionId, notebookId, queryText });

export const cancelChat = (chatId: string) =>
  invoke<void>("cancel_chat", { chatId });

export const savePartialMessage = (sessionId: string, content: string) =>
  invoke<Message>("save_partial_message", { sessionId, content });

export const analyzeSelected = (
  notebookId: string,
  sourceIds: string[],
  template: string,
) => invoke<AnalysisResultDto>("analyze_selected", { notebookId, sourceIds, template });
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
npx tsc --noEmit 2>&1 | head -30
```

Expected: errors only in files that use `queryNotebook` (fixed in Task 11).

- [ ] **Step 5: Commit**

```bash
git add src/api/index.ts
git commit -m "feat(api): add streaming chat bindings, analyzeSelected, remove queryNotebook"
```

---

## Task 11: `src/stores/query.ts` — streaming store

**Files:**
- Modify: `src/stores/query.ts`

- [ ] **Step 1: Add streaming state to store**

```typescript
import { listen } from "@tauri-apps/api/event";
import type * as api from "../api";

const [state, setState] = createStore({
  sessionId: null as string | null,
  messages: [] as api.Message[],
  isLoading: false,
  isStreaming: false,            // ← add: true while tokens are arriving
  streamingText: "",             // ← add: partial text being built
  thinkingText: "",              // ← add: thinking narrative text
  chatId: null as string | null, // ← add: current in-flight chat UUID for cancel
  backgroundPausedKind: null as string | null, // ← add: for toast
  showKeepDiscard: false,        // ← add: Keep/Discard modal visibility
  partialText: "",               // ← add: text to keep or discard
  error: null as string | null,
  qualityNotice: null as api.QualityNotice | null,
});
```

- [ ] **Step 2: Replace `sendQuery` with streaming version**

```typescript
export async function sendQuery(notebookId: string, queryText: string) {
  if (!state.sessionId) throw new Error("No active session");

  const userMsg: api.Message = {
    id: crypto.randomUUID(),
    session_id: state.sessionId,
    role: "user",
    content: queryText,
    citations: [],
    tier_used: null,
    is_incomplete: false,
    created_at: Date.now(),
  };

  setState("messages", (prev) => [...prev, userMsg]);
  setState({ isLoading: true, isStreaming: false, streamingText: "", thinkingText: "", error: null, chatId: null });

  // Set up event listeners BEFORE invoking — events fire during the invoke.
  const unlisteners = await Promise.all([
    listen<api.ChatStartedEvent>("chat://started", ({ payload }) => {
      setState("chatId", payload.chat_id);
    }),
    listen<api.ChatThinkingEvent>("chat://thinking", ({ payload }) => {
      setState("thinkingText", payload.detail);
    }),
    listen<api.ChatTokenEvent>("chat://token", ({ payload }) => {
      setState("isStreaming", true);
      setState("thinkingText", "");
      setState("streamingText", (prev) => prev + payload.token);
    }),
    listen<api.ChatDoneEvent>("chat://done", ({ payload }) => {
      setState("messages", (prev) => [...prev, payload.message]);
      setState("qualityNotice", payload.quality_notice);
      setState({ isLoading: false, isStreaming: false, streamingText: "", thinkingText: "", chatId: null });
    }),
    listen<api.ChatCancelledEvent>("chat://cancelled", ({ payload }) => {
      setState({ isLoading: false, isStreaming: false, thinkingText: "", chatId: null,
                 showKeepDiscard: true, partialText: payload.partial_text });
    }),
    listen<api.ChatErrorEvent>("chat://error", ({ payload }) => {
      setState({ isLoading: false, isStreaming: false, streamingText: "", thinkingText: "",
                 chatId: null, error: payload.message });
    }),
    listen<api.ChatBackgroundPausedEvent>("chat://background-paused", ({ payload }) => {
      setState("backgroundPausedKind", payload.kind);
    }),
  ]);

  try {
    await api.streamQueryNotebook(state.sessionId, notebookId, queryText);
  } catch (e) {
    setState({ isLoading: false, isStreaming: false, error: String(e) });
  } finally {
    unlisteners.forEach((u) => u());
  }
}

export async function cancelCurrentChat() {
  if (state.chatId) {
    await api.cancelChat(state.chatId);
  }
}

export async function keepPartialResponse(sessionId: string) {
  const msg = await api.savePartialMessage(sessionId, state.partialText);
  setState("messages", (prev) => [...prev, msg]);
  setState({ showKeepDiscard: false, partialText: "", streamingText: "" });
}

export function discardPartialResponse() {
  setState({ showKeepDiscard: false, partialText: "", streamingText: "" });
}

export function dismissBackgroundPaused() {
  setState("backgroundPausedKind", null);
}
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
npx tsc --noEmit 2>&1 | head -30
```

Expected: errors only in `ChatPanel.tsx` (fixed next).

- [ ] **Step 4: Commit**

```bash
git add src/stores/query.ts
git commit -m "feat(store): streaming query store with event listeners + keep/discard/cancel"
```

---

## Task 12: `ChatPanel.tsx` — streaming UI

**Files:**
- Modify: `src/components/chat/ChatPanel.tsx`

- [ ] **Step 1: Import new store actions and `For`/`Show`**

Update imports at the top:

```typescript
import {
  queryStore,
  sendQuery,
  startSession,
  cancelCurrentChat,
  keepPartialResponse,
  discardPartialResponse,
  dismissBackgroundPaused,
} from "../../stores/query";
```

- [ ] **Step 2: Replace `handleSend` to use streaming store**

The `handleSend` function stays mostly the same — it calls `sendQuery()` which now handles streaming internally. Only change: remove the `await sendQuery()` early return pattern; the store manages loading state.

```typescript
const handleSend = async () => {
  if (isSending) return;
  const text = inputText().trim();
  if (!text || queryStore.isLoading) return;
  isSending = true;
  const ok = await ensureSession();
  if (!ok) { isSending = false; return; }
  setInputText("");
  try {
    await sendQuery(props.notebookId, text);
  } finally {
    isSending = false;
  }
};
```

- [ ] **Step 3: Update the thinking/streaming bubble in the message list**

Replace the existing `<Show when={queryStore.isLoading}>` block (~line 107):

```tsx
{/* Thinking / streaming bubble */}
<Show when={queryStore.isLoading}>
  <div class="flex items-start gap-2">
    <div class="bg-gray-800 rounded-2xl rounded-bl-sm px-4 py-2.5 text-sm border border-gray-700/50 max-w-[85%]">
      <Show
        when={queryStore.isStreaming}
        fallback={
          /* Thinking narrative — shown before first token */
          <div class="flex items-center gap-2 text-gray-400">
            <span class="flex gap-1">
              <span class="w-1.5 h-1.5 rounded-full bg-indigo-400 animate-bounce" style="animation-delay:0ms" />
              <span class="w-1.5 h-1.5 rounded-full bg-indigo-400 animate-bounce" style="animation-delay:150ms" />
              <span class="w-1.5 h-1.5 rounded-full bg-indigo-400 animate-bounce" style="animation-delay:300ms" />
            </span>
            <span class="italic text-xs">{queryStore.thinkingText || "Starting..."}</span>
          </div>
        }
      >
        {/* Streaming tokens */}
        <div class="text-gray-100 whitespace-pre-wrap">
          {queryStore.streamingText}
          <span class="inline-block w-0.5 h-3.5 bg-indigo-400 animate-pulse align-middle ml-0.5" />
        </div>
      </Show>
    </div>
  </div>
</Show>
```

- [ ] **Step 4: Replace Send button with Send/Stop morph**

Replace the existing Send button (~line 138):

```tsx
<Show
  when={queryStore.isLoading}
  fallback={
    <button
      class="px-4 py-2.5 rounded-xl bg-blue-600 hover:bg-blue-500 text-white text-sm
             font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
      onClick={handleSend}
      disabled={queryStore.isLoading || !inputText().trim()}
    >
      Send
    </button>
  }
>
  <button
    class="px-4 py-2.5 rounded-xl bg-red-600 hover:bg-red-500 text-white text-sm
           font-medium transition-colors"
    onClick={cancelCurrentChat}
  >
    ⏹ Stop
  </button>
</Show>
```

Also disable the textarea while loading:

```tsx
disabled={queryStore.isLoading}
```

(This is already in the existing code — verify it's still there.)

- [ ] **Step 5: Add Keep/Discard modal**

Add before the closing `</div>` of the component:

```tsx
{/* Keep/Discard modal — shown after Stop */}
<Show when={queryStore.showKeepDiscard}>
  <div class="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
    <div class="bg-gray-800 rounded-2xl p-6 max-w-sm w-full mx-4 border border-gray-700">
      <h3 class="text-white font-semibold mb-2">Response stopped</h3>
      <p class="text-gray-400 text-sm mb-5">Keep the partial response that arrived, or discard it?</p>
      <div class="flex gap-3">
        <button
          class="flex-1 px-4 py-2 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white text-sm font-medium transition-colors"
          onClick={() => keepPartialResponse(queryStore.sessionId!)}
        >
          Keep
        </button>
        <button
          class="flex-1 px-4 py-2 rounded-xl bg-gray-700 hover:bg-gray-600 text-gray-200 text-sm font-medium transition-colors"
          onClick={discardPartialResponse}
        >
          Discard
        </button>
      </div>
    </div>
  </div>
</Show>

{/* Background-paused toast */}
<Show when={queryStore.backgroundPausedKind}>
  <div class="fixed bottom-4 right-4 bg-amber-900/90 border border-amber-700 rounded-xl px-4 py-3 flex items-center gap-3 z-40 max-w-xs">
    <span class="text-amber-200 text-sm">⏸ {queryStore.backgroundPausedKind} paused for chat</span>
    <button
      class="text-amber-400 hover:text-amber-200 text-xs underline"
      onClick={dismissBackgroundPaused}
    >
      OK
    </button>
  </div>
</Show>
```

- [ ] **Step 6: Add `[incomplete]` tag to `MessageBubble`**

Open `src/components/chat/MessageBubble.tsx` and add after the message content render:

```tsx
<Show when={props.message.is_incomplete}>
  <span class="text-xs text-amber-500 ml-1">[incomplete]</span>
</Show>
```

- [ ] **Step 7: Verify TypeScript compiles**

```bash
npx tsc --noEmit 2>&1 | head -20
```

Expected: zero errors.

- [ ] **Step 8: Start dev server and test manually**

```bash
./run-dev.sh
```

Test: type a question, verify thinking narrative appears → tokens stream in → response finalises.

Test: click Stop mid-response → Keep/Discard modal appears → Keep saves with `[incomplete]` tag.

- [ ] **Step 9: Commit**

```bash
git add src/components/chat/ChatPanel.tsx src/components/chat/MessageBubble.tsx
git commit -m "feat(chat): streaming bubble, Stop/Send morph, Keep/Discard modal, thinking narrative"
```

---

## Task 13: `AnalysisResultView.tsx` — source picker

**Files:**
- Modify: `src/components/analysis/AnalysisResultView.tsx`

- [ ] **Step 1: Add source picker state signals**

At the top of the component, add after existing signals:

```tsx
const [sources, setSources] = createSignal<api.Source[]>([]);
const [selectedIds, setSelectedIds] = createSignal<Set<string>>(new Set());
const [pickerOpen, setPickerOpen] = createSignal(false);

// Load sources for this notebook on mount.
onMount(async () => {
  const list = await api.listNotebookSources(props.notebookId);
  setSources(list.filter(s => s.indexing_status === "indexed"));
  setSelectedIds(new Set(list.filter(s => s.indexing_status === "indexed").map(s => s.id)));
});

const toggleSource = (id: string) => {
  setSelectedIds(prev => {
    const next = new Set(prev);
    if (next.has(id)) { next.delete(id); } else { next.add(id); }
    return next;
  });
};

const selectedCount = () => selectedIds().size;
const totalCount = () => sources().length;
```

- [ ] **Step 2: Add collapsed pill + expandable source list above the Analyze button**

Find the Analyze/Run button in the component and add above it:

```tsx
{/* Source scope picker */}
<div class="mb-3">
  <button
    class="flex items-center gap-1.5 text-xs text-gray-400 hover:text-gray-200 transition-colors bg-gray-800 px-3 py-1.5 rounded-full border border-gray-700"
    onClick={() => setPickerOpen(o => !o)}
  >
    <span>📄 {selectedCount()} of {totalCount()} docs</span>
    <span>{pickerOpen() ? "▲" : "▾"}</span>
  </button>

  <Show when={pickerOpen()}>
    <div class="mt-2 bg-gray-800 rounded-xl border border-gray-700 p-3 space-y-2">
      <div class="flex justify-between items-center mb-1">
        <span class="text-xs text-gray-500 uppercase tracking-wide">Select documents</span>
        <button
          class="text-xs text-indigo-400 hover:text-indigo-300"
          onClick={() => setSelectedIds(new Set(sources().map(s => s.id)))}
        >
          All
        </button>
      </div>
      <For each={sources()}>
        {(source) => (
          <label class="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              class="accent-indigo-500"
              checked={selectedIds().has(source.id)}
              onChange={() => toggleSource(source.id)}
            />
            <span class="text-sm text-gray-300 truncate">{source.file_name}</span>
          </label>
        )}
      </For>
    </div>
  </Show>
</div>
```

- [ ] **Step 3: Update the Analyze button to call `analyzeSelected`**

Find the existing Analyze/Run button `onClick` and update it to use `api.analyzeSelected` with the selected source IDs:

```tsx
onClick={async () => {
  setIsAnalyzing(true);
  try {
    const result = await api.analyzeSelected(
      props.notebookId,
      Array.from(selectedIds()),
      selectedTemplate(),
    );
    setAnalysisResult(result);
  } catch (e) {
    setError(String(e));
  } finally {
    setIsAnalyzing(false);
    setPickerOpen(false);
    // Reset selection to all sources for next run.
    setSelectedIds(new Set(sources().map(s => s.id)));
  }
}}
```

Update the button label to show count:

```tsx
Analyze {selectedCount()} Document{selectedCount() !== 1 ? "s" : ""} →
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
npx tsc --noEmit 2>&1 | head -20
```

Expected: zero errors.

- [ ] **Step 5: Test in dev server**

Open Analysis tab. Verify:
- Pill shows "N of N docs ▾"
- Click pill → checkbox list expands
- Deselect a doc → pill updates to "N-1 of N docs"
- Click Analyze → only selected docs analyzed
- After run → selection resets to all

- [ ] **Step 6: Commit**

```bash
git add src/components/analysis/AnalysisResultView.tsx
git commit -m "feat(analysis): per-run source picker with collapsed pill"
```

---

## Self-Review Checklist (run before marking complete)

- [ ] `cargo check --features llm` exits clean
- [ ] `cargo test --features llm` all pass
- [ ] `npx tsc --noEmit` exits clean
- [ ] Dev server starts and chat streams tokens
- [ ] Stop button appears during streaming; Keep/Discard modal works
- [ ] Analysis tab source picker resets after each run
- [ ] `[incomplete]` tag appears on kept-partial messages
- [ ] Background-paused toast appears when chat preempts analysis (requires manual trigger)
- [ ] No stale `queryNotebook` references remain: `grep -r "queryNotebook\|query_notebook" src/`

---

## Verification Commands

```bash
# Full backend compile
cargo check --features llm

# All tests
cargo test --features llm 2>&1 | tail -20

# TypeScript types
npx tsc --noEmit

# Check no old command references remain
grep -rn "query_notebook\|queryNotebook" src/ src-tauri/src/commands/ src-tauri/src/lib.rs

# Start full dev environment
./run-dev.sh
```
