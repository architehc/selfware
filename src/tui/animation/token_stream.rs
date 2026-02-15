//! Token Stream Animation
//!
//! Visualizes a stream of tokens with size-based rendering and
//! a wave animation background.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::collections::VecDeque;
use std::time::Instant;

/// Represents the size category of a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSize {
    /// A small token (1-10 tokens)
    Small,
    /// A medium token (11-100 tokens)
    Medium,
    /// A large token (101-1000 tokens)
    Large,
    /// A massive token (1001+ tokens)
    Massive,
}

impl TokenSize {
    /// Get the display character width for this token size.
    pub fn display_width(&self) -> u16 {
        match self {
            TokenSize::Small => 1,
            TokenSize::Medium => 2,
            TokenSize::Large => 3,
            TokenSize::Massive => 4,
        }
    }

    /// Get the display character for this token size.
    pub fn symbol(&self) -> &'static str {
        match self {
            TokenSize::Small => "\u{2581}",   // lower one eighth block
            TokenSize::Medium => "\u{2584}",  // lower half block
            TokenSize::Large => "\u{2586}",   // lower three quarters block
            TokenSize::Massive => "\u{2588}", // full block
        }
    }

    /// Get the color for this token size.
    pub fn color(&self) -> Color {
        match self {
            TokenSize::Small => Color::Rgb(143, 151, 121),    // Sage
            TokenSize::Medium => Color::Rgb(96, 108, 56),     // Garden green
            TokenSize::Large => Color::Rgb(212, 163, 115),    // Amber
            TokenSize::Massive => Color::Rgb(184, 115, 51),   // Copper
        }
    }

    /// Classify a token count into a size category.
    pub fn from_count(count: usize) -> Self {
        match count {
            0..=10 => TokenSize::Small,
            11..=100 => TokenSize::Medium,
            101..=1000 => TokenSize::Large,
            _ => TokenSize::Massive,
        }
    }
}

/// A single token entry in the stream.
#[derive(Debug, Clone)]
pub struct Token {
    /// The text content of the token
    pub text: String,
    /// The size category
    pub size: TokenSize,
    /// When this token was added
    pub timestamp: Instant,
}

impl Token {
    /// Create a new token.
    pub fn new(text: &str, size: TokenSize) -> Self {
        Self {
            text: text.to_string(),
            size,
            timestamp: Instant::now(),
        }
    }
}

/// A stream of tokens with wave animation background.
#[derive(Debug)]
pub struct TokenStream {
    /// Queue of tokens to display
    tokens: VecDeque<Token>,
    /// Maximum number of tokens to keep in the buffer
    max_tokens: usize,
    /// Total tokens processed (including evicted ones)
    total_processed: usize,
    /// Animation frame counter for wave effect
    animation_frame: u8,
    /// Last update timestamp
    last_update: Instant,
}

impl TokenStream {
    /// Create a new token stream with the given buffer capacity.
    pub fn new(max_tokens: usize) -> Self {
        Self {
            tokens: VecDeque::with_capacity(max_tokens),
            max_tokens,
            total_processed: 0,
            animation_frame: 0,
            last_update: Instant::now(),
        }
    }

    /// Push a new token onto the stream.
    /// If the buffer is full, the oldest token is removed.
    pub fn push(&mut self, token: Token) {
        if self.tokens.len() >= self.max_tokens {
            self.tokens.pop_front();
        }
        self.tokens.push_back(token);
        self.total_processed += 1;
    }

    /// Push a token by text and count, automatically classifying the size.
    pub fn push_text(&mut self, text: &str, count: usize) {
        let size = TokenSize::from_count(count);
        self.push(Token::new(text, size));
    }

    /// Get the number of tokens currently in the buffer.
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Get the total number of tokens processed (including evicted).
    pub fn total_processed(&self) -> usize {
        self.total_processed
    }

    /// Get a reference to the tokens.
    pub fn tokens(&self) -> &VecDeque<Token> {
        &self.tokens
    }

    /// Clear all tokens from the buffer.
    pub fn clear(&mut self) {
        self.tokens.clear();
    }

    /// Advance the wave animation by one frame.
    pub fn tick(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
        self.last_update = Instant::now();
    }

    /// Get the current animation frame.
    pub fn animation_frame(&self) -> u8 {
        self.animation_frame
    }

    /// Compute the wave background color for a given column position.
    fn wave_background(&self, col: u16) -> Color {
        let t = self.animation_frame as f32 / 15.0;
        let wave = ((col as f32 / 4.0) + t).sin();
        let intensity = ((wave + 1.0) / 2.0 * 30.0) as u8 + 10;
        Color::Rgb(intensity, intensity + 5, intensity)
    }
}

impl Clone for TokenStream {
    fn clone(&self) -> Self {
        Self {
            tokens: self.tokens.clone(),
            max_tokens: self.max_tokens,
            total_processed: self.total_processed,
            animation_frame: self.animation_frame,
            last_update: self.last_update,
        }
    }
}

impl Widget for TokenStream {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Render wave background on the first row
        for col in 0..area.width {
            let px = area.x + col;
            if px < area.right() && area.y < area.bottom() {
                let bg_color = self.wave_background(col);
                let cell = buf.get_mut(px, area.y);
                cell.set_style(Style::default().bg(bg_color));
            }
        }

