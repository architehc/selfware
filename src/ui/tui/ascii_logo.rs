//! Enhanced ASCII Art Logo for Selfware
//!
//! Inspired by Hermes Agent's stylized display

/// The main Selfware ASCII logo - large version for splash screen
pub const LOGO_LARGE: &str = r#"
    ╭──────────────────────────────────────────────────────────────╮
    │                                                              │
    │           ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣀⡀⠀⣀⣀⠀⢀⣀⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀                         │
    │           ⠀⠀⠀⠀⠀⠀⢀⣠⣴⣾⣿⣿⣇⠸⣿⣿⠇⣸⣿⣿⣷⣦⣄⡀⠀⠀⠀⠀⠀⠀      ╭────────────────────────╮     │
    │      ⠀⢀⣠⣴⣶⠿⠋⣩⡿⣿⡿⠻⣿⡇⢠⡄⢸⣿⠟⢿⣿⢿⣍⠙⠿⣶⣦⣄⡀⠀      │                        │     │
    │      ⠀⠀⠉⠉⠁⠶⠟⠋⠀⠉⠀⢀⣈⣁⡈⢁⣈⣁⡀⠀⠉⠀⠙⠻⠶⠈⠉⠉⠀⠀      │   S E L F W A R E      │     │
    │      ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣴⣿⡿⠛⢁⡈⠛⢿⣿⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀      │                        │     │
    │      ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠿⣿⣦⣤⣈⠁⢠⣴⣿⠿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀      │  Your Personal AI      │     │
    │      ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠉⠻⢿⣿⣦⡉⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀      │     Workshop           │     │
    │      ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠘⢷⣦⣈⠛⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀      │                        │     │
    │      ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⣴⠦⠈⠙⠿⣦⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀      ╰────────────────────────╯     │
    │      ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠸⣿⣤⡈⠁⢤⣿⠇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀                         │
    │      ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠛⠷⠄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀                         │
    │      ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣀⠑⢶⣄⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀                         │
    │      ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⠁⢰⡆⠈⡿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀                         │
    │      ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠳⠈⣡⠞⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀                         │
    │                                                              │
    │              Software you own. Software that knows you.        │
    │                   Software that lasts.                         │
    │                                                              │
    ╰──────────────────────────────────────────────────────────────╯
"#;

/// Compact logo for status bar
pub const LOGO_COMPACT: &str = r#"⛭ 🦊 Selfware"#;

/// The fox mascot ASCII art
pub const FOX_MASCOT: &str = r#"
       /\___/\
      ( o   o )    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
      (  =^=  )    selfware — Your Personal AI Workshop
       )     (     Software you own. Software that knows you.
      (       )    Software that lasts.
     ( |     | )   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
      \|     |/
"#;

/// Animated fox frames for loading states
pub const FOX_FRAMES: &[&str] = &[
    r#"
       /\___/\
      ( o   o )
      (  =^=  )
       )     (
      (       )
    "#,
    r#"
       /\___/\
      ( -   o )
      (  =^=  )
       )     (
      (       )
    "#,
    r#"
       /\___/\
      ( o   - )
      (  =^=  )
       )     (
      (       )
    "#,
    r#"
       /\___/\
      ( o   o )
      (  =^=  )  ~
       )     (
      (       )
    "#,
];

/// Gear icon ASCII
pub const GEAR_ICON: &str = r#"
    ⠀⠀⠀⠀⠀⠀⠀⠀⣀⣤⣶⣶⣿⣿⣿⣷⣦⣄⠀⠀⠀⠀⠀⠀⠀⠀⠀
    ⠀⠀⠀⠀⠀⠀⢀⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⡀⠀⠀⠀⠀⠀⠀⠀
    ⠀⠀⠀⠀⠀⢀⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⡀⠀⠀⠀⠀⠀⠀
    ⠀⠀⠀⠀⢀⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⡀⠀⠀⠀⠀⠀
    ⠀⠀⠀⢀⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⡀⠀⠀⠀⠀
    ⠀⠀⢀⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⡀⠀⠀⠀
    ⠀⢀⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⡀⠀⠀
    ⢀⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⡀⠀
    ⠈⠻⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠟⠁
    ⠀⠀⠈⠙⠻⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠟⠋⠉⠀⠀⠀⠀
    ⠀⠀⠀⠀⠀⠀⠉⠛⠿⢿⣿⣿⣿⣿⣿⡿⠿⠛⠉⠀⠀⠀⠀⠀⠀⠀⠀
"#;

/// Garden/sprout icon
pub const SPROUT_ICON: &str = r#"
        🌱
       /||\
      / || \
       ||
    ~~~||~~~
"#;

/// Render the startup banner with version info
pub fn render_startup_banner(version: &str, model: &str) -> String {
    format!(
        r#"
╭────────────────────────────────────────────────────────────────────────╮
│                                                                        │
│                    ⛭ 🦊 S E L F W A R E ⛭                             │
│                                                                        │
│                    Version: {:<20}                           │
│                    Model:   {:<20}                           │
│                                                                        │
│          "Software you own. Software that knows you."                 │
│                                                                        │
╰────────────────────────────────────────────────────────────────────────╯
"#,
        version, model
    )
}

/// Render a decorative separator line
pub fn render_separator(width: usize) -> String {
    format!("╭{}╯", "─".repeat(width.saturating_sub(2)))
}

/// Render a status badge with icon
pub fn render_status_badge(status: &str, icon: &str) -> String {
    format!("{} {}", icon, status)
}

/// Tool category icons
pub const TOOL_ICONS: &[(&str, &str)] = &[
    ("file", "📄"),
    ("search", "🔍"),
    ("cargo", "📦"),
    ("browser", "🌐"),
    ("shell", "⚡"),
    ("git", "🔀"),
    ("computer", "🖥️ "),
    ("http", "🌐"),
];

/// Get icon for a tool category
pub fn get_tool_icon(category: &str) -> &str {
    TOOL_ICONS
        .iter()
        .find(|(cat, _)| category.contains(cat))
        .map(|(_, icon)| *icon)
        .unwrap_or("🔧")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logo_not_empty() {
        assert!(!LOGO_LARGE.is_empty());
        assert!(!FOX_MASCOT.is_empty());
    }

    #[test]
    fn test_startup_banner_formatting() {
        let banner = render_startup_banner("0.7.0", "qwen3.5-122b");
        assert!(banner.contains("SELFWARE"));
        assert!(banner.contains("0.7.0"));
        assert!(banner.contains("qwen3.5-122b"));
    }

    #[test]
    fn test_tool_icons() {
        assert_eq!(get_tool_icon("file_read"), "📄");
        assert_eq!(get_tool_icon("cargo_build"), "📦");
        assert_eq!(get_tool_icon("unknown_tool"), "🔧");
    }
}
