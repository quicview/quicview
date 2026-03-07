use std::collections::VecDeque;

/// Adaptive bitrate controller for video streaming.
///
/// Tracks frame sizes over a sliding window and recommends quality
/// adjustments to stay within a target bitrate budget.
pub struct BitrateController {
    /// Target bitrate in bits per second.
    target_bps: u64,
    /// Current quality level (0–100, 100 = best quality).
    quality: u8,
    /// Sliding window of (timestamp_ms, frame_size_bytes).
    window: VecDeque<(u64, usize)>,
    /// Window duration in milliseconds.
    window_ms: u64,
    /// Number of consecutive over-budget measurements.
    over_budget_count: u32,
    /// Number of consecutive under-budget measurements.
    under_budget_count: u32,
}

impl BitrateController {
    /// Create a new controller with the given target bitrate (bits/s).
    pub fn new(target_bps: u64) -> Self {
        Self {
            target_bps,
            quality: 80,
            window: VecDeque::new(),
            window_ms: 2000,
            over_budget_count: 0,
            under_budget_count: 0,
        }
    }

    /// Record a frame that was sent.
    pub fn record_frame(&mut self, timestamp_ms: u64, size_bytes: usize) {
        self.window.push_back((timestamp_ms, size_bytes));

        // Evict entries outside the sliding window.
        let cutoff = timestamp_ms.saturating_sub(self.window_ms);
        while self.window.front().is_some_and(|(ts, _)| *ts < cutoff) {
            self.window.pop_front();
        }

        self.adjust_quality();
    }

    /// Measured bitrate over the sliding window (bits/s).
    pub fn measured_bps(&self) -> u64 {
        if self.window.len() < 2 {
            return 0;
        }
        let first_ts = self.window.front().unwrap().0;
        let last_ts = self.window.back().unwrap().0;
        let duration_ms = last_ts.saturating_sub(first_ts).max(1);
        let total_bytes: usize = self.window.iter().map(|(_, s)| s).sum();
        (total_bytes as u64 * 8 * 1000) / duration_ms
    }

    /// Current recommended quality (0–100).
    pub fn quality(&self) -> u8 {
        self.quality
    }

    /// Target bitrate (bits/s).
    pub fn target_bps(&self) -> u64 {
        self.target_bps
    }

    /// Update the target bitrate.
    pub fn set_target_bps(&mut self, target: u64) {
        self.target_bps = target;
    }

    /// Whether the controller recommends skipping a frame to stay in budget.
    pub fn should_skip_frame(&self) -> bool {
        self.measured_bps() > self.target_bps * 3 / 2
    }

    fn adjust_quality(&mut self) {
        let measured = self.measured_bps();
        if measured > self.target_bps {
            self.over_budget_count += 1;
            self.under_budget_count = 0;
            if self.over_budget_count >= 3 {
                self.quality = self.quality.saturating_sub(5).max(10);
                self.over_budget_count = 0;
            }
        } else if measured < self.target_bps * 3 / 4 {
            self.under_budget_count += 1;
            self.over_budget_count = 0;
            if self.under_budget_count >= 5 {
                self.quality = (self.quality + 5).min(100);
                self.under_budget_count = 0;
            }
        } else {
            self.over_budget_count = 0;
            self.under_budget_count = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_quality() {
        let ctrl = BitrateController::new(10_000_000);
        assert_eq!(ctrl.quality(), 80);
        assert_eq!(ctrl.measured_bps(), 0);
    }

    #[test]
    fn measured_bitrate_calculation() {
        let mut ctrl = BitrateController::new(10_000_000);
        // Simulate 30 frames of 50KB each over 1 second.
        for i in 0..30 {
            ctrl.record_frame(i * 33, 50_000);
        }
        let bps = ctrl.measured_bps();
        // 30 frames × 50KB × 8 bits ≈ 12 Mbps.
        assert!(bps > 10_000_000);
        assert!(bps < 15_000_000);
    }

    #[test]
    fn quality_decreases_when_over_budget() {
        let mut ctrl = BitrateController::new(1_000_000); // 1 Mbps target
        let initial = ctrl.quality();
        // Send large frames to exceed budget.
        for i in 0..20 {
            ctrl.record_frame(i * 33, 100_000); // ~24 Mbps
        }
        assert!(ctrl.quality() < initial);
    }

    #[test]
    fn quality_increases_when_under_budget() {
        let mut ctrl = BitrateController::new(100_000_000); // 100 Mbps target
        ctrl.quality = 50; // start low
        // Send small frames — well under budget.
        for i in 0..30 {
            ctrl.record_frame(i * 33, 1_000); // ~0.24 Mbps
        }
        assert!(ctrl.quality() > 50);
    }

    #[test]
    fn should_skip_frame_when_far_over_budget() {
        let mut ctrl = BitrateController::new(1_000_000);
        for i in 0..20 {
            ctrl.record_frame(i * 33, 200_000);
        }
        assert!(ctrl.should_skip_frame());
    }
}
