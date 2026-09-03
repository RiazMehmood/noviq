# Design: User-Selectable LLM Model

**Date:** 2026-04-26
**Feature branch:** `003-model-selection`
**Status:** Approved — ready for implementation plan

---

## Problem

The current system auto-selects 3B or 7B model based on hardware tier. Users have no visibility into this choice, no way to override it, and no awareness of the quality/speed tradeoff they're getting. The 1.5B model is not offered at all.

---

## Goals

1. Let users explicitly choose which model to download and activate (1.5B / 3B / 7B).
2. Surface hardware-aware recommendations with honest tradeoff descriptions.
3. Prevent users from choosing models their hardware can't support.
4. Allow switching the active model instantly without restart.
5. Support multiple downloaded models on disk simultaneously (no auto-delete).

---

## Non-Goals

- Automatic model switching based on workload type.
- Custom / third-party model upload.
- Model quantization selection (Q4_K_M is fixed for all tiers).

---

## Model Catalog

Hardcoded in `src-tauri/src/commands/system.rs`:

| ID    | Display Name                    | File                                   | Size  | Min RAM | URL (HuggingFace bartowski)                            |
|-------|---------------------------------|----------------------------------------|-------|---------|--------------------------------------------------------|
| `1b5` | Qwen2.5-1.5B Instruct Q4_K_M   | qwen2.5-1.5b-instruct-q4_k_m.gguf     | ~1 GB | 4 GB    | bartowski/Qwen2.5-1.5B-Instruct-GGUF                  |
| `3b`  | Qwen2.5-3B Instruct Q4_K_M     | qwen2.5-3b-instruct-q4_k_m.gguf       | ~2 GB | 6 GB    | bartowski/Qwen2.5-3B-Instruct-GGUF                    |
| `7b`  | Qwen2.5-7B Instruct Q4_K_M     | qwen2.5-7b-instruct-q4_k_m.gguf       | ~4 GB | 12 GB   | bartowski/Qwen2.5-7B-Instruct-GGUF                    |

Recommended model = highest compatible model for the user's RAM.

---

## Persistence

**File:** `{app_data_dir}/model_preference.json`
```json
{ "active_model": "3b" }
```

- Loaded at startup into `Mutex<String>` Tauri managed state (`ModelPreference`).
- Written synchronously on `set_active_model` command.
- If file is absent → onboarding modal is shown on next launch.
- `select_model()` in `query_router.rs` reads this state instead of inferring from hardware tier.

---

## Backend Changes

### New / Modified Commands (`commands/system.rs`)

**`list_models()`** → `Vec<ModelInfo>`
```rust
pub struct ModelInfo {
    pub id: String,           // "1b5" | "3b" | "7b"
    pub name: String,
    pub size_gb: f32,
    pub min_ram_gb: f32,
    pub speed_label: String,  // "~20s" | "~60s" | "~120s"
    pub quality_label: String,// "Basic" | "Good" | "Best"
    pub url: String,
    pub file_name: String,
    pub exists: bool,         // model file present on disk
    pub is_active: bool,      // matches ModelPreference state
    pub is_compatible: bool,  // ram_gb >= min_ram_gb
    pub is_recommended: bool, // highest compatible model
}
```

**`set_active_model(id: String)`**
- Validates `id` is in catalog and `is_compatible`.
- Evicts current LLM from `ModelSlot` (calls `model_slot.evict()`).
- Writes `model_preference.json`.
- Updates managed `ModelPreference` state.
- Returns `Ok(())`.

**`download_model(id: String)`** ← replaces current `download_model()`
- Takes explicit model `id` instead of inferring from tier.
- Same streaming `model:download-progress` events as today.
- After completion does NOT auto-activate; user must call `set_active_model`.

**`check_model()`** ← updated
- Returns status for the **active** model (reads `ModelPreference`), not tier-inferred.

**`get_model_preference()`** → `String` (active model id)
- Read-only accessor for frontend store initialisation.

### New Managed State

```rust
pub struct ModelPreference(pub Mutex<String>); // active model id
```

Registered in `lib.rs` alongside `HardwareTier`.

### `query_router.rs` — `select_model()`

Reads `ModelPreference` state instead of `HardwareTier.max_model_tier`. Falls back to 3B if preference file missing or model file absent.

---

## Frontend Changes

### New Component: `ModelSetupModal.tsx`

- Shown on app mount when `model_preference.json` is absent (`get_model_preference()` returns empty/null).
- Full-screen overlay, cannot be dismissed without choosing or skipping.
- Three model cards with: name, size, speed, quality, recommended badge.
- Incompatible models greyed out with tooltip.
- "Download & Activate" → inline progress bar → "Start using EchoAI" on complete.
- "Skip for now" → closes modal; `ModelDownloadBanner` handles fallback.

### Updated: `SettingsPage.tsx`

New **AI Model** section above the hardware block:

- One row per model: name, size, status badge, action button.
- **Active**: green "Active ✓" badge, no button.
- **Downloaded, not active**: "Set Active" button.
- **Not downloaded**: "Download" button with inline progress bar.
- **Incompatible**: greyed row, tooltip showing RAM requirement.
- Footnote: *"Switching models takes effect on the next query. No restart needed."*

### Updated: `ModelDownloadBanner.tsx`

- Calls `list_models()` to check active model status instead of `check_model()`.
- Still shows banner when active model file is absent.

### Updated: `api/index.ts`

- Add `ModelInfo` type.
- Add `listModels`, `setActiveModel`, `downloadModel(id)`, `getModelPreference` bindings.
- Remove old zero-arg `downloadModel`.

### Store

No dedicated store needed — `SettingsPage` and `ModelSetupModal` call commands directly. `ModelDownloadBanner` uses `listModels()` on mount.

---

## Error Handling

| Scenario | Behaviour |
|----------|-----------|
| User picks incompatible model | Backend rejects with error; frontend greyes option out |
| Download fails mid-stream | `.gguf.tmp` cleaned up; error shown inline; retry available |
| `set_active_model` called with missing file | Returns error; preference not written |
| Model file deleted externally | `check_model()` returns `exists: false`; banner re-appears |
| Preference file corrupt / unreadable | Falls back to 3B silently; re-shows onboarding if file absent |

---

## SDD Alignment

This feature follows the `001-notebook-knowledge-engine` SDD pattern:

- **Spec:** this document → `docs/superpowers/specs/2026-04-26-model-selection-design.md`
- **Feature branch:** `003-model-selection`
- **PHRs:** under `history/prompts/003-model-selection/`
- **Spec file (SDD):** `specs/003-model-selection/spec.md`
- **Plan + tasks:** generated by `writing-plans` skill next

---

## Acceptance Criteria

1. Fresh install with no `model_preference.json` → onboarding modal appears before any UI is usable.
2. User on 16 GB RAM sees all 3 models; 7B has "Recommended" badge; 1.5B/3B selectable.
3. User on 6 GB RAM sees 7B greyed with "Requires 12 GB RAM" tooltip.
4. Download progress shows MB downloaded / total MB with percentage bar.
5. After download, user must explicitly click "Set Active" (or "Download & Activate") — no auto-switch.
6. Switching active model: next query uses new model; no restart prompt shown.
7. Both models remain on disk after switch — previous file not deleted.
8. `ModelDownloadBanner` disappears once active model file is present.
9. Settings page reflects live state — no page reload needed after switch.
