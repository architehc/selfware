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

    /// Get reference to garden data
    pub fn garden(&self) -> Option<&DigitalGarden> {
        self.garden.as_ref()
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
            beds.sort_by_key(|x| std::cmp::Reverse(x.total_lines));

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
                plants.sort_by_key(|x| std::cmp::Reverse(x.lines));

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

    /// Return the relative path of the currently selected bed or plant.
    pub fn selected_path(&self) -> Option<&str> {
        match self.selected_item()? {
            GardenItem::Bed { path, .. } => Some(path.as_str()),
            GardenItem::Plant { plant, .. } => Some(plant.path.as_str()),
        }
    }

    /// Expand the currently selected item if it is a collapsed bed.
    pub fn expand_selected(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let selected = self.selected.min(self.items.len() - 1);
        if let GardenItem::Bed {
            path,
            expanded: false,
            ..
        } = &self.items[selected]
        {
            let path = path.clone();
            self.expand_bed(selected, &path);
        }
    }

    /// Collapse the currently selected bed, or jump to and collapse the parent
    /// bed if a plant is currently selected.
    pub fn collapse_selected(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let selected = self.selected.min(self.items.len() - 1);
        match &self.items[selected] {
            GardenItem::Bed { expanded: true, .. } => {
                self.collapse_bed(selected);
            }
            GardenItem::Plant { .. } => {
                // Find the parent bed above `selected`
                for idx in (0..selected).rev() {
                    if self.items[idx].is_bed() {
                        self.selected = idx;
                        self.list_state.select(Some(idx));
                        self.collapse_bed(idx);
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    /// Toggle all beds between expanded and collapsed.
    pub fn toggle_expand_all(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let any_collapsed = self.items.iter().any(|item| {
            matches!(
                item,
                GardenItem::Bed {
                    expanded: false,
                    ..
                }
            )
        });
        if any_collapsed {
            let mut idx = 0;
            while idx < self.items.len() {
                if let GardenItem::Bed {
                    path,
                    expanded: false,
                    ..
                } = &self.items[idx]
                {
                    let path = path.clone();
                    self.expand_bed(idx, &path);
                }
                idx += 1;
            }
        } else {
            self.rebuild_items();
            self.selected = 0;
            self.list_state.select(Some(0));
        }
    }

    /// Generate a formatted text report of all garden beds and codebase health
    /// for `/garden status` in TUI Chat.
    pub fn summary_report(&self) -> String {
        let Some(garden) = &self.garden else {
            return "🌱 Garden View: no workspace scan available yet. Run `/garden rescan`."
                .to_string();
        };
        let mut beds: Vec<_> = garden.beds.values().collect();
        beds.sort_by_key(|b| std::cmp::Reverse(b.total_lines));

        let overall_pct = garden
            .tended_fraction()
            .map(|h| format!("{:.0}%", h * 100.0))
            .unwrap_or_else(|| "n/a".to_string());

        let mut out = format!(
            "🌳 Digital Garden Status (`{}`):\n  \
             • Season: {} {}\n  \
             • Total:  {} files ({} lines) across {} beds · {} active (changed ≤90d)\n  \
             • Top Garden Beds (Directories):\n",
            garden.project_name,
            garden.season.glyph(),
            garden.season.description(),
            garden.total_plants,
            garden.total_lines,
            beds.len(),
            overall_pct,
        );

        for bed in beds.iter().take(10) {
            let icon = if bed.health_score > 0.8 {
                "🌸"
            } else if bed.health_score > 0.5 {
                "🌿"
            } else {
                "🥀"
            };
            out.push_str(&format!(
                "    {} {:<22} {:>4} files · {:>7} lines · {:>3.0}% active\n",
                icon,
                bed.path,
                bed.plants.len(),
                bed.total_lines,
                bed.health_score * 100.0
            ));
        }
        out.push_str(
            "  • Tip: Press `Ctrl+G` (or run `/garden`) to open the interactive Garden Explorer (`↑/↓/j/k` move, `Enter` expand, `a` expand all, `i` inspect in chat).",
        );
        out
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

    /// Render the garden view (adapts automatically between compact dashboard
    /// widget and full-screen / zoomed interactive explorer).
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if area.width < 10 || area.height < 3 {
            return;
        }
        frame.render_widget(ratatui::widgets::Clear, area);

        let border_style = if self.focused {
            TuiPalette::title_style()
        } else {
            TuiPalette::border_style()
        };

        // Get season and stats from garden
        let (season_glyph, season_desc, total_plants, total_lines, tended_pct) = self
            .garden
            .as_ref()
            .map(|g| {
                let pct = g
                    .tended_fraction()
                    .map(|h| (h * 100.0).round() as u8)
                    .unwrap_or(0);
                (
                    g.season.glyph(),
                    g.season.description(),
                    g.total_plants,
                    g.total_lines,
                    pct,
                )
            })
            .unwrap_or(("🌱", "unknown", 0, 0, 0));

        let title = if self.focused {
            format!(
                " 🌳 Garden View {} [FOCUSED · ↑↓/jk · Enter expand · a all · i inspect · Esc chat] ",
                season_glyph
            )
        } else {
            format!(
                " 🌳 Garden View {} ({} files · Ctrl+G or /garden to explore) ",
                season_glyph, total_plants
            )
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title, TuiPalette::title_style()));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if self.items.is_empty() {
            let empty = Paragraph::new(
                "  🌱 No garden data scanned yet.\n  Press [r] or run `/garden rescan` in chat.",
            )
            .style(TuiPalette::muted_style());
            frame.render_widget(empty, inner);
            return;
        }

        // Compact mode when embedded in the right-hand Dashboard column (`width < 68` or `height < 12`):
        // avoid a 70/30 horizontal split that crushes the Details panel into ~10 columns.
        if inner.width < 68 || inner.height < 12 {
            let has_footer = inner.height >= 6;
            let constraints = if has_footer {
                vec![
                    Constraint::Length(1),
                    Constraint::Min(2),
                    Constraint::Length(2),
                ]
            } else {
                vec![Constraint::Length(1), Constraint::Min(1)]
            };
            let vchunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(inner);

            let header_line = Line::from(vec![
                Span::styled(
                    format!(" {} {} files", season_glyph, total_plants),
                    Style::default()
                        .fg(TuiPalette::AMBER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {} lines · {}% ≤90d", total_lines, tended_pct),
                    TuiPalette::muted_style(),
                ),
            ]);
            frame.render_widget(Paragraph::new(header_line), vchunks[0]);

            self.render_tree(frame, vchunks[1]);

            if has_footer {
                let selected_summary = match self.selected_item() {
                    Some(GardenItem::Bed {
                        path,
                        plant_count,
                        health,
                        expanded,
                        ..
                    }) => format!(
                        " 📁 {} ({} files, {:.0}% ≤90d) — {}",
                        path,
                        plant_count,
                        health * 100.0,
                        if *expanded {
                            "Enter: collapse"
                        } else {
                            "Enter: expand"
                        }
                    ),
                    Some(GardenItem::Plant { plant, .. }) => format!(
                        " 📄 {} ({} lines, {} {})",
                        plant.path,
                        plant.lines,
                        plant.growth_stage.glyph(),
                        plant.growth_stage.description()
                    ),
                    None => " Select a bed or file".to_string(),
                };
                let footer_lines = vec![
                    Line::from(Span::styled(
                        selected_summary,
                        Style::default().fg(TuiPalette::BLOOM),
                    )),
                    Line::from(Span::styled(
                        " Keys: Ctrl+G zoom · Tab focus · ↑↓/jk move · Enter expand · a all · i inspect",
                        TuiPalette::muted_style(),
                    )),
                ];
                frame.render_widget(Paragraph::new(footer_lines), vchunks[2]);
            }
            return;
        }

        // Full / Zoomed mode (`inner.width >= 68 && inner.height >= 12`):
        // Split vertically into main body (Tree 58% | Details 42%) + 2-row interactive key bar.
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(2)])
            .split(inner);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(vchunks[0]);

        // Render tree view
        self.render_tree(frame, chunks[0]);

        // Render details panel
        self.render_details(frame, chunks[1], season_desc, total_plants, total_lines);

        // Render interactive key legend at the bottom
        let footer = Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    " 🧭 Garden Controls: ",
                    Style::default()
                        .fg(TuiPalette::AMBER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("[↑/↓/j/k]", Style::default().fg(TuiPalette::BLOOM)),
                Span::raw(" Navigate  "),
                Span::styled("[Enter/Space]", Style::default().fg(TuiPalette::BLOOM)),
                Span::raw(" Expand/Collapse Bed  "),
                Span::styled("[a]", Style::default().fg(TuiPalette::BLOOM)),
                Span::raw(" Expand/Collapse All  "),
                Span::styled("[r]", Style::default().fg(TuiPalette::BLOOM)),
                Span::raw(" Rescan  "),
                Span::styled("[i]", Style::default().fg(TuiPalette::BLOOM)),
                Span::raw(" Inspect in Chat  "),
                Span::styled("[Ctrl+G / Esc]", Style::default().fg(TuiPalette::AMBER)),
                Span::raw(" Return to Chat"),
            ]),
            Line::from(Span::styled(
                format!(
                    " 📊 Workspace: {} files · {} total lines · {}% changed in last 90 days · Season: {} {}",
                    total_plants, total_lines, tended_pct, season_glyph, season_desc
                ),
                TuiPalette::muted_style(),
            )),
        ]);
        frame.render_widget(footer, vchunks[1]);
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
                                format!(
                                    " ({} files · {:.0}% ≤90d){}",
                                    plant_count,
                                    health * 100.0,
                                    growth_anim
                                ),
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
                        Span::styled("Changed ≤90 days: ", TuiPalette::muted_style()),
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
                        Span::styled(" expand/collapse · ", TuiPalette::muted_style()),
                        Span::styled(
                            "a",
                            Style::default()
                                .fg(TuiPalette::SAGE)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" all · ", TuiPalette::muted_style()),
                        Span::styled(
                            "i",
                            Style::default()
                                .fg(TuiPalette::SAGE)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" inspect", TuiPalette::muted_style()),
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
                    Line::from(Span::styled(&plant.path, TuiPalette::path_style())),
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
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            "Press [i]",
                            Style::default()
                                .fg(TuiPalette::SAGE)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" to analyze in Chat", TuiPalette::muted_style()),
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

        let paragraph = Paragraph::new(details).wrap(ratatui::widgets::Wrap { trim: false });
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
#[path = "../../../tests/unit/ui/tui/garden_view/garden_view_test.rs"]
mod tests;
