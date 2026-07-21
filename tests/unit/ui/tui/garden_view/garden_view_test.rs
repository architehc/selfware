use super::*;
use crate::ui::garden::{GardenBed, Season};
use std::collections::HashMap;

fn create_test_garden() -> DigitalGarden {
    let mut garden = DigitalGarden {
        project_name: "test-project".to_string(),
        beds: HashMap::new(),
        total_plants: 0,
        total_lines: 0,
        season: Season::Summer,
    };

    let mut bed = GardenBed::new("src");
    bed.add_plant(GardenPlant {
        path: "src/main.rs".to_string(),
        name: "main.rs".to_string(),
        extension: "rs".to_string(),
        lines: 100,
        age_days: 30,
        last_tended_days: 1,
        growth_stage: GrowthStage::Established,
        plant_type: PlantType::Flower,
    });
    bed.add_plant(GardenPlant {
        path: "src/lib.rs".to_string(),
        name: "lib.rs".to_string(),
        extension: "rs".to_string(),
        lines: 250,
        age_days: 45,
        last_tended_days: 0,
        growth_stage: GrowthStage::Mature,
        plant_type: PlantType::Flower,
    });
    garden.total_plants = 2;
    garden.total_lines = 350;
    garden.beds.insert("src".to_string(), bed);

    garden
}

#[test]
fn test_garden_view_new() {
    let view = GardenView::new();
    assert!(view.garden.is_none());
    assert!(view.items.is_empty());
    assert_eq!(view.selected, 0);
}

#[test]
fn test_garden_view_set_garden() {
    let mut view = GardenView::new();
    let garden = create_test_garden();
    view.set_garden(garden);

    assert!(view.garden.is_some());
    assert!(!view.items.is_empty());
    assert_eq!(view.items.len(), 1); // One bed
}

#[test]
fn test_garden_view_expand_collapse() {
    let mut view = GardenView::new();
    let garden = create_test_garden();
    view.set_garden(garden);

    assert_eq!(view.items.len(), 1); // Just the bed

    view.toggle_expand();
    assert_eq!(view.items.len(), 3); // Bed + 2 plants

    view.toggle_expand();
    assert_eq!(view.items.len(), 1); // Back to just bed
}

#[test]
fn test_garden_view_navigation() {
    let mut view = GardenView::new();
    let garden = create_test_garden();
    view.set_garden(garden);
    view.toggle_expand(); // Expand the bed

    assert_eq!(view.selected, 0);

    view.select_next();
    assert_eq!(view.selected, 1);

    view.select_next();
    assert_eq!(view.selected, 2);

    view.select_next();
    assert_eq!(view.selected, 0); // Wrap around

    view.select_prev();
    assert_eq!(view.selected, 2); // Wrap to end
}

#[test]
fn test_garden_view_recent_changes() {
    let mut view = GardenView::new();

    view.mark_changed("src/main.rs");
    assert!(view.has_recent_changes("src/main.rs"));
    assert!(view.has_recent_changes("src")); // Parent path

    view.clear_changes();
    assert!(!view.has_recent_changes("src/main.rs"));
}

#[test]
fn test_garden_item_is_bed() {
    let bed_item = GardenItem::Bed {
        name: "test".to_string(),
        path: "test".to_string(),
        plant_count: 5,
        health: 0.9,
        expanded: false,
    };
    assert!(bed_item.is_bed());

    let plant_item = GardenItem::Plant {
        plant: GardenPlant {
            path: "test.rs".to_string(),
            name: "test.rs".to_string(),
            extension: "rs".to_string(),
            lines: 50,
            age_days: 1,
            last_tended_days: 0,
            growth_stage: GrowthStage::Seedling,
            plant_type: PlantType::Vegetable,
        },
        bed_path: "test".to_string(),
    };
    assert!(!plant_item.is_bed());
}

