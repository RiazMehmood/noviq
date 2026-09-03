# GPU Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable GPU-accelerated LLM inference on NVIDIA (CUDA) and AMD (Vulkan) hardware with auto-detected VRAM, auto-calculated layer offloading, and a user toggle in Settings.

**Architecture:** Replace the stubbed `detect_vram_gb()` in `hardware_detector.rs` with real cross-platform GPU detection via `nvidia-smi` (NVIDIA) and `/sys/class/drm` or `wmic` (AMD). Add `AtomicBool gpu_enabled` to `ModelSlot` and replace hardcoded `n_gpu_layers=35` with a runtime calculation based on VRAM and model file size. Expose two Tauri commands (`get_gpu_settings`, `set_gpu_enabled`) from `system.rs` and add a GPU section to `SettingsPage.tsx`.

**Tech Stack:** Rust 1.87, llama-cpp-2 with `cuda`+`vulkan` features, SQLite db_config table, SolidJS + Tailwind CSS

---

## File Map

| File | Change |
|------|--------|
| `src-tauri/Cargo.toml` | Add `cuda` + `vulkan` feature flags to llama-cpp-2 |
| `src-tauri/src/services/hardware_detector.rs` | Replace stub; add `GpuVendor` enum; add `gpu_vendor`, `gpu_name` to `HardwareTier` |
| `src-tauri/src/services/llm.rs` | Add `calculate_gpu_layers()`; add `gpu_enabled: AtomicBool` to `ModelSlot`; add `set_gpu_enabled()` |
| `src-tauri/src/db/migrations/011_gpu_settings.sql` | Seed `gpu_enabled=1` in `db_config` |
| `src-tauri/src/commands/system.rs` | Add `GpuSettings` struct + `get_gpu_settings` + `set_gpu_enabled` commands |
| `src-tauri/src/lib.rs` | Read persisted `gpu_enabled` at startup; call `model_slot.set_gpu_enabled()`; register two new commands |
| `src/api/index.ts` | Add `GpuSettings` interface + `getGpuSettings()` + `setGpuEnabled()` |
| `src/pages/SettingsPage.tsx` | Add GPU acceleration section with toggle + read-only info |

---

## Task 1: Add GPU Feature Flags to Cargo.toml

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Open Cargo.toml and locate the llama-cpp-2 line**

Current line (around line 40):
```toml
llama-cpp-2 = { version = "0.1", optional = true }
```

- [ ] **Step 2: Update the llama-cpp-2 dependency and add feature flags**

Replace the existing `llama-cpp-2` line and the `[features]` section. Find:
```toml
[features]
llm = ["dep:llama-cpp-2", "dep:encoding_rs"]
stt = ["dep:whisper-rs"]
default = ["llm"]
```

Replace with:
```toml
[features]
llm = ["dep:llama-cpp-2", "dep:encoding_rs"]
stt = ["dep:whisper-rs"]
cuda = ["llama-cpp-2/cuda"]
vulkan = ["llama-cpp-2/vulkan"]
default = ["llm", "cuda", "vulkan"]
```

- [ ] **Step 3: Verify it compiles**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo check 2>&1 | tail -5
```

Expected: `warning: ...` lines only, no `error[` lines.

- [ ] **Step 4: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/Cargo.toml
git commit -m "feat(gpu): add cuda + vulkan feature flags to llama-cpp-2"
```

---

## Task 2: Real GPU Detection in hardware_detector.rs

**Files:**
- Modify: `src-tauri/src/services/hardware_detector.rs`

- [ ] **Step 1: Write the failing unit test**

Add at the bottom of `hardware_detector.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nvidia_smi_output() {
        // Simulates: nvidia-smi --query-gpu=name,memory.total --format=csv,noheader
        let line = "NVIDIA GeForce RTX 3060, 6144 MiB";
        let parts: Vec<&str> = line.splitn(2, ',').collect();
        assert_eq!(parts.len(), 2);
        let name = parts[0].trim();
        let vram_str = parts[1].trim(); // "6144 MiB"
        let vram_mib: f32 = vram_str.split_whitespace().next().unwrap().parse().unwrap();
        let vram_gb = vram_mib / 1024.0;
        assert_eq!(name, "NVIDIA GeForce RTX 3060");
        assert!((vram_gb - 6.0).abs() < 0.1, "expected ~6.0 GB, got {vram_gb}");
    }

    #[test]
    fn test_hardware_tier_has_gpu_fields() {
        // HardwareTier must carry gpu_vendor and gpu_name
        let tier = HardwareTier::detect();
        // gpu_name is either a real name or empty string — never panics
        let _ = tier.gpu_name.len();
        // vendor is one of the three variants — always valid
        match tier.gpu_vendor {
            GpuVendor::Nvidia | GpuVendor::Amd | GpuVendor::None => {}
        }
    }
}
```

- [ ] **Step 2: Run the tests to confirm they fail**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo test hardware_detector 2>&1 | tail -20
```

Expected: `error[E0412]: cannot find type GpuVendor` — confirms the test is wired and the type is missing.

- [ ] **Step 3: Replace the full content of hardware_detector.rs**

```rust
//! Hardware detection — determines the `HardwareTier` used to select models and
//! constrain background task scheduling.

use std::process::Command;
use serde::{Deserialize, Serialize};
use sysinfo::System;

/// GPU hardware vendor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    None,
}

