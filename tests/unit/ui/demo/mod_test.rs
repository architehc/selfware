use super::*;

#[test]
fn test_demo_config_default() {
    let config = DemoConfig::default();
    assert!((config.speed_multiplier - 1.0).abs() < 0.001);
    assert!(!config.auto_advance);
    assert!(config.particles_enabled);
}

#[test]
fn test_demo_config_fast() {
    let config = DemoConfig::fast();
    assert!(config.speed_multiplier > 1.0);
    assert!(config.auto_advance);
}

#[test]
fn test_demo_config_presentation() {
    let config = DemoConfig::presentation();
    assert!(config.speed_multiplier < 1.0);
    assert!(!config.auto_advance);
    assert!(config.max_particles > 100);
}
