# Quickstart — Officer Walkthrough

**Feature:** 007 Pakistan Government Legal RAG
**Audience:** A government officer (or a developer pretending to be one) testing the slice that just landed.
**Time:** ~10 minutes once a slice ships end-to-end.

This walkthrough is the smallest end-to-end exercise that proves the
feature works for its primary user. It maps directly to the three User
Stories in `spec.md`.

---

## Prerequisites

- Latest `007-legal-rag-pakistan` branch built locally (`cargo build --manifest-path src-tauri/Cargo.toml` and `npm install` succeed).
- Tauri dev shell running: `npm run tauri dev`.
- These three PDFs available on disk (sourced from gazette or sindhassembly.gov.pk):
  1. The Sindh Local Government Act, 2013
  2. The Sindh Right of Access to Information Act, 2017
  3. The Sindh Local Government Rules, 2018 (or a similar Rules instrument made under the LG Act)
- Models on disk under `models/` (already present from Feature 006):
  - `models/ir_builder/Mistral-7B-Instruct-v0.3-Q4_K_M.gguf`
  - `models/orchestrator/Qwen2.5-3B-Instruct-Q4_K_M.gguf`
  - `models/classifier/Qwen2.5-0.5B-Instruct-Q4_K_M.gguf`
- No internet required.

---

## Story 1 — Fast path on a freshly uploaded Act (P1)

1. Open the EchoAI desktop shell. Pick or create a folder, e.g. "Sindh corpus".
2. Drop **Sindh RTI Act 2017** into that folder.
3. Wait for the document badge to flip from "indexing…" to "fast-indexed" (target: under 60 seconds for a ~50-page Act).
4. Open the chat panel. Confirm scope is set to *Both* (default).
5. Ask: **"Show me section 6 of the RTI Act."**
   - Expected: chat bubble returns the text of Section 6 with a citation rendered by `services/citation/format_short` (something like `RTI Act 2017 §6 — p. 8`). On fast tier the citation discloses "hierarchical detail unavailable on fast tier" if asked.
6. Ask: **"What chapters are in this Act?"**
   - Expected: ordered list of chapter titles with page-anchored citations.
7. Ask: **"Summarize Chapter II."**
   - Expected: 3–5 sentence officer-grade summary; citations point to the chapter range.
8. Ask: **"What duties does this Act place on the Public Information Officer?"**
   - Expected: bulleted duties, each citing the section that imposes it.

If any of the above fails: Slice A or Slice B is not yet shipped.

---

## Story 2 — Deep re-index and amendment-aware querying (P2)

1. On the RTI Act document card, click **"Deep re-index with Mistral"**.
2. Observe the `DeepReindexProgress` overlay. It must show ≥ one update every 10 seconds, across the five phases (classify → ir_build → amendment_parse → xref_extract → procedure_extract → chunks_replace → reembed). Verify that the "Cancel" button works mid-run: click it once, then re-trigger Deep re-index from scratch.
3. Wait for completion. The document badge should flip to "deep-indexed". Expected runtime for ~50 pages on a CPU workstation: **≈ 2–4 hours**. Reasoning: research §R5 chunks per Section (not per page), a 50-page Act yields ~20–40 IR chunks, and Feature 006 memory records steady-state ~4–6 min/chunk on Mistral 7B CPU. Faster on GPU; resumable across power-cycles via `progress_store` (no chunk re-runs needed).
4. Ask in chat: **"Show me Section 6(2)(a)."**
   - Expected: the exact clause text, citation now in full hierarchical form (e.g. `The Sindh Right of Access to Information Act, 2017, Section 6(2)(a)` — and if the clause has been amended, the citation includes `— substituted by ...`).
5. If the source PDF contains amendment markers, ask: **"Show the amendment history of Section 6."**
   - Expected: a Timeline (amendment flavour) chat bubble listing each amendment event with the amending instrument cited.
6. Re-upload the same PDF (drag in again). Document MUST remain "deep-indexed" (sticky tier). Re-processing should reuse the JSON IR sidecar at `{app_data_dir}/ir_cache/<sha>.json` and complete in under a minute.

If 4–5 fail: Slice A is shipped but Slice C (intent router + amendment-diff/Timeline handlers) is not yet there. Story 1 should still work.

---

## Story 3 — Cross-document procedure flow (P3)

1. Deep re-index all three documents (Sindh LG Act, Sindh RTI Act, Sindh LG Rules). One at a time.
2. Verify the Sindh LG Rules card shows at least one `implements` cross-reference to the Sindh LG Act in its "see also" panel (lights up once Slice C ships).
3. Ask in chat: **"What is the approval flow for a building plan application under the Sindh LG Act and Rules?"**
   - Expected: an ordered actor-step path stitched across the Act + Rules. Each step renders a citation. The flow renders linearly by default.
4. Click the **"Show branches"** affordance on a step that has a known conditional ("if amount > 5 lakh", "if dispute, escalate to …"). Expected: branching tree renders.
5. Narrow the scope picker to just the Sindh RTI Act (a single document) and ask: **"Compare how the RTI Act and the LG Act handle penalties."**
   - Expected: the CrossCompare handler refuses and explains that the active scope contains only one instrument; the officer is invited to widen scope.
6. Widen scope back to *Both* and re-ask. Expected: a structured side-by-side comparison with citations from each instrument.

If 3–6 fail: Slice C or D is not yet shipped. Stories 1–2 should still work.

---

## Acceptance signals (per slice)

| Slice | Story exercised | Pass when… |
|---|---|---|
| 0 | n/a | `cargo test --test citation_golden_test` is 12/12 GREEN; `cargo test --test migration_020_test` GREEN. |
| A | Story 1 + 2 (steps 1–4) | Three Sindh docs index without error; ≥ 90% top-level Sections captured; 100% obvious amendment markers found; SC-001 hit. |
| B | Story 1 (steps 5–8) | 40-prompt officer eval ≥ 75% OK on offline-mid; citation accuracy ≥ 95% on lookups. |
| C | Story 2 (step 5) + Story 3 (steps 5–6) | 70-prompt eval ≥ 80% OK on amendment-diff (offline-mid); xref edge precision ≥ 85% on 100-edge sample. |
| D | Story 3 (steps 3–4) | 15 procedure queries ≥ 70% return full flow with correct actors + per-step citations; "show branches" works on 3+ procedures. |

---

## Troubleshooting

- **"Deep re-index" greyed out.** RAM tier insufficient (Tier 1, < 8 GB). Constitution §VII degrades gracefully — UI shows a message; document remains fast-indexed.
- **Mistral run hangs.** Cancel via the overlay; re-run. If it fails twice in the same phase, check `/tmp/echo-ai-debug/` for the dumped failed chunk. The retry-with-rising-temperature path (0.4 / 0.7 / 0.9) inherited from Feature 006 should recover the majority of single-chunk failures.
- **Wrong intent classified.** Reply in chat with the suggested intent chip ("did you mean …?"). If no chip appears, the classifier's confidence is high; check the rule patterns in `intent_router/classifier.rs` and add coverage.
- **Citation looks wrong.** All citations route through `services::citation::LegalCitation::format_*`. Update the goldens in `citation_golden_test.rs` if officer review confirms the new form is correct.
