//! Feature 011 (T005) — per-hardware-tier tuning for the bundled PostgreSQL
//! engine. Principle III: the single model slot owns the RAM budget; the
//! engine is tuned small so PG + warm 3B + demand-loaded 7B coexist on
//! 8–16 GB machines (the oomd-kill class of failure this feature removes).

use crate::services::hardware_detector::HardwareTier;

/// Engine configuration derived from the detected hardware tier.
#[derive(Debug, Clone)]
pub struct PgTuning {
    /// `shared_buffers`, e.g. "512MB".
    pub shared_buffers: &'static str,
    /// `hnsw.ef_search` session default for KNN queries (research R3; raised
    /// only if SC-005 parity < 95%).
    pub hnsw_ef_search: u32,
    /// `max_parallel_workers_per_gather`.
    pub parallel_workers: u32,
}

pub fn tuning_for(tier: &HardwareTier) -> PgTuning {
    let ram = tier.ram_gb;
    if ram < 8.0 {
        PgTuning { shared_buffers: "256MB", hnsw_ef_search: 40, parallel_workers: 1 }
    } else if ram < 16.0 {
        PgTuning { shared_buffers: "512MB", hnsw_ef_search: 40, parallel_workers: 2 }
    } else {
        PgTuning { shared_buffers: "1GB", hnsw_ef_search: 64, parallel_workers: 2 }
    }
}

/// `postgresql.conf` overrides applied at engine start (loopback enforcement
/// lives in engine.rs via Settings/host, not here).
pub fn server_config(t: &PgTuning) -> Vec<(&'static str, String)> {
    vec![
        ("shared_buffers", t.shared_buffers.to_string()),
        ("max_parallel_workers_per_gather", t.parallel_workers.to_string()),
        // Keep the engine's own caches modest; the OS page cache does the rest.
        ("effective_cache_size", "2GB".to_string()),
        ("work_mem", "32MB".to_string()),
        // Local single-user app: same durability stance as the old SQLite
        // `synchronous=NORMAL` choice (power loss may lose last frames, not the DB).
        ("synchronous_commit", "off".to_string()),
        ("max_connections", "20".to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::hardware_detector::{GpuVendor, HardwareTier};

    fn tier(ram_gb: f32) -> HardwareTier {
        HardwareTier {
            max_model_tier: 3,
            embedding_model: "nomic-embed-text-v1.5".into(),
            has_discrete_gpu: false,
            vram_gb: 0.0,
            ram_gb,
            gpu_vendor: GpuVendor::None,
            gpu_name: String::new(),
        }
    }

    #[test]
    fn tuning_scales_with_ram() {
        assert_eq!(tuning_for(&tier(4.0)).shared_buffers, "256MB");
        assert_eq!(tuning_for(&tier(15.5)).shared_buffers, "512MB");
        assert_eq!(tuning_for(&tier(32.0)).shared_buffers, "1GB");
    }
}
