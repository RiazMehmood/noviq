---
id: 104
title: Integrate provider catalogue and key validation into the API integration tab
stage: green
date: 2026-07-26
surface: agent
model: claude-opus-5
feature: api-integration-tab
branch: release-deploy
user: Riaz Mehmood
command: (freeform request)
labels: [api-integration, inference, catalogue, admin-ui, gpu-credits]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/services/inference/catalog.json (new)
  - src-tauri/src/services/inference/catalog.rs (new)
  - src-tauri/src/services/inference/mod.rs
  - src-tauri/src/api/admin.rs
  - src-tauri/src/api/router.rs
  - src/pages/Admin.tsx
  - docs/gpu-cloud-credits-analysis-2026-07.{html,pdf} (new)
  - CHANGELOG_DEV.md
tests:
  - "catalog.rs: 7 unit tests written, NEVER RUN (no cargo on this box)"
  - "catalog.json: parses, 12 providers / 41 models, invariants checked via python — PASS"
  - "frontend: tsc --noEmit clean — PASS"
  - "frontend: vite build clean — PASS"
  - "vitest: could not run (jsdom dev-dep missing, pre-existing)"
---

## Prompt

now integrate this feature in our app

(mid-turn) in between work on another task also, i want to know free credit offering for gpu server on aws/google/ or any other provider, and for 1 department currently with 100 users how much time we can use that. search for this and share results so i started working on that.

(mid-turn) what about gpu work? did u stored that into the file?

## Response snapshot

**Discovery changed the shape of the work.** Explored before writing and found the backend seam already
existed: `EngineKind::Remote`, a runtime-swappable `RemoteConfig` behind an `RwLock`, `db_config`
persistence, a write-only API key, and `GET/POST /api/admin/inference` — plus a provider picker in the
"Model & Inference" tab with Groq/Google/OpenRouter/Custom. So this was an enrichment of a working seam,
not a new subsystem. Asked the architect two decisions that materially changed the work (catalogue
location; provider coverage) and got: backend-served JSON, all viable providers.

**Shipped.**
- `catalog.json` — 12 providers / 41 models (17 free, 24 paid) with tier, context, price, RPM/TPM/RPD,
  `shutdown_date`, `trains_on_data`, `non_commercial_only`.
- `catalog.rs` — override-then-bundled resolution, `effective_rpm()`, `CONTEXT_WARN_TOKENS`, 7 tests.
- `GET /api/admin/providers`, `POST /api/admin/inference/validate`.
- `Admin.tsx` — deleted the hardcoded `PROVIDERS`/`ProviderId`; tagged filterable model list,
  "Test key & model" button, provider-terms panel, graceful degradation when the catalogue is absent.

**Second task (GPU credits)** — delivered as `docs/gpu-cloud-credits-analysis-2026-07.pdf` (6 pp).
The framing in the question needed correcting: GPU credits are consumed by *wall-clock uptime*, not by
users or tokens. Sized Noviq's real load at ~1.25 GPU-hours/day for 100 users (~5% utilisation of one
24 GB card), so runway varies 17× by uptime policy and only ~3× by provider choice. NVIDIA Inception is
the highest-value application (no fee/equity/funding minimum, and it gates the AWS + Nebius tiers).

## Outcome

- ✅ Impact: The API-integration tab now presents free/paid tags plus three disqualifying-condition
  tags, a derived (not headline) rate limit, and a live per-model entitlement probe. Catalogue is
  refreshable on the VM without a frontend rebuild.
- 🧪 Tests: JSON invariants PASS; `tsc` PASS; `vite build` PASS. **Rust never compiled** — no toolchain
  on this machine; the 7 `catalog.rs` tests have never executed. Must be built on the VM before deploy.
- 📁 Files: 2 new Rust/JSON files, 4 edited, 2 new docs, changelog entry.
- 🔁 Next prompts: build + `cargo test --lib catalog` on the VM; probe the shipped model ids and prune
  any that 404; wire prompt caching into the provider adapter.
- 🧠 Reflection: Two process notes worth keeping. (1) I reported "cargo check passed" off a background
  job whose exit code came from `tail` in a pipeline, not from `cargo` — `cargo` was never even found.
  A piped exit code is not a build result; `set -o pipefail` or checking the output would have caught it
  immediately. (2) The pre-existing `PROVIDERS` constant looked like the natural place to extend, and
  extending it would have been half the diff — but model ids retire on a ~12-month cycle, so the right
  answer was to delete it and move the data server-side.

## Evaluation notes (flywheel)

- Failure modes observed: **false green from a piped exit code** (see reflection); `reqwest` is declared
  `default-features = false` without `json`, which looked like a blocker until the lock file showed
  `serde_json` pulled in via feature unification and `remote.rs` already using `.json()`.
- Graders run and results (PASS/FAIL): JSON invariants PASS · tsc PASS · vite build PASS · cargo NOT RUN
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): on the VM, run the probe against every model id in
  `catalog.json` and record which return 200 — that turns the catalogue from researched to verified,
  and is the cheapest way to find the ids that were hallucinated by aggregator sources.
