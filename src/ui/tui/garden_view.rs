//! Interactive Garden View for TUI
//!
//! A zoomable, navigable tree view of the digital garden with:
//! - Hierarchical display of garden beds (directories) and plants (files)
//! - Selection highlighting with file details
//! - Growth animation effects on recently changed files
//! - Health indicators with visual feedback

use super::TuiPalette;
use crate::ui::garden::{DigitalGarden, GardenPlant, GrowthStage, PlantType};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};
use std::time::Instant;

/// Navigation state for the garden view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GardenFocus {
    /// Browsing garden beds (directories)
    Beds,
    /// Browsing plants in a selected bed
    Plants,
}

/// A single item in the tree view
#[derive(Debug, Clone)]
pub enum GardenItem {
    /// A garden bed (directory) with plant count
    Bed {
        name: String,
        path: String,
        plant_count: usize,
        health: f32,
        expanded: bool,
    },
    /// A plant (file) within a bed
    Plant {
        plant: GardenPlant,
        bed_path: String,
    },
}

impl GardenItem {
    /// Check if this is a bed
    pub fn is_bed(&self) -> bool {
        matches!(self, GardenItem::Bed { .. })
    }

    /// Get the display name
    pub fn name(&self) -> &str {
        match self {
            GardenItem::Bed { name, .. } => name,
            GardenItem::Plant { plant, .. } => &plant.name,
        }
    }
}

/// Interactive garden view with tree navigation
pub struct GardenView {
    /// The digital garden data
    garden: Option<DigitalGarden>,
    /// Flattened list of visible items for rendering
    items: Vec<GardenItem>,
    /// Currently selected index
    selected: usize,
    /// List state for scrolling
    list_state: ListState,
    /// Whether the view is focused
    focused: bool,
    /// Animation frame for growth effects
    animation_frame: u8,
    /// Last animation update
    last_animation: Instant,
    /// Recently modified paths (for growth animation)
    recent_changes: Vec<String>,
}

