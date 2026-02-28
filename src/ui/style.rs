//! Selfware Style System
//!
//! Warm, organic palette for the personal workshop aesthetic.
//! Like aged paper, wood grain, and amber resin.
//! Supports multiple themes via the theme module.

use std::sync::atomic::{AtomicBool, Ordering};

use super::theme::current_theme;
use colored::{Colorize, CustomColor};

/// When true, all glyphs use plain ASCII instead of Unicode/emoji.
static ASCII_MODE: AtomicBool = AtomicBool::new(false);

/// Enable ASCII-only mode (no emoji or extended Unicode).
pub fn set_ascii_mode(enabled: bool) {
    ASCII_MODE.store(enabled, Ordering::Relaxed);
}

/// Check if ASCII mode is active.
pub fn is_ascii_mode() -> bool {
    ASCII_MODE.load(Ordering::Relaxed)
}

/// The Selfware color palette - warm, organic, hand-crafted
pub struct Palette;

impl Palette {
    // Primary colors - warm and inviting
    pub const AMBER: CustomColor = CustomColor {
        r: 212,
        g: 163,
        b: 115,
    }; // #D4A373 - Primary action, warmth
    pub const GARDEN_GREEN: CustomColor = CustomColor {
        r: 96,
        g: 108,
        b: 56,
    }; // #606C38 - Growth, success
    pub const SOIL_BROWN: CustomColor = CustomColor {
        r: 188,
        g: 108,
        b: 37,
    }; // #BC6C25 - Earth, warnings
    pub const INK: CustomColor = CustomColor {
        r: 40,
        g: 54,
        b: 24,
    }; // #283618 - Deep text
    pub const PARCHMENT: CustomColor = CustomColor {
        r: 254,
        g: 250,
        b: 224,
    }; // #FEFAE0 - Light background

    // Accent colors
    pub const RUST: CustomColor = CustomColor {
        r: 139,
        g: 69,
        b: 19,
    }; // Aged metal
    pub const COPPER: CustomColor = CustomColor {
        r: 184,
        g: 115,
        b: 51,
    }; // Warm accent
    pub const SAGE: CustomColor = CustomColor {
        r: 143,
        g: 151,
        b: 121,
    }; // Muted green
    pub const STONE: CustomColor = CustomColor {
        r: 128,
        g: 128,
        b: 128,
    }; // Neutral

    // Status colors (organic alternatives to red/green/yellow)
    pub const BLOOM: CustomColor = CustomColor {
        r: 144,
        g: 190,
        b: 109,
    }; // Success - fresh growth
    pub const WILT: CustomColor = CustomColor {
        r: 188,
        g: 108,
        b: 37,
    }; // Warning - needs attention
    pub const FROST: CustomColor = CustomColor {
        r: 100,
        g: 100,
        b: 120,
    }; // Error - cold, needs warmth
}

/// Semantic styling for different UI elements
pub trait SelfwareStyle {
    fn workshop_title(self) -> colored::ColoredString;
    fn garden_healthy(self) -> colored::ColoredString;
    fn garden_wilting(self) -> colored::ColoredString;
    fn tool_name(self) -> colored::ColoredString;
    fn path_local(self) -> colored::ColoredString;
    fn timestamp(self) -> colored::ColoredString;
    fn muted(self) -> colored::ColoredString;
    fn emphasis(self) -> colored::ColoredString;
    fn craftsman_voice(self) -> colored::ColoredString;
}

impl SelfwareStyle for &str {
    fn workshop_title(self) -> colored::ColoredString {
        self.custom_color(current_theme().primary).bold()
    }

    fn garden_healthy(self) -> colored::ColoredString {
        self.custom_color(current_theme().success)
    }

    fn garden_wilting(self) -> colored::ColoredString {
        self.custom_color(current_theme().warning)
    }

    fn tool_name(self) -> colored::ColoredString {
        self.custom_color(current_theme().tool).bold()
    }

    fn path_local(self) -> colored::ColoredString {
        self.custom_color(current_theme().path).italic()
    }

    fn timestamp(self) -> colored::ColoredString {
        self.custom_color(current_theme().muted).dimmed()
    }

    fn muted(self) -> colored::ColoredString {
        self.custom_color(current_theme().muted)
    }

    fn emphasis(self) -> colored::ColoredString {
        self.custom_color(current_theme().primary)
    }

