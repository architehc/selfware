use super::*;

#[test]
fn test_progress_bar_new() {
    let bar = AnimatedProgressBar::new(0.5);
    assert!((bar.progress() - 0.5).abs() < 0.001);
}

#[test]
fn test_progress_bar_clamp() {
    let bar1 = AnimatedProgressBar::new(-0.5);
    assert!((bar1.progress() - 0.0).abs() < 0.001);

    let bar2 = AnimatedProgressBar::new(1.5);
    assert!((bar2.progress() - 1.0).abs() < 0.001);
}

#[test]
fn test_progress_bar_with_label() {
    let bar = AnimatedProgressBar::new(0.5).with_label("Loading");
    assert!(bar.label.is_some());
}

#[test]
fn test_progress_bar_update() {
    let mut bar = AnimatedProgressBar::new(0.0);
    bar.set_progress(1.0);

    // After small update, should move towards target but not reach it
    bar.update(0.1);
    assert!(bar.progress() > 0.0);
    assert!(bar.progress() < 1.0);
}

#[test]
fn test_set_progress_clamping() {
    let mut bar = AnimatedProgressBar::new(0.5);
    bar.set_progress(1.5);
    assert!((bar.progress() - 0.5).abs() < 0.1); // Still near old since smooth transition
                                                 // After enough updates, should converge to 1.0
    for _ in 0..100 {
        bar.update(0.1);
    }
    assert!((bar.progress() - 1.0).abs() < 0.01);
}

#[test]
fn test_set_progress_negative_clamped() {
    let mut bar = AnimatedProgressBar::new(0.5);
    bar.set_progress(-1.0);
    // Target is clamped to 0.0
    for _ in 0..100 {
        bar.update(0.1);
    }
    assert!((bar.progress() - 0.0).abs() < 0.01);
}

#[test]
fn test_show_percentage_builder() {
    let bar = AnimatedProgressBar::new(0.5).show_percentage(false);
    assert!(!bar.show_percentage);
}

#[test]
fn test_animated_builder() {
    let bar = AnimatedProgressBar::new(0.5).animated(false);
    assert!(!bar.animated);
}

#[test]
fn test_is_complete() {
    let bar = AnimatedProgressBar::new(1.0);
    assert!(!bar.is_complete()); // Progress bars never complete
}

#[test]
fn test_animation_trait_update() {
    let mut bar = AnimatedProgressBar::new(0.0);
    bar.set_progress(1.0);
    Animation::update(&mut bar, 0.1);
    assert!(bar.progress() > 0.0);
}
