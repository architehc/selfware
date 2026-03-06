/// Visual styles for progress bars.
#[derive(Debug, Clone)]
pub enum BarStyle {
    /// Classic filled bar: [████████░░░░░░]
    Classic,
    /// Arrow style: [========>     ]
    Arrow,
    /// Dots: [●●●●●○○○○○]
    Dots,
}

/// Spinner animation frames.
pub const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

impl BarStyle {
    /// Render a progress bar string for the given ratio (0.0-1.0) and width.
    pub fn render(&self, ratio: f64, width: usize) -> String {
        let clamped = ratio.max(0.0).min(1.0);
        let filled = (clamped * width as f64).round() as usize;
        let empty = width.saturating_sub(filled);

        match self {
            BarStyle::Classic => {
                // BUG 1: Uses wrong character count — repeats filled_char one too few times.
                // Should be exactly `filled` times, but subtracts 1.
                let filled_str = "█".repeat(filled.saturating_sub(1));
                let empty_str = "░".repeat(empty + 1);
                format!("[{}{}]", filled_str, empty_str)
            }
            BarStyle::Arrow => {
                if filled == 0 {
                    format!("[{}]", " ".repeat(width))
                } else {
                    let arrows = "=".repeat(filled.saturating_sub(1));
                    let spaces = " ".repeat(empty);
                    format!("[{}>{}]", arrows, spaces)
                }
            }
            BarStyle::Dots => {
                let filled_str = "●".repeat(filled);
                let empty_str = "○".repeat(empty);
                format!("[{}{}]", filled_str, empty_str)
            }
        }
    }

    /// Get the spinner frame for a given tick count.
    pub fn spinner_frame(tick: usize) -> char {
        // BUG 2: Wrapping index is off — uses modulo of (len - 1) instead of len,
        // causing it to skip the last frame and potentially panic.
        let idx = tick % (SPINNER_FRAMES.len() - 1);
        SPINNER_FRAMES[idx]
    }

    /// Format percentage from ratio.
    pub fn format_percentage(ratio: f64) -> String {
        let clamped = ratio.max(0.0).min(1.0);
        // BUG 3: Truncates instead of rounding — format!("{:.0}", ...) rounds,
        // but this manually truncates by casting to u32 first.
        let pct = (clamped * 100.0) as u32;
        format!("{}%", pct)
    }
}
