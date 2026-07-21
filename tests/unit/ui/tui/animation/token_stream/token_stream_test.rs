use super::*;

#[test]
fn test_token_size_from_count() {
    assert_eq!(TokenSize::from_count(500), TokenSize::Small);
    assert_eq!(TokenSize::from_count(50_000), TokenSize::Medium);
    assert_eq!(TokenSize::from_count(200_000), TokenSize::Large);
    assert_eq!(TokenSize::from_count(1_000_000), TokenSize::Massive);
}

#[test]
fn test_token_stream_new() {
    let stream = TokenStream::new(100);
    assert_eq!(stream.particle_count(), 0);
    assert_eq!(stream.total_tokens(), 0);
}

#[test]
fn test_token_stream_add_token() {
    let mut stream = TokenStream::new(5);

    stream.add_token(TokenSize::Small);
    assert_eq!(stream.particle_count(), 1);

    // Add more than max
    for _ in 0..10 {
        stream.add_token(TokenSize::Medium);
    }
    assert_eq!(stream.particle_count(), 5);
}

#[test]
fn test_token_stream_update() {
    let mut stream = TokenStream::new(10).with_auto_spawn(false);
    stream.add_token(TokenSize::Small);

    // After update, particle should move
    stream.update(0.5);
    // After more updates, particle should exit
    for _ in 0..10 {
        stream.update(0.5);
    }
    assert_eq!(stream.particle_count(), 0);
}

#[test]
fn test_token_size_symbol() {
    assert_eq!(TokenSize::Small.symbol(), "●");
    assert_eq!(TokenSize::Medium.symbol(), "◆");
    assert_eq!(TokenSize::Large.symbol(), "▲");
    assert_eq!(TokenSize::Massive.symbol(), "★");
}

#[test]
fn test_token_size_color() {
    // Just verify they all return colors without panicking
    let _ = TokenSize::Small.color();
    let _ = TokenSize::Medium.color();
    let _ = TokenSize::Large.color();
    let _ = TokenSize::Massive.color();
}

#[test]
fn test_set_rate_and_rate() {
    let mut stream = TokenStream::new(10);
    stream.set_rate(500.0);
    assert!((stream.rate() - 500.0).abs() < 0.01);
}

#[test]
fn test_set_total_and_total_tokens() {
    let mut stream = TokenStream::new(10);
    stream.set_total(42000);
    assert_eq!(stream.total_tokens(), 42000);
}

#[test]
fn test_is_complete() {
    let stream = TokenStream::new(10);
    assert!(!stream.is_complete());
}

#[test]
fn test_auto_spawn_with_high_rate() {
    let mut stream = TokenStream::new(100).with_auto_spawn(true);
    stream.set_rate(10000.0);
    stream.set_total(50000);
    // Several updates should spawn particles
    for _ in 0..20 {
        stream.update(0.1);
    }
    assert!(stream.particle_count() > 0);
}

#[test]
fn test_token_size_boundary_values() {
    assert_eq!(TokenSize::from_count(0), TokenSize::Small);
    assert_eq!(TokenSize::from_count(9_999), TokenSize::Small);
    assert_eq!(TokenSize::from_count(10_000), TokenSize::Medium);
    assert_eq!(TokenSize::from_count(99_999), TokenSize::Medium);
    assert_eq!(TokenSize::from_count(100_000), TokenSize::Large);
    assert_eq!(TokenSize::from_count(499_999), TokenSize::Large);
    assert_eq!(TokenSize::from_count(500_000), TokenSize::Massive);
}
