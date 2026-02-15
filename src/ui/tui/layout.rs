//! Adaptive Layout Engine
//!
//! Tiling window manager for terminal UI with multiple pane types,
//! presets, and flexible resizing.

// Feature-gated module - dead_code lint disabled at crate level

use ratatui::layout::Rect;
use std::collections::HashMap;

/// Unique identifier for panes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(pub u32);

impl PaneId {
    /// Create a new pane ID
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

/// Types of panes available
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneType {
    /// Chat/conversation pane
    Chat,
    /// Code editor pane
    Editor,
    /// Terminal/command output pane
    Terminal,
    /// File explorer pane
    Explorer,
    /// Diff viewer pane
    Diff,
    /// Debug/logs pane
    Debug,
    /// Help/documentation pane
    Help,
    /// Status bar widget (model, tokens, time)
    StatusBar,
    /// Garden health widget
    GardenHealth,
    /// Active tools widget
    ActiveTools,
    /// Log output widget
    Logs,
    /// Full interactive garden view with tree navigation
    GardenView,
}

impl PaneType {
    /// Get the icon for this pane type
    pub fn icon(&self) -> &'static str {
        match self {
            PaneType::Chat => "💬",
            PaneType::Editor => "📝",
            PaneType::Terminal => "🖥️",
            PaneType::Explorer => "📁",
            PaneType::Diff => "📊",
            PaneType::Debug => "🔍",
            PaneType::Help => "❓",
            PaneType::StatusBar => "⚙️",
            PaneType::GardenHealth => "🌱",
            PaneType::ActiveTools => "🔧",
            PaneType::Logs => "📜",
            PaneType::GardenView => "🌳",
        }
    }

    /// Get the title for this pane type
    pub fn title(&self) -> &'static str {
        match self {
            PaneType::Chat => "Chat",
            PaneType::Editor => "Editor",
            PaneType::Terminal => "Terminal",
            PaneType::Explorer => "Explorer",
            PaneType::Diff => "Diff",
            PaneType::Debug => "Debug",
            PaneType::Help => "Help",
            PaneType::StatusBar => "Status",
            PaneType::GardenHealth => "Garden Health",
            PaneType::ActiveTools => "Active Tools",
            PaneType::Logs => "Logs",
            PaneType::GardenView => "Garden View",
        }
    }
}

/// A pane in the layout
#[derive(Debug, Clone)]
pub struct Pane {
    /// Unique identifier
    pub id: PaneId,
    /// Type of pane
    pub pane_type: PaneType,
    /// Whether this pane is focused
    pub focused: bool,
    /// Whether this pane is visible
    pub visible: bool,
    /// Custom title (overrides default)
    pub custom_title: Option<String>,
}

impl Pane {
    /// Create a new pane
    pub fn new(id: PaneId, pane_type: PaneType) -> Self {
        Self {
            id,
            pane_type,
            focused: false,
            visible: true,
            custom_title: None,
        }
    }

    /// Get the display title
    pub fn title(&self) -> String {
        self.custom_title
            .clone()
            .unwrap_or_else(|| self.pane_type.title().to_string())
    }

    /// Set a custom title
    pub fn with_title(mut self, title: &str) -> Self {
        self.custom_title = Some(title.to_string());
        self
    }
}

/// Layout presets for common workflows
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPreset {
    /// Single chat pane (full screen)
    Focus,
    /// Chat on left, editor on right [30% | 70%]
    Coding,
    /// Chat on left, code center, terminal bottom [25% | 50% | 25%]
    Debugging,
    /// Diff view (full screen)
    Review,
    /// Chat with file explorer sidebar [20% | 80%]
    Explore,
    /// Three-column: explorer, editor, chat [20% | 50% | 30%]
    FullWorkspace,
    /// Dashboard: status bar, chat, garden health, active tools, logs
    Dashboard,
}