impl Default for GpuVendor {
    fn default() -> Self {
        GpuVendor::None
    }
}

/// Describes the hardware capability tier of the current machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareTier {
    /// Maximum LLM tier that can be used (1-5).
    pub max_model_tier: u8,
    /// Name of the embedding model to load.
    pub embedding_model: String,
    /// Whether the machine has a discrete (non-integrated) GPU with ≥ 2 GB VRAM.
    pub has_discrete_gpu: bool,
    /// VRAM in gigabytes (0 for integrated / unknown).
    pub vram_gb: f32,
    /// Total system RAM in gigabytes.
    pub ram_gb: f32,
    /// GPU vendor detected at startup.
    pub gpu_vendor: GpuVendor,
    /// GPU display name, e.g. "NVIDIA GeForce RTX 3060".
    pub gpu_name: String,
}

impl HardwareTier {
    /// Detect hardware and return an appropriate tier descriptor.
    pub fn detect() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        let ram_bytes = sys.total_memory();
        let ram_gb = ram_bytes as f32 / 1_073_741_824.0;

        let (gpu_vendor, vram_gb, gpu_name) = detect_gpu();
        let has_discrete_gpu = vram_gb >= 2.0;

        let (max_model_tier, embedding_model) = if ram_gb >= 32.0 {
            (5, "nomic-embed-text-v1.5".to_string())
        } else if ram_gb >= 16.0 {
            (4, "nomic-embed-text-v1.5".to_string())
        } else if ram_gb >= 12.0 {
            (3, "nomic-embed-text-v1.5".to_string())
        } else if ram_gb >= 8.0 {
            (3, "all-MiniLM-L6-v2".to_string())
        } else {
            (2, "all-MiniLM-L6-v2".to_string())
        };

        log::info!(
            "Hardware detected: RAM={:.1} GB, VRAM={:.1} GB, GPU={:?} ({}), tier={}",
            ram_gb, vram_gb, gpu_vendor, gpu_name, max_model_tier
        );

        HardwareTier {
            max_model_tier,
            embedding_model,
            has_discrete_gpu,
            vram_gb,
            ram_gb,
            gpu_vendor,
            gpu_name,
        }
    }
}

/// Try NVIDIA then AMD; return (vendor, vram_gb, display_name).
/// Returns (GpuVendor::None, 0.0, "") if no GPU found — never panics.
fn detect_gpu() -> (GpuVendor, f32, String) {
    if let Some(info) = detect_nvidia() {
        return info;
    }
    if let Some(info) = detect_amd() {
        return info;
    }
    (GpuVendor::None, 0.0, String::new())
}

/// Parse `nvidia-smi` output. Works on both Linux and Windows when NVIDIA drivers are installed.
fn detect_nvidia() -> Option<(GpuVendor, f32, String)> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.total", "--format=csv,noheader"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?;
    // Format: "NVIDIA GeForce RTX 3060, 6144 MiB"
    let parts: Vec<&str> = line.splitn(2, ',').collect();
    if parts.len() < 2 {
        return None;
    }
    let name = parts[0].trim().to_string();
    let vram_str = parts[1].trim();
    let vram_mib: f32 = vram_str.split_whitespace().next()?.parse().ok()?;
    let vram_gb = vram_mib / 1024.0;
    Some((GpuVendor::Nvidia, vram_gb, name))
}