        // Render tokens left-to-right on the first row
        let mut x_offset: u16 = 0;
        for token in &self.tokens {
            if x_offset >= area.width {
                break;
            }

            let px = area.x + x_offset;
            if px < area.right() && area.y < area.bottom() {
                let cell = buf.get_mut(px, area.y);
                cell.set_symbol(token.size.symbol());
                cell.set_style(Style::default().fg(token.size.color()));
            }
            x_offset += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_size_from_count() {
        assert_eq!(TokenSize::from_count(0), TokenSize::Small);
        assert_eq!(TokenSize::from_count(5), TokenSize::Small);
        assert_eq!(TokenSize::from_count(10), TokenSize::Small);
        assert_eq!(TokenSize::from_count(11), TokenSize::Medium);
        assert_eq!(TokenSize::from_count(50), TokenSize::Medium);
        assert_eq!(TokenSize::from_count(100), TokenSize::Medium);
        assert_eq!(TokenSize::from_count(101), TokenSize::Large);
        assert_eq!(TokenSize::from_count(500), TokenSize::Large);
        assert_eq!(TokenSize::from_count(1000), TokenSize::Large);
        assert_eq!(TokenSize::from_count(1001), TokenSize::Massive);
        assert_eq!(TokenSize::from_count(10000), TokenSize::Massive);
    }

    #[test]
    fn test_token_size_display_width() {
        assert_eq!(TokenSize::Small.display_width(), 1);
        assert_eq!(TokenSize::Medium.display_width(), 2);
        assert_eq!(TokenSize::Large.display_width(), 3);
        assert_eq!(TokenSize::Massive.display_width(), 4);
    }

    #[test]
    fn test_token_size_symbols() {
        assert_eq!(TokenSize::Small.symbol(), "\u{2581}");
        assert_eq!(TokenSize::Medium.symbol(), "\u{2584}");
        assert_eq!(TokenSize::Large.symbol(), "\u{2586}");
        assert_eq!(TokenSize::Massive.symbol(), "\u{2588}");
    }

    #[test]
    fn test_token_size_colors() {
        // Each size should have a distinct color
        let colors = vec![
            TokenSize::Small.color(),
            TokenSize::Medium.color(),
            TokenSize::Large.color(),
            TokenSize::Massive.color(),
        ];
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j]);
            }
        }
    }

    #[test]
    fn test_token_creation() {
        let token = Token::new("hello", TokenSize::Medium);
        assert_eq!(token.text, "hello");
        assert_eq!(token.size, TokenSize::Medium);
    }

    #[test]
    fn test_token_stream_push_and_len() {
        let mut stream = TokenStream::new(10);
        assert!(stream.is_empty());
        assert_eq!(stream.len(), 0);

        stream.push(Token::new("a", TokenSize::Small));
        assert_eq!(stream.len(), 1);
        assert!(!stream.is_empty());

        stream.push(Token::new("b", TokenSize::Medium));
        assert_eq!(stream.len(), 2);
    }

    #[test]
    fn test_token_stream_eviction() {
        let mut stream = TokenStream::new(3);

        stream.push(Token::new("a", TokenSize::Small));
        stream.push(Token::new("b", TokenSize::Small));
        stream.push(Token::new("c", TokenSize::Small));
        assert_eq!(stream.len(), 3);

        // Push a 4th token, oldest should be evicted
        stream.push(Token::new("d", TokenSize::Small));
        assert_eq!(stream.len(), 3);
        assert_eq!(stream.total_processed(), 4);

        // The oldest token "a" should be gone
        let texts: Vec<&str> = stream.tokens().iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, vec!["b", "c", "d"]);
    }

    #[test]
    fn test_token_stream_total_processed() {
        let mut stream = TokenStream::new(2);
        stream.push(Token::new("a", TokenSize::Small));
        stream.push(Token::new("b", TokenSize::Small));
        stream.push(Token::new("c", TokenSize::Small));

        assert_eq!(stream.total_processed(), 3);
        assert_eq!(stream.len(), 2); // only 2 retained
    }

    #[test]
    fn test_token_stream_push_text() {
        let mut stream = TokenStream::new(10);
        stream.push_text("hello", 5);

        assert_eq!(stream.len(), 1);
        let token = &stream.tokens()[0];
        assert_eq!(token.text, "hello");
        assert_eq!(token.size, TokenSize::Small);
    }

    #[test]
    fn test_token_stream_clear() {
        let mut stream = TokenStream::new(10);
        stream.push(Token::new("a", TokenSize::Small));
        stream.push(Token::new("b", TokenSize::Medium));
        assert_eq!(stream.len(), 2);

        stream.clear();
        assert!(stream.is_empty());
        // total_processed should not be reset by clear
        assert_eq!(stream.total_processed(), 2);
    }

    #[test]
    fn test_token_stream_tick() {
        let mut stream = TokenStream::new(10);
        assert_eq!(stream.animation_frame(), 0);

        stream.tick();
        assert_eq!(stream.animation_frame(), 1);

        // Tick 255 more times to wrap
        for _ in 0..255 {
            stream.tick();
        }
        assert_eq!(stream.animation_frame(), 0);
    }

    #[test]
    fn test_token_stream_clone() {
        let mut stream = TokenStream::new(5);
        stream.push(Token::new("x", TokenSize::Large));
        stream.tick();

        let cloned = stream.clone();
        assert_eq!(cloned.len(), stream.len());
        assert_eq!(cloned.animation_frame(), stream.animation_frame());
        assert_eq!(cloned.total_processed(), stream.total_processed());
    }

    #[test]
    fn test_wave_background_returns_rgb() {
        let stream = TokenStream::new(10);
        let color = stream.wave_background(5);
        assert!(matches!(color, Color::Rgb(_, _, _)));
    }
}
