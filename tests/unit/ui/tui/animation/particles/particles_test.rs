use super::*;

#[test]
fn test_particle_new() {
    let p = Particle::new(10.0, 20.0);
    assert!((p.x - 10.0).abs() < 0.001);
    assert!((p.y - 20.0).abs() < 0.001);
    assert!(p.is_alive());
}

#[test]
fn test_particle_update() {
    let mut p = Particle::new(0.0, 0.0)
        .with_velocity(10.0, 5.0)
        .with_lifetime(1.0);

    p.update(0.1);
    assert!((p.x - 1.0).abs() < 0.001);
    assert!((p.y - 0.5).abs() < 0.001);
    assert!((p.lifetime - 0.9).abs() < 0.001);
}

#[test]
fn test_particle_gravity() {
    let mut p = Particle::new(0.0, 0.0)
        .with_velocity(0.0, 0.0)
        .with_gravity(10.0)
        .with_lifetime(1.0);

    p.update(0.1);
    assert!((p.vy - 1.0).abs() < 0.001); // Gravity accelerates vy
}

#[test]
fn test_particle_system_new() {
    let ps = ParticleSystem::new(100);
    assert_eq!(ps.particle_count(), 0);
}

#[test]
fn test_particle_system_add() {
    let mut ps = ParticleSystem::new(5);

    for i in 0..10 {
        ps.add(Particle::new(i as f32, 0.0));
    }

    // Should cap at max
    assert_eq!(ps.particle_count(), 5);
}

#[test]
fn test_particle_system_update_removes_dead() {
    let mut ps = ParticleSystem::new(10);
    ps.add(Particle::new(0.0, 0.0).with_lifetime(0.1));

    assert_eq!(ps.particle_count(), 1);

    // Update past lifetime
    ps.update(0.2);
    assert_eq!(ps.particle_count(), 0);
}

#[test]
fn test_particle_system_sparkle() {
    let mut ps = ParticleSystem::new(50);
    ps.sparkle(10.0, 10.0, 10);
    assert!(ps.particle_count() > 0);
}