impl GardenView {
    /// Create a new empty garden view
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            garden: None,
            items: Vec::new(),
            selected: 0,
            list_state,
            focused: false,
            animation_frame: 0,
            last_animation: Instant::now(),
            recent_changes: Vec::new(),
        }
    }

    /// Set the garden data
    pub fn set_garden(&mut self, garden: DigitalGarden) {
        self.garden = Some(garden);
        self.rebuild_items();
    }

    /// Mark a path as recently changed (for growth animation)
    pub fn mark_changed(&mut self, path: &str) {
        if !self.recent_changes.contains(&path.to_string()) {
            self.recent_changes.push(path.to_string());
            // Keep only last 20 changes
            if self.recent_changes.len() > 20 {
                self.recent_changes.remove(0);
            }
        }
    }

    /// Clear recent changes
    pub fn clear_changes(&mut self) {
        self.recent_changes.clear();
    }

    /// Rebuild the flattened item list from garden data
    fn rebuild_items(&mut self) {
        self.items.clear();

        if let Some(garden) = &self.garden {
            // Sort beds by total lines (largest first)
            let mut beds: Vec<_> = garden.beds.values().collect();
            beds.sort_by(|a, b| b.total_lines.cmp(&a.total_lines));

            for bed in beds {
                // Add the bed
                let bed_item = GardenItem::Bed {
                    name: bed.name.clone(),
                    path: bed.path.clone(),
                    plant_count: bed.plants.len(),
                    health: bed.health_score,
                    expanded: false,
                };
                self.items.push(bed_item);
            }
        }
    }

    /// Toggle expansion of the selected bed
    pub fn toggle_expand(&mut self) {
        if self.items.is_empty() {
            return;
        }

        let selected = self.selected.min(self.items.len() - 1);

        match &self.items[selected] {
            GardenItem::Bed { path, expanded, .. } => {
                let path = path.clone();
                let was_expanded = *expanded;

                if was_expanded {
                    // Collapse: remove plants after this bed
                    self.collapse_bed(selected);
                } else {
                    // Expand: insert plants after this bed
                    self.expand_bed(selected, &path);
                }
            }
            GardenItem::Plant { .. } => {
                // Select the plant for details
            }
        }
    }

    /// Expand a bed at the given index
    fn expand_bed(&mut self, index: usize, bed_path: &str) {
        if let Some(GardenItem::Bed { expanded, .. }) = self.items.get_mut(index) {
            *expanded = true;
        }

        if let Some(garden) = &self.garden {
            if let Some(bed) = garden.beds.get(bed_path) {
                // Sort plants by lines
                let mut plants: Vec<_> = bed.plants.iter().collect();
                plants.sort_by(|a, b| b.lines.cmp(&a.lines));

                // Insert plants after the bed
                let plant_items: Vec<GardenItem> = plants
                    .into_iter()
                    .map(|p| GardenItem::Plant {
                        plant: p.clone(),
                        bed_path: bed_path.to_string(),
                    })
                    .collect();

                for (i, item) in plant_items.into_iter().enumerate() {
                    self.items.insert(index + 1 + i, item);
                }
            }
        }
    }

    /// Collapse a bed at the given index
    fn collapse_bed(&mut self, index: usize) {
        if let Some(GardenItem::Bed { expanded, .. }) = self.items.get_mut(index) {
            *expanded = false;
        }

        // Remove all plants following this bed until we hit another bed or end
        let mut remove_count = 0;
        for i in (index + 1)..self.items.len() {
            if self.items[i].is_bed() {
                break;
            }
            remove_count += 1;
        }

        for _ in 0..remove_count {
            self.items.remove(index + 1);
        }

        // Adjust selection if needed
        if self.selected > index && self.selected <= index + remove_count {
            self.selected = index;
            self.list_state.select(Some(index));
        } else if self.selected > index + remove_count {
            self.selected -= remove_count;
            self.list_state.select(Some(self.selected));
        }
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if self.items.is_empty() {
            return;
        }

        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = self.items.len() - 1;
        }
        self.list_state.select(Some(self.selected));
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.items.is_empty() {
            return;
        }

        if self.selected < self.items.len() - 1 {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
        self.list_state.select(Some(self.selected));
    }

    /// Get the currently selected item
    pub fn selected_item(&self) -> Option<&GardenItem> {
        self.items.get(self.selected)
    }

    /// Set focused state
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Update animation state
    pub fn tick(&mut self) {
        if self.last_animation.elapsed().as_millis() > 150 {
            self.animation_frame = (self.animation_frame + 1) % 4;
            self.last_animation = Instant::now();
        }
    }

    /// Get growth animation character
    fn growth_char(&self) -> &'static str {
        match self.animation_frame {
            0 => "🌱",
            1 => "🌿",
            2 => "🍃",
            _ => "✨",
        }
    }

    /// Check if a path has recent changes
    fn has_recent_changes(&self, path: &str) -> bool {
        self.recent_changes
            .iter()
            .any(|p| p.contains(path) || path.contains(p))
    }

    /// Render the garden view
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            TuiPalette::title_style()
        } else {
            TuiPalette::border_style()
        };

        // Get season and stats from garden
        let (season_glyph, season_desc, total_plants, total_lines) = self
            .garden
            .as_ref()
            .map(|g| {
                (
                    g.season.glyph(),
                    g.season.description(),
                    g.total_plants,
                    g.total_lines,
                )
            })
            .unwrap_or(("🌱", "unknown", 0, 0));

        let title = format!(" 🌳 Garden View {} ", season_glyph);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title, TuiPalette::title_style()));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.items.is_empty() {
            let empty = Paragraph::new("  No garden data. Run a scan to see your codebase.")
                .style(TuiPalette::muted_style());
            frame.render_widget(empty, inner);
            return;
        }

        // Split into tree view (70%) and details (30%)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(inner);

        // Render tree view
        self.render_tree(frame, chunks[0]);

        // Render details panel
        self.render_details(frame, chunks[1], season_desc, total_plants, total_lines);
    }

    /// Render the tree view
    fn render_tree(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let is_selected = i == self.selected;

                match item {
                    GardenItem::Bed {
                        name,
                        plant_count,
                        health,
                        expanded,
                        path,
                    } => {
                        let expand_icon = if *expanded { "▼" } else { "▶" };
                        let health_icon = if *health > 0.8 {
                            "🌸"
                        } else if *health > 0.5 {
                            "🌿"
                        } else {
                            "🥀"
                        };

                        let growth_anim = if self.has_recent_changes(path) {
                            format!(" {}", self.growth_char())
                        } else {
                            String::new()
                        };

                        let style = if is_selected {
                            TuiPalette::selected_style()
                        } else {
                            Style::default().fg(TuiPalette::AMBER)
                        };

                        ListItem::new(Line::from(vec![
                            Span::styled(format!("{} ", expand_icon), TuiPalette::muted_style()),
                            Span::styled(format!("{} ", health_icon), style),
                            Span::styled(name, style.add_modifier(Modifier::BOLD)),
                            Span::styled(
                                format!(" ({} plants){}", plant_count, growth_anim),
                                TuiPalette::muted_style(),
                            ),
                        ]))
                    }
                    GardenItem::Plant { plant, bed_path } => {
                        let indent = "    ";
                        let type_icon = match plant.plant_type {
                            PlantType::Flower => "🌺",
                            PlantType::Herb => "🌿",
                            PlantType::Vegetable => "🥬",
                            PlantType::Fruit => "🍎",
                            PlantType::Pollinator => "🐝",
                            PlantType::Roots => "🥕",
                            PlantType::Trellis => "🏗️",
                        };

                        let stage_color = match plant.growth_stage {
                            GrowthStage::Seedling => TuiPalette::SAGE,
                            GrowthStage::Sprout => TuiPalette::GARDEN_GREEN,
                            GrowthStage::Established => TuiPalette::BLOOM,
                            GrowthStage::Mature => TuiPalette::AMBER,
                            GrowthStage::Ancient => TuiPalette::COPPER,
                            GrowthStage::Wilting => TuiPalette::FROST,
                        };

                        let growth_anim = if self.has_recent_changes(&plant.path)
                            || self.has_recent_changes(bed_path)
                        {
                            format!(" {}", self.growth_char())
                        } else {
                            String::new()
                        };

                        let style = if is_selected {
                            TuiPalette::selected_style()
                        } else {
                            Style::default().fg(stage_color)
                        };

                        ListItem::new(Line::from(vec![
                            Span::raw(indent),
                            Span::styled(format!("{} ", type_icon), style),
                            Span::styled(&plant.name, style),
                            Span::styled(
                                format!(" {} lines{}", plant.lines, growth_anim),
                                TuiPalette::muted_style(),
                            ),
                        ]))
                    }
                }
            })
            .collect();

        let list = List::new(items).highlight_style(TuiPalette::selected_style());

        frame.render_stateful_widget(list, area, &mut self.list_state);

        // Render scrollbar if needed
        if self.items.len() > area.height as usize {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            let mut scrollbar_state = ScrollbarState::new(self.items.len()).position(self.selected);
            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }

    /// Render the details panel
    fn render_details(
        &self,
        frame: &mut Frame,
        area: Rect,
        season: &str,
        total_plants: usize,
        total_lines: usize,
    ) {
        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(TuiPalette::border_style())
            .title(Span::styled(" Details ", TuiPalette::muted_style()));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let details = match self.selected_item() {
            Some(GardenItem::Bed {
                name,
                path,
                plant_count,
                health,
                ..
            }) => {
                let health_bar = self.render_health_bar(*health, 15);
                let health_pct = format!("{:.0}%", health * 100.0);

                vec![
                    Line::from(vec![
                        Span::styled("📁 Bed: ", TuiPalette::muted_style()),
                        Span::styled(name, TuiPalette::title_style()),
                    ]),
                    Line::from(""),
                    Line::from(vec![Span::styled("Path: ", TuiPalette::muted_style())]),
                    Line::from(Span::styled(path, TuiPalette::path_style())),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Plants: ", TuiPalette::muted_style()),
                        Span::styled(
                            plant_count.to_string(),
                            Style::default().fg(TuiPalette::AMBER),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Health: ", TuiPalette::muted_style()),
                        Span::styled(
                            health_pct,
                            if *health > 0.7 {
                                TuiPalette::success_style()
                            } else if *health > 0.4 {
                                TuiPalette::warning_style()
                            } else {
                                TuiPalette::error_style()
                            },
                        ),
                    ]),
                    Line::from(Span::raw(health_bar)),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            "Enter",
                            Style::default()
                                .fg(TuiPalette::SAGE)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" to expand/collapse", TuiPalette::muted_style()),
                    ]),
                ]
            }
            Some(GardenItem::Plant { plant, .. }) => {
                let stage_desc = plant.growth_stage.description();
                let type_desc = plant.plant_type.description();
                let age_str = if plant.age_days == 1 {
                    "1 day".to_string()
                } else {
                    format!("{} days", plant.age_days)
                };
                let tended_str = if plant.last_tended_days == 0 {
                    "today".to_string()
                } else if plant.last_tended_days == 1 {
                    "yesterday".to_string()
                } else {
                    format!("{} days ago", plant.last_tended_days)
                };

                vec![
                    Line::from(vec![
                        Span::styled("📄 Plant: ", TuiPalette::muted_style()),
                        Span::styled(&plant.name, TuiPalette::title_style()),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Type: ", TuiPalette::muted_style()),
                        Span::styled(type_desc, Style::default().fg(TuiPalette::COPPER)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Stage: ", TuiPalette::muted_style()),
                        Span::styled(
                            format!("{} {}", plant.growth_stage.glyph(), stage_desc),
                            match plant.growth_stage {
                                GrowthStage::Wilting => TuiPalette::warning_style(),
                                GrowthStage::Ancient => Style::default().fg(TuiPalette::COPPER),
                                _ => TuiPalette::success_style(),
                            },
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Lines: ", TuiPalette::muted_style()),
                        Span::styled(
                            plant.lines.to_string(),
                            Style::default().fg(TuiPalette::AMBER),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Age: ", TuiPalette::muted_style()),
                        Span::styled(age_str, TuiPalette::muted_style()),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Last tended: ", TuiPalette::muted_style()),
                        Span::styled(
                            tended_str,
                            if plant.last_tended_days > 30 {
                                TuiPalette::warning_style()
                            } else {
                                TuiPalette::muted_style()
                            },
                        ),
                    ]),
                ]
            }
            None => {
                // Show garden summary
                vec![
                    Line::from(Span::styled("🌳 Garden Summary", TuiPalette::title_style())),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Season: ", TuiPalette::muted_style()),
                        Span::styled(season, Style::default().fg(TuiPalette::AMBER)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Total plants: ", TuiPalette::muted_style()),
                        Span::styled(
                            total_plants.to_string(),
                            Style::default().fg(TuiPalette::GARDEN_GREEN),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Total lines: ", TuiPalette::muted_style()),
                        Span::styled(
                            total_lines.to_string(),
                            Style::default().fg(TuiPalette::BLOOM),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled("Navigate with ↑↓", TuiPalette::muted_style())),
                    Line::from(Span::styled("Enter to expand", TuiPalette::muted_style())),
                ]
            }
        };

        let paragraph = Paragraph::new(details);
        frame.render_widget(paragraph, inner);
    }

    /// Render a health bar
    fn render_health_bar(&self, health: f32, width: usize) -> String {
        let filled = ((health * width as f32) as usize).min(width);
        let empty = width - filled;
        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    }
}

impl Default for GardenView {
    fn default() -> Self {
        Self::new()
    }
}

/// Render the garden view pane (standalone function for integration)
pub fn render_garden_view(frame: &mut Frame, area: Rect, view: &mut GardenView, focused: bool) {
    view.set_focused(focused);
    view.tick();
    view.render(frame, area);
}

#[cfg(test)]
mod tests {
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
}