impl LayoutPreset {
    /// Get description of this preset
    pub fn description(&self) -> &'static str {
        match self {
            LayoutPreset::Focus => "Full-screen chat (distraction-free)",
            LayoutPreset::Coding => "Chat + Editor side-by-side",
            LayoutPreset::Debugging => "Chat + Code + Terminal",
            LayoutPreset::Review => "Full-screen diff view",
            LayoutPreset::Explore => "Chat with file explorer",
            LayoutPreset::FullWorkspace => "Explorer + Editor + Chat",
            LayoutPreset::Dashboard => "Dashboard with status, garden, tools",
        }
    }

    /// Get the keyboard shortcut for this preset
    pub fn shortcut(&self) -> &'static str {
        match self {
            LayoutPreset::Focus => "Alt+1",
            LayoutPreset::Coding => "Alt+2",
            LayoutPreset::Debugging => "Alt+3",
            LayoutPreset::Review => "Alt+4",
            LayoutPreset::Explore => "Alt+5",
            LayoutPreset::FullWorkspace => "Alt+6",
            LayoutPreset::Dashboard => "Alt+d",
        }
    }
}

/// Split direction for layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Node in the layout tree
#[derive(Debug, Clone)]
pub enum LayoutNode {
    /// A leaf node containing a pane
    Pane(PaneId),
    /// A split containing two children
    Split {
        direction: SplitDirection,
        /// Ratio of first child (0.0 to 1.0)
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

/// The main layout engine
pub struct LayoutEngine {
    /// All panes in the layout
    panes: HashMap<PaneId, Pane>,
    /// The root of the layout tree
    root: Option<LayoutNode>,
    /// Currently focused pane
    focused_pane: Option<PaneId>,
    /// Next pane ID to assign
    next_id: u32,
    /// Current layout preset
    current_preset: LayoutPreset,
    /// Zoomed pane (if any)
    zoomed_pane: Option<PaneId>,
}

impl LayoutEngine {
    /// Create a new layout engine with default single-pane layout
    pub fn new() -> Self {
        let mut engine = Self {
            panes: HashMap::new(),
            root: None,
            focused_pane: None,
            next_id: 1,
            current_preset: LayoutPreset::Focus,
            zoomed_pane: None,
        };

        // Create initial chat pane
        let chat_id = engine.create_pane(PaneType::Chat);
        engine.root = Some(LayoutNode::Pane(chat_id));
        engine.focused_pane = Some(chat_id);

        engine
    }

    /// Create a new pane and return its ID
    pub fn create_pane(&mut self, pane_type: PaneType) -> PaneId {
        let id = PaneId::new(self.next_id);
        self.next_id += 1;

        let pane = Pane::new(id, pane_type);
        self.panes.insert(id, pane);

        id
    }

    /// Get a pane by ID
    pub fn get_pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.get(&id)
    }

    /// Get a mutable pane by ID
    pub fn get_pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.get_mut(&id)
    }

    /// Get the focused pane
    pub fn focused(&self) -> Option<PaneId> {
        self.focused_pane
    }

    /// Set focus to a pane
    pub fn set_focus(&mut self, id: PaneId) {
        if let Some(old_id) = self.focused_pane {
            if let Some(pane) = self.panes.get_mut(&old_id) {
                pane.focused = false;
            }
        }

        if let Some(pane) = self.panes.get_mut(&id) {
            pane.focused = true;
            self.focused_pane = Some(id);
        }
    }

    /// Toggle zoom on the focused pane
    pub fn toggle_zoom(&mut self) {
        if self.zoomed_pane.is_some() {
            self.zoomed_pane = None;
        } else {
            self.zoomed_pane = self.focused_pane;
        }
    }

    /// Check if a pane is zoomed
    pub fn is_zoomed(&self) -> bool {
        self.zoomed_pane.is_some()
    }

