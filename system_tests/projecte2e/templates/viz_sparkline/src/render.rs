/// The 8 Unicode block characters from lowest to highest.
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Map normalized values [0.0–1.0] to Unicode block characters.
pub fn render_blocks(normalized: &[f64]) -> String {
    normalized
        .iter()
        .map(|&v| {
            // Clamp to [0, 1]
            let clamped = v.max(0.0).min(1.0);
            // Map 0.0 → index 0 (▁), 1.0 → index 7 (█)
            let idx = (clamped * 7.0).round() as usize;
            BLOCKS[idx.min(7)]
        })
        .collect()
}

/// Render a horizontal sparkline bar (repeated block chars) of given width.
pub fn render_bar(value: f64, max_value: f64, width: usize) -> String {
    if max_value <= 0.0 || width == 0 {
        return String::new();
    }
    
    // Clamp value to [0, max_value] to handle negative values
    let clamped_value = value.max(0.0).min(max_value);
    let ratio = clamped_value / max_value;
    let filled = (ratio * width as f64).round() as usize;

    // Build bar with only filled blocks (no trailing spaces)
    BLOCKS[7].to_string().repeat(filled)
}
