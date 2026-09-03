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
    /// Deterministic tier used by integration tests so they don't depend on the
    /// host's CPU/GPU/RAM. Mirrors a "tier 3 / 8 GB / no-GPU" baseline laptop.
    #[doc(hidden)]
    pub fn default_for_test() -> Self {
        Self {
            max_model_tier: 3,
            embedding_model: "all-MiniLM-L6-v2".to_string(),
            has_discrete_gpu: false,
            vram_gb: 0.0,
            ram_gb: 8.0,
            gpu_vendor: GpuVendor::None,
            gpu_name: String::new(),
        }
    }

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
