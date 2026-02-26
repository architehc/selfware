//! Token Stream Visualization
//!
//! Shows tokens flowing through a stream with:
//! - Wave background animation
//! - Token particles of different sizes
//! - Color-coded by token size

use super::{colors, Animation};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};
use std::collections::VecDeque;

/// Token size categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSize {
    /// Small tokens (1K-10K)
    Small,
    /// Medium tokens (10K-100K)
    Medium,
    /// Large tokens (100K-500K)
    Large,
    /// Massive tokens (500K+)
    Massive,
}

impl TokenSize {
    /// Get the symbol for this token size
    pub fn symbol(&self) -> &'static str {
        match self {
            TokenSize::Small => "●",
            TokenSize::Medium => "◆",
            TokenSize::Large => "▲",
            TokenSize::Massive => "★",
        }
    }

    /// Get the color for this token size
    pub fn color(&self) -> Color {
        match self {
            TokenSize::Small => colors::SECONDARY, // Blue
            TokenSize::Medium => colors::ACCENT,   // Mint
            TokenSize::Large => colors::WARNING,   // Yellow
            TokenSize::Massive => colors::PRIMARY, // Coral
        }
    }

    /// Create TokenSize from a token count
    pub fn from_count(count: u64) -> Self {
        if count >= 500_000 {
            TokenSize::Massive
        } else if count >= 100_000 {
            TokenSize::Large
        } else if count >= 10_000 {
            TokenSize::Medium
        } else {
            TokenSize::Small
        }
    }
}

/// A single token particle in the stream
#[derive(Debug, Clone)]
struct TokenParticle {
    /// Horizontal position (0.0 to 1.0)
    position: f32,
    /// Movement speed (position units per second)
    speed: f32,
    /// Token size category
    size: TokenSize,
    /// Vertical offset for wave effect
    wave_offset: f32,
}

/// Animated token stream visualization
pub struct TokenStream {
    /// Active token particles
    particles: VecDeque<TokenParticle>,
    /// Maximum particles to show
    max_particles: usize,
    /// Wave animation phase
    wave_phase: f32,
    /// Total tokens processed
    total_tokens: u64,
    /// Tokens per second rate
    tokens_per_second: f64,
    /// Auto-spawn particles based on rate
    auto_spawn: bool,
    /// Time since last auto-spawn
    spawn_timer: f32,
}

impl TokenStream {
    pub fn new(max_particles: usize) -> Self {
        Self {
            particles: VecDeque::new(),
            max_particles,
            wave_phase: 0.0,
            total_tokens: 0,
            tokens_per_second: 0.0,
            auto_spawn: true,
            spawn_timer: 0.0,
        }
    }

    pub fn with_auto_spawn(mut self, auto: bool) -> Self {
        self.auto_spawn = auto;
        self
    }

    /// Add a token particle to the stream
    pub fn add_token(&mut self, size: TokenSize) {
        if self.particles.len() >= self.max_particles {
            self.particles.pop_front();
        }

        // Random speed variation
        let base_speed = 0.3;
        let speed_variation = (self.particles.len() as f32 * 0.1) % 0.2;
        let speed = base_speed + speed_variation;

        // Random wave offset
        let wave_offset = (self.particles.len() as f32 * 0.5) % std::f32::consts::PI;

        self.particles.push_back(TokenParticle {
            position: 0.0,
            speed,
            size,
            wave_offset,
        });
    }

    /// Set the tokens per second rate
    pub fn set_rate(&mut self, rate: f64) {
        self.tokens_per_second = rate;
    }

    /// Set total tokens processed
    pub fn set_total(&mut self, total: u64) {
        self.total_tokens = total;
    }

    /// Get total tokens
    pub fn total_tokens(&self) -> u64 {
        self.total_tokens
    }

    /// Get current rate
    pub fn rate(&self) -> f64 {
        self.tokens_per_second
    }

    /// Get particle count
    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }
}

impl Animation for TokenStream {
    fn update(&mut self, delta_time: f32) {
        // Update wave phase
        self.wave_phase += delta_time * 3.0;
        if self.wave_phase > std::f32::consts::PI * 2.0 {
            self.wave_phase -= std::f32::consts::PI * 2.0;
        }

        // Update particle positions
        for particle in &mut self.particles {
            particle.position += particle.speed * delta_time;
        }

        // Remove particles that have exited
        self.particles.retain(|p| p.position < 1.5);

        // Auto-spawn based on rate
        if self.auto_spawn && self.tokens_per_second > 0.0 {
            self.spawn_timer += delta_time;
            let spawn_interval = 1.0 / (self.tokens_per_second as f32 / 1000.0).max(0.1);

            if self.spawn_timer >= spawn_interval {
                self.spawn_timer = 0.0;
                // Spawn particle based on recent activity
                let size = TokenSize::from_count(self.total_tokens / 10);
                self.add_token(size);
            }
        }
    }

    fn is_complete(&self) -> bool {
        false // Token streams don't complete
    }
}

impl Widget for &TokenStream {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 5 || area.height < 3 {
            return;
        }

        // Wave symbols for background
        let wave_symbols = ["≋", "≈", "∿", "~"];

        // Draw wave background
        for x in area.x..area.x + area.width {
            let wave_idx = ((x as f32 + self.wave_phase * 5.0) as usize) % wave_symbols.len();
            let wave_color = Color::Rgb(0x25, 0x25, 0x3D);

            for y in area.y..area.y + area.height {
                // Alternate wave patterns vertically
                let symbol_idx = (wave_idx + (y - area.y) as usize) % wave_symbols.len();
                buf.get_mut(x, y)
                    .set_symbol(wave_symbols[symbol_idx])
                    .set_style(Style::default().fg(wave_color));
            }
        }

        // Draw particles
        let center_y = area.y + area.height / 2;

        for particle in &self.particles {
            let x = area.x + (particle.position * area.width as f32) as u16;

            if x >= area.x && x < area.x + area.width {
                // Calculate vertical position with wave effect
                let wave = (self.wave_phase + particle.wave_offset).sin();
                let y_offset = (wave * (area.height as f32 / 4.0)) as i16;
                let y = (center_y as i16 + y_offset)
                    .clamp(area.y as i16, (area.y + area.height - 1) as i16)
                    as u16;

                let symbol = particle.size.symbol();
                let color = particle.size.color();

                // Draw particle with glow effect
                buf.get_mut(x, y)
                    .set_symbol(symbol)
                    .set_style(Style::default().fg(color).add_modifier(Modifier::BOLD));

                // Glow on sides
                if x > area.x {
                    buf.get_mut(x - 1, y)
                        .set_symbol("·")
                        .set_style(Style::default().fg(color));
                }
                if x < area.x + area.width - 1 {
                    buf.get_mut(x + 1, y)
                        .set_symbol("·")
                        .set_style(Style::default().fg(color));
                }
            }
        }

        // Draw stats at bottom
        if area.height > 2 {
            let stats = format!(
                "💫 {} tok/s │ {} total",
                self.tokens_per_second as u64,
                if self.total_tokens >= 1_000_000 {
                    format!("{:.1}M", self.total_tokens as f64 / 1_000_000.0)
                } else if self.total_tokens >= 1_000 {
                    format!("{}K", self.total_tokens / 1_000)
                } else {
                    format!("{}", self.total_tokens)
                }
            );

            let stats_y = area.y + area.height - 1;
            for (i, ch) in stats.chars().enumerate() {
                let x = area.x + i as u16;
                if x < area.x + area.width {
                    buf.get_mut(x, stats_y)
                        .set_symbol(&ch.to_string())
                        .set_style(Style::default().fg(Color::Gray));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
}
