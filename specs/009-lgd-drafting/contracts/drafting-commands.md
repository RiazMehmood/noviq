# Contracts: Drafting Tauri Commands

Backend exposes these via `src-tauri/src/commands/drafting.rs`. All run offline against `echo.db` and the existing services. Field names use serde `camelCase` for the TS boundary (per project serde/TS convention). Errors return a human-readable message + remediation (Development Constraint).

Progress/notification events use the existing event channel pattern (like `importer://progress`): `drafting://progress` and `drafting://docset-updated`.

---

## Template setup (US2)

### `ingest_template` *(reshaped 2026-06-10 — Option C, R5a)*
Parse an uploaded format into a *proposed* slot schema via model-assisted segmentation + deterministic verbatim-verify gate.
- **In**: `{ filePath: string, instrumentType: "summary" | "note", isFilledExample: boolean }`
- **Out**: `ProposedSchema { name?: string, instrumentType, slots: Slot[] }`
  - `Slot { ordering, kind: "boilerplate"|"cited"|"prose", label?, boilerplateText?, requiredSourceCategory?, provisionRef?, proseGuidance?, lowConfidence?: boolean, rejectionReason?: string }`
- **Behavior**: reuse extraction; demand-load the Tier-4 segmentation model (Mistral-7B free-text JSON, tolerant parse); gate every proposed slot deterministically — text + provision verbatim in source, provision re-resolved hyphen-tolerant via `decompose::resolve_provision_label`, category derived from the matched provision, structural boilerplate never cited. Gate-rejected slots returned with `lowConfidence: true` + `rejectionReason` for officer decision (FR-010a). One-time offline op (~minutes on CPU); emits `drafting://progress` during segmentation. If `isFilledExample`, also stage a `gold_example` (not yet committed).
- **Errors**: unsupported format → "Unsupported file type; use PDF, DOCX, TXT, or Markdown." · segmentation model missing or output unparseable after retries → actionable error naming the required model; **no schema produced, no heuristic fallback** (FR-010b).

### `confirm_template`
Persist a corrected schema (create or overwrite).
- **In**: `{ templateId?: number, name: string, instrumentType, slots: Slot[], goldExampleRef?: string }`
- **Out**: `{ templateId: number }`
- **Behavior**: validate per data-model rules; insert/overwrite `template_schema` + `template_slot`; commit any staged `gold_example`. **Invariant**: gold content never enters retrieval tables (SC-007).
- **Errors**: validation failure → field-specific message.

### `list_templates` / `get_template` / `delete_template`
- `list_templates() → Template[]` (id, name, instrumentType, updatedAt)
- `get_template(templateId) → Template` (with slots)
- `delete_template(templateId) → {}` (cascades slots; gold_example association nulled)

---

## Drafting flow (US1, US3, US4)

### `start_draft`
Begin a draft from a template + intent answers; returns the proposed document set.
- **In**: `{ templateId: number, intent: { instrumentType, addressee: string, purpose: string, scheme: string } }`
- **Out**: `DraftSession { draftId: string, candidateDocs: DocRef[] }`
  - `DocRef { docId, title, sourcePath, role: "rules" | "case" }`
- **Behavior**: search the WHOLE corpus (FR-016); propose candidates; tag role (R2), officer-correctable.

### `update_doc_set` *(gap shape reshaped 2026-06-10 — two-tier semantic coverage, R11)*
Officer curates the working set (add/remove/retag).
- **In**: `{ draftId, confirmedDocs: DocRef[] }`
- **Out**: `{ gaps: Gap[] }`  where `Gap { slotId, requiredSourceCategory, tier: "suggestion" | "missing", suggestedDocs?: DocRef[], destinationFolder? }`
  - `tier: "suggestion"` → category is covered (content-semantic) by indexed doc(s) not in the confirmed set; `suggestedDocs` present, one-turn accept.
  - `tier: "missing"` → no indexed document covers the category; `destinationFolder` present (watched corpus folder).
- **Behavior**: persist working set in session; coverage judged by embedding `requiredSourceCategory` against document chunk embeddings (threshold-gated), never doc titles; whole corpus searched for tier-a suggestions. `fact:` slots with no covering case doc surface the officer-supplied prompt (FR-008a) instead of a `missing` gap.