#[cfg(target_os = "linux")]
fn detect_amd() -> Option<(GpuVendor, f32, String)> {
    // Vendor ID 0x1002 = AMD
    let vendor = std::fs::read_to_string("/sys/class/drm/card0/device/vendor").ok()?;
    if !vendor.trim().eq_ignore_ascii_case("0x1002") {
        return None;
    }
    let vram_bytes: u64 = std::fs::read_to_string(
        "/sys/class/drm/card0/device/mem_info_vram_total",
    )
    .ok()?
    .trim()
    .parse()
    .ok()?;
    let vram_gb = vram_bytes as f32 / 1_073_741_824.0;
    let name = std::fs::read_to_string("/sys/class/drm/card0/device/product_name")
        .unwrap_or_else(|_| "AMD GPU\n".to_string())
        .trim()
        .to_string();
    Some((GpuVendor::Amd, vram_gb, name))
}

#[cfg(target_os = "windows")]
fn detect_amd() -> Option<(GpuVendor, f32, String)> {
    let output = Command::new("wmic")
        .args(["path", "Win32_VideoController", "get", "Name,AdapterRAM", "/format:csv"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[2].trim();
        let name_lower = name.to_lowercase();
        if !name_lower.contains("amd") && !name_lower.contains("radeon") {
            continue;
        }
        let adapter_ram: u64 = parts[1].trim().parse().unwrap_or(0);
        if adapter_ram == 0 {
            continue;
        }
        let vram_gb = adapter_ram as f32 / 1_073_741_824.0;
        return Some((GpuVendor::Amd, vram_gb, name.to_string()));
    }
    None
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn detect_amd() -> Option<(GpuVendor, f32, String)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nvidia_smi_output() {
        let line = "NVIDIA GeForce RTX 3060, 6144 MiB";
        let parts: Vec<&str> = line.splitn(2, ',').collect();
        assert_eq!(parts.len(), 2);
        let name = parts[0].trim();
        let vram_str = parts[1].trim();
        let vram_mib: f32 = vram_str.split_whitespace().next().unwrap().parse().unwrap();
        let vram_gb = vram_mib / 1024.0;
        assert_eq!(name, "NVIDIA GeForce RTX 3060");
        assert!((vram_gb - 6.0).abs() < 0.1, "expected ~6.0 GB, got {vram_gb}");
    }

    #[test]
    fn test_hardware_tier_has_gpu_fields() {
        let tier = HardwareTier::detect();
        let _ = tier.gpu_name.len();
        match tier.gpu_vendor {
            GpuVendor::Nvidia | GpuVendor::Amd | GpuVendor::None => {}
        }
    }
}
```

- [ ] **Step 4: Run the tests — expect PASS**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo test hardware_detector 2>&1 | tail -10
```

Expected:
```
test services::hardware_detector::tests::test_parse_nvidia_smi_output ... ok
test services::hardware_detector::tests::test_hardware_tier_has_gpu_fields ... ok
test result: ok. 2 passed; 0 failed
```

- [ ] **Step 5: Cargo check — no errors**

```bash
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo check 2>&1 | grep "^error" | head -10
```

Expected: no output (zero errors).

- [ ] **Step 6: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/services/hardware_detector.rs
git commit -m "feat(gpu): real GPU detection — nvidia-smi + AMD sysfs/wmic; GpuVendor enum"
```

---

## Task 3: calculate_gpu_layers + gpu_enabled in ModelSlot (llm.rs)

**Files:**
- Modify: `src-tauri/src/services/llm.rs`

- [ ] **Step 1: Write the failing tests**

Add at the bottom of `llm.rs` (after all existing code):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_gpu_layers_full_offload() {
        // 6 GB VRAM, 3B model (2400 MB, 36 layers) → full offload
        // usable = 6144 - 512 = 5632 MB; per_layer = 2400/36 = 66 MB; 5632/66 = 85 → capped at 36
        let layers = calculate_gpu_layers(6.0, 36, 2400);
        assert_eq!(layers, 36, "6 GB VRAM should fully offload 36-layer 3B model");
    }

    #[test]
    fn test_calculate_gpu_layers_partial_offload() {
        // 4 GB VRAM, 7B model (4100 MB, 32 layers) → partial offload
        // usable = 4096 - 512 = 3584 MB; per_layer = 4100/32 = 128 MB; 3584/128 = 28
        let layers = calculate_gpu_layers(4.0, 32, 4100);
        assert_eq!(layers, 28, "4 GB VRAM should partially offload 28/32 layers of 7B model");
    }

    #[test]
    fn test_calculate_gpu_layers_zero_vram() {
        let layers = calculate_gpu_layers(0.0, 36, 2400);
        assert_eq!(layers, 0, "zero VRAM should return 0 layers");
    }

    #[test]
    fn test_estimate_model_layers_by_name() {
        assert_eq!(estimate_model_layers_from_name("qwen2.5-3b-instruct-q4_k_m.gguf"), 36);
        assert_eq!(estimate_model_layers_from_name("qwen2.5-7b-instruct-q4_k_m.gguf"), 32);
        assert_eq!(estimate_model_layers_from_name("unknown-model.gguf"), 32);
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo test llm::tests 2>&1 | tail -10
```

Expected: `error[E0425]: cannot find function calculate_gpu_layers`

- [ ] **Step 3: Add the helper functions to llm.rs**

Find the line `pub const GROUNDING_SYSTEM_PROMPT_VERSION` (around line 59 in the `#[cfg(feature = "llm")]` section). Add these two functions **before** that const (outside any cfg block so tests can reach them):

```rust
/// Calculate how many transformer layers to offload to GPU.
/// Takes explicit model_layers and model_size_mb to allow unit testing without a real file.
pub(crate) fn calculate_gpu_layers(vram_gb: f32, model_layers: u32, model_size_mb: u64) -> u32 {
    if vram_gb < 0.1 || model_layers == 0 || model_size_mb == 0 {
        return 0;
    }
    let mb_per_layer = model_size_mb / model_layers as u64;
    if mb_per_layer == 0 {
        return model_layers;
    }
    let usable_mb = ((vram_gb * 1024.0) as u64).saturating_sub(512);
    let n = usable_mb / mb_per_layer;
    n.min(model_layers as u64) as u32
}

/// Estimate layer count from model filename (e.g. "qwen2.5-3b-..." → 36).
pub(crate) fn estimate_model_layers_from_name(filename: &str) -> u32 {
    let lower = filename.to_lowercase();
    if lower.contains("3b") { 36 }
    else if lower.contains("7b") { 32 }
    else if lower.contains("13b") { 40 }
    else if lower.contains("70b") { 80 }
    else { 32 }
}

/// Get model file size in MB and layer count from path.
fn model_stats_from_path(path: &std::path::Path) -> (u32, u64) {
    let size_mb = std::fs::metadata(path)
        .map(|m| m.len() / 1_048_576)
        .unwrap_or(2400); // default: assume 3B model
    let name = path.file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let layers = estimate_model_layers_from_name(&name);
    (layers, size_mb)
}
```

- [ ] **Step 4: Add `gpu_enabled: AtomicBool` to the real ModelSlot struct**

Find the struct definition under `#[cfg(feature = "llm")]` (around line 142):

```rust
// FIND this:
pub struct ModelSlot {
    backend: Arc<LlamaBackend>,
    inner: Mutex<SlotInner>,
}

// REPLACE with:
pub struct ModelSlot {
    backend: Arc<LlamaBackend>,
    inner: Mutex<SlotInner>,
    gpu_enabled: std::sync::atomic::AtomicBool,
}
```

- [ ] **Step 5: Update ModelSlot::new() to initialise the AtomicBool**

Find `ModelSlot::new()` under `#[cfg(feature = "llm")]` and add the field:

```rust
// FIND the Ok(Self { ... }) block in new() and add gpu_enabled:
Ok(Self {
    backend: Arc::new(LlamaBackend::init()?),
    inner: Mutex::new(SlotInner { current: None, evict_at: None }),
    gpu_enabled: std::sync::atomic::AtomicBool::new(true),
})
```

- [ ] **Step 6: Add set_gpu_enabled() method to the real ModelSlot**

Add this method inside the `#[cfg(feature = "llm")]` impl block for `ModelSlot`:

```rust
/// Toggle GPU acceleration. Evicts the current model so next load picks up the new setting.
pub fn set_gpu_enabled(&self, enabled: bool) {
    self.gpu_enabled.store(enabled, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut g) = self.inner.lock() {
        g.current = None; // force reload on next infer()
    }
    log::info!("GPU acceleration {}", if enabled { "enabled" } else { "disabled" });
}
```

- [ ] **Step 7: Replace the hardcoded n_gpu_layers in ModelSlot::load()**

Find line 175:
```rust
let gpu: u32 = if tier.has_discrete_gpu { 35 } else { 0 };
```

Replace with:
```rust
let gpu_on = self.gpu_enabled.load(std::sync::atomic::Ordering::Relaxed);
let gpu: u32 = if tier.has_discrete_gpu && gpu_on {
    let (layers, size_mb) = model_stats_from_path(std::path::Path::new(model_path));
    calculate_gpu_layers(tier.vram_gb, layers, size_mb)
} else {
    0
};
log::info!("GPU layers: {gpu} (gpu_enabled={gpu_on}, vram={:.1}GB)", tier.vram_gb);
```

- [ ] **Step 8: Add stub set_gpu_enabled() to the non-llm ModelSlot**

Find the `#[cfg(not(feature = "llm"))]` impl block for ModelSlot (around line 88). Add:

```rust
pub fn set_gpu_enabled(&self, _enabled: bool) {}
```

- [ ] **Step 9: Run the tests — expect PASS**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo test llm::tests 2>&1 | tail -10
```

Expected:
```
test services::llm::tests::test_calculate_gpu_layers_full_offload ... ok
test services::llm::tests::test_calculate_gpu_layers_partial_offload ... ok
test services::llm::tests::test_calculate_gpu_layers_zero_vram ... ok
test services::llm::tests::test_estimate_model_layers_by_name ... ok
test result: ok. 4 passed; 0 failed
```

- [ ] **Step 10: Cargo check**

```bash
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo check 2>&1 | grep "^error" | head -10
```

Expected: no output.

- [ ] **Step 11: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/services/llm.rs
git commit -m "feat(gpu): calculate_gpu_layers from VRAM + model size; AtomicBool gpu_enabled in ModelSlot"
```

---

## Task 4: DB Migration — Seed gpu_enabled in db_config

**Files:**
- Create: `src-tauri/src/db/migrations/011_gpu_settings.sql`

- [ ] **Step 1: Create the migration file**

```sql
-- GPU settings: seed gpu_enabled=1 (on by default).
-- Stored in db_config as a key-value pair alongside embedding_dim.
-- Value '1' = GPU enabled, '0' = GPU disabled.
INSERT OR IGNORE INTO db_config (key, value) VALUES ('gpu_enabled', '1');
```

- [ ] **Step 2: Verify sqlx picks up the migration (cargo check)**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo check 2>&1 | grep "^error" | head -10
```

Expected: no errors. SQLx validates migration files at compile time.

- [ ] **Step 3: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add "src-tauri/src/db/migrations/011_gpu_settings.sql"
git commit -m "feat(gpu): seed gpu_enabled=1 in db_config via migration 011"
```

---

## Task 5: GPU Commands in system.rs

**Files:**
- Modify: `src-tauri/src/commands/system.rs`

- [ ] **Step 1: Add imports at the top of system.rs**

Find the existing imports at the top:
```rust
use crate::services::hardware_detector::HardwareTier;
```

Add below it:
```rust
use crate::services::llm::{calculate_gpu_layers, estimate_model_layers_from_name, ModelSlot};
use std::sync::Arc;
```

- [ ] **Step 2: Add GpuSettings struct**

Add after the `ModelStatus` struct (around line 24):

```rust
/// GPU acceleration status returned to the frontend.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuSettings {
    /// Whether GPU acceleration is currently enabled (persisted preference).
    pub enabled: bool,
    /// Vendor string: "Nvidia", "Amd", or "None".
    pub vendor: String,
    /// Detected VRAM in GB (0.0 if no GPU).
    pub vram_gb: f32,
    /// Display name e.g. "NVIDIA GeForce RTX 3060".
    pub gpu_name: String,
    /// Auto-calculated layer count when enabled (display only).
    pub layers_auto: u32,
    /// Total layers in current model (display only).
    pub layers_total: u32,
}
```

- [ ] **Step 3: Add get_gpu_settings command**

Add at the bottom of `system.rs`:

```rust
#[tauri::command]
pub async fn get_gpu_settings(
    tier: tauri::State<'_, HardwareTier>,
    pool: tauri::State<'_, sqlx::SqlitePool>,
) -> Result<GpuSettings, String> {
    let enabled: bool = sqlx::query_scalar!(
        "SELECT value FROM db_config WHERE key = 'gpu_enabled'"
    )
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| e.to_string())?
    .map(|v: String| v != "0")
    .unwrap_or(true);

    let t = tier.inner();
    // Approximate layer count for display: use the expected model for this tier.
    let (rel_path, _, _) = model_info_for_tier(t);
    let filename = std::path::Path::new(rel_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let layers_total = estimate_model_layers_from_name(&filename);
    // For layer display, approximate model size: 3B ≈ 2400 MB, 7B ≈ 4100 MB
    let model_size_mb: u64 = if filename.contains("7b") { 4100 } else { 2400 };
    let layers_auto = if t.has_discrete_gpu {
        calculate_gpu_layers(t.vram_gb, layers_total, model_size_mb)
    } else {
        0
    };

    Ok(GpuSettings {
        enabled,
        vendor: format!("{:?}", t.gpu_vendor),
        vram_gb: t.vram_gb,
        gpu_name: t.gpu_name.clone(),
        layers_auto,
        layers_total,
    })
}
```

- [ ] **Step 4: Add set_gpu_enabled command**

```rust
#[tauri::command]
pub async fn set_gpu_enabled(
    enabled: bool,
    pool: tauri::State<'_, sqlx::SqlitePool>,
    model_slot: tauri::State<'_, Arc<ModelSlot>>,
) -> Result<(), String> {
    let value = if enabled { "1" } else { "0" };
    sqlx::query!(
        "INSERT INTO db_config (key, value) VALUES ('gpu_enabled', ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        value
    )
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    model_slot.set_gpu_enabled(enabled);
    log::info!("GPU enabled set to {enabled} by user");
    Ok(())
}
```

- [ ] **Step 5: Cargo check**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo check 2>&1 | grep "^error" | head -10
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/commands/system.rs
git commit -m "feat(gpu): get_gpu_settings + set_gpu_enabled Tauri commands"
```

---

## Task 6: Wire gpu_enabled into App Startup + Register Commands (lib.rs)

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Read persisted gpu_enabled at startup and apply it**

In `setup_models_and_watchers()`, find the warm-up block (around line 177). After the `model_slot` is created (line 173-174) and before the warm-up `spawn_blocking` block, add:

```rust
// Apply persisted GPU preference before warm-up so first load uses correct setting.
{
    let gpu_val: Option<String> = sqlx::query_scalar!(
        "SELECT value FROM db_config WHERE key = 'gpu_enabled'"
    )
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);
    let gpu_on = gpu_val.map(|v| v != "0").unwrap_or(true);
    model_slot.set_gpu_enabled(gpu_on);
    log::info!("GPU acceleration at startup: {gpu_on}");
}
```

- [ ] **Step 2: Register the two new commands in the invoke_handler**

Find the `system::` section in the invoke_handler (around lines 85-89):

```rust
// FIND:
    system::get_hardware_tier,
    system::check_model,
    system::download_model,
    system::restart_app,

