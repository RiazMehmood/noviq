---
id: 012
title: Fix LLM panic and chat answer synthesis
stage: green
date: 2026-04-15
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: inline fix — LLM panic + empty chat answers
labels: ["llm","panic","answer-synthesis","citations","query-router","fts5","vec"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/llm.rs
  - src-tauri/src/services/query_router.rs
tests:
  - cargo check — zero errors (10 pre-existing warnings only)
---

## Prompt

app crashed

(App terminated due to llama-cpp-2 hard panic when model file was missing)

---

while chating it was only sharing resources of question from document, not the details of that answer, we want details of answers in chat and in answers end there must be reference number to verify

## Response snapshot

### Fix 1 — LLM panic on missing model file (`llm.rs`)

`llama-cpp-2` calls `LlamaModel::load_from_file` which hard-panics (unwinds the whole process) when the model file doesn't exist. Added a file existence guard before calling into the library:

```rust
pub fn load(&self, model_path: &str, tier: &HardwareTier) -> Result<()> {
    if !std::path::Path::new(model_path).exists() {
        anyhow::bail!(
            "LLM model not found: {model_path} — \
             download the model file and place it in the models/ directory"
        );
    }
    // ... rest of load
}
```

Result: missing model now returns a clean `anyhow::Error` surfaced to the frontend instead of killing the process.

### Fix 2 — Detailed chat answers with citations (`query_router.rs`)

Both `fts5_query` and `vec_query` were returning shallow answers (empty string or "Found X sections matching…"). Added `synthesize_answer` function that:

- Shows up to 800 chars from the top chunk, 400 chars from remaining chunks
- Appends inline `**[N]**` citation markers after each paragraph
- Adds a `---\n**Sources:**` footer with per-citation page numbers and 120-char snippets

```rust
fn synthesize_answer(chunks: &[(String, String, String, Option<i64>)]) -> String {
    // ...
    body_parts.push(format!("{body}{ellipsis} **[{n}]**"));
    ref_lines.push(format!("[{n}]{page_tag} — {snippet}…"));
    // ...
    format!("{}\n\n---\n**Sources:**\n{}", body_parts.join("\n\n"), ref_lines.join("\n"))
}
```

Both `fts5_query` and `vec_query` now call `synthesize_answer(&chunks_for_answer)` instead of returning empty/shallow answers.

## Outcome

- ✅ Impact: App no longer crashes when LLM model files are absent
- ✅ Impact: Chat answers now show full document content with `[1]`, `[2]` inline citation markers and a Sources footer
- 🧪 Tests: cargo check — zero errors, 10 pre-existing warnings only
- 📁 Files: 2 files modified
- 🔁 Next: T059 [US2] E2E folder semantic search + revocation

## Evaluation notes (flywheel)

- Failure modes observed: (1) llama-cpp-2 hard panic on missing model file; (2) chat returning empty/shallow answers with no document content
- Graders run and results: cargo check PASS
- Next experiment: Ask "What are the fundamental rights?" in notebook — verify answer shows constitutional text with [1][2] markers