### `resolve_gap` *(decisions extended 2026-06-10)*
Record a per-gap decision.
- **In**: `{ draftId, slotId, decision: "proceedWithout" | "supplied" | "acceptSuggestion" | "officerSupplied", value?: string }`
- **Out**: `{ accepted: boolean }`
- **Behavior**: `proceedWithout` marks the slot for refuse-or-flag. `supplied` is normally driven by the watch event below rather than called directly. `acceptSuggestion` adds the gap's `suggestedDocs` to the confirmed set and recomputes gaps (one-turn accept). `officerSupplied` (fact slots only) stores `value` in the session; rendered with explicit "officer-supplied" attribution, never as a citation; the model never alters it.

### Event: `drafting://docset-updated`
Emitted when the watched corpus folder indexes a new document during an open draft.
- **Payload**: `{ draftId, addedDocs: DocRef[], updatedGaps: Gap[] }`
- **Behavior**: reuse `file_watcher` + `indexer`; re-propose doc set; notify (FR-020).

### `render_draft`
Produce the final draft after explicit confirmation (FR-022).
- **In**: `{ draftId, confirmed: true }`
- **Out**: `DraftOutput { blocks: Block[] }`
  - `Block` is one of: `{ type:"boilerplate", label, text }` | `{ type:"cited", label, quote, citation }` | `{ type:"fact", label, value, attribution }` | `{ type:"prose", label, text }` | `{ type:"flagged", label, reason }`
- **Behavior**:
  - Cited slots → scoped retrieval over confirmed `rules` docs → `decompose::resolve_provision_label` selector → verbatim quote + deterministic hierarchical citation; `None` → flagged (FR-004/005/006, R1).
  - Fact slots → extractive value from confirmed `case` docs + attribution (FR-008).
  - Prose slots → single warm generator, bounded prompt, no number/citation allowed (FR-007); sequential (Principle III).
  - Boilerplate → verbatim (FR-014).
  - Emits `drafting://progress` per slot (R3).
- **Errors**: generator unavailable on current tier → degrade: render cited/fact/boilerplate, mark prose slots "generation unavailable on this hardware tier" (Principle VII).

### `export_draft`
Save/copy the rendered draft.
- **In**: `{ draftId, format: "txt" | "pdf" }`
- **Out**: `{ path: string }`
- **Behavior**: reuse existing `exporters`; user-initiated save (CAUTIOUS per Principle V).

---

## Contract acceptance checks (map to tests)

- [ ] `render_draft` output: every `cited` block's number+quote appears verbatim in a confirmed `rules` doc chunk (SC-001).
- [ ] Any cited slot with no anchoring chunk → `flagged`, never a guessed `cited` (SC-002).
- [ ] No `prose` block contains a rule/article/section number or citation (FR-007).
- [ ] `confirm_template` with a gold example → example never appears in any subsequent `render_draft` retrieval (SC-007).
- [ ] After `drafting://docset-updated`, gaps shrink without restarting the draft (SC-005).
- [ ] `start_draft` proposes from the whole corpus with no manual scope input (FR-016).
- [ ] `ingest_template` on the real gold (GOLD_SPSC_age_relaxation_2025): actual citations (Rule-6(c) SPSC Reg-2023, Rule-23 SCUG 2020) land in cited slots with verbatim `provisionRef`; letterhead/dateline/C.C./signature regions are NOT cited slots (FR-010/010a).
- [ ] Every `ingest_template` slot's text + provision is a verbatim substring of the source; gate-rejected slots carry `lowConfidence` + `rejectionReason` (FR-010a).
- [ ] `ingest_template` with no segmentation model on disk → actionable error, no schema (FR-010b).
- [ ] On the 8-PDF live corpus, a category covered by an indexed-but-unconfirmed doc yields `tier: "suggestion"` (e.g. "consultations" → RoB 1986 via content, not title); only genuinely uncovered categories yield `tier: "missing"` (FR-019).
- [ ] `officerSupplied` fact value renders with "officer-supplied" attribution, visually distinct, never as a citation (FR-008a).
