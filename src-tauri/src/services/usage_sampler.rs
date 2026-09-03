//! Per-query resource sampler (F014 follow-on — admin usage visibility).
//!
//! Server-side generation is serialized (one chat turn at a time behind the gen
//! gate), so sampling the whole server process + GPU *during* a turn attributes
//! cleanly to the single user whose query is running. A background sampler polls
//! the current process (CPU%, RSS) and the GPU (via `nvidia-smi`) and reports the
//! PEAK observed between [`Sampler::start`] and [`Sampler::finish`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

/// Peak resource use observed across one sampling window.
#[derive(Debug, Clone, Default)]
pub struct ResourcePeak {
    /// Peak server-process CPU% (can exceed 100 on multi-core).
    pub cpu_pct: f32,
    /// Peak server-process resident memory, MB.
    pub ram_mb: f32,
    /// Peak GPU utilization%, `None` when no GPU / `nvidia-smi` absent.
    pub gpu_util_pct: Option<f32>,
    /// Peak GPU memory used, MB; `None` when no GPU.
    pub gpu_mem_mb: Option<f32>,
}

/// A running background sampler. Drop without `finish` is safe (the task ends on
/// the next tick once its `Arc<AtomicBool>` is the only owner left? no — call
/// `finish` to stop it deterministically and collect the peak).
pub struct Sampler {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<ResourcePeak>,
}

impl Sampler {
    /// Begin sampling the current process + GPU in a blocking background task.
    pub fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_c = stop.clone();
        let handle = tokio::task::spawn_blocking(move || sample_loop(&stop_c));
        Self { stop, handle }
    }

    /// Signal the sampler to stop and return the peak it observed.
    pub async fn finish(self) -> ResourcePeak {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.await.unwrap_or_default()
    }
}

fn sample_loop(stop: &AtomicBool) -> ResourcePeak {
    use sysinfo::{Pid, ProcessRefreshKind, System};

    let mut peak = ResourcePeak::default();
    let pid: Pid = match sysinfo::get_current_pid() {
        Ok(p) => p,
        Err(_) => return peak,
    };
    let mut sys = System::new();
    let refresh = ProcessRefreshKind::new().with_cpu().with_memory();
    // Prime the CPU baseline — the first sample after this yields a real delta.
    sys.refresh_process_specifics(pid, refresh);

    let mut tick: u32 = 0;
    loop {
        std::thread::sleep(Duration::from_millis(250));
        tick += 1;

        sys.refresh_process_specifics(pid, refresh);
        if let Some(p) = sys.process(pid) {
            let cpu = p.cpu_usage();
            let ram_mb = p.memory() as f32 / (1024.0 * 1024.0); // sysinfo 0.30: bytes
            if cpu > peak.cpu_pct {
                peak.cpu_pct = cpu;
            }
            if ram_mb > peak.ram_mb {
                peak.ram_mb = ram_mb;
            }
        }

        // GPU is heavier to sample (spawns nvidia-smi) — poll ~once a second.
        if tick % 4 == 0 {
            if let Some((util, mem)) = sample_gpu() {
                if peak.gpu_util_pct.map_or(true, |v| util > v) {
                    peak.gpu_util_pct = Some(util);
                }
                if peak.gpu_mem_mb.map_or(true, |v| mem > v) {
                    peak.gpu_mem_mb = Some(mem);
                }
            }
        }

        if stop.load(Ordering::Relaxed) {
            break;
        }
    }
    peak
}

/// One `nvidia-smi` sample → `(utilization %, memory used MB)`. `None` when the
/// binary is missing (CPU-only host) or the query fails.
fn sample_gpu() -> Option<(f32, f32)> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?;
    let mut parts = line.split(',').map(str::trim);
    let util: f32 = parts.next()?.parse().ok()?;
    let mem: f32 = parts.next()?.parse().ok()?;
    Some((util, mem))
}
