//! EWMA-based ETA for workload progress. Spec §7.

pub const WARMUP_ITEMS: u64 = 10;
pub const ALPHA: f64 = 0.2;

#[derive(Clone, Debug, Default)]
pub struct EtaTracker {
    ewma_per_item_ms: Option<f64>,
    last_item_at_ms: Option<i64>,
    observed_items: u64,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EtaState {
    Calibrating,
    Ready { remaining_ms: i64 },
}

impl EtaTracker {
    pub fn observe(&mut self, now_ms: i64) {
        if let Some(prev) = self.last_item_at_ms {
            let dt = (now_ms - prev).max(1) as f64;
            self.ewma_per_item_ms = Some(match self.ewma_per_item_ms {
                None => dt,
                Some(prev_ewma) => ALPHA * dt + (1.0 - ALPHA) * prev_ewma,
            });
        }
        self.last_item_at_ms = Some(now_ms);
        self.observed_items += 1;
    }

    pub fn estimate(&self, remaining: u64) -> EtaState {
        if self.observed_items < WARMUP_ITEMS || self.ewma_per_item_ms.is_none() {
            return EtaState::Calibrating;
        }
        let per = self.ewma_per_item_ms.unwrap();
        EtaState::Ready { remaining_ms: (per * remaining as f64) as i64 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibrating_before_warmup() {
        let mut t = EtaTracker::default();
        for i in 0..5 { t.observe(i * 100); }
        assert!(matches!(t.estimate(100), EtaState::Calibrating));
    }

    #[test]
    fn ready_after_warmup() {
        let mut t = EtaTracker::default();
        for i in 0..15 { t.observe(i * 100); } // 100ms/item
        match t.estimate(10) {
            EtaState::Ready { remaining_ms } => {
                assert!((900..=1100).contains(&remaining_ms),
                    "expected ~1000ms, got {remaining_ms}");
            }
            _ => panic!("expected Ready"),
        }
    }

    #[test]
    fn stable_under_outlier() {
        let mut t = EtaTracker::default();
        let mut now = 0i64;
        for _ in 0..15 { now += 100; t.observe(now); } // warmup at 100ms/item
        now += 1000; t.observe(now); // single 10x outlier
        match t.estimate(10) {
            EtaState::Ready { remaining_ms } => {
                // With α=0.2: new ewma = 0.2*1000 + 0.8*100 = 280ms
                // Bound: ETA within 20% of 280*10 = 2800ms
                assert!((2240..=3360).contains(&remaining_ms),
                    "expected ~2800ms ±20%, got {remaining_ms}");
            }
            _ => panic!("expected Ready"),
        }
    }
}
