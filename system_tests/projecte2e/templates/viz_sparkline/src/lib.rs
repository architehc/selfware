pub mod scale;
pub mod render;

/// Generate a Unicode sparkline string from numeric data.
///
/// Uses the Unicode block characters: ▁▂▃▄▅▆▇█
/// Values are normalized to the range [min, max] and mapped
/// to one of 8 block characters.
///
/// Returns an empty string for empty input.
///
/// # Examples
/// ```
/// let spark = viz_sparkline::sparkline(&[1.0, 5.0, 3.0, 8.0, 2.0]);
/// assert_eq!(spark.len(), 5 * '▁'.len_utf8());
/// ```
pub fn sparkline(data: &[f64]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let normalized = scale::normalize(data);
    render::render_blocks(&normalized)
}

/// Generate a sparkline with min/max markers.
///
/// Appends ` (min: {min:.1}, max: {max:.1})` to the sparkline.
pub fn sparkline_with_stats(data: &[f64]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let spark = sparkline(data);
    format!("{} (min: {:.1}, max: {:.1})", spark, min, max)
}

/// Detect trend direction from data.
///
/// Compares the average of the first third to the last third.
/// Returns "↑" for upward, "↓" for downward, "→" for stable.
pub fn trend(data: &[f64]) -> &'static str {
    if data.len() < 3 {
        return "→";
    }
    let third = data.len() / 3;
    let first_avg: f64 = data[..third].iter().sum::<f64>() / third as f64;
    let last_avg: f64 = data[data.len() - third..].iter().sum::<f64>() / third as f64;
    let diff = last_avg - first_avg;
    let threshold = (data.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
        - data.iter().cloned().fold(f64::INFINITY, f64::min))
        * 0.1;

    if diff > threshold {
        "↑"
    } else if diff < -threshold {
        "↓"
    } else {
        "→"
    }
}
