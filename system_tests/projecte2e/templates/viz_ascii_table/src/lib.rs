/// Text alignment for table columns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Alignment {
    Left,
    Right,
    Center,
}

/// A formatted ASCII table with box-drawing characters.
#[derive(Debug, Clone)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    alignments: Vec<Alignment>,
}

impl Table {
    /// Create a new table with the given column headers.
    pub fn new(headers: &[&str]) -> Self {
        let n = headers.len();
        Self {
            headers: headers.iter().map(|s| s.to_string()).collect(),
            rows: Vec::new(),
            alignments: vec![Alignment::Left; n],
        }
    }

    /// Set column alignments. Length must match header count.
    pub fn set_alignments(&mut self, alignments: Vec<Alignment>) {
        if alignments.len() == self.headers.len() {
            self.alignments = alignments;
        }
    }

    /// Add a row of data. Panics if column count doesn't match headers.
    pub fn add_row(&mut self, row: &[&str]) {
        assert_eq!(
            row.len(),
            self.headers.len(),
            "Row has {} columns but table has {}",
            row.len(),
            self.headers.len()
        );
        self.rows.push(row.iter().map(|s| s.to_string()).collect());
    }

    /// Render the table as a formatted string with box-drawing characters.
    ///
    /// Example output for a 2-column table:
    /// ```text
    /// ┌───────┬───────┐
    /// │ Name  │ Score │
    /// ├───────┼───────┤
    /// │ Alice │   95  │
    /// │ Bob   │   87  │
    /// └───────┴───────┘
    /// ```
    pub fn render(&self) -> String {
        if self.headers.is_empty() {
            return String::new();
        }

        // BUG 1: Column width calculation is off-by-one.
        // Should be max of header width and all row cell widths,
        // but we subtract 1 making columns too narrow.
        let col_widths: Vec<usize> = (0..self.headers.len())
            .map(|col| {
                let header_w = self.headers[col].len();
                let max_row_w = self
                    .rows
                    .iter()
                    .map(|row| row[col].len())
                    .max()
                    .unwrap_or(0);
                // BUG: off-by-one, should be .max() not .max() - 1
                header_w.max(max_row_w).saturating_sub(1)
            })
            .collect();

        let mut output = String::new();

        // Top border
        output.push_str(&self.horizontal_line('┌', '┬', '┐', &col_widths));
        output.push('\n');

        // Header row
        output.push_str(&self.format_row(&self.headers, &col_widths, &self.alignments));
        output.push('\n');

        // Header separator
        output.push_str(&self.horizontal_line('├', '┼', '┤', &col_widths));
        output.push('\n');

        // Data rows
        for row in &self.rows {
            output.push_str(&self.format_row(row, &col_widths, &self.alignments));
            output.push('\n');
        }

        // Bottom border
        output.push_str(&self.horizontal_line('└', '┴', '┘', &col_widths));
        output.push('\n');

        output
    }

    fn horizontal_line(&self, left: char, mid: char, right: char, widths: &[usize]) -> String {
        let mut line = String::new();
        line.push(left);
        for (i, &w) in widths.iter().enumerate() {
            // BUG 2: Uses wrong box-drawing character for horizontal lines.
            // Should use '─' (U+2500 BOX DRAWINGS LIGHT HORIZONTAL)
            // but uses '-' (regular hyphen) instead.
            for _ in 0..w + 2 {
                line.push('-');
            }
            if i < widths.len() - 1 {
                line.push(mid);
            }
        }
        line.push(right);
        line
    }

    fn format_row(
        &self,
        cells: &[String],
        widths: &[usize],
        alignments: &[Alignment],
    ) -> String {
        let mut row = String::new();
        row.push('│');
        for (i, cell) in cells.iter().enumerate() {
            let w = widths[i];
            let aligned = match alignments.get(i).unwrap_or(&Alignment::Left) {
                // BUG 3: Right-aligned numbers are formatted as left-aligned.
                // format!("{:>width$}", ...) should be used for Right alignment,
                // but all alignments use left-align format.
                Alignment::Left => format!(" {:<width$} ", cell, width = w),
                Alignment::Right => format!(" {:<width$} ", cell, width = w),
                Alignment::Center => {
                    let padding = w.saturating_sub(cell.len());
                    let left_pad = padding / 2;
                    let right_pad = padding - left_pad;
                    format!(
                        " {}{}{} ",
                        " ".repeat(left_pad),
                        cell,
                        " ".repeat(right_pad)
                    )
                }
            };
            row.push_str(&aligned);
            row.push('│');
        }
        row
    }
}