    fn craftsman_voice(self) -> colored::ColoredString {
        self.custom_color(current_theme().muted).italic()
    }
}

impl SelfwareStyle for String {
    fn workshop_title(self) -> colored::ColoredString {
        self.as_str().workshop_title()
    }

    fn garden_healthy(self) -> colored::ColoredString {
        self.as_str().garden_healthy()
    }

    fn garden_wilting(self) -> colored::ColoredString {
        self.as_str().garden_wilting()
    }

    fn tool_name(self) -> colored::ColoredString {
        self.as_str().tool_name()
    }

    fn path_local(self) -> colored::ColoredString {
        self.as_str().path_local()
    }

    fn timestamp(self) -> colored::ColoredString {
        self.as_str().timestamp()
    }

    fn muted(self) -> colored::ColoredString {
        self.as_str().muted()
    }

    fn emphasis(self) -> colored::ColoredString {
        self.as_str().emphasis()
    }

    fn craftsman_voice(self) -> colored::ColoredString {
        self.as_str().craftsman_voice()
    }
}

/// Unicode glyphs for the workshop aesthetic.
///
/// Each glyph is exposed as a method that returns the Unicode version
/// by default, or a plain-ASCII fallback when [`set_ascii_mode`]
/// has been called.
pub struct Glyphs;

impl Glyphs {
    // Garden metaphors
    pub fn seedling() -> &'static str {
        if is_ascii_mode() {
            "[*]"
        } else {
            "🌱"
        }
    }
    pub fn sprout() -> &'static str {
        if is_ascii_mode() {
            "[^]"
        } else {
            "🌿"
        }
    }
    pub fn tree() -> &'static str {
        if is_ascii_mode() {
            "[T]"
        } else {
            "🌳"
        }
    }
    pub fn leaf() -> &'static str {
        if is_ascii_mode() {
            "[-]"
        } else {
            "🍃"
        }
    }
    pub fn fallen_leaf() -> &'static str {
        if is_ascii_mode() {
            "[.]"
        } else {
            "🍂"
        }
    }
    pub fn flower() -> &'static str {
        if is_ascii_mode() {
            "[o]"
        } else {
            "🌸"
        }
    }
    pub fn harvest() -> &'static str {
        if is_ascii_mode() {
            "[H]"
        } else {
            "🌾"
        }
    }

    // Workshop tools
    pub fn hammer() -> &'static str {
        if is_ascii_mode() {
            "[#]"
        } else {
            "🔨"
        }
    }
    pub fn wrench() -> &'static str {
        if is_ascii_mode() {
            "[%]"
        } else {
            "🔧"
        }
    }
    pub fn magnifier() -> &'static str {
        if is_ascii_mode() {
            "[?]"
        } else {
            "🔍"
        }
    }
    pub fn scissors() -> &'static str {
        if is_ascii_mode() {
            "[X]"
        } else {
            "✂️"
        }
    }
    pub fn gear() -> &'static str {
        if is_ascii_mode() {
            "[G]"
        } else {
            "⚙️"
        }
    }
    pub fn compass() -> &'static str {
        if is_ascii_mode() {
            "[>]"
        } else {
            "🧭"
        }
    }

    // Personal items
    pub fn journal() -> &'static str {
        if is_ascii_mode() {
            "[J]"
        } else {
            "📓"
        }
    }
    pub fn bookmark() -> &'static str {
        if is_ascii_mode() {
            "[!]"
        } else {
            "🔖"
        }
    }
    pub fn lantern() -> &'static str {
        if is_ascii_mode() {
            "[i]"
        } else {
            "🏮"
        }
    }
    pub fn key() -> &'static str {
        if is_ascii_mode() {
            "[K]"
        } else {
            "🔑"
        }
    }
    pub fn home() -> &'static str {
        if is_ascii_mode() {
            "[~]"
        } else {
            "🏠"
        }
    }
    pub fn chest() -> &'static str {
        if is_ascii_mode() {
            "[C]"
        } else {
            "📦"
        }
    }

    // Status indicators (organic)
    pub fn bloom() -> &'static str {
        if is_ascii_mode() {
            "[B]"
        } else {
            "✿"
        }
    }
    pub fn wilt() -> &'static str {
        if is_ascii_mode() {
            "[W]"
        } else {
            "❀"
        }
    }
    pub fn frost() -> &'static str {
        if is_ascii_mode() {
            "[F]"
        } else {
            "❄"
        }
    }

    // Borders (hand-drawn feel) — widely supported unicode,
    // but still provide ASCII fallback for minimal terminals
    pub fn corner_tl() -> &'static str {
        if is_ascii_mode() {
            "+"
        } else {
            "╭"
        }
    }
    pub fn corner_tr() -> &'static str {
        if is_ascii_mode() {
            "+"
        } else {
            "╮"
        }
    }
    pub fn corner_bl() -> &'static str {
        if is_ascii_mode() {
            "+"
        } else {
            "╰"
        }
    }
    pub fn corner_br() -> &'static str {
        if is_ascii_mode() {
            "+"
        } else {
            "╯"
        }
    }
    pub fn horiz() -> &'static str {
        if is_ascii_mode() {
            "-"
        } else {
            "─"
        }
    }
    pub fn vert() -> &'static str {
        if is_ascii_mode() {
            "|"
        } else {
            "│"
        }
    }
    pub fn branch() -> &'static str {
        if is_ascii_mode() {
            "+"
        } else {
            "├"
        }
    }
    pub fn leaf_branch() -> &'static str {
        if is_ascii_mode() {
            "+"
        } else {
            "└"
        }
    }

    // Progress indicators
    pub fn tending() -> &'static str {
        if is_ascii_mode() {
            "(.)"
        } else {
            "◌"
        }
    }
    pub fn growing() -> &'static str {
        if is_ascii_mode() {
            "(o)"
        } else {
            "◐"
        }
    }
    pub fn blooming() -> &'static str {
        if is_ascii_mode() {
            "(O)"
        } else {
            "◑"
        }
    }
    pub fn complete() -> &'static str {
        if is_ascii_mode() {
            "(@)"
        } else {
            "●"
        }
    }
}

