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
