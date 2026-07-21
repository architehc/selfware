use super::*;

struct TestAnimation {
    updates: u32,
    complete_after: u32,
}

impl Animation for TestAnimation {
    fn update(&mut self, _delta_time: f32) {
        self.updates += 1;
    }

    fn is_complete(&self) -> bool {
        self.updates >= self.complete_after
    }
}

#[test]
fn test_animation_manager_new() {
    let manager = AnimationManager::new();
    assert_eq!(manager.animation_count(), 0);
    assert!(!manager.is_paused());
}

#[test]
fn test_animation_manager_add() {
    let mut manager = AnimationManager::new();
    manager.add(TestAnimation {
        updates: 0,
        complete_after: 5,
    });
    assert_eq!(manager.animation_count(), 1);
}

#[test]
fn test_animation_manager_update() {
    let mut manager = AnimationManager::new();
    manager.add(TestAnimation {
        updates: 0,
        complete_after: 3,
    });

    manager.update(0.016);
    assert_eq!(manager.animation_count(), 1);

    manager.update(0.016);
    manager.update(0.016);
    assert_eq!(manager.animation_count(), 0); // Should be removed after completing
}

#[test]
fn test_animation_manager_pause() {
    let mut manager = AnimationManager::new();
    manager.add(TestAnimation {
        updates: 0,
        complete_after: 10,
    });

    manager.toggle_pause();
    assert!(manager.is_paused());

    // Updates should not happen while paused
    manager.update(0.016);
    assert_eq!(manager.animation_count(), 1);

    manager.toggle_pause();
    assert!(!manager.is_paused());
}