#[test]
fn test_garden_view_tick() {
    let mut view = GardenView::new();
    let initial_frame = view.animation_frame;

    // Force enough time to pass
    view.last_animation = Instant::now() - std::time::Duration::from_millis(200);
    view.tick();

    assert_ne!(view.animation_frame, initial_frame);
}

#[test]
fn test_garden_view_selected_item() {
    let mut view = GardenView::new();
    assert!(view.selected_item().is_none());

    let garden = create_test_garden();
    view.set_garden(garden);

    let item = view.selected_item();
    assert!(item.is_some());
    assert!(item.unwrap().is_bed());
}

#[test]
fn test_render_health_bar() {
    let view = GardenView::new();

    let bar = view.render_health_bar(1.0, 10);
    assert_eq!(bar.chars().filter(|&c| c == '█').count(), 10);

    let bar = view.render_health_bar(0.5, 10);
    assert_eq!(bar.chars().filter(|&c| c == '█').count(), 5);

    let bar = view.render_health_bar(0.0, 10);
    assert_eq!(bar.chars().filter(|&c| c == '░').count(), 10);
}

#[test]
fn test_garden_item_name_bed() {
    let item = GardenItem::Bed {
        name: "src".to_string(),
        path: "src".to_string(),
        plant_count: 5,
        health: 0.9,
        expanded: false,
    };
    assert_eq!(item.name(), "src");
}

#[test]
fn test_garden_item_name_plant() {
    let item = GardenItem::Plant {
        plant: GardenPlant {
            path: "src/main.rs".to_string(),
            name: "main.rs".to_string(),
            extension: "rs".to_string(),
            lines: 100,
            age_days: 30,
            last_tended_days: 1,
            growth_stage: GrowthStage::Established,
            plant_type: PlantType::Flower,
        },
        bed_path: "src".to_string(),
    };
    assert_eq!(item.name(), "main.rs");
}

#[test]
fn test_set_focused() {
    let mut view = GardenView::new();
    assert!(!view.focused);
    view.set_focused(true);
    assert!(view.focused);
    view.set_focused(false);
    assert!(!view.focused);
}

#[test]
fn test_navigation_on_empty_list() {
    let mut view = GardenView::new();
    // Should not panic
    view.select_next();
    view.select_prev();
    view.toggle_expand();
    assert_eq!(view.selected, 0);
}

#[test]
fn test_select_prev_wrap_around() {
    let mut view = GardenView::new();
    let garden = create_test_garden();
    view.set_garden(garden);
    view.toggle_expand(); // Expand to get 3 items

    assert_eq!(view.selected, 0);
    view.select_prev(); // Should wrap to last item
    assert_eq!(view.selected, view.items.len() - 1);
}

#[test]
fn test_mark_changed_dedup() {
    let mut view = GardenView::new();
    view.mark_changed("src/main.rs");
    view.mark_changed("src/main.rs"); // Duplicate
    view.mark_changed("src/main.rs"); // Duplicate
    assert_eq!(view.recent_changes.len(), 1);
}

#[test]
fn test_mark_changed_max_20() {
    let mut view = GardenView::new();
    for i in 0..25 {
        view.mark_changed(&format!("file_{}.rs", i));
    }
    assert_eq!(view.recent_changes.len(), 20);
    // First 5 should have been removed
    assert!(!view.recent_changes.contains(&"file_0.rs".to_string()));
    assert!(view.recent_changes.contains(&"file_24.rs".to_string()));
}

#[test]
fn test_garden_view_default() {
    let view = GardenView::default();
    assert!(view.garden.is_none());
    assert!(view.items.is_empty());
}

#[test]
fn test_growth_char_all_frames() {
    let mut view = GardenView::new();
    let chars: Vec<&str> = (0..4)
        .map(|i| {
            view.animation_frame = i;
            view.growth_char()
        })
        .collect();
    assert_eq!(chars, vec!["🌱", "🌿", "🍃", "✨"]);
}
