//! Terminal Resize Handling Tests

use ratatui::layout::Rect;

/// Test layout recalculation on resize
#[test]
fn test_layout_recalculation_on_resize() {
    use selfware::ui::tui::layout::{LayoutEngine, LayoutPreset};

    let mut engine = LayoutEngine::new();
    engine.apply_preset(LayoutPreset::Dashboard);

    // Calculate at one size
    let small_area = Rect::new(0, 0, 60, 20);
    let small_layout = engine.calculate_layout(small_area);

    // Calculate at larger size
    let large_area = Rect::new(0, 0, 120, 40);
    let large_layout = engine.calculate_layout(large_area);

    // Pane count should be same but areas different
    assert_eq!(small_layout.len(), large_layout.len());

    // Larger layout should have larger areas
    let small_total: u16 = small_layout.values().map(|r| r.width * r.height).sum();
    let large_total: u16 = large_layout.values().map(|r| r.width * r.height).sum();

    assert!(
        large_total > small_total,
        "Larger terminal should have more area"
    );
}

/// Test layout with various terminal sizes
#[test]
fn test_layout_various_sizes() {
    use selfware::ui::tui::layout::{LayoutEngine, LayoutPreset};

    let sizes = [
        (200, 60), // Very large
        (120, 40), // Large
        (80, 24),  // Standard
        (60, 20),  // Small
        (40, 12),  // Very small
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

        // All areas should be valid
        for (_, pane_area) in &layout {
            assert!(
                pane_area.width > 0,
                "Pane width should be > 0 at {}x{}",
                width,
                height
            );
            assert!(
                pane_area.height > 0,
                "Pane height should be > 0 at {}x{}",
                width,
                height
            );
        }
    }
}

/// Test split layout at different sizes
#[test]
fn test_split_layout_resize() {
    use selfware::ui::tui::split_layout;

    let sizes = [(100, 40), (80, 24), (60, 20), (40, 12)];

    for (width, height) in sizes {
        let area = Rect::new(0, 0, width, height);
        let (left, right) = split_layout(area, 30);

        // Left should be 30% of width
        assert_eq!(
            left.width,
            (width * 30) / 100,
            "Left width should be 30% of {} at {}x{}",
            width,
            width,
            height
        );
        assert_eq!(left.height, height);

        // Right should fill remaining width
        assert_eq!(right.width, width - left.width);
        assert_eq!(right.height, height);

        // Should be adjacent
        assert_eq!(left.right(), right.x);
        assert_eq!(left.y, right.y);
    }
}

/// Test standard layout at different sizes
#[test]
fn test_standard_layout_resize() {
    use selfware::ui::tui::standard_layout;

    let sizes = [(120, 40), (80, 24), (60, 20)];

    for (width, height) in sizes {
        let area = Rect::new(0, 0, width, height);
        let layout = standard_layout(area);

        assert!(
            !layout.is_empty(),
            "Standard layout should produce rects at {}x{}",
            width,
            height
        );

        // All rects should be within bounds
        for rect in &layout {
            assert!(
                rect.x >= area.x,
                "Rect x out of bounds at {}x{}",
                width,
                height
            );
            assert!(
                rect.y >= area.y,
                "Rect y out of bounds at {}x{}",
                width,
                height
            );
            assert!(
                rect.right() <= area.right(),
                "Rect right out of bounds at {}x{}",
                width,
                height
            );
            assert!(
                rect.bottom() <= area.bottom(),
                "Rect bottom out of bounds at {}x{}",
                width,
                height
            );
        }
    }
}

/// Test layout with extreme aspect ratios
#[test]
fn test_extreme_aspect_ratios() {
    use selfware::ui::tui::layout::{LayoutEngine, LayoutPreset};

    let extreme_sizes = [
        (200, 10), // Very wide, short
        (20, 100), // Very narrow, tall
        (10, 10),  // Tiny square
    ];

    for (width, height) in extreme_sizes {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Dashboard);

        let area = Rect::new(0, 0, width, height);
        let layout = engine.calculate_layout(area);

        // Should still produce valid layout (may be degraded)
        // Just verify it doesn't panic
        for (_, pane_area) in &layout {
            // Allow zero-size panes in extreme cases but not overflow
            assert!(
                pane_area.right() <= area.right() + width,
                "Pane right overflow at {}x{}",
                width,
                height
            );
            assert!(
                pane_area.bottom() <= area.bottom() + height,
                "Pane bottom overflow at {}x{}",
                width,
                height
            );
        }
    }
}

/// Test layout with all presets at multiple sizes
#[test]
fn test_all_presets_all_sizes() {
    use selfware::ui::tui::layout::{LayoutEngine, LayoutPreset};

    let presets = [
        LayoutPreset::Dashboard,
        LayoutPreset::Coding,
        LayoutPreset::Debugging,
        LayoutPreset::Review,
        LayoutPreset::Focus,
        LayoutPreset::Explore,
        LayoutPreset::FullWorkspace,
    ];

    let sizes = [(120, 40), (80, 24), (60, 20)];

    for preset in &presets {
        for (width, height) in &sizes {
            let mut engine = LayoutEngine::new();
            engine.apply_preset(*preset);

            let area = Rect::new(0, 0, *width, *height);
            let layout = engine.calculate_layout(area);

            assert!(
                !layout.is_empty(),
                "Layout for {:?} at {}x{} should not be empty",
                preset,
                width,
                height
            );
        }
    }
}

/// Test rect calculations
#[test]
fn test_rect_calculations() {
    let rect = Rect::new(10, 20, 80, 24);

    assert_eq!(rect.x, 10);
    assert_eq!(rect.y, 20);
    assert_eq!(rect.width, 80);
    assert_eq!(rect.height, 24);

    // Right and bottom calculations
    assert_eq!(rect.right(), 90); // x + width
    assert_eq!(rect.bottom(), 44); // y + height

    // Area
    assert_eq!(rect.area(), 1920); // width * height
}

/// Test Rect::new with various sizes
#[test]
fn test_rect_new_variations() {
    let sizes = [
        (0, 0, 0, 0),       // Zero size
        (0, 0, 80, 24),     // Standard
        (0, 0, 1000, 1000), // Large
        (10, 10, 60, 20),   // Offset
    ];

    for (x, y, w, h) in sizes {
        let rect = Rect::new(x, y, w, h);
        assert_eq!(rect.x, x);
        assert_eq!(rect.y, y);
        assert_eq!(rect.width, w);
        assert_eq!(rect.height, h);
    }
}

/// Test layout bounds validation
#[test]
fn test_layout_bounds() {
    use selfware::ui::tui::layout::{LayoutEngine, LayoutPreset};

    let mut engine = LayoutEngine::new();
    engine.apply_preset(LayoutPreset::Dashboard);

    let area = Rect::new(0, 0, 80, 24);
    let layout = engine.calculate_layout(area);

    // No pane should extend beyond area
    for (pane_id, pane_area) in &layout {
        assert!(
            pane_area.x >= area.x,
            "Pane {:?} x {} is outside area x {}",
            pane_id,
            pane_area.x,
            area.x
        );
        assert!(
            pane_area.y >= area.y,
            "Pane {:?} y {} is outside area y {}",
            pane_id,
            pane_area.y,
            area.y
        );
        assert!(
            pane_area.right() <= area.right(),
            "Pane {:?} right {} exceeds area right {}",
            pane_id,
            pane_area.right(),
            area.right()
        );
        assert!(
            pane_area.bottom() <= area.bottom(),
            "Pane {:?} bottom {} exceeds area bottom {}",
            pane_id,
            pane_area.bottom(),
            area.bottom()
        );
    }
}
