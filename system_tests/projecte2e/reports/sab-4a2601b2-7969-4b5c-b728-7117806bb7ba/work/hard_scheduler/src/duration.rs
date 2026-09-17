/// Parse duration strings like "30s", "5m", "2h", or "1d" into seconds.
pub fn parse_duration(input: &str) -> Option<u64> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let (value_part, unit) = input.split_at(input.len().saturating_sub(1));
    let value = value_part.parse::<u64>().ok()?;

    if value == 0 {
        return None;
    }

    match unit {
        "s" => Some(value),
        "m" => Some(value * 60),
        "h" => Some(value * 60 * 60),
        "d" => Some(value * 86400),
        _ => None,
    }
}
