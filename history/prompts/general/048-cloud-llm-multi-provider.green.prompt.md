---
id: 048
title: Cloud LLM multi-provider (Groq + Cerebras + 6 more)
stage: green
date: 2026-05-23
surface: agent
model: claude-opus-4-7
feature: ""
branch: 003-constitution-hierarchical-graph
user: engr.riaz2010@gmail.com
command: chat (cloud-provider swap)
labels: [cloud-llm, settings, provider-swap, multi-provider, openai-compatible]
links:
  spec: ""
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: src-tauri/src/services/cloud_llm.rs
    change: GROQ_ENDPOINT constant removed; new endpoint(settings) helper maps 8 providers (groq, cerebras, openrouter, together, deepseek, openai, fireworks, custom) to chat-completions URLs; CloudSettings gains endpoint_url; 3 call sites and 3 error messages now provider-agnostic
  - path: src/api/index.ts
    change: CloudSettings TS interface gains optional endpoint_url
  - path: src/pages/SettingsPage.tsx
    change: Cloud Inference section rewritten — provider dropdown (8 options with suggested model + key-console URL per provider), conditional custom-endpoint input, free-text model input replacing the old 3-option Groq-only dropdown; selecting a provider pre-fills the model field only when current value is empty or a known suggestion (never clobbers user input)
  - path: CHANGELOG_DEV.md
    change: appended multi-provider entry above the Preamble fix entry
tests: []
---

## Prompt

> tested with three different models, and also want to try cloud inference but
> i think due to quota hit it is not working, saved details inside "test 5
> prompts" at desktop with three different folders cloud folder is created but
> due to quota it is not working properly, if we can change api to other cloud
> option so i can test also
>
> (follow-up) 1 and then 2 also  [F5 re-test first, then cloud swap]

## Response snapshot

Investigated cloud_llm.rs and found the integration was already OpenAI-
compatible — only the endpoint URL was hardcoded to Groq. Refactored:

1. Replaced `GROQ_ENDPOINT` constant with an `endpoint(&CloudSettings)`
   helper that maps `provider` strings to per-provider chat-completions URLs.
   Added an optional `endpoint_url` override for users with internal proxies
   or providers we haven't built in. Unknown provider strings fall back to
   Groq (preserves existing `cloud_settings.json` files).
2. Updated 3 call sites (streaming + non-streaming + error format strings)
   to use `endpoint()` and the provider name instead of literal "Groq".
3. Added `endpoint_url?: string` to the TS `CloudSettings` interface.
4. Rewrote Settings UI: provider `<select>` with 8 options, conditional
   custom-endpoint field shown only when `provider === "custom"`, free-text
   model input replacing the Groq-only dropdown, provider-aware help text
   (suggested model + key-console URL update with selection). Pre-fill
   logic never clobbers a user's custom model id.

Function names (`groq_chat_streaming`, `groq_complete`, `groq_complete_blocking`)
kept as-is to avoid touching 4 unrelated call sites; flagged in CHANGELOG as
a deferred cleanup.

## Outcome

- ✅ Impact: user can switch from Groq to Cerebras (or 6 other built-in providers, or any custom OpenAI-compatible endpoint) entirely via Settings UI — no rebuild, no env vars, no DB migration. Unblocks the F1–F5 cloud comparison run that was stalled by Groq quota.
- 🧪 Tests: none — pure config plumbing, no logic change in the request/response path. `cargo check --lib` queued to verify no compile errors. Real verification is a manual cloud-call smoke test once user picks a provider and pastes a key.
- 📁 Files: cloud_llm.rs (~25 lines added + 6 lines edited), api/index.ts (+3 lines), SettingsPage.tsx (~80 lines rewritten in Cloud section), CHANGELOG_DEV.md (+65 lines).
- 🔁 Next prompts: (a) once F5 re-test completes locally, run the same F1–F5 × 3 registers through a cloud provider (Cerebras Llama-3.3-70B recommended) for the capability-ceiling reference; (b) optional cleanup — rename `groq_*` functions to `cloud_*` (4 call sites).
- 🧠 Reflection: The original integration was already 95% multi-provider — the URL was the only thing pinning it to Groq. A 30-line refactor unlocked 7 providers at once because they all speak the same OpenAI shape. Same lesson as the Preamble fix: most "missing features" in this codebase are one or two lines from the surface.

## Evaluation notes (flywheel)

- Failure modes observed: none yet (no provider tested post-refactor).
- Graders run and results (PASS/FAIL): pending compile (`cargo check`) + user smoke test.
- Prompt variant (if applicable): n/a
- Next experiment: pick Cerebras (free, fast, user has key per MEMORY.md) and re-run F1–F5 × 3 registers; compare to local 3B results to quantify the cloud-capability uplift.
