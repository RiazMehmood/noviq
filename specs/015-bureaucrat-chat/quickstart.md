# Quickstart: Unified Bureaucrat Chat Assistant

## Prerequisites
- F013 ingestion fixes merged to `master` (this branch is off `master` and does NOT include them — rebase 015 after 013 merges).
- Indexed Sindh pilot corpus in the bundled PostgreSQL store.
- A GGUF model in the models folder (`ECHO_MODELS_DIR`). Any size — the window is derived from it.

## Run (dev)
```
ECHO_OCR_DPI=300 ./run-dev.sh        # launches Tauri dev (model auto-discovered)
# optional: ECHO_N_CTX=8192 to force a window for testing
```

## Verify the P1 invariants (MVP)

1. **Grounded general-answer-first (US1)** — ask "I want long leave, what is the procedure and rules?"
   - Expect: a complete cited general answer + an explicit offer to tailor. No clarifying-question gate.
2. **Zero confabulation (US2)** — ask something NOT in the corpus.
   - Expect: a flag ("couldn't find this in the indexed documents") + offer to index. No fabricated numbers.
3. **No truncation (US3)** — drive a long multi-turn conversation past the window.
   - Expect (query trace): `Σ prompt ≤ n_ctx` every turn; no `llm.rs` head+tail truncation warning; retrieved provisions appear whole; facts stated early still honored late; a "context filling — start a fresh chat" notice appears past the threshold.

## Verify capabilities (later phases)
- **Summarize a book (US6)**: "summarize the Sindh Local Government Act 2013" → structured cited summary covering all major parts; trace shows `map`/`reduce` passes.
- **Amendment chain (US8)**: "how has Section 18 been amended?" → ordered oldest→recent, each link cited; trace shows the chain came from the graph (no model call for order).
- **Compare/conflict (US7)**: name two acts → aligned cited comparison; conflicts shown as cited candidates.

## Tests
```
cargo test --lib services::assistant            # allocator, n_ctx resolver, citation gate, intent, memory
cargo test --test bureaucrat_chat_acceptance    # P1 invariants: no-truncation, zero-confabulation
```

## Acceptance gates (map to Success Criteria)
- SC-001 zero fabricated citations · SC-002 no truncated law · SC-003 zero window overflow ·
  SC-004 memory fidelity · SC-005 first-answer completeness · SC-008 context-fill notice ·
  SC-010 amendment-chain fidelity · SC-011 conflicts cited.
