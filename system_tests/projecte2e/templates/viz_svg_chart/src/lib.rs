/// A data point for the bar chart.
#[derive(Debug, Clone)]
pub struct DataPoint {
    pub label: String,
    pub value: f64,
}

/// Configuration for SVG bar chart rendering.
#[derive(Debug, Clone)]
pub struct BarChart {
    pub title: String,
    pub data: Vec<DataPoint>,
    pub width: u32,
    pub height: u32,
    pub bar_color: String,
    pub background_color: String,
    pub padding: u32,
}

impl BarChart {
    /// Create a new bar chart with default styling.
    pub fn new(title: &str, data: Vec<DataPoint>) -> Self {
        Self {
            title: title.to_string(),
            data,
            width: 600,
            height: 400,
            bar_color: "#4A90D9".to_string(),
            background_color: "#FFFFFF".to_string(),
            padding: 40,
        }
    }

    /// Render the bar chart as an SVG string.
    ///
    /// The SVG should contain:
    /// - A root <svg> element with the correct width, height, and viewBox
    /// - A <rect> background element
    /// - A <text> title centered at the top
    /// - One <rect> per data point (the bars), evenly spaced
    /// - One <text> label below each bar
    /// - Y-axis value labels on the left side
    ///
    /// Bars should scale proportionally: the tallest bar fills the available
    /// chart height, and shorter bars scale relative to the maximum value.
    /// If all values are zero, all bars should have zero height.
    pub fn render(&self) -> String {
        todo!("Implement SVG bar chart rendering")
    }
}

/// Convenience function: create a chart from label-value pairs.
pub fn quick_chart(title: &str, items: &[(&str, f64)]) -> String {
    let data: Vec<DataPoint> = items
        .iter()
        .map(|(label, value)| DataPoint {
            label: label.to_string(),
            value: *value,
        })
        .collect();
    BarChart::new(title, data).render()
}
