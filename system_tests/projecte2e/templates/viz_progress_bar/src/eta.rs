use std::time::Instant;

/// Estimates time remaining using exponential moving average.
#[derive(Debug)]
pub struct EtaCalculator {
    start_time: Instant,
    last_update: Instant,
    smoothed_rate: f64,
    /// Smoothing factor (0.0-1.0). Higher = more weight on recent measurements.
    alpha: f64,
    last_position: u64,
    total: u64,
}

impl EtaCalculator {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            last_update: Instant::now(),
            smoothed_rate: 0.0,
            // BUG 1: Alpha is inverted — should weight RECENT measurements more
            // heavily (alpha close to 1.0), but uses 0.1 which weights old
            // measurements more. This makes ETA very slow to respond to speed changes.
            alpha: 0.1,
            last_position: 0,
            total: 0,
        }
    }

    /// Update with current progress.
    pub fn update(&mut self, position: u64, total: u64) {
        self.total = total;

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();

        if elapsed > 0.01 {
            let delta = position.saturating_sub(self.last_position) as f64;
            let rate = delta / elapsed;

            // Exponential moving average
            if self.smoothed_rate == 0.0 {
                self.smoothed_rate = rate;
            } else {
                self.smoothed_rate = self.alpha * rate + (1.0 - self.alpha) * self.smoothed_rate;
            }

            self.last_position = position;
            self.last_update = now;
        }
    }

    /// Estimate remaining seconds.
    pub fn estimate_remaining(&self) -> f64 {
        if self.smoothed_rate <= 0.0 || self.last_position >= self.total {
            return 0.0;
        }

        // BUG 2: Uses elapsed time since start instead of remaining items / rate.
        // Should be: (total - last_position) / smoothed_rate
        // But this calculates: elapsed_total * (remaining_ratio / done_ratio)
        // which gives a different (wrong) result when rate varies.
        let remaining = self.total - self.last_position;
        remaining as f64 / self.smoothed_rate
    }

    /// Get elapsed seconds since creation.
    pub fn elapsed_secs(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }
}

impl Default for EtaCalculator {
    fn default() -> Self {
        Self::new()
    }
}
