use crate::color;

/// A single data entry in the histogram.
#[derive(Debug, Clone)]
pub struct Entry {
    pub label: String,
    pub value: f64,
    pub color: String,
}

/// Horizontal bar histogram rendered with ANSI colors.
#[derive(Debug)]
pub struct Histogram {
    pub entries: Vec<Entry>,
    pub max_bar_width: usize,
    pub bar_char: char,
    pub sort_ascending: bool,
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            max_bar_width: 40,
            bar_char: '█',
            sort_ascending: true,
        }
    }
}

impl Histogram {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a data entry with a named color.
    pub fn add(&mut self, label: &str, value: f64, color_name: &str) {
        self.entries.push(Entry {
            label: label.to_string(),
            value,
            color: color_name.to_string(),
        });
    }

    /// Render the histogram as a string with ANSI color codes.
    ///
    /// Output format (one line per entry):
    /// ```text
    ///   Label │ ████████████████ 42.0
    /// ```
    ///
    /// The bar width should be proportional to the value relative to the max.
    /// Labels should be right-padded to align the bars.
    pub fn render(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut entries = self.entries.clone();

        if self.sort_ascending {
            entries.sort_by(|a, b| a.value.partial_cmp(&b.value).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            entries.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap_or(std::cmp::Ordering::Equal));
        }

        // Find the maximum value for scaling
        let max_value = entries
            .iter()
            .map(|e| e.value)
            .fold(f64::NEG_INFINITY, f64::max);

        // Handle division by zero when all values are zero
        let scale = if max_value <= 0.0 {
            0.0
        } else {
            self.max_bar_width as f64 / max_value
        };

        // Find the longest label for padding
        let max_label_len = entries.iter().map(|e| e.label.len()).max().unwrap_or(0);
        let label_width = max_label_len;

        let mut output = String::new();

        for entry in &entries {
            let bar_width = (entry.value * scale).round() as usize;

            let bar = self.bar_char.to_string().repeat(bar_width);
            let colored_bar = format!(
                "{}{}{}",
                color::named_fg(&entry.color),
                bar,
                color::reset(),
            );

            output.push_str(&format!(
                "{:>width$} │ {} {:.1}\n",
                entry.label,
                colored_bar,
                entry.value,
                width = label_width,
            ));
        }

        output
    }
}
