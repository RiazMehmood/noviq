---
id: 0021
title: Plan amendment Option C semantic gaps
stage: plan
date: 2026-06-10
surface: agent
model: claude-fable-5
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: riaz
command: /sp.plan
labels: [plan-amendment, template-ingestion, option-c, gap-detection, semantic-coverage, mistral-7b]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - specs/009-lgd-drafting/plan.md
  - specs/009-lgd-drafting/research.md
  - specs/009-lgd-drafting/data-model.md
  - specs/009-lgd-drafting/contracts/drafting-commands.md
  - specs/009-lgd-drafting/quickstart.md
  - CLAUDE.md (agent context, via update-agent-context.sh)
tests:
  - none run (plan-only); new contract acceptance checks added for FR-008a/010/010a/010b/019
---

## Prompt

Amend 009-lgd-drafting plan for the 2026-06-10 clarifications: (1) template ingestion rewrite — Option C: Mistral-7B free-text segmentation (local model, one-time offline) + deterministic verbatim-verify gate (FR-010/010a/010b), replacing heading-heuristic ingest.rs; grammar/llguidance path is dead (hard abort, proven in spike); refuse-not-degrade when model missing. (2) gaps.rs semantic content-aware two-tier coverage (FR-019): embed required_source_category vs confirmed-doc chunks, whole-corpus tier-a suggestions. (3) officer-supplied fact channel (FR-008a). Spike evidence at ~/Desktop/Noviq Documents reports/option_c_ingestion_spike_20260610/

## Response snapshot

Amendment to the existing 009 plan (did NOT run setup-plan.sh — it would clobber the committed plan.md with the blank template). No Phase-0 unknowns remained: the 2026-06-10 Option-C spike is the research (raw outputs preserved on Desktop).

- **research.md**: R5 marked SUPERSEDED (heuristic produced 13 garbage slots on a flawless real instrument; 3 root causes recorded). New **R5a** = Option C decision: Mistral-7B-Instruct-v0.3 Q4_K_M (already at `models/ir_builder/` from F006), free-text JSON + tolerant parse (llguidance = hard C++ abort, dead), deterministic verbatim-verify gate (text+provision verbatim substring, hyphen-tolerant `resolve_provision_label`, category-from-provision, boilerplate exclusion), flag-for-review on rejection, refuse-not-degrade on missing model. New **R11** = two-tier semantic gap coverage (embed category vs chunk_embeddings, threshold-gated; tier-a suggestion / tier-b missing). New **R12** = officer-supplied fact channel. Follow-ups updated (vec0 ANN perf note; true Summary gold still needed).
- **data-model.md**: `template_slot.provision_ref` via **additive migration 024**; in-memory `ProposedSlot.low_confidence/rejection_reason`; `Gap` reshaped two-tier (Suggestion{suggested_docs} | Missing{destination_folder}); `GapResolution` gains AcceptedSuggestion + OfficerSupplied{value}; FactBlock officer-supplied attribution; FR mapping updated.
- **contracts/drafting-commands.md**: `ingest_template` reshaped (model-assisted + gate, lowConfidence/rejectionReason/provisionRef fields, model-missing error, no heuristic fallback); `update_doc_set` Gap gains tier/suggestedDocs; `resolve_gap` decisions extended (acceptSuggestion, officerSupplied); 6 new contract acceptance checks incl. gold-anchored ingestion check.
- **plan.md**: amendment preamble (D1/D2/D3); Technical Context adds Mistral-7B Tier-4 worker + fastembed coverage + migration 024; Constitution Check re-evaluated — II (Tier-4 escalation empirically justified), III (7B exclusive demand-load, one-time, evicted), VII (refusal = visible graceful degradation) — **PASS**; structure tree updated (ingest.rs/gaps.rs rewrites); phasing adds **Phase D** (ingestion rewrite) + **Phase E** (semantic coverage + officer facts), recommended order **D → E → B → C**.
- **quickstart.md**: template-setup + gap walkthrough updated; 4 new smoke checks.
- Agent context updated via `update-agent-context.sh claude`.

## Outcome

- ✅ Impact: Plan artifacts now fully encode the Option-C ingestion rewrite and two-tier semantic gap coverage; constitution gates re-checked and PASS; implementation phased with D/E fixing live-pilot blockers first.
- 🧪 Tests: none run (plan stage); acceptance anchors defined (real gold ingestion, 8-PDF corpus coverage).
- 📁 Files: plan.md, research.md, data-model.md, contracts/drafting-commands.md, quickstart.md, CLAUDE.md (auto).
- 🔁 Next prompts: /sp.adr model-assisted-template-ingestion (significance test passes); then /sp.tasks to generate Phase D/E tasks.
- 🧠 Reflection: setup-plan.sh is destructive for amendments — skipped deliberately; the spike doubled as Phase-0 research so no research agents were needed.

## Evaluation notes (flywheel)

- Failure modes observed: none this session; plan encodes the empirically proven failure modes (heuristic segmentation, title-token coverage, grammar abort, 3B paraphrase).
- Graders run and results (PASS/FAIL): constitution gate PASS (13 principles).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): calibrate the coverage similarity threshold on the 8-PDF live corpus ("consultations" → RoB 1986 must clear it).
