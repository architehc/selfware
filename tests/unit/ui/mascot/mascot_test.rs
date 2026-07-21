use super::*;

#[test]
fn test_render_mascot() {
    let greeting = render_mascot(MascotMood::Greeting);
    assert!(!greeting.is_empty());
    assert!(greeting.contains("/\\___/\\"));
}

#[test]
fn test_inline_mascot() {
    let inline = render_inline_mascot(MascotMood::Success);
    assert!(inline.contains("🎉"));
}

#[test]
fn test_thinking_frames() {
    let frame0 = thinking_frame(0);
    let frame1 = thinking_frame(1);
    let frame2 = thinking_frame(2);
    let frame3 = thinking_frame(3); // Should wrap to 0

    assert_eq!(frame3, frame0);
    assert_ne!(frame0, frame1);
    assert_ne!(frame1, frame2);
}
