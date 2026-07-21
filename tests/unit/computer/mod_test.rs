use super::*;

#[test]
fn test_rate_limiter_allows_first_action() {
    let limiter = ActionRateLimiter::new(5);
    assert!(limiter.check());
}

#[test]
fn test_rate_limiter_allows_within_limit() {
    let limiter = ActionRateLimiter::new(5);
    for _ in 0..5 {
        assert!(limiter.check());
    }
}

#[test]
fn test_rate_limiter_blocks_over_limit() {
    let limiter = ActionRateLimiter::new(2);
    assert!(limiter.check()); // 1st
    assert!(limiter.check()); // 2nd
    assert!(!limiter.check()); // 3rd should fail
}

#[test]
fn test_rate_limiter_default() {
    let limiter = ActionRateLimiter::default();
    assert_eq!(limiter.max_actions_per_sec, 10);
    // First action should always pass
    assert!(limiter.check());
}

#[test]
fn rate_limiter_caps_concurrent_burst() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    // Many threads hammering a fresh limiter within one ~1s window must not
    // let a burst through: the CAS window-claim prevents multiple threads
    // from each resetting the counter (the old Relaxed load/store race).
    let limiter = Arc::new(ActionRateLimiter::new(10));
    let allowed = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let l = Arc::clone(&limiter);
        let a = Arc::clone(&allowed);
        handles.push(std::thread::spawn(move || {
            for _ in 0..50 {
                if l.check() {
                    a.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let total = allowed.load(Ordering::Relaxed);
    // 400 attempts complete in microseconds (~1 window). Allow 2 windows of
    // slack for a rare boundary crossing; without the fix this blew far past.
    assert!(
        (1..=20).contains(&total),
        "concurrent burst must be capped near the limit, got {total}"
    );
}

#[test]
fn test_blocked_combos() {
    assert!(is_blocked_combo("ctrl+alt+delete"));
    assert!(is_blocked_combo("Ctrl+Alt+Delete"));
    assert!(is_blocked_combo("cmd+q"));
    assert!(is_blocked_combo("alt+f4"));
    assert!(is_blocked_combo("ctrl+alt+f1"));
    assert!(is_blocked_combo("ctrl+alt+f2"));
    assert!(is_blocked_combo("ctrl+alt+f3"));
    assert!(!is_blocked_combo("ctrl+c"));
    assert!(!is_blocked_combo("ctrl+v"));
    assert!(!is_blocked_combo("ctrl+s"));
    assert!(!is_blocked_combo("ctrl+shift+t"));
    assert!(!is_blocked_combo(""));
}

#[test]
fn test_blocked_combos_whitespace_normalization() {
    // Spaces should be stripped
    assert!(is_blocked_combo("ctrl + alt + delete"));
    assert!(is_blocked_combo("cmd + q"));
}

#[test]
fn test_movement_profile_default() {
    let profile = MovementProfile::default();
    assert!(matches!(profile, MovementProfile::Linear));
}

#[test]
fn test_movement_profile_serde_roundtrip() {
    let profiles = vec![
        MovementProfile::Linear,
        MovementProfile::EaseInOut,
        MovementProfile::Bezier,
    ];
    for profile in profiles {
        let json = serde_json::to_string(&profile).unwrap();
        let parsed: MovementProfile = serde_json::from_str(&json).unwrap();
        let _ = format!("{:?}", parsed);
    }
}

#[test]
fn test_typing_profile_default() {
    let profile = TypingProfile::default();
    assert_eq!(profile.base_delay_ms, 0);
    assert_eq!(profile.variation_ms, 0);
}

#[test]
fn test_typing_profile_serde_roundtrip() {
    let profile = TypingProfile {
        base_delay_ms: 50,
        variation_ms: 10,
    };
    let json = serde_json::to_string(&profile).unwrap();
    let parsed: TypingProfile = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.base_delay_ms, 50);
    assert_eq!(parsed.variation_ms, 10);
}

#[test]
fn test_typing_profile_serde_defaults() {
    // Missing fields should default to 0
    let parsed: TypingProfile = serde_json::from_str("{}").unwrap();
    assert_eq!(parsed.base_delay_ms, 0);
    assert_eq!(parsed.variation_ms, 0);
}