// REPLACE with:
    system::get_hardware_tier,
    system::check_model,
    system::download_model,
    system::restart_app,
    system::get_gpu_settings,
    system::set_gpu_enabled,
```

- [ ] **Step 3: Cargo check**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo check 2>&1 | grep "^error" | head -10
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src-tauri/src/lib.rs
git commit -m "feat(gpu): wire gpu_enabled into startup; register get_gpu_settings + set_gpu_enabled"
```

---

## Task 7: TypeScript Bindings + Settings UI

**Files:**
- Modify: `src/api/index.ts`
- Modify: `src/pages/SettingsPage.tsx`

- [ ] **Step 1: Add GpuSettings interface and invoke functions to api/index.ts**

Find the last export in `src/api/index.ts`. Append:

```typescript
export interface GpuSettings {
  enabled: boolean;
  vendor: string;       // "Nvidia" | "Amd" | "None"
  vram_gb: number;
  gpu_name: string;
  layers_auto: number;
  layers_total: number;
}

export async function getGpuSettings(): Promise<GpuSettings> {
  return await invoke<GpuSettings>("get_gpu_settings");
}

export async function setGpuEnabled(enabled: boolean): Promise<void> {
  await invoke<void>("set_gpu_enabled", { enabled });
}
```

- [ ] **Step 2: Add GPU section to SettingsPage.tsx**

