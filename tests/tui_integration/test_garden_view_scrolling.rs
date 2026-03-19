//! Garden View Scrolling Tests

use selfware::ui::garden::{DigitalGarden, GardenBed, GardenPlant, GrowthStage, PlantType, Season};
use selfware::ui::tui::garden_view::{GardenItem, GardenView};

/// Helper to create a test garden with plants
fn create_test_garden_with_plants(count: usize) -> DigitalGarden {
    let mut garden = DigitalGarden {
        project_name: "Test Garden".to_string(),
        beds: std::collections::HashMap::new(),
        total_plants: count,
        total_lines: count * 100,
        season: Season::Summer,
    };
    
    let mut bed = GardenBed {
        name: "Test Bed".to_string(),
        description: "A test bed".to_string(),
        path: std::path::PathBuf::from("/test"),
        plants: Vec::new(),
        total_lines: count * 100,
    };
    
    for i in 0..count {
        bed.plants.push(GardenPlant {
            name: format!("Plant {}", i),
            description: format!("Description {}", i),
            path: std::path::PathBuf::from(format!("/test/plant{}", i)),
            plant_type: if i % 2 == 0 { PlantType::Flower } else { PlantType::Tree },
            growth_stage: GrowthStage::Blooming,
            health: (i % 100) as f32 / 100.0,
            line_count: 100,
            last_modified: std::time::SystemTime::now(),
            dependencies: vec![],
        });
    }
    
    garden.beds.insert("test-bed".to_string(), bed);
    garden
}

/// Test garden view with many items
#[test]
fn test_garden_view_with_many_items() {
    let garden = create_test_garden_with_plants(100);
    
    let mut view = GardenView::new();
    view.set_garden(garden);
    
    // Test selection can navigate through items
    for _ in 0..50 {
        view.select_next();
    }
    
    // Should be able to go back
    for _ in 0..50 {
        view.select_prev();
    }
}

/// Test garden view scroll boundary conditions
#[test]
fn test_garden_view_scroll_boundaries() {
    let garden = create_test_garden_with_plants(10);
    
    let mut view = GardenView::new();
    view.set_garden(garden);
    
    // Start at first item
    let initial = view.selected_item();
    assert!(initial.is_some());
    
    // Scroll through all items
    for _ in 0..10 {
        view.select_next();
    }
    
    let after_next = view.selected_item();
    assert!(after_next.is_some());
    
    // Scroll back
    for _ in 0..20 {
        view.select_prev();
    }
    
    let after_prev = view.selected_item();
    assert!(after_prev.is_some());
}

/// Test garden view with empty garden
#[test]
fn test_garden_view_empty() {
    let garden = DigitalGarden {
        project_name: "Empty Garden".to_string(),
        beds: std::collections::HashMap::new(),
        total_plants: 0,
        total_lines: 0,
        season: Season::Winter,
    };
    
    let mut view = GardenView::new();
    view.set_garden(garden);
    
    // Should not panic with empty garden
    view.select_next();
    view.select_prev();
    
    assert!(view.selected_item().is_none());
}

/// Test garden item expansion/collapse
#[test]
fn test_garden_item_expansion() {
    let garden = create_test_garden_with_plants(5);
    
    let mut view = GardenView::new();
    view.set_garden(garden);
    view.set_focused(true);
    
    // Toggle expansion
    view.toggle_expand();
    
    // Should be able to navigate
    view.select_next();
    
    // Collapse and verify no panic
    view.toggle_expand();
}

/// Test garden view tick animation
#[test]
fn test_garden_view_animation_tick() {
    let garden = create_test_garden_with_plants(1);
    
    let mut view = GardenView::new();
    view.set_garden(garden);
    
    // Run multiple ticks
    for _ in 0..60 {
        view.tick();
    }
    
    // View should still be valid
    assert!(view.selected_item().is_some());
}

/// Test recent changes tracking
#[test]
fn test_garden_recent_changes() {
    let garden = create_test_garden_with_plants(5);
    
    let mut view = GardenView::new();
    view.set_garden(garden);
    
    // Mark some changes
    view.mark_changed("/path/to/file1.rs");
    view.mark_changed("/path/to/file2.rs");
    view.mark_changed("/path/to/file3.rs");
    
    // Changes should be tracked
    view.clear_changes(); // Just verify this doesn't panic
}

/// Test GardenItem types
#[test]
fn test_garden_item_types() {
    use selfware::ui::tui::garden_view::GardenFocus;
    
    let bed_item = GardenItem::Bed {
        name: "Test Bed".to_string(),
        path: "/test".to_string(),
        plant_count: 5,
        expanded: false,
    };
    
    let plant_item = GardenItem::Plant {
        name: "Test Plant".to_string(),
        path: "/test/plant.rs".to_string(),
        plant_type: PlantType::Flower,
        growth_stage: GrowthStage::Seedling,
        health: 0.8,
        line_count: 100,
    };
    
    let header_item = GardenItem::Header("Section".to_string());
    
    // Verify is_bed works
    assert!(bed_item.is_bed());
    assert!(!plant_item.is_bed());
    assert!(!header_item.is_bed());
    
    // Verify name access
    assert_eq!(bed_item.name(), "Test Bed");
    assert_eq!(plant_item.name(), "Test Plant");
    assert_eq!(header_item.name(), "Section");
}

/// Test GardenFocus states
#[test]
fn test_garden_focus() {
    use selfware::ui::tui::garden_view::GardenFocus;
    
    let _beds = GardenFocus::Beds;
    let _plants = GardenFocus::Plants;
    let _details = GardenFocus::Details;
}

/// Test garden view with focused state
#[test]
fn test_garden_view_focused() {
    let garden = create_test_garden_with_plants(5);
    
    let mut view = GardenView::new();
    view.set_garden(garden);
    
    // Test both focus states
    view.set_focused(true);
    view.set_focused(false);
}

/// Test growth stage variants
#[test]
fn test_growth_stages() {
    use selfware::ui::garden::GrowthStage;
    
    let stages = [
        GrowthStage::Seed,
        GrowthStage::Seedling,
        GrowthStage::Growing,
        GrowthStage::Blooming,
        GrowthStage::Mature,
        GrowthStage::Dormant,
    ];
    
    // Verify all stages exist and have descriptions
    for stage in &stages {
        let _desc = format!("{:?}", stage);
    }
}

/// Test plant types
#[test]
fn test_plant_types() {
    use selfware::ui::garden::PlantType;
    
    let types = [
        PlantType::Flower,
        PlantType::Tree,
        PlantType::Shrub,
        PlantType::Vine,
        PlantType::Root,
        PlantType::Succulent,
    ];
    
    for t in &types {
        let _name = format!("{:?}", t);
    }
}
