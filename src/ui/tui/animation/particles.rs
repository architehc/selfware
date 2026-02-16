//! Particle System
//!
//! General purpose particle system for effects like:
//! - Sparkles
//! - Confetti
//! - Explosions
//! - Ambient effects

use std::sync::atomic::{AtomicU32, Ordering};

use super::{colors, Animation};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// A single particle with position, velocity, and appearance
#[derive(Debug, Clone)]
pub struct Particle {
    /// X position
    pub x: f32,
    /// Y position
    pub y: f32,
    /// X velocity (units per second)
    pub vx: f32,
    /// Y velocity (units per second)
    pub vy: f32,
    /// Particle lifetime remaining (seconds)
    pub lifetime: f32,
    /// Maximum lifetime (for fade calculations)
    pub max_lifetime: f32,
    /// Display symbol
    pub symbol: char,
    /// Particle color
    pub color: Color,
    /// Gravity multiplier (0 = no gravity)
    pub gravity: f32,
}

impl Particle {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            lifetime: 1.0,
            max_lifetime: 1.0,
            symbol: '·',
            color: Color::White,
            gravity: 0.0,
        }
    }

    pub fn with_velocity(mut self, vx: f32, vy: f32) -> Self {
        self.vx = vx;
        self.vy = vy;
        self
    }

    pub fn with_lifetime(mut self, lifetime: f32) -> Self {
        self.lifetime = lifetime;
        self.max_lifetime = lifetime;
        self
    }

    pub fn with_symbol(mut self, symbol: char) -> Self {
        self.symbol = symbol;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn with_gravity(mut self, gravity: f32) -> Self {
        self.gravity = gravity;
        self
    }

    /// Check if particle is still alive
    pub fn is_alive(&self) -> bool {
        self.lifetime > 0.0
    }

    /// Get fade factor (0.0 to 1.0)
    pub fn fade(&self) -> f32 {
        (self.lifetime / self.max_lifetime).clamp(0.0, 1.0)
    }

    /// Update particle physics
    pub fn update(&mut self, delta_time: f32) {
        // Apply velocity
        self.x += self.vx * delta_time;
        self.y += self.vy * delta_time;

        // Apply gravity
        self.vy += self.gravity * delta_time;

        // Decrease lifetime
        self.lifetime -= delta_time;
    }
}

/// Particle system manager
pub struct ParticleSystem {
    /// Active particles
    particles: Vec<Particle>,
    /// Maximum particle count
    max_particles: usize,
    /// Bounds for particle rendering
    bounds: Option<Rect>,
}

