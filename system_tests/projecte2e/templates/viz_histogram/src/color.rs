/// ANSI escape code helpers for terminal colors.
/// Generate an ANSI foreground color escape code from RGB values.
///
/// Should produce: `\x1b[38;2;R;G;Bm`
pub fn fg_rgb(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

/// Generate an ANSI background color escape code from RGB values.
///
/// Should produce: `\x1b[48;2;R;G;Bm`
pub fn bg_rgb(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[48;2;{};{};{}m", r, g, b)
}
/// Reset all ANSI formatting.
///
/// Should produce: `\x1b[0m`
pub fn reset() -> String {
    "\x1b[0m".to_string()
}

/// Named colors for convenience.
pub fn named_fg(name: &str) -> String {
    match name {
        "red" => fg_rgb(255, 0, 0),
        "green" => fg_rgb(0, 255, 0),
        "blue" => fg_rgb(0, 0, 255),
        "yellow" => fg_rgb(255, 255, 0),
        "cyan" => fg_rgb(0, 255, 255),
        "magenta" => fg_rgb(255, 0, 255),
        "white" => fg_rgb(255, 255, 255),
        _ => fg_rgb(255, 255, 255),
    }
}
