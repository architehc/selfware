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
#[path = "../../tests/unit/ui/style/style_test.rs"]
mod tests;
