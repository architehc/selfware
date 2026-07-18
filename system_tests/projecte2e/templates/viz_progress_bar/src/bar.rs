use crate::style::BarStyle;
use crate::eta::EtaCalculator;

/// A configurable progress bar.
#[derive(Debug)]
pub struct ProgressBar {
    /// Current position (0..=total).
    position: u64,
    /// Maximum value.
    total: u64,
    /// Display width in characters.
    width: usize,
    /// Visual style.
    style: BarStyle,
    /// Optional message displayed after the bar.
    message: String,
    /// ETA calculator.
    eta: EtaCalculator,
    /// Whether the bar is finished.
    finished: bool,
}

impl ProgressBar {
    pub fn new(total: u64) -> Self {
        Self {
            position: 0,
            total,
            width: 30,
            style: BarStyle::Classic,
            message: String::new(),
            eta: EtaCalculator::new(),
            finished: false,
        }
    }

    pub fn with_style(mut self, style: BarStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_width(mut self, width: usize) -> Self {
        self.width = width.max(5);
        self
    }

    pub fn set_message(&mut self, msg: &str) {
        self.message = msg.to_string();
    }

    /// Advance position by `delta` and update ETA.
    pub fn tick(&mut self, delta: u64) {
        // BUG 1: position can exceed total — not clamped.
        self.position += delta;

        // BUG 2: ETA is not updated on tick.
        // Should call self.eta.update(self.position, self.total) here.
    }

    /// Set position to a specific value.
    pub fn set_position(&mut self, pos: u64) {
        self.position = pos.min(self.total);
        self.eta.update(self.position, self.total);
    }

    /// Mark the bar as finished (100%).
    pub fn finish(&mut self) {
        // BUG 3: Doesn't set position to total, so render shows incomplete.
        self.finished = true;
    }

    /// Get the current completion ratio (0.0-1.0).
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        // BUG 4: Missing clamp — if position > total, ratio exceeds 1.0.
        self.position as f64 / self.total as f64
    }

    /// Get the current position.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Get the total.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Check if finished.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Render the progress bar as a string.
    ///
    /// Format: `[████████░░░░░░] 53% (ETA: 12s) message`
    pub fn render(&self) -> String {
        let ratio = self.ratio();
        let bar = self.style.render(ratio, self.width);
        let pct = BarStyle::format_percentage(ratio);
        let eta = self.eta.estimate_remaining();
        let eta_str = if eta > 0.0 {
            format!(" (ETA: {:.0}s)", eta)
        } else {
            String::new()
        };
        let msg = if self.message.is_empty() {
            String::new()
        } else {
            format!(" {}", self.message)
        };

        // BUG 5: Display width ignores the actual terminal width.
        // Not a functional bug per se, but the bar + text can exceed
        // the configured width. The bar itself is fine, but the combined
        // output isn't width-aware. We document it but don't fix it here.

        format!("{} {}{}{}", bar, pct, eta_str, msg)
    }
}
