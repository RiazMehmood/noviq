# Claude Code Rules

## Session-start mandatory
Read `CHANGELOG_DEV.md` (root) before any code work — it is the source of truth
for every code change with reasoning; avoids re-introducing reverted patterns.

After shipping any non-trivial change, append a dated entry to `CHANGELOG_DEV.md`:
symptom/why, fix (file paths + functions), cross-cutting impact or follow-ups.

Precedence on conflict: CLAUDE.md > `CHANGELOG_DEV.md` > MEMORY.md > code.

## Role
Expert AI assistant for Spec-Driven Development (SDD). Work with the architect to build products.

## Core policies
- Clarify and plan first; separate business understanding from technical plan.
- Smallest viable diff; do not refactor unrelated code.
- Don't invent APIs/data/contracts — ask targeted clarifiers if missing.
- Never hardcode secrets; use `.env`.
- Cite existing code as `start:end:path`; propose new code in fenced blocks.
- Prefer MCP tools and CLI for verification; don't assume from internal knowledge.

## Execution contract per request
1. Confirm surface and success criteria (one sentence).
2. List constraints, invariants, non-goals.
3. Produce artifact with inline acceptance checks (checkboxes/tests).
4. Add follow-ups and risks (≤3 bullets).
5. Create PHR (see below).
6. If a significant architectural decision was made, surface ADR suggestion.

## Human-as-tool triggers
Invoke the user when: requirements ambiguous (ask 2-3 clarifiers), unforeseen
dependencies discovered, multiple valid approaches with significant tradeoffs,
or at major-milestone checkpoints.

## PHR (Prompt History Record) — required after every user request
Skip only for `/sp.phr` itself.

**Stage**: one of constitution | spec | plan | tasks | red | green | refactor | explainer | misc | general.

**Route under `history/prompts/`**:
- `constitution` → `history/prompts/constitution/`
- Feature stages → `history/prompts/<feature-name>/` (auto-detect from branch)
- `general` → `history/prompts/general/`

**Filename**: `<ID>-<slug>.<stage>.prompt.md` (3–7 word title; increment ID on collision).

**Process**:
1. Read template from `.specify/templates/phr-template.prompt.md` or `templates/phr-template.prompt.md`.
2. Fill all placeholders: ID, TITLE, STAGE, DATE_ISO, SURFACE="agent", MODEL, FEATURE, BRANCH, USER, COMMAND, LABELS, LINKS, FILES_YAML, TESTS_YAML, PROMPT_TEXT (verbatim, untruncated), RESPONSE_TEXT, OUTCOME.
3. Write file with agent file tools.
4. Validate: no unresolved `{{...}}`/`[...]` placeholders, PROMPT_TEXT complete, path matches route.
5. Report ID, path, stage, title. Failures warn but never block the main command.

Shell fallback if agent-native flow fails and shell is permitted:
`.specify/scripts/bash/create-phr.sh --title "<title>" --stage <stage> [--feature <name>] --json`,
then patch placeholders.

## ADR suggestions (never auto-create)
After plan/tasks work, test for significance — ALL three must hold:
- **Impact**: long-term consequences (framework, data model, API, security, platform).
- **Alternatives**: multiple viable options considered.
- **Scope**: cross-cutting, influences system design.

If yes, surface (and wait for consent):
> 📋 Architectural decision detected: <brief> — Document reasoning and tradeoffs? Run `/sp.adr <decision-title>`

Group related decisions (stack, auth, deployment) into one ADR when appropriate.

## Project structure
- `.specify/memory/constitution.md` — principles (also: code/testing/perf/security standards)
- `specs/<feature>/{spec,plan,tasks}.md` — feature artifacts
- `history/prompts/` — PHRs · `history/adr/` — ADRs
- `.specify/` — SpecKit Plus templates and scripts

## Active stack
Rust 1.87 + TypeScript (SolidJS frontend); SQLite at `echo.db`.

