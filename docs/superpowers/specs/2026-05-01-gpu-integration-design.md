# GPU Integration Design

**Date:** 2026-05-01
**Feature:** GPU Acceleration for LLM Inference
**Priority:** 1 (highest — 5–10x LLM inference speedup)
**Status:** Design approved, pending implementation plan

---

## 1. Goals

- Auto-detect GPU vendor and VRAM at startup (NVIDIA via CUDA, AMD/other via Vulkan)
- Auto-calculate optimal `n_gpu_layers` from VRAM + model size — no hardcoded values
- Default: GPU enabled if detected; user can disable via Settings toggle
- CI remains CPU-only (free GitHub Actions runner); GPU code compiles but falls back to CPU gracefully
- Zero impact on existing scheduler, embedder, query pipeline, or priority_model_slot

## 2. Non-Goals

- macOS Metal support (out of scope for now)
- GPU-accelerated embedding (fastembed ONNX is already fast at ~95ms; not a bottleneck)
- GPU runner in CI
- Manual layer count configuration by user

---

## 3. Architecture Overview

```
startup
   │
   ▼
hardware_detector.rs
   ├── detect RAM (existing)
   ├── detect GPU vendor: NVIDIA | AMD | None  (NEW)
   ├── detect VRAM in GB                        (NEW)
   └── HardwareTier gains: { vram_gb, gpu_vendor, gpu_name }
   │
   ▼
settings_store — gpu_enabled flag (DEFAULT = true)
   │
   ▼
llm.rs — on model load
   ├── gpu_enabled=false  → n_gpu_layers = 0
   ├── gpu_enabled=true, NVIDIA → CUDA backend, auto-calculate layers
   └── gpu_enabled=true, AMD   → Vulkan backend, auto-calculate layers
   │
   ▼
Cargo.toml
   └── llama-cpp-2 compiled with cuda + vulkan features
       llama.cpp selects correct backend at runtime automatically

Frontend Settings Panel
   └── GPU section: vendor name + VRAM + ON/OFF toggle + layer count info
```

---

## 4. GPU Detection (`hardware_detector.rs`)

Replace the stubbed `detect_vram_gb() → 0.0` with real cross-platform detection.

### 4.1 New Types

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    None,
}

#[derive(Debug, Clone)]
pub struct GpuInfo {
    pub vendor: GpuVendor,
    pub vram_gb: f32,
    pub name: String,
}
```

### 4.2 Detection Logic

**NVIDIA (Linux + Windows):**
```
run: nvidia-smi --query-gpu=name,memory.total --format=csv,noheader
parse: "NVIDIA GeForce RTX 3060, 6144 MiB"
→ vendor=Nvidia, vram_gb=6.0, name="NVIDIA GeForce RTX 3060"
```

**AMD Linux:**
```
read: /sys/class/drm/card0/device/vendor  → "0x1002" = AMD
read: /sys/class/drm/card0/device/mem_info_vram_total  → bytes → GB
read: /sys/class/drm/card0/device/product_name  → GPU name
```

**AMD Windows:**
```
run: wmic path Win32_VideoController get Name,AdapterRAM /format:csv
parse: AdapterRAM (bytes) → GB, Name → gpu_name
filter: rows where Name contains "AMD" or "Radeon"
```

**Fallback chain:**
1. Try nvidia-smi (both platforms)
2. If fails → try AMD path (platform-specific)
3. If both fail → `GpuInfo { vendor: None, vram_gb: 0.0, name: "" }`
4. No crash, no panic — silent CPU-only fallback

### 4.3 HardwareTier Updates

Add fields to existing struct:
```rust
pub gpu_vendor: GpuVendor,
pub vram_gb: f32,
pub gpu_name: String,
```

`has_discrete_gpu` (existing bool) now set to `true` when `vram_gb >= 2.0` and `gpu_vendor != None`.

---

## 5. GPU Layer Auto-Calculation (`llm.rs`)

Replace hardcoded `n_gpu_layers=35` with runtime calculation.

### 5.1 Formula

```rust
fn calculate_gpu_layers(vram_gb: f32, model_path: &Path) -> u32 {
    let model_size_mb = fs::metadata(model_path).map(|m| m.len() / 1_048_576).unwrap_or(4096);
    let total_layers = estimate_layers_from_path(model_path); // 36 for 3B, 32 for 7B
    let mb_per_layer = model_size_mb / total_layers as u64;
    let usable_mb = (vram_gb * 1024.0) as u64 - 512; // 512 MB reserved for runtime + KV cache
    let n = usable_mb / mb_per_layer;
    n.min(total_layers as u64) as u32
}

