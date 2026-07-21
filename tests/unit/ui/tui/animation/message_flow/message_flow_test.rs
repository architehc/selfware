use super::*;

#[test]
fn test_message_type_symbol() {
    assert_eq!(MessageType::Request.symbol(), "●");
    assert_eq!(MessageType::Response.symbol(), "◆");
}

#[test]
fn test_message_flow_new() {
    let flow = MessageFlow::new((0.0, 0.0), (10.0, 10.0), MessageType::Request);
    assert!((flow.progress() - 0.0).abs() < 0.001);
}

#[test]
fn test_message_flow_current_position() {
    let mut flow = MessageFlow::new((0.0, 0.0), (10.0, 10.0), MessageType::Request);

    let (x, y) = flow.current_position();
    assert!((x - 0.0).abs() < 0.001);
    assert!((y - 0.0).abs() < 0.001);

    flow.progress = 0.5;
    let (x, y) = flow.current_position();
    assert!((x - 5.0).abs() < 0.001);
    assert!((y - 5.0).abs() < 0.001);
}

#[test]
fn test_message_flow_update() {
    let mut flow = MessageFlow::new((0.0, 0.0), (10.0, 10.0), MessageType::Request).with_speed(2.0);

    flow.update(0.25);
    assert!((flow.progress() - 0.5).abs() < 0.001);
}

#[test]
fn test_message_flow_completion() {
    let mut flow =
        MessageFlow::new((0.0, 0.0), (10.0, 10.0), MessageType::Request).with_speed(10.0);

    assert!(!flow.is_complete());
    flow.update(1.0);
    assert!(flow.is_complete());
}

#[test]
fn test_message_flow_looping() {
    let mut flow = MessageFlow::new((0.0, 0.0), (10.0, 10.0), MessageType::Request)
        .with_speed(10.0)
        .looping(true);

    flow.update(1.0);
    assert!(!flow.is_complete());
    assert!((flow.progress() - 0.0).abs() < 0.001);
}

#[test]
fn test_message_flow_manager() {
    let mut manager = MessageFlowManager::new();
    manager.add(MessageFlow::new(
        (0.0, 0.0),
        (10.0, 10.0),
        MessageType::Request,
    ));
    assert_eq!(manager.flow_count(), 1);

    // Fast forward to completion
    for _ in 0..20 {
        manager.update(0.1);
    }
    assert_eq!(manager.flow_count(), 0);
}
