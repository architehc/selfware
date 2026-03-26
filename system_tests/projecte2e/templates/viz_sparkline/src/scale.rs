/// Normalize data to [0.0, 1.0] range.
///
/// Maps the minimum value to 0.0 and the maximum to 1.0.
/// If all values are the same, returns 0.5 for all.
/// For a single element, returns 0.5.
pub fn normalize(data: &[f64]) -> Vec<f64> {
    if data.is_empty() {
        return vec![];
    }

    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    // Handle single element or all-same-values case (range == 0)
    if range == 0.0 {
        return vec![0.5; data.len()];
    }

    data.iter().map(|&v| (v - min) / range).collect()
}