    /// Apply a layout preset
    pub fn apply_preset(&mut self, preset: LayoutPreset) {
        self.current_preset = preset;
        self.panes.clear();
        self.zoomed_pane = None;

        match preset {
            LayoutPreset::Focus => {
                let chat_id = self.create_pane(PaneType::Chat);
                self.root = Some(LayoutNode::Pane(chat_id));
                self.focused_pane = Some(chat_id);
            }
            LayoutPreset::Coding => {
                let chat_id = self.create_pane(PaneType::Chat);
                let editor_id = self.create_pane(PaneType::Editor);

                self.root = Some(LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.3,
                    first: Box::new(LayoutNode::Pane(chat_id)),
                    second: Box::new(LayoutNode::Pane(editor_id)),
                });
                self.focused_pane = Some(editor_id);
            }
            LayoutPreset::Debugging => {
                let chat_id = self.create_pane(PaneType::Chat);
                let editor_id = self.create_pane(PaneType::Editor);
                let terminal_id = self.create_pane(PaneType::Terminal);

                let right_split = LayoutNode::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.7,
                    first: Box::new(LayoutNode::Pane(editor_id)),
                    second: Box::new(LayoutNode::Pane(terminal_id)),
                };

                self.root = Some(LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.25,
                    first: Box::new(LayoutNode::Pane(chat_id)),
                    second: Box::new(right_split),
                });
                self.focused_pane = Some(editor_id);
            }
            LayoutPreset::Review => {
                let diff_id = self.create_pane(PaneType::Diff);
                self.root = Some(LayoutNode::Pane(diff_id));
                self.focused_pane = Some(diff_id);
            }
            LayoutPreset::Explore => {
                let explorer_id = self.create_pane(PaneType::Explorer);
                let chat_id = self.create_pane(PaneType::Chat);

                self.root = Some(LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.2,
                    first: Box::new(LayoutNode::Pane(explorer_id)),
                    second: Box::new(LayoutNode::Pane(chat_id)),
                });
                self.focused_pane = Some(chat_id);
            }
            LayoutPreset::FullWorkspace => {
                let explorer_id = self.create_pane(PaneType::Explorer);
                let editor_id = self.create_pane(PaneType::Editor);
                let chat_id = self.create_pane(PaneType::Chat);

                let right_split = LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.6,
                    first: Box::new(LayoutNode::Pane(editor_id)),
                    second: Box::new(LayoutNode::Pane(chat_id)),
                };

                self.root = Some(LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.2,
                    first: Box::new(LayoutNode::Pane(explorer_id)),
                    second: Box::new(right_split),
                });
                self.focused_pane = Some(editor_id);
            }
            LayoutPreset::Dashboard => {
                // Dashboard layout:
                // ┌──────────────────────────────────────────────────────┐
                // │ [Status Bar] Model: xxx | Tokens: 45K | ⏱ 2h 34m    │
                // ├─────────────────────────┬────────────────────────────┤
                // │   Chat/Output Pane      │   Garden Health Widget     │
                // │   (60%)                 │   ████████░░ 82%           │
                // │                         ├────────────────────────────┤
                // │                         │   Active Tools Widget      │
                // │                         │   🔧 file_read ●●●○○       │
                // ├─────────────────────────┴────────────────────────────┤
                // │   Logs (compact)                                     │
                // └──────────────────────────────────────────────────────┘

                let status_id = self.create_pane(PaneType::StatusBar);
                let chat_id = self.create_pane(PaneType::Chat);
                let garden_id = self.create_pane(PaneType::GardenView);
                let tools_id = self.create_pane(PaneType::ActiveTools);
                let logs_id = self.create_pane(PaneType::Logs);

                // Right column: garden health on top, active tools below
                let right_widgets = LayoutNode::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                    first: Box::new(LayoutNode::Pane(garden_id)),
                    second: Box::new(LayoutNode::Pane(tools_id)),
                };

                // Middle row: chat on left (60%), widgets on right (40%)
                let middle_row = LayoutNode::Split {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.6,
                    first: Box::new(LayoutNode::Pane(chat_id)),
                    second: Box::new(right_widgets),
                };

                // Main body: middle row on top (85%), logs at bottom (15%)
                let main_body = LayoutNode::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.85,
                    first: Box::new(middle_row),
                    second: Box::new(LayoutNode::Pane(logs_id)),
                };

                // Full layout: status bar on top (fixed ~3 lines), main body below
                self.root = Some(LayoutNode::Split {
                    direction: SplitDirection::Vertical,
                    ratio: 0.05, // Status bar takes ~5% of height
                    first: Box::new(LayoutNode::Pane(status_id)),
                    second: Box::new(main_body),
                });
                self.focused_pane = Some(chat_id);
            }
        }
    }

    /// Get the current preset
    pub fn current_preset(&self) -> LayoutPreset {
        self.current_preset
    }

    /// Calculate layout rectangles for all panes
    pub fn calculate_layout(&self, area: Rect) -> HashMap<PaneId, Rect> {
        let mut result = HashMap::new();

        // If zoomed, only show the zoomed pane
        if let Some(zoomed_id) = self.zoomed_pane {
            result.insert(zoomed_id, area);
            return result;
        }

        if let Some(ref root) = self.root {
            self.calculate_node_layout(root, area, &mut result);
        }

        result
    }

    #[allow(clippy::only_used_in_recursion)]
    fn calculate_node_layout(
        &self,
        node: &LayoutNode,
        area: Rect,
        result: &mut HashMap<PaneId, Rect>,
    ) {
        match node {
            LayoutNode::Pane(id) => {
                result.insert(*id, area);
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_area, second_area) = match direction {
                    SplitDirection::Horizontal => {
                        let first_width = (area.width as f32 * ratio) as u16;
                        let second_width = area.width.saturating_sub(first_width);

                        let first_rect = Rect::new(area.x, area.y, first_width, area.height);
                        let second_rect =
                            Rect::new(area.x + first_width, area.y, second_width, area.height);
                        (first_rect, second_rect)
                    }
                    SplitDirection::Vertical => {
                        let first_height = (area.height as f32 * ratio) as u16;
                        let second_height = area.height.saturating_sub(first_height);

                        let first_rect = Rect::new(area.x, area.y, area.width, first_height);
                        let second_rect =
                            Rect::new(area.x, area.y + first_height, area.width, second_height);
                        (first_rect, second_rect)
                    }
                };

                self.calculate_node_layout(first, first_area, result);
                self.calculate_node_layout(second, second_area, result);
            }
        }
    }

    /// Get all pane IDs
    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.panes.keys().copied().collect()
    }

    /// Focus next pane
    pub fn focus_next(&mut self) {
        let ids: Vec<_> = self.panes.keys().copied().collect();
        if ids.is_empty() {
            return;
        }

        let current_idx = self
            .focused_pane
            .and_then(|id| ids.iter().position(|&i| i == id))
            .unwrap_or(0);

        let next_idx = (current_idx + 1) % ids.len();
        self.set_focus(ids[next_idx]);
    }

    /// Focus previous pane
    pub fn focus_prev(&mut self) {
        let ids: Vec<_> = self.panes.keys().copied().collect();
        if ids.is_empty() {
            return;
        }

        let current_idx = self
            .focused_pane
            .and_then(|id| ids.iter().position(|&i| i == id))
            .unwrap_or(0);

        let prev_idx = if current_idx == 0 {
            ids.len() - 1
        } else {
            current_idx - 1
        };
        self.set_focus(ids[prev_idx]);
    }
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pane_id_creation() {
        let id = PaneId::new(42);
        assert_eq!(id.0, 42);
    }

    #[test]
    fn test_pane_type_icon() {
        assert_eq!(PaneType::Chat.icon(), "💬");
        assert_eq!(PaneType::Editor.icon(), "📝");
        assert_eq!(PaneType::Terminal.icon(), "🖥️");
        assert_eq!(PaneType::Explorer.icon(), "📁");
        assert_eq!(PaneType::Diff.icon(), "📊");
        assert_eq!(PaneType::Debug.icon(), "🔍");
        assert_eq!(PaneType::Help.icon(), "❓");
    }

    #[test]
    fn test_pane_type_title() {
        assert_eq!(PaneType::Chat.title(), "Chat");
        assert_eq!(PaneType::Editor.title(), "Editor");
        assert_eq!(PaneType::Terminal.title(), "Terminal");
    }

    #[test]
    fn test_pane_creation() {
        let pane = Pane::new(PaneId::new(1), PaneType::Chat);
        assert_eq!(pane.id.0, 1);
        assert_eq!(pane.pane_type, PaneType::Chat);
        assert!(!pane.focused);
        assert!(pane.visible);
    }

    #[test]
    fn test_pane_title() {
        let pane = Pane::new(PaneId::new(1), PaneType::Chat);
        assert_eq!(pane.title(), "Chat");

        let pane_with_title = pane.with_title("My Chat");
        assert_eq!(pane_with_title.title(), "My Chat");
    }

    #[test]
    fn test_layout_preset_description() {
        assert!(!LayoutPreset::Focus.description().is_empty());
        assert!(!LayoutPreset::Coding.description().is_empty());
        assert!(!LayoutPreset::Debugging.description().is_empty());
    }

    #[test]
    fn test_layout_preset_shortcut() {
        assert_eq!(LayoutPreset::Focus.shortcut(), "Alt+1");
        assert_eq!(LayoutPreset::Coding.shortcut(), "Alt+2");
    }

    #[test]
    fn test_layout_engine_creation() {
        let engine = LayoutEngine::new();
        assert_eq!(engine.current_preset(), LayoutPreset::Focus);
        assert!(engine.focused().is_some());
    }

    #[test]
    fn test_layout_engine_default() {
        let engine = LayoutEngine::default();
        assert!(engine.focused().is_some());
    }

    #[test]
    fn test_create_pane() {
        let mut engine = LayoutEngine::new();
        let id = engine.create_pane(PaneType::Editor);
        assert!(engine.get_pane(id).is_some());
    }

    #[test]
    fn test_set_focus() {
        let mut engine = LayoutEngine::new();
        let id = engine.create_pane(PaneType::Editor);
        engine.set_focus(id);
        assert_eq!(engine.focused(), Some(id));
    }

    #[test]
    fn test_toggle_zoom() {
        let mut engine = LayoutEngine::new();
        assert!(!engine.is_zoomed());

        engine.toggle_zoom();
        assert!(engine.is_zoomed());

        engine.toggle_zoom();
        assert!(!engine.is_zoomed());
    }

    #[test]
    fn test_apply_preset_focus() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Focus);
        assert_eq!(engine.current_preset(), LayoutPreset::Focus);
        assert_eq!(engine.pane_ids().len(), 1);
    }

    #[test]
    fn test_apply_preset_coding() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Coding);
        assert_eq!(engine.current_preset(), LayoutPreset::Coding);
        assert_eq!(engine.pane_ids().len(), 2);
    }

    #[test]
    fn test_apply_preset_debugging() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Debugging);
        assert_eq!(engine.current_preset(), LayoutPreset::Debugging);
        assert_eq!(engine.pane_ids().len(), 3);
    }

    #[test]
    fn test_apply_preset_review() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Review);
        assert_eq!(engine.current_preset(), LayoutPreset::Review);
        assert_eq!(engine.pane_ids().len(), 1);
    }

    #[test]
    fn test_apply_preset_explore() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Explore);
        assert_eq!(engine.current_preset(), LayoutPreset::Explore);
        assert_eq!(engine.pane_ids().len(), 2);
    }

    #[test]
    fn test_apply_preset_full_workspace() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::FullWorkspace);
        assert_eq!(engine.current_preset(), LayoutPreset::FullWorkspace);
        assert_eq!(engine.pane_ids().len(), 3);
    }

    #[test]
    fn test_calculate_layout_single_pane() {
        let engine = LayoutEngine::new();
        let area = Rect::new(0, 0, 100, 50);
        let layouts = engine.calculate_layout(area);
        assert_eq!(layouts.len(), 1);
    }

    #[test]
    fn test_calculate_layout_coding() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Coding);
        let area = Rect::new(0, 0, 100, 50);
        let layouts = engine.calculate_layout(area);
        assert_eq!(layouts.len(), 2);
    }

    #[test]
    fn test_calculate_layout_zoomed() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Coding);
        engine.toggle_zoom();

        let area = Rect::new(0, 0, 100, 50);
        let layouts = engine.calculate_layout(area);
        assert_eq!(layouts.len(), 1); // Only zoomed pane visible
    }

    #[test]
    fn test_focus_next() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Coding);

        let first_focus = engine.focused();
        engine.focus_next();
        let second_focus = engine.focused();

        assert_ne!(first_focus, second_focus);
    }

    #[test]
    fn test_focus_prev() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Coding);

        let first_focus = engine.focused();
        engine.focus_prev();
        let second_focus = engine.focused();

        assert_ne!(first_focus, second_focus);
    }

    #[test]
    fn test_get_pane_mut() {
        let mut engine = LayoutEngine::new();
        let id = engine.create_pane(PaneType::Editor);

        if let Some(pane) = engine.get_pane_mut(id) {
            pane.visible = false;
        }

        assert!(!engine.get_pane(id).unwrap().visible);
    }

    #[test]
    fn test_pane_ids() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Debugging);

        let ids = engine.pane_ids();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_split_direction() {
        let h = SplitDirection::Horizontal;
        let v = SplitDirection::Vertical;
        assert_ne!(h, v);
    }

    #[test]
    fn test_apply_preset_dashboard() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Dashboard);
        assert_eq!(engine.current_preset(), LayoutPreset::Dashboard);
        // Dashboard has: StatusBar, Chat, GardenView, ActiveTools, Logs = 5 panes
        assert_eq!(engine.pane_ids().len(), 5);
    }

    #[test]
    fn test_dashboard_pane_types() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Dashboard);

        let pane_types: Vec<_> = engine
            .pane_ids()
            .iter()
            .filter_map(|id| engine.get_pane(*id))
            .map(|p| p.pane_type)
            .collect();

        assert!(pane_types.contains(&PaneType::StatusBar));
        assert!(pane_types.contains(&PaneType::Chat));
        assert!(pane_types.contains(&PaneType::GardenView));
        assert!(pane_types.contains(&PaneType::ActiveTools));
        assert!(pane_types.contains(&PaneType::Logs));
    }

    #[test]
    fn test_dashboard_pane_icons() {
        assert_eq!(PaneType::StatusBar.icon(), "⚙️");
        assert_eq!(PaneType::GardenHealth.icon(), "🌱");
        assert_eq!(PaneType::ActiveTools.icon(), "🔧");
        assert_eq!(PaneType::Logs.icon(), "📜");
    }

    #[test]
    fn test_dashboard_pane_titles() {
        assert_eq!(PaneType::StatusBar.title(), "Status");
        assert_eq!(PaneType::GardenHealth.title(), "Garden Health");
        assert_eq!(PaneType::ActiveTools.title(), "Active Tools");
        assert_eq!(PaneType::Logs.title(), "Logs");
    }

    #[test]
    fn test_dashboard_layout_calculation() {
        let mut engine = LayoutEngine::new();
        engine.apply_preset(LayoutPreset::Dashboard);

        let area = Rect::new(0, 0, 100, 50);
        let layouts = engine.calculate_layout(area);

        // All 5 panes should have layout areas
        assert_eq!(layouts.len(), 5);

        // Each pane should have non-zero dimensions
        for rect in layouts.values() {
            assert!(rect.width > 0);
            assert!(rect.height > 0);
        }
    }
}