## Active Technologies
- Rust 1.87 (workspace), TypeScript with SolidJS for the frontend + tauri 2.0, llama-cpp-2 0.1.143 (llguidance constrained sampling), sqlx 0.8, fastembed 4, async-trait, serde, serde_json, jsonschema (new), chrono (006-llm-pdf-importer)
- SQLite via sqlx at `echo.db` (single source of truth, additive migration 018); JSON payloads on disk at `{app_data_dir}/ir_cache/<sha>.json`; rasterized PDF pages at `{app_data_dir}/page_cache/<sha>/p<N>.png` (006-llm-pdf-importer)
- Rust 1.87 (workspace) + TypeScript with SolidJS (frontend) + tauri 2.0, llama-cpp-2 0.1.143 (with `llguidance` sub-feature for grammar-constrained sampling), sqlx 0.8 (SQLite), sqlite-vec (vec0), fastembed 4, async-trait, serde, serde_json, jsonschema, chrono. Reused from Feature 004/005/006: lopdf + pdftotext + tesseract OCR pipeline, FTS5 indexing, `progress_store`, `mistral_builder`. (007-legal-rag-pakistan)
- SQLite via sqlx at `echo.db` (single source of truth — constitution §IV). JSON IR sidecars on disk at `{app_data_dir}/ir_cache/<sha>.json`; rasterised PDF pages at `{app_data_dir}/page_cache/<sha>/p<N>.png` (existing). Migration 020 is additive. (007-legal-rag-pakistan)
- Rust 1.87 (workspace `echo-ai`) + existing only — `regex_lite` (slot/marker parsing), `tokio` (008-query-decomposition)
- N/A — no migration. Reuses `echo.db` read paths through the seam. Query (008-query-decomposition)
- Rust 1.87 (workspace `echo-ai`, `src-tauri`) + TypeScript with SolidJS (frontend) + tauri 2.0, sqlx 0.8 (SQLite), sqlite-vec, fastembed 4, llama-cpp-2 0.1.143, regex_lite, serde/serde_json, tokio; existing services `indexer`, `file_watcher`, `query_router`, `intent_router::decompose` (Fix B helpers), `exporters` (009-lgd-drafting)
- SQLite via sqlx at `echo.db` (single source of truth). New additive migration for template/slot-schema + gold-example metadata. Gold-example file content referenced on disk (existing app_data layout), NOT inserted into the retrieval corpus. (009-lgd-drafting)
- Rust 1.87 (`src-tauri`) + TypeScript with SolidJS (frontend), Tauri 2.0 + sqlx 0.8 (SQLite); existing single model slot (`llama-cpp-2` 0.1.143) for prose + summarization; existing drafting services (`services/drafting/{session,docset,gaps}.rs`, `commands/drafting.rs`); `regex_lite` for turn-action parsing; `serde`/`serde_json`; reused `decompose` citation helpers (009-lgd-drafting)
- `echo.db` (single source of truth, Principle IV). New **additive migration 023** for `chat_message` + `conversation_summary`; reuses migration 022 `draft_session` (already persisted via `session.rs` `DraftState`) (009-lgd-drafting)
- Rust 1.87 (`src-tauri`) + TypeScript with SolidJS (frontend), Tauri 2.0 + sqlx 0.8 (SQLite); existing single model slot (`llama-cpp-2` 0.1.143) for prose + summarization, **plus demand-loaded Tier-4 worker Mistral-7B-Instruct-v0.3 Q4_K_M (already on disk at `models/ir_builder/` from F006) for one-time template-ingestion segmentation — free-text JSON only, NO llguidance**; existing **fastembed** embedder + `chunk_embeddings` for semantic gap coverage; existing drafting services (`services/drafting/{session,docset,gaps,ingest}.rs`, `commands/drafting.rs`); `regex_lite` for turn-action parsing; `serde`/`serde_json`; reused `decompose` citation helpers (extended hyphen-tolerant) (009-lgd-drafting)
- `echo.db` (single source of truth, Principle IV). **Additive migration 023** for `chat_message` + `conversation_summary`; **additive migration 024** adds `template_slot.provision_ref` (R5a); reuses migration 022 `draft_session` (already persisted via `session.rs` `DraftState`) (009-lgd-drafting)
- Rust 1.87 (workspace `echo-ai`, `src-tauri`) + TypeScript with SolidJS (frontend), Tauri 2.0 + sqlx 0.8 (SQLite), sqlite-vec (vec0), fastembed 4, llama-cpp-2 0.1.143 (free-text only — llguidance crashes, proven twice), regex_lite, serde/serde_json, tokio. Reused services: `doc_ir::mistral_builder::discover_ir_builder_gguf`, single ModelSlot (Principle III), `intent_router::decompose` citation helpers (hyphen-tolerant `resolve_provision_label`), drafting `conversation.rs` (chat_message), cancellation-token registry pattern (F006), `drafting://` event channels (010-one-ask-drafting)
- `echo.db` (single source of truth, Principle IV). Reuses `chat_message` (migration 023). New **additive migration 025** `one_ask_session(draft_id PK, state_json, updated_at)`. 009 tables untouched; `template_schema` rows retained as future style examples (010-one-ask-drafting)
- Rust 1.87 (workspace `echo-ai`, `src-tauri`) + TypeScript/SolidJS, Tauri 2.0 + sqlx 0.8 (swap `sqlite` → `postgres` feature; keep `runtime-tokio`, `macros` off compile-time checks — runtime queries only), `postgresql_embedded` (theseus-rs, `bundled` feature, PG 17.x), pgvector ≥ 0.8 (vendored extension artifacts installed into the bundled engine offline), `pgvector`-rust or raw `vector` casts for bind/decode, existing fastembed 4 / llama-cpp-2 untouched (011-postgres-pgvector-port)
- Bundled PostgreSQL 17 + pgvector; data dir under `{app_data_dir}/pgdata/`; loopback/Unix-socket only; old `echo.db` retained read-only until user confirms migration (011-postgres-pgvector-port)
- Rust 1.87 (workspace `echo-ai`, `src-tauri`); TypeScript + SolidJS (frontend) + Tauri 2.0, `pdftotext` (poppler, system binary) for `-bbox`/`-layout` geometry & text, `regex_lite`, `fastembed` 4 (re-embed), `sqlx` 0.8 (postgres feature, runtime queries), `pgvector` casts, `serde`/`serde_json`, `tokio`. **NO lopdf for tables.** PaddleOCR/PP-Structure reserved for true scans (unused at launch — corpus has no scanned-only pages). (012-doc-intelligence-production)
- Bundled PostgreSQL 17 + pgvector (`{app_data_dir}/pg/data`), additive migrations under `src-tauri/src/db/pg/migrations/`. **Diverges from Constitution §IV (SQLite) — see Complexity Tracking; pending amendment / merge gate (F011).** (012-doc-intelligence-production)
- Rust 1.87 (workspace `echo-ai`, `src-tauri`); deterministic — no model/LLM + `pdftotext` (poppler, `-bbox` + `-layout` geometry), `regex_lite`, `sqlx` (013-ingestion-hardening)
- Bundled PostgreSQL 17 + pgvector (`{app_data_dir}/pg/data`); existing `chunks`, (013-ingestion-hardening)
- Rust 1.87 (`src-tauri`, `noviq-server` bin); Python 3.10+ for the + vLLM ≥ 0.8.5 (CUDA-12.x wheel) + CUDA forward-compat (016-vllm-serving)
- bundled PostgreSQL + pgvector (unchanged); model weights on disk under (016-vllm-serving)
- Rust 1.87 (`src-tauri`) + TypeScript/SolidJS; **no new crates** — reuses `sqlx` 0.8 (postgres, runtime queries), `axum` routing, existing `fastembed`/retrieval stack. New server modules `api/projects.rs`, `db/repositories/{projects,chat_shares}.rs`, `services/projects/`; new frontend routes `ProjectsPage`/`ProjectPage`/`SharedChatPage` + `stores/projects.ts` (027-my-projects)
- Bundled PostgreSQL 17 + pgvector; **additive migration 023** — `project`, `project_source` (⚠️ no `owner_id` column, by design), `project_chat`, `chat_share`; gate `db_config('my_projects_enabled')` seeded `'0'` (027-my-projects)

## Recent Changes
- 027-my-projects: `visible_source_ids` gained an **exclusive (subtractive) mode** — the first scope in the system that removes rather than adds; all six call sites still funnel through it, so global search and citation opening inherit project isolation. Sharing = **copy** (metadata only), so `hard_delete_source` must consult `sources::is_referenced` across `notebook_sources` + `source_link` + `project_source` — a delete can now destroy another user's data. ⚠️ `rename_all="camelCase"` + `Option<T>` **fails open** on scope fields; use `#[serde(default, alias = "project_id")]`. See ADR-0028/0029 and `CHANGELOG_DEV.md` 2026-08-05 (F027).
- 006-llm-pdf-importer: Added Rust 1.87 (workspace), TypeScript with SolidJS for the frontend + tauri 2.0, llama-cpp-2 0.1.143 (llguidance constrained sampling), sqlx 0.8, fastembed 4, async-trait, serde, serde_json, jsonschema (new), chrono