/// Tool operation names in workshop/garden language
pub fn tool_metaphor(tool_name: &str) -> &'static str {
    match tool_name {
        // File operations
        "file_read" => "examining",
        "file_write" => "inscribing",
        "file_edit" => "pruning",
        "directory_tree" => "surveying",

        // Git operations
        "git_status" => "checking the weather",
        "git_diff" => "comparing growth",
        "git_commit" => "preserving your harvest",
        "git_checkpoint" => "marking the season",

        // Cargo/build operations
        "cargo_test" => "testing the soil",
        "cargo_check" => "inspecting the joinery",
        "cargo_clippy" => "polishing",
        "cargo_fmt" => "tidying the workshop",

        // Search operations
        "grep_search" => "foraging",
        "glob_find" => "mapping the terrain",
        "symbol_search" => "cataloging specimens",

        // Shell operations
        "shell_exec" => "working at the bench",

        // Process management
        "process_start" => "kindling",
        "process_stop" => "banking the fire",
        "process_list" => "taking inventory",
        "process_logs" => "reading the ledger",

        // Container operations
        "container_run" => "planting in pots",
        "container_stop" => "putting to rest",
        "container_build" => "crafting a vessel",

        // Browser operations
        "browser_fetch" => "gathering from afar",
        "browser_screenshot" => "capturing a moment",

        // Knowledge graph
        "knowledge_add" => "recording wisdom",
        "knowledge_query" => "consulting the archives",

        // Default
        _ => "tending",
    }
}

/// Status messages in craftsman's voice
pub fn status_message(status: ToolStatus) -> String {
    match status {
        ToolStatus::Starting(tool) => {
            format!(
                "{} {} your garden...",
                Glyphs::sprout(),
                tool_metaphor(tool)
            )
        }
        ToolStatus::Success(tool) => format!(
            "{} Finished {} — all is well.",
            Glyphs::bloom(),
            tool_metaphor(tool)
        ),
        ToolStatus::Warning(tool, msg) => format!(
            "{} {} complete, but the soil whispers: {}",
            Glyphs::wilt(),
            tool_metaphor(tool),
            msg
        ),
        ToolStatus::Error(tool, msg) => format!(
            "{} A frost has touched {} — {}",
            Glyphs::frost(),
            tool_metaphor(tool),
            msg
        ),
    }
}

