# Quickstart: Query Decomposition (Feature 008)

## Build & unit tests

```bash
cd "src-tauri"
# Unit tests for the new pure functions (fast — no model load):
cargo test -p echo-ai --lib intent_router::decompose
# Regression — slot + follow-up (Fixes 1-3 this branch carries):
cargo test -p echo-ai --lib intent_router
```

Expected: new `decompose` tests green; existing `intent_router` 21/21 still green.

## Manual acceptance (in-app, 15-prompt eval)

1. Confirm a **small** model is active (a `*0.5b*.gguf` or `*1.5b*.gguf` in `models/`).
   The gate should report enabled. To force during testing: `ECHO_DECOMPOSE=1`.
2. Rebuild + run: `npm run tauri dev`. Re-index `fundamental_rights.pdf` if needed.
3. Ask the targeted multi-fact questions and check the trace + answer:
   - **Q15** "Identify the Principles of Policy articles dealing with (a) the Islamic way
     of life, (b) promotion of local Government institutions, and (c) participation of
     people in the Armed Forces."
     → trace shows 3 sub-questions; merged answer states **Article 31 / 32 / 39**, one
     labelled block each, each cited. (SC-001, SC-003)
   - **Q9** "What property rights does Article 23 grant, and what additional protection
     does Article 24 provide?" → 2 sub-questions; Article 23 = property, Article 24 = its
     protection, neither conflated. (SC-001)
   - **Q4** "What are the two protections in Article 14 concerning dignity and torture?"
     → split on "dignity"/"torture"; both protections stated, not "13, 36". (SC-001)
4. Ask a **single-fact** question (e.g. Q2 Article 9) → trace shows NO sub-questions,
   single pass, unchanged answer + latency. (SC-002)

## Capability-gate check (US4)

- With a `*3b*` or larger GGUF active (or `ECHO_DECOMPOSE=0`): ask Q15 → trace shows a
  single native pass, decomposition does NOT trigger. (SC-006, SC-007)

## What to look for in the trace

- A "Decomposition" step listing N sub-questions, each with its own retrieval block.
- Merged answer: labelled blocks `(a) … [1]`, `(b) … [2]`, `(c) … [3]` with globally
  consistent citation numbers.
- Single-fact and capable-deployment questions: identical to current behaviour.
</content>
