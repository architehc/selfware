/// The 8 Unicode block characters from lowest to highest.
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Map normalized values [0.0–1.0] to Unicode block characters.
pub fn render_blocks(normalized: &[f64]) -> String {
    // BUG 3: The block mapping is inverted — 0.0 maps to █ (highest)
    // and 1.0 maps to ▁ (lowest). Should be the opposite.
    normalized
        .iter()
        .map(|&v| {
            // Clamp to [0, 1]
            let clamped = v.max(0.0).min(1.0);
            // BUG: inverted index — uses (1.0 - clamped) instead of clamped
            let idx = ((1.0 - clamped) * 7.0).round() as usize;
            BLOCKS[idx.min(7)]
        })
        .collect()
}

/// Render a horizontal sparkline bar (repeated block chars) of given width.
pub fn render_bar(value: f64, max_value: f64, width: usize) -> String {
    if max_value <= 0.0 || width == 0 {
        return String::new();
    }
    let ratio = (value / max_value).min(1.0).max(0.0);
    let filled = (ratio * width as f64).round() as usize;

    // BUG 4: Negative values cause underflow when cast to usize.
    // value can be negative, making ratio negative, and .round() as usize wraps.
    // The .max(0.0) above should fix this, but it's applied to ratio, not value.
    // Actually this is partly guarded, but let's add another issue:

    // BUG 5: Empty input (width=0) is handled above, but when width > 0
    // and value is exactly 0, the function should return all spaces, but
    // instead returns empty string because filled=0 means no chars are pushed.
    let bar: String = BLOCKS[7].to_string().repeat(filled);
    bar
}