impl ParticleSystem {
    pub fn new(max_particles: usize) -> Self {
        Self {
            particles: Vec::with_capacity(max_particles),
            max_particles,
            bounds: None,
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    /// Add a particle to the system
    pub fn add(&mut self, particle: Particle) {
        if self.particles.len() < self.max_particles {
            self.particles.push(particle);
        }
    }

    /// Emit particles at a position with random velocities
    pub fn emit(&mut self, x: f32, y: f32, count: usize, config: EmitConfig) {
        for _ in 0..count {
            if self.particles.len() >= self.max_particles {
                break;
            }

            // Random angle
            let angle = config.angle_min + pseudo_random() * (config.angle_max - config.angle_min);
            let speed = config.speed_min + pseudo_random() * (config.speed_max - config.speed_min);

            let vx = angle.cos() * speed;
            let vy = angle.sin() * speed;

            let lifetime =
                config.lifetime_min + pseudo_random() * (config.lifetime_max - config.lifetime_min);

            let symbol = config.symbols[pseudo_random_index(config.symbols.len())];
            let color = config.colors[pseudo_random_index(config.colors.len())];

            self.add(
                Particle::new(x, y)
                    .with_velocity(vx, vy)
                    .with_lifetime(lifetime)
                    .with_symbol(symbol)
                    .with_color(color)
                    .with_gravity(config.gravity),
            );
        }
    }

    /// Create a sparkle effect
    pub fn sparkle(&mut self, x: f32, y: f32, count: usize) {
        self.emit(
            x,
            y,
            count,
            EmitConfig {
                speed_min: 2.0,
                speed_max: 8.0,
                angle_min: 0.0,
                angle_max: std::f32::consts::PI * 2.0,
                lifetime_min: 0.3,
                lifetime_max: 0.8,
                gravity: 0.0,
                symbols: &['✦', '✧', '·', '∘'],
                colors: &[colors::WARNING, colors::ACCENT, Color::White],
            },
        );
    }

    /// Create an explosion effect
    pub fn explode(&mut self, x: f32, y: f32, count: usize) {
        self.emit(
            x,
            y,
            count,
            EmitConfig {
                speed_min: 5.0,
                speed_max: 15.0,
                angle_min: 0.0,
                angle_max: std::f32::consts::PI * 2.0,
                lifetime_min: 0.5,
                lifetime_max: 1.5,
                gravity: 5.0,
                symbols: &['●', '◆', '▲', '■'],
                colors: &[colors::PRIMARY, colors::WARNING, colors::ERROR],
            },
        );
    }

    /// Create a success celebration
    pub fn celebrate(&mut self, x: f32, y: f32) {
        self.emit(
            x,
            y,
            20,
            EmitConfig {
                speed_min: 3.0,
                speed_max: 10.0,
                angle_min: -std::f32::consts::PI,
                angle_max: 0.0, // Upward
                lifetime_min: 1.0,
                lifetime_max: 2.0,
                gravity: 3.0,
                symbols: &['★', '✦', '✧', '◆'],
                colors: &[
                    colors::SUCCESS,
                    colors::ACCENT,
                    colors::WARNING,
                    colors::SECONDARY,
                ],
            },
        );
    }

    /// Get particle count
    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    /// Clear all particles
    pub fn clear(&mut self) {
        self.particles.clear();
    }
}

impl Animation for ParticleSystem {
    fn update(&mut self, delta_time: f32) {
        // Update all particles
        for particle in &mut self.particles {
            particle.update(delta_time);
        }

        // Remove dead particles
        self.particles.retain(|p| p.is_alive());

        // Remove particles outside bounds if set
        if let Some(bounds) = self.bounds {
            self.particles.retain(|p| {
                let x = p.x as u16;
                let y = p.y as u16;
                x >= bounds.x
                    && x < bounds.x + bounds.width
                    && y >= bounds.y
                    && y < bounds.y + bounds.height
            });
        }
    }

    fn is_complete(&self) -> bool {
        self.particles.is_empty()
    }
}

impl Widget for &ParticleSystem {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for particle in &self.particles {
            let x = particle.x.round() as u16;
            let y = particle.y.round() as u16;

            if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
                // Apply fade to color
                let fade = particle.fade();
                let color = if let Color::Rgb(r, g, b) = particle.color {
                    Color::Rgb(
                        (r as f32 * fade) as u8,
                        (g as f32 * fade) as u8,
                        (b as f32 * fade) as u8,
                    )
                } else {
                    particle.color
                };

                let mut style = Style::default().fg(color);
                if fade > 0.7 {
                    style = style.add_modifier(Modifier::BOLD);
                }

                buf.get_mut(x, y)
                    .set_symbol(&particle.symbol.to_string())
                    .set_style(style);
            }
        }
    }
}

/// Configuration for particle emission
pub struct EmitConfig<'a> {
    pub speed_min: f32,
    pub speed_max: f32,
    pub angle_min: f32,
    pub angle_max: f32,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub gravity: f32,
    pub symbols: &'a [char],
    pub colors: &'a [Color],
}

// Simple pseudo-random number generator (deterministic for testing)
static RANDOM_SEED: AtomicU32 = AtomicU32::new(12345);

fn pseudo_random() -> f32 {
    let mut seed = RANDOM_SEED.load(Ordering::Relaxed);
    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
    RANDOM_SEED.store(seed, Ordering::Relaxed);
    ((seed >> 16) & 0x7FFF) as f32 / 32768.0
}

fn pseudo_random_index(max: usize) -> usize {
    (pseudo_random() * max as f32) as usize % max
}

#[cfg(test)]
mod tests {
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
}
