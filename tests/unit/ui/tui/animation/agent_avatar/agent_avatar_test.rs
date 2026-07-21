use super::*;

#[test]
fn test_agent_role_icon() {
    assert_eq!(AgentRole::Coder.icon(), "💻");
    assert_eq!(AgentRole::Architect.icon(), "🏗️");
}

#[test]
fn test_agent_role_color() {
    let color = AgentRole::Coder.color();
    assert_eq!(color, colors::SECONDARY);
}

#[test]
fn test_agent_avatar_new() {
    let avatar = AgentAvatar::new(AgentRole::Coder);
    assert_eq!(avatar.role(), AgentRole::Coder);
    assert_eq!(avatar.activity(), ActivityLevel::Idle);
}

#[test]
fn test_agent_avatar_with_activity() {
    let avatar = AgentAvatar::new(AgentRole::Tester).with_activity(ActivityLevel::High);
    assert_eq!(avatar.activity(), ActivityLevel::High);
}

#[test]
fn test_agent_avatar_format_tokens() {
    let avatar1 = AgentAvatar::new(AgentRole::Coder).with_tokens(500);
    assert_eq!(avatar1.format_tokens(), "500");

    let avatar2 = AgentAvatar::new(AgentRole::Coder).with_tokens(5_000);
    assert_eq!(avatar2.format_tokens(), "5K");

    let avatar3 = AgentAvatar::new(AgentRole::Coder).with_tokens(1_500_000);
    assert_eq!(avatar3.format_tokens(), "1.5M");
}

#[test]
fn test_activity_level_dots() {
    assert_eq!(ActivityLevel::Idle.dots(), 0);
    assert_eq!(ActivityLevel::Low.dots(), 1);
    assert_eq!(ActivityLevel::Max.dots(), 4);
    assert_eq!(ActivityLevel::Complete.dots(), 5);
}

#[test]
fn test_all_agent_role_icons() {
    assert_eq!(AgentRole::Architect.icon(), "🏗️");
    assert_eq!(AgentRole::Tester.icon(), "🧪");
    assert_eq!(AgentRole::Reviewer.icon(), "👁️");
    assert_eq!(AgentRole::Documenter.icon(), "📚");
    assert_eq!(AgentRole::DevOps.icon(), "🚀");
    assert_eq!(AgentRole::Security.icon(), "🔒");
    assert_eq!(AgentRole::Performance.icon(), "⚡");
}

#[test]
fn test_all_agent_role_ascii_icons() {
    assert_eq!(AgentRole::Architect.ascii_icon(), "[A]");
    assert_eq!(AgentRole::Coder.ascii_icon(), "[C]");
    assert_eq!(AgentRole::Tester.ascii_icon(), "[T]");
    assert_eq!(AgentRole::Reviewer.ascii_icon(), "[R]");
    assert_eq!(AgentRole::Documenter.ascii_icon(), "[D]");
    assert_eq!(AgentRole::DevOps.ascii_icon(), "[O]");
    assert_eq!(AgentRole::Security.ascii_icon(), "[S]");
    assert_eq!(AgentRole::Performance.ascii_icon(), "[P]");
}

#[test]
fn test_all_agent_role_names() {
    assert_eq!(AgentRole::Architect.name(), "Architect");
    assert_eq!(AgentRole::Coder.name(), "Coder");
    assert_eq!(AgentRole::Tester.name(), "Tester");
    assert_eq!(AgentRole::Reviewer.name(), "Reviewer");
    assert_eq!(AgentRole::Documenter.name(), "Documenter");
    assert_eq!(AgentRole::DevOps.name(), "DevOps");
    assert_eq!(AgentRole::Security.name(), "Security");
    assert_eq!(AgentRole::Performance.name(), "Performance");
}

#[test]
fn test_all_agent_role_colors() {
    // Just verify they all return colors without panicking
    let _ = AgentRole::Architect.color();
    let _ = AgentRole::Tester.color();
    let _ = AgentRole::Reviewer.color();
    let _ = AgentRole::Documenter.color();
    let _ = AgentRole::DevOps.color();
    let _ = AgentRole::Security.color();
    let _ = AgentRole::Performance.color();
}

#[test]
fn test_all_activity_level_symbols() {
    assert_eq!(ActivityLevel::Idle.symbol(), "○");
    assert_eq!(ActivityLevel::Low.symbol(), "◐");
    assert_eq!(ActivityLevel::Medium.symbol(), "◑");
    assert_eq!(ActivityLevel::High.symbol(), "◓");
    assert_eq!(ActivityLevel::Max.symbol(), "●");
    assert_eq!(ActivityLevel::Complete.symbol(), "✓");
    assert_eq!(ActivityLevel::Error.symbol(), "✗");
}

#[test]
fn test_with_name() {
    let avatar = AgentAvatar::new(AgentRole::Coder).with_name("my-coder");
    assert_eq!(avatar.name, "my-coder");
}

#[test]
fn test_ascii_mode() {
    let avatar = AgentAvatar::new(AgentRole::Coder).ascii_mode(true);
    assert!(avatar.ascii_mode);
}

#[test]
fn test_set_tokens_and_add_tokens() {
    let mut avatar = AgentAvatar::new(AgentRole::Coder);
    avatar.set_tokens(100);
    assert_eq!(avatar.token_count, 100);
    avatar.add_tokens(50);
    assert_eq!(avatar.token_count, 150);
}

#[test]
fn test_update_advances_pulse() {
    let mut avatar = AgentAvatar::new(AgentRole::Coder).with_activity(ActivityLevel::High);
    let initial_phase = avatar.pulse_phase;
    avatar.update(0.5);
    assert!(avatar.pulse_phase > initial_phase);
}

#[test]
fn test_update_wraps_pulse() {
    let mut avatar = AgentAvatar::new(AgentRole::Coder).with_activity(ActivityLevel::Max);
    // Push phase past 2π
    for _ in 0..100 {
        avatar.update(0.5);
    }
    assert!(avatar.pulse_phase < std::f32::consts::PI * 2.0);
}

#[test]
fn test_is_complete_always_false() {
    let avatar = AgentAvatar::new(AgentRole::Coder);
    assert!(!avatar.is_complete());
}

#[test]
fn test_activity_level_default() {
    let level = ActivityLevel::default();
    assert_eq!(level, ActivityLevel::Idle);
}

#[test]
fn test_medium_dots() {
    assert_eq!(ActivityLevel::Medium.dots(), 2);
    assert_eq!(ActivityLevel::High.dots(), 3);
    assert_eq!(ActivityLevel::Error.dots(), 0);
}