fn estimate_layers_from_path(path: &Path) -> u32 {
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    if name.contains("3b") { 36 }
    else if name.contains("7b") { 32 }
    else if name.contains("13b") { 40 }
    else { 32 } // safe default
}
```

### 5.2 Examples

| VRAM | Model | Usable MB | MB/layer | Layers offloaded |
|------|-------|-----------|----------|-----------------|
| 6 GB | 3B (2.4 GB) | 5632 | 67 | 36/36 — full GPU |
| 4 GB | 7B (4.1 GB) | 3584 | 128 | 28/32 — partial |
| 8 GB | 7B (4.1 GB) | 7680 | 128 | 32/32 — full GPU |
| 0 GB | any | — | — | 0 — CPU only |

Partial offload (e.g., 28/32 layers) still provides significant speedup — first N layers run on GPU.

---

## 6. Cargo.toml Feature Flags

```toml
[dependencies]
llama-cpp-2 = { version = "0.1", optional = true }

[features]
llm = ["dep:llama-cpp-2", "dep:encoding_rs"]
cuda = ["llama-cpp-2/cuda"]
vulkan = ["llama-cpp-2/vulkan"]
default = ["llm", "cuda", "vulkan"]
```

**Runtime backend selection:** llama.cpp automatically selects the best available backend:
- NVIDIA machine with CUDA compiled in → CUDA selected
- AMD machine with Vulkan compiled in → Vulkan selected
- CPU-only machine → CPU fallback (no crash)

**Local build requirements:**
- NVIDIA GPU users: CUDA toolkit must be installed
- AMD GPU users: Vulkan SDK (usually present via GPU drivers)
- CPU-only users: no change, works as before

---

## 7. Settings Toggle

### 7.1 Database

```sql
-- additive migration, does not break existing data
ALTER TABLE settings ADD COLUMN gpu_enabled INTEGER NOT NULL DEFAULT 1;
```

Default `1` = enabled. Stored in `echo.db`.

### 7.2 New Tauri Commands

```rust
#[tauri::command]
async fn get_gpu_settings(state: AppState) -> GpuSettings

#[tauri::command]
async fn set_gpu_enabled(enabled: bool, state: AppState) -> Result<()>

pub struct GpuSettings {
    pub enabled: bool,
    pub vendor: String,       // "NVIDIA" | "AMD" | "None"
    pub vram_gb: f32,
    pub gpu_name: String,     // "NVIDIA GeForce RTX 3060"
    pub layers_auto: u32,     // calculated layers when enabled
}
```

`set_gpu_enabled()` persists the flag and triggers model slot reload with new `n_gpu_layers`.

### 7.3 Frontend Settings Panel

```
┌─ GPU Acceleration ──────────────────────────────────┐
│  NVIDIA GeForce RTX 3060 — 6.0 GB VRAM             │
│  [●  Enabled]              Layers: 36 / 36 (full)  │
└─────────────────────────────────────────────────────┘

┌─ GPU Acceleration ──────────────────────────────────┐
│  No GPU detected — running CPU-only                 │
└─────────────────────────────────────────────────────┘
```

- Toggle hidden when no GPU detected (no confusion for CPU-only users)
- VRAM, layer count, and GPU name are read-only info
- Toggle change triggers model unload + reload with new layer count

---

## 8. Files Modified

| File | Change |
|------|--------|
| `src-tauri/Cargo.toml` | Add cuda + vulkan features to llama-cpp-2 |
| `src-tauri/src/services/hardware_detector.rs` | Replace stub `detect_vram_gb()` with real detection; add GpuInfo, GpuVendor; update HardwareTier |
| `src-tauri/src/services/llm.rs` | Replace hardcoded `n_gpu_layers=35` with `calculate_gpu_layers()`; respect `gpu_enabled` flag |
| `src-tauri/src/db/migrations.rs` | Add `gpu_enabled` column migration |
| `src-tauri/src/commands/settings.rs` | Add `get_gpu_settings` + `set_gpu_enabled` commands |
| `src-tauri/src/lib.rs` | Register new commands |
| `src/components/settings/SettingsPanel.tsx` | Add GPU section with toggle + info display |
| `src/api/index.ts` | Add TS bindings for new commands |

---

## 9. Error Handling

| Scenario | Behavior |
|----------|----------|
| nvidia-smi not found | Silent fallback to AMD detection path |
| AMD sysfs path missing | Silent fallback to GpuInfo::none() |
| VRAM < 2 GB | `has_discrete_gpu=false`, n_gpu_layers=0, CPU mode |
| GPU detected but CUDA build fails | llama.cpp falls back to CPU silently |
| User disables GPU in settings | n_gpu_layers=0 on next model load |
| Model file not found for size calc | Assume 4096 MB, conservative layer estimate |

---

## 10. Expected Performance Impact

| Scenario | Before GPU | After GPU (estimate) |
|----------|-----------|----------------------|
| Tier 3 chat (3B, full GPU) | ~15s | ~2–4s |
| Tier 4 chat (7B, full GPU) | ~65s | ~7–12s |
| NER Pass 2 (LLM-based) | slow | 4–5x faster |
| Embedding (fastembed ONNX) | ~95ms | no change (already fast) |

---

## 11. CI / Build

- GitHub Actions `build-windows.yml`: **no changes required**
- `npm run tauri build` uses default features (cuda + vulkan compiled in)
- On GitHub Actions CPU runner: CUDA/Vulkan compiled but unused — CPU fallback active
- No GPU runner needed, no extra cost
