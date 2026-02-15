//! Selfware Style System
//!
//! Warm, organic palette for the personal workshop aesthetic.
//! Like aged paper, wood grain, and amber resin.

use colored::{Colorize, CustomColor};

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
        self.custom_color(Palette::AMBER).bold()
    }

    fn garden_healthy(self) -> colored::ColoredString {
        self.custom_color(Palette::GARDEN_GREEN)
    }

    fn garden_wilting(self) -> colored::ColoredString {
        self.custom_color(Palette::SOIL_BROWN)
    }

    fn tool_name(self) -> colored::ColoredString {
        self.custom_color(Palette::COPPER).bold()
    }

    fn path_local(self) -> colored::ColoredString {
        self.custom_color(Palette::SAGE).italic()
    }

    fn timestamp(self) -> colored::ColoredString {
        self.custom_color(Palette::STONE).dimmed()
    }

    fn muted(self) -> colored::ColoredString {
        self.custom_color(Palette::STONE)
    }

    fn emphasis(self) -> colored::ColoredString {
        self.custom_color(Palette::AMBER)
    }

    fn craftsman_voice(self) -> colored::ColoredString {
        self.custom_color(Palette::INK).italic()
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

/// Unicode glyphs for the workshop aesthetic
pub struct Glyphs;

impl Glyphs {
    // Garden metaphors
    pub const SEEDLING: &'static str = "🌱";
    pub const SPROUT: &'static str = "🌿";
    pub const TREE: &'static str = "🌳";
    pub const LEAF: &'static str = "🍃";
    pub const FALLEN_LEAF: &'static str = "🍂";
    pub const FLOWER: &'static str = "🌸";
    pub const HARVEST: &'static str = "🌾";

    // Workshop tools
    pub const HAMMER: &'static str = "🔨";
    pub const WRENCH: &'static str = "🔧";
    pub const MAGNIFIER: &'static str = "🔍";
    pub const SCISSORS: &'static str = "✂️";
    pub const GEAR: &'static str = "⚙️";
    pub const COMPASS: &'static str = "🧭";

    // Personal items
    pub const JOURNAL: &'static str = "📓";
    pub const BOOKMARK: &'static str = "🔖";
    pub const LANTERN: &'static str = "🏮";
    pub const KEY: &'static str = "🔑";
    pub const HOME: &'static str = "🏠";
    pub const CHEST: &'static str = "📦";

    // Status indicators (organic)
    pub const BLOOM: &'static str = "✿";
    pub const WILT: &'static str = "❀";
    pub const FROST: &'static str = "❄";

    // Borders (hand-drawn feel)
    pub const CORNER_TL: &'static str = "╭";
    pub const CORNER_TR: &'static str = "╮";
    pub const CORNER_BL: &'static str = "╰";
    pub const CORNER_BR: &'static str = "╯";
    pub const HORIZ: &'static str = "─";
    pub const VERT: &'static str = "│";
    pub const BRANCH: &'static str = "├";
    pub const LEAF_BRANCH: &'static str = "└";

    // Progress indicators
    pub const TENDING: &'static str = "◌";
    pub const GROWING: &'static str = "◐";
    pub const BLOOMING: &'static str = "◑";
    pub const COMPLETE: &'static str = "●";
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
            format!("{} {} your garden...", Glyphs::SPROUT, tool_metaphor(tool))
        }
        ToolStatus::Success(tool) => format!(
            "{} Finished {} — all is well.",
            Glyphs::BLOOM,
            tool_metaphor(tool)
        ),
        ToolStatus::Warning(tool, msg) => format!(
            "{} {} complete, but the soil whispers: {}",
            Glyphs::WILT,
            tool_metaphor(tool),
            msg
        ),
        ToolStatus::Error(tool, msg) => format!(
            "{} A frost has touched {} — {}",
            Glyphs::FROST,
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
        assert!(!Glyphs::SEEDLING.is_empty());
        assert!(!Glyphs::HAMMER.is_empty());
        assert!(!Glyphs::JOURNAL.is_empty());
    }

    #[test]
    fn test_all_glyphs() {
        // Garden metaphors
        assert!(!Glyphs::SPROUT.is_empty());
        assert!(!Glyphs::TREE.is_empty());
        assert!(!Glyphs::LEAF.is_empty());
        assert!(!Glyphs::FALLEN_LEAF.is_empty());
        assert!(!Glyphs::FLOWER.is_empty());
        assert!(!Glyphs::HARVEST.is_empty());

        // Workshop tools
        assert!(!Glyphs::WRENCH.is_empty());
        assert!(!Glyphs::MAGNIFIER.is_empty());
        assert!(!Glyphs::SCISSORS.is_empty());
        assert!(!Glyphs::GEAR.is_empty());
        assert!(!Glyphs::COMPASS.is_empty());

        // Personal items
        assert!(!Glyphs::BOOKMARK.is_empty());
        assert!(!Glyphs::LANTERN.is_empty());
        assert!(!Glyphs::KEY.is_empty());
        assert!(!Glyphs::HOME.is_empty());
        assert!(!Glyphs::CHEST.is_empty());

        // Status indicators
        assert!(!Glyphs::BLOOM.is_empty());
        assert!(!Glyphs::WILT.is_empty());
        assert!(!Glyphs::FROST.is_empty());

        // Borders
        assert!(!Glyphs::CORNER_TL.is_empty());
        assert!(!Glyphs::CORNER_TR.is_empty());
        assert!(!Glyphs::CORNER_BL.is_empty());
        assert!(!Glyphs::CORNER_BR.is_empty());
        assert!(!Glyphs::HORIZ.is_empty());
        assert!(!Glyphs::VERT.is_empty());
        assert!(!Glyphs::BRANCH.is_empty());
        assert!(!Glyphs::LEAF_BRANCH.is_empty());

        // Progress indicators
        assert!(!Glyphs::TENDING.is_empty());
        assert!(!Glyphs::GROWING.is_empty());
        assert!(!Glyphs::BLOOMING.is_empty());
        assert!(!Glyphs::COMPLETE.is_empty());
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
        let msg = status_message(ToolStatus::Starting("file_read"));
        assert!(msg.contains("examining"));
        assert!(msg.contains(Glyphs::SPROUT));
    }

    #[test]
    fn test_status_message_success() {
        let msg = status_message(ToolStatus::Success("git_commit"));
        assert!(msg.contains("preserving your harvest"));
        assert!(msg.contains(Glyphs::BLOOM));
        assert!(msg.contains("all is well"));
    }

    #[test]
    fn test_status_message_warning() {
        let msg = status_message(ToolStatus::Warning("cargo_test", "some tests slow"));
        assert!(msg.contains("testing the soil"));
        assert!(msg.contains(Glyphs::WILT));
        assert!(msg.contains("some tests slow"));
    }

    #[test]
    fn test_status_message_error() {
        let msg = status_message(ToolStatus::Error("cargo_check", "compilation failed"));
        assert!(msg.contains("inspecting the joinery"));
        assert!(msg.contains(Glyphs::FROST));
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
