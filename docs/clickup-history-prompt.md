# ClickUp diagram-design prompt — EchoAI project history

Use this prompt verbatim in ClickUp AI (or any diagram-capable LLM) along with
`docs/HISTORY.md` as the input source.

---

## Prompt

You are a visual-design assistant. You will be given `HISTORY.md`, a chronological
decision-level record of the EchoAI project from April 2026 to June 2026.

Produce a **single beautiful diagram** that shows, at a glance:

1. **The 15 project phases** along a horizontal time axis (Apr 13 → Jun 3, 2026).
   Use distinct color bands per phase. Phase list:
   - Foundation (ADRs 0001–0006)
   - App 1 — Notebook Knowledge Engine
   - App 2 — Agent Task Overlay (cancelled)
   - Meeting Agent
   - Speed audit + cloud
   - Benchmark + prompt iteration
   - Tier 3 hierarchical retrieval
   - Reranker fine-tune attempt
   - Embedder + linker fixes
   - Feature 003 — Constitution v1
   - Feature 004 — OCR Integration
   - Feature 005 — Generic Structured Importer
   - Feature 006 — LLM PDF→JSON Importer
   - Constitution IR investor demo
   - Feature 007 — Legal RAG Pakistan

2. **Per phase, the key decisions as nodes**, color-coded by status:
   - 🟢 **Green** = Integrated (shipped)
   - 🔴 **Red** = Rejected
   - 🟡 **Yellow** = Reverted / Replaced
   - 🔵 **Blue** = Experiment / Diagnostic only
   - ⚪ **Grey** = Deferred

3. **Arrows showing causal chains** between decisions across phases. Highlight
   especially:
   - App 2 (cancelled) → Meeting Agent (replaced it)
   - In-prompt history injection (drift bug) → retrieval-side keyword carry-over
   - bge-reranker-large zero-shot (failed) → fine-tune dataset (deferred) → reranker hardcoded OFF
   - Feature 006 generic IR → Feature 007 legal-only pivot
   - `<think>` short-circuit (LFM2.5 regression) → removed same day
   - Investor demo K=20 finding → Feature 007 default `CONTEXT_TOP_K=20`
   - ADR-0007 hierarchy+graph → SQLite (rejected CozoDB/KuzuDB/Oxigraph)

4. **Each node label format**: `<short decision> · <YYYY-MM-DD>`. Tooltip /
   sub-text: the **why** (one sentence pulled verbatim from `HISTORY.md`).

5. **Below the timeline**, two supporting diagrams:
   - A **prompt-version flowchart** v2.1 → v2.5 → v4.0 → v4.1 → v4.2 → v4.3 →
     v5.0 → v5.1 → v5.2 → LegalCitation prompts (Feature 007), each annotated
     with the trigger ("worked example removed", "Qwen3 placeholder copy fix",
     etc.).
   - A **rejected → replaced chain** diagram (Sankey or directed graph)
     showing every cancelled thread and what replaced it.

6. **Visual style preferences**:
   - Clean, modern, suitable for an investor deck or LinkedIn share.
   - Monochrome background with the 5-color status palette as the only accent.
   - Phase headers in a strong sans-serif; decision nodes in a softer weight.
   - Bottom-right footer: "EchoAI · Apr–Jun 2026 · 60 decisions, 15 phases".

## Required output

- One SVG (or PNG export) at 1920×1080 minimum.
- Source file (e.g. ClickUp diagram, Mermaid, draw.io XML) so I can edit later.
- A 3-sentence written summary describing what the diagram shows.

## Hard constraints

- **Do not invent decisions** — use only entries that appear in `HISTORY.md`.
- **Do not collapse rejected decisions out of the diagram** — they're the most
  important part. Keep `App 2 cancelled`, `Qwen3-1.7B rejected`,
  `bge-large zero-shot failed`, `K=25 regression`, `<think> short-circuit
  reverted`, `CozoDB rejected` all visible.
- Preserve dates exactly. If two decisions share a date, stack them vertically
  in the same time-slot.

## Input file

Attach `docs/HISTORY.md` from the EchoAI repo. The "Big-picture timeline"
section and the per-phase tables are the authoritative source.
