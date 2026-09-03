---
id: 103
title: LLM provider free and paid API analysis for integration tab
stage: explainer
date: 2026-07-26
surface: agent
model: claude-opus-5
feature: api-integration-tab
branch: release-deploy
user: Riaz Mehmood
command: (freeform request)
labels: [research, providers, pricing, api-integration, pdf-deliverable]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - docs/llm-provider-api-analysis-2026-07.html
  - docs/llm-provider-api-analysis-2026-07.pdf
tests:
  - none (research deliverable, no code changed)
---

## Prompt

now we have one thing to work on, first we need to search the free api's and models available their stats and when will be they remains available and what is their current state and after one or in two years, and we also need a statical data for paid options also with same details, so we can integrate those into our api integration tab, where users will pic the provider and list will appear with tags of free models and paid models then user can put api key and use them. so first write the pdf of those analysis of models of all providers in list form then we will integrate this feature in our app.

## Response snapshot

Researched the current (July 2026) LLM provider landscape via web search/fetch across ~12 sources
(OpenRouter, CloudZero, Morph, TokenMix, Epoch AI, official Anthropic/OpenAI/Google deprecation docs,
benchr deprecation calendar), then produced a 10-page A4 PDF at
`docs/llm-provider-api-analysis-2026-07.pdf`, rendered from a print-styled HTML source kept alongside
it so it can be edited and re-rendered.

Report structure:
1. Executive summary (5 findings)
2. Provider landscape table — 18 providers split into permanent-free / trial-credit / paid-only
3. Free-tier detail ranked *for our RAG workload* (context ceiling weighted above token volume)
4. Paid pricing table — 25 models, USD per 1M in/out, banded frontier / workhorse / budget
5. Current state — six observed free-tier changes in the last 12 months
6. Outlook to mid-2028, with EVIDENCE and JUDGEMENT columns kept explicitly separate
7. Non-price selection criteria (training-on-data, licence, SLA, quantisation, residency)
8. Implications for the API Integration tab (the actionable section)
9. Sources

Key findings that change the integration design:
- Model IDs have roughly a 12-month half-life; 14 widely-used IDs (GPT-4o, GPT-4 Turbo, o1,
  Gemini 2.5 Pro/Flash, …) have published shutdown dates within the next 6 months. A compiled-in
  dropdown will be partially dead within a year → catalogue must be versioned refreshable data
  with a `known_shutdown_date` field driving a UI deprecation warning.
- Free/paid is an insufficient tag set. Three conditions are disqualifying and invisible under a
  binary split: TRAINS-ON-YOUR-DATA (Mistral free, Google free outside EU/UK/EEA, xAI credits),
  NON-COMMERCIAL (Cohere free), CONTEXT-LIMITED (Cerebras free 8K, Cloudflare 2K–8K — both below
  our ~8K RAG prompt, so they would silently truncate retrieved passages).
- TRIAL must be visually distinct from FREE or users will build on a tier that expires.
- TPM, not RPD, is the binding free-tier constraint for RAG. Groq's headline 14,400 RPD is really
  ~0.75 req/min sustained at an 8K prompt against a 6,000 TPM cap. The UI must show the effective
  rate derived from our measured prompt size — this is the project's recurring
  "two places measuring one thing" failure mode.
- Prompt caching (up to 90% off cached input) applies to our constant system+grounding prefix on
  every query; should be default-on in the provider adapter, not a user toggle.
- Recommend OpenRouter as the first integration (one key ≈ 30 free + most paid models, absorbs
  upstream churn), direct provider keys second for DPA / data-residency customers.

Stated limitation up front in the document: the 1–2 year availability question cannot be answered
factually — no provider guarantees a *tier* will exist in 2028; they publish per-*model* retirement
floors, which is a different promise. Section 6 therefore separates published evidence from our
inference and labels confidence, with an instruction not to present the judgement column to
customers as a commitment.

## Outcome

- ✅ Impact: Research deliverable complete; supplies the data model, tag taxonomy, and validation
  strategy for the forthcoming API Integration tab. No production code touched.
- 🧪 Tests: None applicable. PDF verified as generated (exit 0, 10 pages, 349 KB); pages were not
  visually inspected — `pdftoppm`/`pdftotext` are absent on this machine.
- 📁 Files: `docs/llm-provider-api-analysis-2026-07.html` (source), `.pdf` (deliverable).
- 🔁 Next prompts: implement the provider/model catalogue schema; build the integration tab UI with
  the 7-tag taxonomy; wire per-model key validation probe.
- 🧠 Reflection: Ranking free tiers by the generic "generosity" metric the sources use would have
  recommended Cerebras (1M tokens/day, top of most public rankings). Re-ranking against our actual
  constraint inverted that — its 8K free context sits below our prompt size, so it would truncate
  retrieved passages and produce exactly the ungrounded-answer failure we work hardest to avoid.

## Evaluation notes (flywheel)

- Failure modes observed: one source (cloudzero via morphllm) returned HTTP 429; substituted an
  equivalent source rather than retrying. Headless Chrome's `--print-to-pdf` silently no-ops without
  an explicit `--user-data-dir`; needed `Start-Process -Wait` with a dedicated profile.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): before building the UI, probe 3–4 candidate providers
  with a real Noviq RAG prompt to measure true sustained throughput against published TPM caps,
  and confirm the effective-rate formula in §8.4 before it is shown to users.
