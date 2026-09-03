---
id: 033
title: Grounding prompt v5.1 + post-hoc CoT scrubber (PART 1 anchor)
stage: green
date: 2026-05-11
surface: agent
model: claude-opus-4-7
feature: 001-notebook-knowledge-engine
branch: master
user: engr.riaz2010@gmail.com
command: chat
labels: [llm, prompt, grounding, scrubber, lfm25, regression-fix]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - path: src-tauri/src/services/llm.rs
    change: GROUNDING_SYSTEM_PROMPT body replaced (v5.0 → v5.1); VERSION bumped to 5.1.0; doc-comment history extended
  - path: src-tauri/src/services/query_router.rs
    change: clean_llm_output upgraded with PART-1 anchor + canonical-refusal fallback; INSUFFICIENT_INFO_MSG constant added; 5 new tests
  - path: CHANGELOG_DEV.md
    change: Appended 2026-05-11 (night) entry documenting the v5.1 bump and scrubber rationale
tests:
  - name: clean_llm_output_strips_think_block
    result: pass
  - name: clean_llm_output_strips_unclosed_think_block
    result: pass (assertion updated — now expects INSUFFICIENT_INFO_MSG)
  - name: clean_llm_output_drops_untagged_cot_preamble
    result: pass (new — repros P1_v3.log)
  - name: clean_llm_output_replaces_pure_cot_with_refusal
    result: pass (new)
  - name: clean_llm_output_passes_canonical_refusal_through
    result: pass (new)
  - name: clean_llm_output_anchor_is_case_insensitive
    result: pass (new)
---

## Prompt

> check memory and now responses are like this: What is the procedure for filing a departmental appeal against a penalty imposed under the Sindh Efficiency and Discipline Rules?
> ℹ Answer may not include citations — ask a more specific question for sourced references.
> [… 5 KB of raw chain-of-thought leaked into the chat answer, including phrases like "Wait, let me check my own knowledge", "REPLUMAIN PASSAGE", "Okay, let me try again carefully", with no `<think>` tags and no PART 1/PART 2 structure …]
> log file is at desktop p1_v3.log file
>
> [follow-up]: share the prompt with me first so i can decide.
>
> [follow-up]: <user pastes a tighter draft v5.1 prompt> suggested prompt, and perform any suitable modification if possible, and perform those changes which are applicable globally on others models too.
>
> [follow-up]: go for changes approved

## Response snapshot

Diagnosed three causes from `~/Desktop/P1_v3.log` (Trace 36f9ee3a, LFM2.5-1.2B-Thinking, 156 s):

1. Model emitted **no `<think>` tags** — streaming filter in `llm.rs` had nothing to strip; raw CoT passed through to the UI verbatim (log lines 250-271).
2. v5.0 prompt was **2077 tokens, capped at 1800** (log line 85). The tail of the system block — `INTERNAL REASONING / Think silently / Do NOT reveal chain-of-thought` — was truncated.
3. 1.2B thinking models are unreliable about emitting `<think>` tags under heavy system-prompt load. A code-side scrubber is the only robust fix.

Applied:
- Replaced `GROUNDING_SYSTEM_PROMPT` with user-supplied v5.1 draft plus my modifications: output-start-marker line ("Begin your output with the literal line: PART 1 — ANSWER:"); removed `<actual-filename.pdf>` angle-bracket placeholder (avoids LFM2.5 copying literal `<…>`); merged redundant "IMPORTANT RULES" block. Final prompt is ~275 tokens — fits under the 1800-token cap with margin.
- Bumped `GROUNDING_SYSTEM_PROMPT_VERSION` to `"5.1.0"`.
- Upgraded `clean_llm_output` with a PART-1 anchor scrubber that runs after the existing `<think>` strip. If the anchor is missing and the canonical refusal phrase is also missing, the scrubber substitutes the canonical refusal sentence so the user never sees raw CoT.
- Added 5 unit tests covering untagged-CoT preamble stripping, pure-CoT → refusal substitution, canonical-refusal pass-through, and case-insensitive `Part 1 -- answer:` matching. Updated the existing unclosed-think-block test assertion.

Verification: `cargo test --lib --features llm clean_llm_output` → 6/6 passed (3m 04s build).

## Outcome

- ✅ Impact: LFM2.5-1.2B (and any future model that leaks untagged CoT) can no longer surface raw chain-of-thought to the user. Smaller prompt also stops the 1800-token cap from chopping the silent-reasoning directive. Both fixes are model-agnostic — they apply equally to Qwen3, DeepSeek-R1, Qwen2.5, and LFM2.5 families.
- 🧪 Tests: 6/6 `clean_llm_output_*` tests pass under `cargo test --lib --features llm`.
- 📁 Files: `src-tauri/src/services/llm.rs`, `src-tauri/src/services/query_router.rs`, `CHANGELOG_DEV.md`.
- 🔁 Next prompts: re-baseline the offline-model benchmark sweep against v5.1 (Qwen2.5-1.5B first per active memory, then LFM2.5-1.2B); capture a fresh trace to confirm the P1 procedural question now produces a clean PART 1/PART 2 answer or the canonical refusal.
- 🧠 Reflection: the v5.0 prompt was correct in spirit but too long — the silent-reasoning directive at the tail was the most important instruction and was being silently truncated. Defence-in-depth (prompt + scrubber) is cheaper than chasing every model's quirks.

## Evaluation notes (flywheel)

- Failure modes observed: (1) untagged CoT leak on LFM2.5-1.2B with long prompts; (2) prompt truncation hiding the silent-reasoning rule; (3) streaming `<think>` filter false sense of security.
- Graders run and results: 6 unit tests PASS; live benchmark re-baseline pending.
- Prompt variant: v5.0.0 → v5.1.0.
- Next experiment: capture a fresh trace for P1 under v5.1 and diff against P1_v3.log to confirm the leak is gone.
