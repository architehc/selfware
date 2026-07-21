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
