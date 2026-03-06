/// Normalize data to [0.0, 1.0] range.
///
/// Maps the minimum value to 0.0 and the maximum to 1.0.
/// If all values are the same, returns 0.5 for all.
/// For a single element, returns 0.5.
pub fn normalize(data: &[f64]) -> Vec<f64> {
    if data.is_empty() {
        return vec![];
    }

    // BUG 1: Panics on single-element input due to division by zero.
    // When data has only one element, min == max, so (max - min) == 0.
    // Should return vec![0.5] for single element.
    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    // BUG 2: Doesn't handle the all-same-values case.
    // When all values are identical, range == 0, causing division by zero
    // producing NaN values. Should return 0.5 for all.
    data.iter().map(|&v| (v - min) / range).collect()
}