In `src/pages/SettingsPage.tsx`, add the import at the top with other api imports:

```typescript
import { GpuSettings, getGpuSettings, setGpuEnabled } from "../api/index";
```

Inside the `SettingsPage` component, add signal and onMount call. Find where `loadHardwareTier()` is called in `onMount` (around line 48-49). Add before the existing `onMount`:

```typescript
const [gpuSettings, setGpuSettings] = createSignal<GpuSettings | null>(null);
```

Inside the existing `onMount`, after `await loadHardwareTier();`:

```typescript
try {
  const gpu = await getGpuSettings();
  setGpuSettings(gpu);
} catch (e) {
  console.warn("GPU settings unavailable:", e);
}
```

- [ ] **Step 3: Add the GPU section to the JSX**

Find the return statement in `SettingsPage`. Add the GPU section before the existing Hardware section (or after it — wherever hardware info is shown):

```tsx
{/* GPU Acceleration */}
<Show when={gpuSettings() !== null}>
  {(gpu) => (
    <div class="bg-gray-800 rounded-lg p-4 mb-4">
      <h3 class="text-white font-semibold mb-3">GPU Acceleration</h3>
      <Show
        when={gpu().vendor !== "None" && gpu().vram_gb > 0}
        fallback={
          <p class="text-gray-400 text-sm">No GPU detected — running CPU-only</p>
        }
      >
        <div class="flex items-center justify-between mb-2">
          <div>
            <p class="text-white text-sm">{gpu().gpu_name}</p>
            <p class="text-gray-400 text-xs">
              {gpu().vram_gb.toFixed(1)} GB VRAM &nbsp;·&nbsp;
              Layers: {gpu().layers_auto}/{gpu().layers_total}
              {gpu().layers_auto === gpu().layers_total ? " (full)" : " (partial)"}
            </p>
          </div>
          <button
            class={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
              gpu().enabled ? "bg-blue-600" : "bg-gray-600"
            }`}
            onClick={async () => {
              const next = !gpu().enabled;
              await setGpuEnabled(next);
              setGpuSettings({ ...gpu(), enabled: next });
            }}
            title={gpu().enabled ? "Disable GPU acceleration" : "Enable GPU acceleration"}
          >
            <span
              class={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                gpu().enabled ? "translate-x-6" : "translate-x-1"
              }`}
            />
          </button>
        </div>
        <Show when={!gpu().enabled}>
          <p class="text-yellow-400 text-xs">GPU disabled — restart inference for change to take effect</p>
        </Show>
      </Show>
    </div>
  )}
</Show>
```