#[derive(Debug, Clone)]
pub enum ToolStatus<'a> {
    Starting(&'a str),
    Success(&'a str),
    Warning(&'a str, &'a str),
    Error(&'a str, &'a str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_metaphors() {
        assert_eq!(tool_metaphor("file_read"), "examining");
        assert_eq!(tool_metaphor("git_commit"), "preserving your harvest");
        assert_eq!(tool_metaphor("cargo_test"), "testing the soil");
    }

    #[test]
    fn test_tool_metaphors_comprehensive() {
        // File operations
        assert_eq!(tool_metaphor("file_write"), "inscribing");
        assert_eq!(tool_metaphor("file_edit"), "pruning");
        assert_eq!(tool_metaphor("directory_tree"), "surveying");

        // Git operations
        assert_eq!(tool_metaphor("git_status"), "checking the weather");
        assert_eq!(tool_metaphor("git_diff"), "comparing growth");
        assert_eq!(tool_metaphor("git_checkpoint"), "marking the season");

        // Cargo operations
        assert_eq!(tool_metaphor("cargo_check"), "inspecting the joinery");
        assert_eq!(tool_metaphor("cargo_clippy"), "polishing");
        assert_eq!(tool_metaphor("cargo_fmt"), "tidying the workshop");

        // Search operations
        assert_eq!(tool_metaphor("grep_search"), "foraging");
        assert_eq!(tool_metaphor("glob_find"), "mapping the terrain");
        assert_eq!(tool_metaphor("symbol_search"), "cataloging specimens");

        // Shell and process
        assert_eq!(tool_metaphor("shell_exec"), "working at the bench");
        assert_eq!(tool_metaphor("process_start"), "kindling");
        assert_eq!(tool_metaphor("process_stop"), "banking the fire");
        assert_eq!(tool_metaphor("process_list"), "taking inventory");
        assert_eq!(tool_metaphor("process_logs"), "reading the ledger");

        // Container operations
        assert_eq!(tool_metaphor("container_run"), "planting in pots");
        assert_eq!(tool_metaphor("container_stop"), "putting to rest");
        assert_eq!(tool_metaphor("container_build"), "crafting a vessel");

        // Browser operations
        assert_eq!(tool_metaphor("browser_fetch"), "gathering from afar");
        assert_eq!(tool_metaphor("browser_screenshot"), "capturing a moment");

        // Knowledge graph
        assert_eq!(tool_metaphor("knowledge_add"), "recording wisdom");
        assert_eq!(tool_metaphor("knowledge_query"), "consulting the archives");

        // Unknown tool
        assert_eq!(tool_metaphor("unknown_tool"), "tending");
    }

    #[test]
    fn test_glyphs_exist() {
        set_ascii_mode(false);
        assert!(!Glyphs::seedling().is_empty());
        assert!(!Glyphs::hammer().is_empty());
        assert!(!Glyphs::journal().is_empty());
    }

    #[test]
    fn test_all_glyphs() {
        set_ascii_mode(false);
        // Garden metaphors
        assert!(!Glyphs::sprout().is_empty());
        assert!(!Glyphs::tree().is_empty());
        assert!(!Glyphs::leaf().is_empty());
        assert!(!Glyphs::fallen_leaf().is_empty());
        assert!(!Glyphs::flower().is_empty());
        assert!(!Glyphs::harvest().is_empty());

        // Workshop tools
        assert!(!Glyphs::wrench().is_empty());
        assert!(!Glyphs::magnifier().is_empty());
        assert!(!Glyphs::scissors().is_empty());
        assert!(!Glyphs::gear().is_empty());
        assert!(!Glyphs::compass().is_empty());

        // Personal items
        assert!(!Glyphs::bookmark().is_empty());
        assert!(!Glyphs::lantern().is_empty());
        assert!(!Glyphs::key().is_empty());
        assert!(!Glyphs::home().is_empty());
        assert!(!Glyphs::chest().is_empty());

        // Status indicators
        assert!(!Glyphs::bloom().is_empty());
        assert!(!Glyphs::wilt().is_empty());
        assert!(!Glyphs::frost().is_empty());

        // Borders
        assert!(!Glyphs::corner_tl().is_empty());
        assert!(!Glyphs::corner_tr().is_empty());
        assert!(!Glyphs::corner_bl().is_empty());
        assert!(!Glyphs::corner_br().is_empty());
        assert!(!Glyphs::horiz().is_empty());
        assert!(!Glyphs::vert().is_empty());
        assert!(!Glyphs::branch().is_empty());
        assert!(!Glyphs::leaf_branch().is_empty());

        // Progress indicators
        assert!(!Glyphs::tending().is_empty());
        assert!(!Glyphs::growing().is_empty());
        assert!(!Glyphs::blooming().is_empty());
        assert!(!Glyphs::complete().is_empty());
    }

    #[test]
    fn test_ascii_mode_toggle() {
        set_ascii_mode(true);
        assert_eq!(Glyphs::seedling(), "[*]");
        assert_eq!(Glyphs::hammer(), "[#]");
        assert_eq!(Glyphs::corner_tl(), "+");
        assert_eq!(Glyphs::horiz(), "-");
        assert_eq!(Glyphs::bloom(), "[B]");

        set_ascii_mode(false);
        assert_eq!(Glyphs::seedling(), "\u{1f331}");
        assert_eq!(Glyphs::hammer(), "\u{1f528}");
        assert_eq!(Glyphs::corner_tl(), "\u{256d}");
        assert_eq!(Glyphs::horiz(), "\u{2500}");
        assert_eq!(Glyphs::bloom(), "\u{273f}");
    }

    #[test]
    fn test_selfware_style_str() {
        let text = "test";

        // All style methods should return non-empty strings
        assert!(!text.workshop_title().to_string().is_empty());
        assert!(!text.garden_healthy().to_string().is_empty());
        assert!(!text.garden_wilting().to_string().is_empty());
        assert!(!text.tool_name().to_string().is_empty());
        assert!(!text.path_local().to_string().is_empty());
        assert!(!text.timestamp().to_string().is_empty());
        assert!(!text.muted().to_string().is_empty());
        assert!(!text.emphasis().to_string().is_empty());
        assert!(!text.craftsman_voice().to_string().is_empty());
    }

    #[test]
    fn test_selfware_style_string() {
        let text = "test".to_string();

        // All style methods should work on String too
        assert!(!text.clone().workshop_title().to_string().is_empty());
        assert!(!text.clone().garden_healthy().to_string().is_empty());
        assert!(!text.clone().garden_wilting().to_string().is_empty());
        assert!(!text.clone().tool_name().to_string().is_empty());
        assert!(!text.clone().path_local().to_string().is_empty());
        assert!(!text.clone().timestamp().to_string().is_empty());
        assert!(!text.clone().muted().to_string().is_empty());
        assert!(!text.clone().emphasis().to_string().is_empty());
        assert!(!text.clone().craftsman_voice().to_string().is_empty());
    }

    #[test]
    fn test_status_message_starting() {
        set_ascii_mode(false);
        let msg = status_message(ToolStatus::Starting("file_read"));
        assert!(msg.contains("examining"));
        assert!(msg.contains(Glyphs::sprout()));
    }

    #[test]
    fn test_status_message_success() {
        set_ascii_mode(false);
        let msg = status_message(ToolStatus::Success("git_commit"));
        assert!(msg.contains("preserving your harvest"));
        assert!(msg.contains(Glyphs::bloom()));
        assert!(msg.contains("all is well"));
    }

    #[test]
    fn test_status_message_warning() {
        set_ascii_mode(false);
        let msg = status_message(ToolStatus::Warning("cargo_test", "some tests slow"));
        assert!(msg.contains("testing the soil"));
        assert!(msg.contains(Glyphs::wilt()));
        assert!(msg.contains("some tests slow"));
    }

    #[test]
    fn test_status_message_error() {
        set_ascii_mode(false);
        let msg = status_message(ToolStatus::Error("cargo_check", "compilation failed"));
        assert!(msg.contains("inspecting the joinery"));
        assert!(msg.contains(Glyphs::frost()));
        assert!(msg.contains("compilation failed"));
    }

    #[test]
    fn test_tool_status_clone() {
        let status = ToolStatus::Starting("test");
        let cloned = status.clone();
        assert!(matches!(cloned, ToolStatus::Starting("test")));
    }

    #[test]
    fn test_palette_colors() {
        // Just verify the colors are defined correctly
        assert_eq!(Palette::AMBER.r, 212);
        assert_eq!(Palette::GARDEN_GREEN.g, 108);
        assert_eq!(Palette::SOIL_BROWN.b, 37);
        assert_eq!(Palette::INK.r, 40);
        assert_eq!(Palette::COPPER.r, 184);
        assert_eq!(Palette::SAGE.g, 151);
        assert_eq!(Palette::STONE.r, 128);
    }
}
