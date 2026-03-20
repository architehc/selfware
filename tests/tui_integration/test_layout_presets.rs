//! TUI Layout Preset Tests

use ratatui::layout::Rect;

/// Test all layout presets exist and can be applied
#[test]
fn test_all_layout_presets() {
    use selfware::ui::tui::layout::LayoutPreset;

    // Verify all presets can be instantiated
    let _presets = [
        LayoutPreset::Dashboard,
        LayoutPreset::Coding,
        LayoutPreset::Debugging,
        LayoutPreset::Review,
        LayoutPreset::Focus,
        LayoutPreset::Explore,
        LayoutPreset::FullWorkspace,
    ];
}

/// Test LayoutEngine with various presets
#[test]
fn test_layout_engine_creation() {
    use selfware::ui::tui::layout::{LayoutEngine, LayoutPreset};

    let mut engine = LayoutEngine::new();
    engine.apply_preset(LayoutPreset::Dashboard);

    // Calculate layout for standard terminal size
    let area = Rect::new(0, 0, 80, 24);
    let layout = engine.calculate_layout(area);

    // Dashboard should produce a non-empty layout
    assert!(!layout.is_empty(), "Dashboard layout should not be empty");
}

/// Test layout calculation for different terminal sizes
#[test]
fn test_layout_various_sizes() {
    use selfware::ui::tui::layout::{LayoutEngine, LayoutPreset};

    let sizes = [
        (200, 60), // Very large
        (120, 40), // Large
        (80, 24),  // Standard
        (60, 20),  // Small
    ];

    for (width, height) in sizes {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Dashboard);

        let area = Rect::new(0, 0, width, height);
        let layout = engine.calculate_layout(area);

        // Layout should never be empty for reasonable sizes
        assert!(
            !layout.is_empty(),
            "Layout for {}x{} should not be empty",
            width,
            height
        );
    }
}

/// Test pane types
#[test]
fn test_pane_types() {
    use selfware::ui::tui::layout::PaneType;

    let types = [
        PaneType::Garden,
        PaneType::Chat,
        PaneType::Logs,
        PaneType::Status,
        PaneType::Preview,
        PaneType::Editor,
    ];

    // Just verify all variants exist and can be matched
    for t in &types {
        match t {
            PaneType::Garden => {}
            PaneType::Chat => {}
            PaneType::Logs => {}
            PaneType::Status => {}
            PaneType::Preview => {}
            PaneType::Editor => {}
        }
    }
}

/// Test split direction
#[test]
fn test_split_direction() {
    use selfware::ui::tui::layout::SplitDirection;

    let _horizontal = SplitDirection::Horizontal;
    let _vertical = SplitDirection::Vertical;
}

/// Test PaneId creation and comparison
#[test]
fn test_pane_id() {
    use selfware::ui::tui::layout::PaneId;

    let id1 = PaneId(1);
    let id2 = PaneId(1);
    let id3 = PaneId(2);

    assert_eq!(id1.0, 1);
    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

/// Test layout standard helpers
#[test]
fn test_standard_layout() {
    use selfware::ui::tui::standard_layout;

    let area = Rect::new(0, 0, 80, 24);
    let layout = standard_layout(area);

    // Should produce at least one rect
    assert!(!layout.is_empty(), "Standard layout should produce rects");

    // All rects should be within bounds
    for rect in &layout {
        assert!(rect.x >= area.x);
        assert!(rect.y >= area.y);
        assert!(rect.right() <= area.right());
        assert!(rect.bottom() <= area.bottom());
    }
}

/// Test split layout helper
#[test]
fn test_split_layout() {
    use selfware::ui::tui::split_layout;

    let area = Rect::new(0, 0, 100, 40);
    let (left, right) = split_layout(area, 30);

    // Left should be 30% of width
    assert_eq!(left.width, 30);
    assert_eq!(left.height, 40);

    // Right should be remaining width
    assert_eq!(right.width, 70);
    assert_eq!(right.height, 40);

    // Should be adjacent
    assert_eq!(left.right(), right.x);
    assert_eq!(left.y, right.y);
}

/// Test LayoutNode structure
#[test]
fn test_layout_node() {
    use selfware::ui::tui::layout::{LayoutNode, PaneType};

    let node = LayoutNode::Pane(PaneType::Garden);
    assert!(matches!(node, LayoutNode::Pane(PaneType::Garden)));

    let split = LayoutNode::Split {
        direction: selfware::ui::tui::layout::SplitDirection::Horizontal,
        ratio: 0.5,
        children: vec![],
    };
    assert!(matches!(split, LayoutNode::Split { .. }));
}
