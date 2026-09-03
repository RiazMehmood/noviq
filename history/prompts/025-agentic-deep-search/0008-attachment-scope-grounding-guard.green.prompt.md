---
id: 0008
title: Attachment-scope grounding guard (no speculation on un-retrieved law)
stage: green
date: 2026-07-15
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (chat)
labels: [backend, grounding, attachments, scope, persona, anti-hallucination]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src-tauri/src/services/assistant/capabilities/additive.rs
  - src-tauri/src/services/assistant/capabilities/draft.rs
  - src-tauri/src/services/assistant/mod.rs
  - CHANGELOG_DEV.md
tests:
  - "cargo build --release --features llm,server,cuda (VM, green)"
  - "live smoke: trace fb93b921 — attachment-scoped refusal, 1.0s"
---

## Prompt

(From the reported defect) "without pressing widen it searches the corpus again." Attach an E&D show-cause
notice, then ask "co-relate this document with procurement rules" — the assistant appears to answer from the
whole corpus instead of the single attached file.

## Response snapshot

Pulled the per-query trace FILE from prod (`logs/ab_rerank/queries/2026-07-15_06-14-18_376bc113.log`; trace
ids live in these per-query files + the admin panel, NOT in `query_log`, which uses its own UUID). Verdict: it
did **NOT** search the corpus — STEP 3 `Scoped to attachment: true`, Sources-in-scope=1, every retrieved chunk
was the SC02 show-cause; the journal logged `live attachment → default scope to 1 attachment(s)`. The real
defect was a **grounding bug, not a scope bug**: with no procurement rules in the file, the model *speculated*
procurement law from training ("…may intersect… may involve procurement of travel services…") instead of
refusing. Root cause: the bureaucrat persona (`config/personas/bureaucrat.toml`) instructs "FIRST give a
complete general answer: the standard procedure and **the governing laws**", which overrides strict grounding
when only an attachment is in scope. User chose (AskUserQuestion) **"stay narrow, refuse cleanly."**

Fix:
- `guidance::run` gains a `scoped_to_attachment: bool`. When true it appends a new const
  `ATTACHMENT_SCOPE_OVERRIDE` to `sys` — answer strictly from the attached document, no governing-laws /
  speculation from memory; if the file lacks the asked subject, reply with one clean sentence
  "The attached document does not contain <subject>." Applied to `sys` BEFORE both the deep and single-pass
  branches so both paths inherit it.
- `is_bare_refusal` learns the `"does not contain"` phrasing so a clean refusal drops any stray citation.
- The flag is threaded explicitly (NOT a task-local) through `additive::run` + `draft::run` + all `run_turn`
  dispatch sites in `mod.rs`, since every caller funnels through `guidance::run`.
- Non-attachment turns are byte-identical (flag false).

The widen button already renders whenever `scoped_to_attachment=true`, so the officer is one tap away from the
real rules in the corpus.

## Outcome

- ✅ Impact: attachment-scoped answers no longer fabricate law the file never contained; they refuse cleanly
  and offer the widen path. DEPLOYED + LIVE-VERIFIED on release `74875c01` (server restarted 12:15:52 PKT,
  health 200, ECHO_AGENTIC_DEEP=1 still on).
- 🧪 Tests: VM CUDA release build green; live smoke via conv `5f683418` (still has SC02 attached+indexed) —
  trace `fb93b921`, `scoped_to_attachment=true`, answer = "The attached document does not contain procurement
  rules." (1.0s, was ~34s of speculation), widen button shows. Note: replaying the original conv `1ccae1b4`
  was an INVALID test — its attachment had been FR-019-cleaned, so it correctly fell back to corpus/12-src;
  SC02 lives in 6 convs.
- 📁 Files: guidance.rs (override const + flag + is_bare_refusal), additive.rs, draft.rs, mod.rs (flag
  threading), CHANGELOG_DEV.md.
- 🔁 Next prompts: (1) separate corpus-path waffle nit — the same query over the corpus still opens with
  "…may intersect with procurement processes…" framing; decide if worth a prompt tweak. (2) Phase-3
  smart-widen-guard proper: "your file CITES these rules but doesn't contain them — pull from the library?"
- 🧠 Reflection: the symptom (looks like a corpus answer) misled toward a scope fix; reading the actual trace
  file proved scope was already correct and relocated the bug to grounding/persona. Threaded an explicit bool
  rather than a task-local because all callers already funnel through one function.

## Evaluation notes (flywheel)

- Failure modes observed: persona breadth instruction ("give the governing laws") beats grounding when the
  only in-scope source is a thin attachment — a general risk wherever a persona encourages proactive breadth.
- Graders run and results (PASS/FAIL): live trace `fb93b921` PASS (clean refusal, correct scope, widen shown).
- Prompt variant (if applicable): ATTACHMENT_SCOPE_OVERRIDE appended to system prompt under attachment scope.
- Next experiment (smallest change to try): apply an analogous grounding clamp to the corpus path so
  "co-relate … with X" leads with the retrieved rules instead of intersection waffle.
