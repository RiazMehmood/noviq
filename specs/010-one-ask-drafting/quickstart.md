# Quickstart: One-Ask Drafting (010)

**Date**: 2026-06-11 · Validates the user stories in [spec.md](spec.md) on a dev machine.

## Prerequisites

- Mistral-7B-Instruct-v0.3 Q4_K_M present at `models/ir_builder/` (existing).
- Sindh corpus indexed (the 10 PDFs at `~/Desktop/Sindh Govt Documents/` via Index Folder).
- `npm run tauri dev` running; NTFS cargo target/CARGO_HOME env set as usual.

## 1. One ask → draft (US1, US4)

1. Open the drafting tab → it shows the **one-ask view** (no template picker, no intent form — FR-015).
2. Type: `I want to get 4 billion to develop schools/IT institutes for youngsters` → submit.
3. **Verify**: command returns instantly; stage indicator shows *Researching* within 1 s; *Researching* completes ≤ 15 s (SC-005 — the vec0 KNN fix); then *Composing 1/5 … 5/5* with visible heartbeat (no silent gap > 10 s); then *Verifying*; then the draft renders.
4. **Verify draft shape** (FR-002): numbered paragraphs flowing Background → Legal/Procedural (with citations) → Financial → Recommendation → Prayer; **no** letterhead/ref#/addressee/signature/CC anywhere.
5. **Verify figures** (FR-003): "4 billion" appears verbatim; no other unexplained monetary figures.
6. Hover each citation → source document + provision label + matched text shown (FR-008). Spot-check 3 citations against the source PDFs: must be verbatim (SC-002).
7. Click **Copy** → paste into a text editor: clean numbered text, flags (if any) visible as bracketed notices (FR-011).

## 2. Refuse-or-flag (US2)

1. New draft: `I want to approve construction of a nuclear power plant in Karachi` (subject absent from corpus).
2. **Verify**: output contains explicit inline flags ("no source available…") or a plain refusal; **zero** rule/section numbers that don't exist verbatim in the corpus (SC-007). Flags are visually distinct from verified citations.

## 3. Conversational revision + resume (US3)

1. On the US1 draft, send: `make paragraph 3 shorter`.
2. **Verify**: only paragraph 3's section regenerates (progress shows a single section); other paragraphs byte-identical; all citations still verified.
3. Send: `add the procurement angle` → verify SPPRA/Procurement Act provisions appear, verified.
4. Close the app entirely; relaunch; reopen the drafting tab.
5. **Verify** (FR-010): session list shows the draft; opening it restores conversation + draft; revision still works.

## 4. Cancel + interrupted recovery (US4, edge cases)

1. Start a new ask; during *Composing 2/5* press **Cancel** → status `Cancelled` within seconds, UI usable, no half-draft shown as finished.
2. Start another ask; kill the app during composing; relaunch → session loads as *failed (interrupted)* with a **Retry** action; retry completes normally.

## 5. Error remediation (constitution)

- Temporarily rename `models/ir_builder/*.gguf` → start an ask → human-readable error naming the model path and how to restore it. Restore the file.
- Fresh profile with no indexed folder → start an ask → "index a folder first" remediation message.

## Automated gates

```bash
# backend (no live LLM — canned writer output fixtures)
CARGO_TARGET_DIR=… cargo test --lib drafting
CARGO_TARGET_DIR=… cargo test --test one_ask_acceptance
# KNN parity (R1 caveat): brute-force top-k ≡ MATCH/k top-k on fixture corpus
CARGO_TARGET_DIR=… cargo test knn_parity
# frontend
npx tsc --noEmit
```

## Live perf measurements to record (T-gate for SC-003/005)

| Measurement | Target | Where |
|---|---|---|
| Research stage wall-clock (10-PDF corpus, 65 GB DB) | ≤ 15 s | dev log timing line |
| End-to-end first draft | ≤ 15 min | dev log |
| Revision turn (one section) | ≤ 5 min | dev log |
| Longest gap between progress events | ≤ 10 s | frontend console |
