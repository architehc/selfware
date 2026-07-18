use crate::bar::ProgressBar;

/// Manages multiple progress bars rendered together.
#[derive(Debug)]
pub struct MultiProgress {
    bars: Vec<ProgressBar>,
}

impl MultiProgress {
    pub fn new() -> Self {
        Self { bars: Vec::new() }
    }

    /// Add a progress bar and return its index.
    pub fn add(&mut self, bar: ProgressBar) -> usize {
        let idx = self.bars.len();
        self.bars.push(bar);
        idx
    }

    /// Get a mutable reference to a bar by index.
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut ProgressBar> {
        self.bars.get_mut(idx)
    }

    /// Get the number of bars.
    pub fn len(&self) -> usize {
        self.bars.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.bars.is_empty()
    }

    /// Render all bars as a multi-line string.
    pub fn render(&self) -> String {
        if self.bars.is_empty() {
            return String::new();
        }

        let mut output = String::new();

        // BUG 1: Renders bars in reverse order.
        // Bars added first should appear first in output.
        for bar in self.bars.iter().rev() {
            output.push_str(&bar.render());
            output.push('\n');
        }

        // BUG 2: Doesn't add a blank line between renders
        // (visual artifact when called multiple times to update).
        // This isn't tested directly but causes display issues.

        output
    }

    /// Check if all bars are finished.
    pub fn all_finished(&self) -> bool {
        self.bars.iter().all(|b| b.is_finished())
    }
}

impl Default for MultiProgress {
    fn default() -> Self {
        Self::new()
    }
}