- [ ] **Step 4: TypeScript compile check**

```bash
cd "/media/riaz/New Volume/echo-AI"
npm run typecheck 2>&1 | grep "error TS" | head -10
```

If `typecheck` script doesn't exist, run:
```bash
npx tsc --noEmit 2>&1 | grep "error TS" | head -10
```

Expected: no TypeScript errors.

- [ ] **Step 5: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add src/api/index.ts src/pages/SettingsPage.tsx
git commit -m "feat(gpu): GpuSettings TS bindings + GPU toggle section in SettingsPage"
```

---

## Task 8: Full Build + Verification

- [ ] **Step 1: Run all Rust tests**

```bash
cd "/media/riaz/New Volume/echo-AI/src-tauri"
CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  cargo test 2>&1 | tail -15
```

Expected: all existing tests pass + 6 new GPU tests pass.

- [ ] **Step 2: Start the dev app**

```bash
cd "/media/riaz/New Volume/echo-AI"
RUSTUP_TOOLCHAIN=stable \
  CARGO_HOME="/media/riaz/New Volume/.cargo" \
  CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target" \
  PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:/home/riaz/.nvm/versions/node/v22.22.2/bin:$PATH" \
  npm run tauri dev 2>&1 | head -30
```

- [ ] **Step 3: Manual verification checklist**

Open Settings page and confirm:

- [ ] If GPU present: GPU card shows vendor name + VRAM + layer count
- [ ] Toggle switches between enabled/disabled state
- [ ] On toggle → log shows "GPU acceleration enabled/disabled"
- [ ] If no GPU: "No GPU detected — running CPU-only" shown (no toggle visible)
- [ ] Chat query response time improved vs CPU baseline (if GPU present)

- [ ] **Step 4: Final commit**

```bash
cd "/media/riaz/New Volume/echo-AI"
git add -p  # review any remaining changes
git commit -m "feat(gpu): GPU integration complete — auto-detect NVIDIA/AMD, calculate layers, settings toggle"
```
