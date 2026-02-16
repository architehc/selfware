/// Parse duration strings like "30s", "5m", or "2h" into seconds.
///
/// BUGS (intentional for e2e scenario):
/// - does not support days ("d")
/// - does not trim whitespace
/// - accepts zero durations
pub fn parse_duration(input: &str) -> Option<u64> {
    if input.is_empty() {
        return None;
    }

    let (value_part, unit) = input.split_at(input.len().saturating_sub(1));
    let value = value_part.parse::<u64>().ok()?;

    match unit {
        "s" => Some(value),
        "m" => Some(value * 60),
        "h" => Some(value * 60 * 60),
        _ => None,
    }
}
