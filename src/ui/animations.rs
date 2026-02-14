//! Animation Framework for Selfware UI
//!
//! Sophisticated ASCII animations with timing control for terminal interfaces.
//! Provides spinners, progress bars, wave effects, and color cycling.

use std::time::{Duration, Instant};

// ============================================================================
// Animation Trait
// ============================================================================

/// Trait for creating animated terminal effects
pub trait Animation: Send + Sync {
    /// Generate the frame for a given tick
    fn frame(&self, tick: u64) -> String;

    /// Check if the animation has completed (for finite animations)
    fn is_complete(&self, tick: u64) -> bool {
        let _ = tick;
        false // Most animations loop indefinitely
    }

    /// Get the recommended frame rate (FPS)
    fn frame_rate(&self) -> u32 {
        10
    }
}

// ============================================================================
// Spinner Presets
// ============================================================================

/// Dot spinner frames (braille-based)
pub const SPINNER_DOTS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Braille spinner frames
pub const SPINNER_BRAILLE: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];

/// Arrow spinner frames
pub const SPINNER_ARROWS: &[&str] = &["←", "↖", "↑", "↗", "→", "↘", "↓", "↙"];

/// Bounce spinner frames
pub const SPINNER_BOUNCE: &[&str] = &["⠁", "⠂", "⠄", "⡀", "⢀", "⠠", "⠐", "⠈"];

/// Clock spinner frames
pub const SPINNER_CLOCK: &[&str] = &[
    "🕐", "🕑", "🕒", "🕓", "🕔", "🕕", "🕖", "🕗", "🕘", "🕙", "🕚", "🕛",
];

/// Garden growth spinner (Selfware theme)
pub const SPINNER_GARDEN: &[&str] = &["🌱", "🌿", "🍃", "🌳"];

/// Moon phase spinner
pub const SPINNER_MOON: &[&str] = &["🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘"];

/// Box drawing spinner
pub const SPINNER_BOX: &[&str] = &["┌", "┐", "┘", "└"];

/// Line spinner
pub const SPINNER_LINE: &[&str] = &["-", "\\", "|", "/"];

/// Arc spinner
pub const SPINNER_ARC: &[&str] = &["◜", "◠", "◝", "◞", "◡", "◟"];

// ============================================================================
// Progress Bar Presets
// ============================================================================

/// Block progress characters (light to dark)
pub const PROGRESS_BLOCKS: &[char] = &['░', '▒', '▓', '█'];

/// Shade progress characters (granular)
pub const PROGRESS_SHADES: &[char] = &[' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// Simple progress characters
pub const PROGRESS_SIMPLE: &[char] = &[' ', '█'];

/// ASCII progress characters
pub const PROGRESS_ASCII: &[char] = &[' ', '#'];

/// Dot progress characters
pub const PROGRESS_DOTS: &[char] = &[' ', '·', '•', '●'];

// ============================================================================
// Wave Animation Characters
// ============================================================================

/// Wave height characters (vertical bars)
pub const WAVE_BARS: &[&str] = &[
    "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂",
];

/// Sine wave pattern for horizontal waves
pub const WAVE_SINE: &[&str] = &["~", "≈", "≋", "≈"];

/// Water wave characters
pub const WAVE_WATER: &[&str] = &["~", "≈", "~", "-"];

// ============================================================================
// Animator Controller
// ============================================================================

/// Animation controller with timing
pub struct Animator {
    frame_rate: u32,
    last_frame: Instant,
    tick: u64,
}

impl Default for Animator {
    fn default() -> Self {
        Self::new(10)
    }
}

impl Animator {
    /// Create a new animator with the given frame rate
    pub fn new(frame_rate: u32) -> Self {
        Self {
            frame_rate,
            last_frame: Instant::now(),
            tick: 0,
        }
    }

    /// Check if it's time to advance and return the new frame
    pub fn update(&mut self) -> Option<u64> {
        let frame_duration = Duration::from_millis(1000 / self.frame_rate as u64);
        if self.last_frame.elapsed() >= frame_duration {
            self.last_frame = Instant::now();
            self.tick += 1;
            Some(self.tick)
        } else {
            None
        }
    }

    /// Get the current tick
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Reset the animator
    pub fn reset(&mut self) {
        self.tick = 0;
        self.last_frame = Instant::now();
    }

    /// Set the frame rate
    pub fn set_frame_rate(&mut self, fps: u32) {
        self.frame_rate = fps.max(1);
    }

    /// Get the frame rate
    pub fn frame_rate(&self) -> u32 {
        self.frame_rate
    }
}

// ============================================================================
// Spinner Animation
// ============================================================================

/// A spinner animation using predefined frames
pub struct SpinnerAnimation {
    frames: Vec<&'static str>,
    message: String,
}

impl SpinnerAnimation {
    /// Create a spinner with the given frames
    pub fn new(frames: &[&'static str]) -> Self {
        Self {
            frames: frames.to_vec(),
            message: String::new(),
        }
    }

    /// Create a dots spinner
    pub fn dots() -> Self {
        Self::new(SPINNER_DOTS)
    }

    /// Create a braille spinner
    pub fn braille() -> Self {
        Self::new(SPINNER_BRAILLE)
    }

    /// Create an arrows spinner
    pub fn arrows() -> Self {
        Self::new(SPINNER_ARROWS)
    }

    /// Create a bounce spinner
    pub fn bounce() -> Self {
        Self::new(SPINNER_BOUNCE)
    }

    /// Create a garden spinner
    pub fn garden() -> Self {
        Self::new(SPINNER_GARDEN)
    }

    /// Create a line spinner
    pub fn line() -> Self {
        Self::new(SPINNER_LINE)
    }

    /// Create an arc spinner
    pub fn arc() -> Self {
        Self::new(SPINNER_ARC)
    }

    /// Set the message
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl Animation for SpinnerAnimation {
    fn frame(&self, tick: u64) -> String {
        let idx = (tick as usize) % self.frames.len();
        if self.message.is_empty() {
            self.frames[idx].to_string()
        } else {
            format!("{} {}", self.frames[idx], self.message)
        }
    }
}

// ============================================================================
// Wave Animation
// ============================================================================

/// A wave animation that cascades across the width
pub struct WaveAnimation {
    width: usize,
    chars: Vec<&'static str>,
}

impl WaveAnimation {
    /// Create a new wave animation
    pub fn new(width: usize) -> Self {
        Self {
            width,
            chars: WAVE_BARS.to_vec(),
        }
    }

    /// Create with custom characters
    pub fn with_chars(mut self, chars: &[&'static str]) -> Self {
        self.chars = chars.to_vec();
        self
    }
}

impl Animation for WaveAnimation {
    fn frame(&self, tick: u64) -> String {
        let char_count = self.chars.len();
        (0..self.width)
            .map(|i| {
                let phase = (tick as usize + i) % char_count;
                self.chars[phase]
            })
            .collect()
    }
}

// ============================================================================
// Progress Bar Animation
// ============================================================================

/// An animated progress bar
pub struct ProgressAnimation {
    width: usize,
    progress: f64,
    chars: Vec<char>,
    show_percentage: bool,
}

impl ProgressAnimation {
    /// Create a new progress bar
    pub fn new(width: usize) -> Self {
        Self {
            width,
            progress: 0.0,
            chars: PROGRESS_SHADES.to_vec(),
            show_percentage: true,
        }
    }

    /// Set the progress (0.0 to 1.0)
    pub fn set_progress(&mut self, progress: f64) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    /// Get current progress
    pub fn progress(&self) -> f64 {
        self.progress
    }

    /// Use block characters
    pub fn with_blocks(mut self) -> Self {
        self.chars = PROGRESS_BLOCKS.to_vec();
        self
    }

    /// Use ASCII characters
    pub fn with_ascii(mut self) -> Self {
        self.chars = PROGRESS_ASCII.to_vec();
        self
    }

    /// Hide percentage
    pub fn hide_percentage(mut self) -> Self {
        self.show_percentage = false;
        self
    }

    /// Render the progress bar
    fn render_bar(&self) -> String {
        let fill_width = (self.progress * self.width as f64) as usize;
        let partial = ((self.progress * self.width as f64) - fill_width as f64)
            * (self.chars.len() - 1) as f64;
        let partial_idx = partial as usize;

        let mut result = String::with_capacity(self.width + 10);

        // Full blocks
        let full_char = *self.chars.last().unwrap_or(&'█');
        for _ in 0..fill_width.min(self.width) {
            result.push(full_char);
        }

        // Partial block
        if fill_width < self.width && partial_idx > 0 {
            result.push(self.chars[partial_idx]);
        }

        // Empty space
        let empty_char = *self.chars.first().unwrap_or(&' ');
        let filled = result.chars().count();
        for _ in filled..self.width {
            result.push(empty_char);
        }

        result
    }
}

impl Animation for ProgressAnimation {
    fn frame(&self, _tick: u64) -> String {
        let bar = self.render_bar();
        if self.show_percentage {
            format!("[{}] {:.0}%", bar, self.progress * 100.0)
        } else {
            format!("[{}]", bar)
        }
    }

    fn is_complete(&self, _tick: u64) -> bool {
        self.progress >= 1.0
    }
}

// ============================================================================
// Progress Worm Animation
// ============================================================================

/// A "worm" that crawls through the progress bar
pub struct ProgressWormAnimation {
    width: usize,
    worm_length: usize,
}

impl ProgressWormAnimation {
    /// Create a new progress worm
    pub fn new(width: usize) -> Self {
        Self {
            width,
            worm_length: 3.min(width / 4).max(1),
        }
    }

    /// Set worm length
    pub fn with_length(mut self, length: usize) -> Self {
        self.worm_length = length.max(1);
        self
    }
}

impl Animation for ProgressWormAnimation {
    fn frame(&self, tick: u64) -> String {
        let cycle_length = self.width + self.worm_length;
        let pos = (tick as usize) % cycle_length;

        let mut result = String::with_capacity(self.width);
        for i in 0..self.width {
            let relative_pos = if pos >= i {
                pos - i
            } else {
                cycle_length - i + pos
            };
            if relative_pos < self.worm_length {
                // Gradient effect
                let intensity = self.worm_length - relative_pos;
                match intensity {
                    1 => result.push('░'),
                    2 => result.push('▒'),
                    _ => result.push('▓'),
                }
            } else {
                result.push(' ');
            }
        }
        result
    }
}

// ============================================================================
// Pulse Animation
// ============================================================================

/// A pulsing circle animation
pub struct PulseAnimation {
    chars: Vec<&'static str>,
}

impl Default for PulseAnimation {
    fn default() -> Self {
        Self::new()
    }
}

impl PulseAnimation {
    /// Create a new pulse animation
    pub fn new() -> Self {
        Self {
            chars: vec!["◯", "◔", "◑", "◕", "●", "◕", "◑", "◔"],
        }
    }

    /// Create with custom characters
    pub fn with_chars(mut self, chars: Vec<&'static str>) -> Self {
        self.chars = chars;
        self
    }
}

impl Animation for PulseAnimation {
    fn frame(&self, tick: u64) -> String {
        let idx = (tick as usize) % self.chars.len();
        self.chars[idx].to_string()
    }
}

// ============================================================================
// Matrix Rain Animation
// ============================================================================

/// Matrix-style falling characters
pub struct MatrixRainAnimation {
    width: usize,
    height: usize,

    #[allow(dead_code)] // Tracks column drop positions for tick() animation
    columns: Vec<usize>,
}

impl MatrixRainAnimation {
    /// Create a new matrix rain animation
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            columns: vec![0; width],
        }
    }

    /// Get a random-ish character based on position and tick
    fn get_char(tick: u64, col: usize, row: usize) -> char {
        let seed = tick
            .wrapping_add(col as u64 * 17)
            .wrapping_add(row as u64 * 31);
        match seed % 36 {
            0..=9 => (b'0' + (seed % 10) as u8) as char,
            10..=35 => (b'A' + ((seed - 10) % 26) as u8) as char,
            _ => '█',
        }
    }
}

impl Animation for MatrixRainAnimation {
    fn frame(&self, tick: u64) -> String {
        let mut lines = Vec::with_capacity(self.height);

        for row in 0..self.height {
            let mut line = String::with_capacity(self.width);
            for col in 0..self.width {
                // Determine if this cell should be "lit"
                let phase = (tick as usize + col * 7) % (self.height * 2);
                let drop_row = phase % self.height;
                let distance = if row >= drop_row {
                    row - drop_row
                } else {
                    self.height - drop_row + row
                };

                if distance < 3 {
                    line.push(Self::get_char(tick, col, row));
                } else {
                    line.push(' ');
                }
            }
            lines.push(line);
        }

        lines.join("\n")
    }
}

// ============================================================================
// Sparkle Animation
// ============================================================================

/// Twinkling sparkle effect
pub struct SparkleAnimation {
    width: usize,
    density: f64,
}

impl SparkleAnimation {
    /// Create a new sparkle animation
    pub fn new(width: usize) -> Self {
        Self {
            width,
            density: 0.3,
        }
    }

    /// Set sparkle density (0.0 to 1.0)
    pub fn with_density(mut self, density: f64) -> Self {
        self.density = density.clamp(0.0, 1.0);
        self
    }
}

impl Animation for SparkleAnimation {
    fn frame(&self, tick: u64) -> String {
        let chars = ['✨', '⭐', '🌟', '💫', '*', '·', '.', ' '];
        (0..self.width)
            .map(|i| {
                // Simple pseudo-random selection based on position and tick
                let seed = tick.wrapping_mul(31).wrapping_add(i as u64 * 17);
                let threshold = (self.density * 100.0) as u64;
                if (seed % 100) < threshold {
                    let char_idx = (seed / 100) as usize % (chars.len() - 1);
                    chars[char_idx]
                } else {
                    ' '
                }
            })
            .collect()
    }
}

// ============================================================================
// Fire Animation
// ============================================================================

/// Flickering fire effect
pub struct FireAnimation {
    width: usize,
    height: usize,
}

impl FireAnimation {
    /// Create a new fire animation
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }
}

impl Animation for FireAnimation {
    fn frame(&self, tick: u64) -> String {
        let fire_chars = [' ', '.', ':', '°', '*', '#', '@'];
        let mut lines = Vec::with_capacity(self.height);

        for row in 0..self.height {
            let mut line = String::with_capacity(self.width);
            for col in 0..self.width {
                // Higher intensity at bottom
                let base_intensity = (self.height - row) as u64 * 2;
                let variation = (tick.wrapping_add((col as u64) * 23)) % 5;
                let intensity = (base_intensity + variation) as usize;
                let char_idx = intensity.min(fire_chars.len() - 1);
                line.push(fire_chars[char_idx]);
            }
            lines.push(line);
        }

        lines.join("\n")
    }
}

// ============================================================================
// Color Cycling
// ============================================================================

/// Color cycling modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CycleMode {
    /// Loop through colors continuously
    #[default]
    Loop,
    /// Bounce back and forth
    Bounce,
    /// Random selection
    Random,
}

/// ANSI color codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    /// Create a new RGB color
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Get ANSI escape code for foreground
    pub fn fg_code(&self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }

    /// Get ANSI escape code for background
    pub fn bg_code(&self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }

    /// Blend two colors
    pub fn blend(c1: Color, c2: Color, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        Color {
            r: ((c1.r as f64) * (1.0 - t) + (c2.r as f64) * t) as u8,
            g: ((c1.g as f64) * (1.0 - t) + (c2.g as f64) * t) as u8,
            b: ((c1.b as f64) * (1.0 - t) + (c2.b as f64) * t) as u8,
        }
    }
}

/// Reset ANSI code
pub const RESET_CODE: &str = "\x1b[0m";

/// Predefined color palettes
pub mod palettes {
    use super::Color;

    /// Selfware warm palette
    pub const SUNSET: &[Color] = &[
        Color::rgb(212, 163, 115), // AMBER
        Color::rgb(184, 115, 51),  // COPPER
        Color::rgb(139, 69, 19),   // RUST
        Color::rgb(188, 108, 37),  // SOIL_BROWN
    ];

    /// Ocean/garden palette
    pub const OCEAN: &[Color] = &[
        Color::rgb(143, 151, 121), // SAGE
        Color::rgb(96, 108, 56),   // GARDEN_GREEN
        Color::rgb(144, 190, 109), // BLOOM
    ];

    /// Fire palette
    pub const FIRE: &[Color] = &[
        Color::rgb(212, 163, 115), // AMBER
        Color::rgb(184, 115, 51),  // COPPER
        Color::rgb(139, 69, 19),   // RUST
        Color::rgb(212, 163, 115), // AMBER (back)
    ];

    /// Cool blues
    pub const ICE: &[Color] = &[
        Color::rgb(100, 149, 237), // Cornflower
        Color::rgb(135, 206, 250), // Light sky
        Color::rgb(176, 224, 230), // Powder blue
        Color::rgb(173, 216, 230), // Light blue
    ];

    /// Rainbow
    pub const RAINBOW: &[Color] = &[
        Color::rgb(255, 0, 0),   // Red
        Color::rgb(255, 127, 0), // Orange
        Color::rgb(255, 255, 0), // Yellow
        Color::rgb(0, 255, 0),   // Green
        Color::rgb(0, 0, 255),   // Blue
        Color::rgb(75, 0, 130),  // Indigo
        Color::rgb(148, 0, 211), // Violet
    ];
}

/// Color cycler for animated color effects
pub struct ColorCycler {
    colors: Vec<Color>,
    mode: CycleMode,
    speed: u64,
}

impl ColorCycler {
    /// Create a new color cycler
    pub fn new(colors: Vec<Color>) -> Self {
        Self {
            colors,
            mode: CycleMode::Loop,
            speed: 1,
        }
    }

    /// Create from a preset palette
    pub fn from_palette(palette: &[Color]) -> Self {
        Self::new(palette.to_vec())
    }

    /// Set cycling mode
    pub fn with_mode(mut self, mode: CycleMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set cycling speed
    pub fn with_speed(mut self, speed: u64) -> Self {
        self.speed = speed.max(1);
        self
    }

    /// Get the current color for a given tick
    pub fn color_at(&self, tick: u64) -> Color {
        if self.colors.is_empty() {
            return Color::rgb(255, 255, 255);
        }

        let adjusted_tick = tick / self.speed;

        match self.mode {
            CycleMode::Loop => {
                let idx = (adjusted_tick as usize) % self.colors.len();
                self.colors[idx]
            }
            CycleMode::Bounce => {
                let cycle_len = (self.colors.len() * 2).saturating_sub(2).max(1);
                let pos = (adjusted_tick as usize) % cycle_len;
                if pos < self.colors.len() {
                    self.colors[pos]
                } else {
                    let reverse_pos = cycle_len - pos;
                    self.colors[reverse_pos]
                }
            }
            CycleMode::Random => {
                // Pseudo-random based on tick
                let idx = ((adjusted_tick * 31 + 17) as usize) % self.colors.len();
                self.colors[idx]
            }
        }
    }

    /// Get interpolated color (smooth transitions)
    pub fn smooth_color_at(&self, tick: u64, steps_per_color: u64) -> Color {
        if self.colors.is_empty() {
            return Color::rgb(255, 255, 255);
        }
        if self.colors.len() == 1 {
            return self.colors[0];
        }

        let total_steps = self.colors.len() as u64 * steps_per_color;
        let pos = tick % total_steps;
        let color_idx = (pos / steps_per_color) as usize;
        let blend_factor = (pos % steps_per_color) as f64 / steps_per_color as f64;

        let c1 = self.colors[color_idx];
        let c2 = self.colors[(color_idx + 1) % self.colors.len()];

        Color::blend(c1, c2, blend_factor)
    }
}

// ============================================================================
// Animated Status Display
// ============================================================================

/// Animated status display combining spinner, message, and timer
pub struct AnimatedStatus {
    spinner: SpinnerAnimation,
    message: String,
    start_time: Instant,
    show_elapsed: bool,
}

impl AnimatedStatus {
    /// Create a new animated status
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            spinner: SpinnerAnimation::dots(),
            message: message.into(),
            start_time: Instant::now(),
            show_elapsed: true,
        }
    }

    /// Use a different spinner
    pub fn with_spinner(mut self, spinner: SpinnerAnimation) -> Self {
        self.spinner = spinner;
        self
    }

    /// Hide elapsed time
    pub fn hide_elapsed(mut self) -> Self {
        self.show_elapsed = false;
        self
    }

    /// Update the message
    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
    }

    /// Get elapsed duration
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Reset the timer
    pub fn reset_timer(&mut self) {
        self.start_time = Instant::now();
    }
}

impl Animation for AnimatedStatus {
    fn frame(&self, tick: u64) -> String {
        let spinner = self.spinner.frame(tick);
        if self.show_elapsed {
            let elapsed = self.start_time.elapsed();
            let secs = elapsed.as_secs();
            let millis = elapsed.subsec_millis() / 100;
            format!("{} {} [{}.{}s]", spinner, self.message, secs, millis)
        } else {
            format!("{} {}", spinner, self.message)
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Format a duration as a human-readable string
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    let millis = duration.subsec_millis();

    if hours > 0 {
        format!("{}h {:02}m {:02}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {:02}s", mins, secs)
    } else if secs > 0 {
        format!("{}.{:03}s", secs, millis)
    } else {
        format!("{}ms", millis)
    }
}

/// Create a simple text-based progress bar
pub fn simple_progress_bar(progress: f64, width: usize) -> String {
    let filled = ((progress * width as f64) as usize).min(width);
    let empty = width - filled;
    format!(
        "[{}{}] {:.0}%",
        "█".repeat(filled),
        "░".repeat(empty),
        progress * 100.0
    )
}

/// Create a gradient progress bar with ANSI colors
pub fn gradient_progress_bar(progress: f64, width: usize, start: Color, end: Color) -> String {
    let filled = ((progress * width as f64) as usize).min(width);
    let mut result = String::new();

    result.push('[');
    for i in 0..width {
        let t = i as f64 / width as f64;
        let color = Color::blend(start, end, t);
        if i < filled {
            result.push_str(&format!("{}█", color.fg_code()));
        } else {
            result.push_str(&format!("{}░", color.fg_code()));
        }
    }
    result.push_str(RESET_CODE);
    result.push_str(&format!("] {:.0}%", progress * 100.0));

    result
}

// ============================================================================
// Multi-Step Progress Tracker
// ============================================================================

/// Status of a phase in the multi-step progress
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseStatus {
    /// Phase not started
    Pending,
    /// Phase currently executing
    Active,
    /// Phase completed successfully
    Completed,
    /// Phase failed
    Failed,
    /// Phase skipped
    Skipped,
}

/// A single phase in multi-step progress
#[derive(Debug, Clone)]
pub struct ProgressPhase {
    /// Phase name
    pub name: String,
    /// Current status
    pub status: PhaseStatus,
    /// Progress within this phase (0.0 to 1.0)
    pub progress: f64,
}

/// Multi-step progress tracker for complex tasks
pub struct MultiStepProgress {
    phases: Vec<ProgressPhase>,
    current_phase: usize,
    start_time: std::time::Instant,
}

impl MultiStepProgress {
    /// Create a new multi-step progress tracker
    pub fn new(phase_names: &[&str]) -> Self {
        Self {
            phases: phase_names
                .iter()
                .map(|name| ProgressPhase {
                    name: name.to_string(),
                    status: PhaseStatus::Pending,
                    progress: 0.0,
                })
                .collect(),
            current_phase: 0,
            start_time: std::time::Instant::now(),
        }
    }

    /// Start the current phase
    pub fn start_phase(&mut self) {
        if self.current_phase < self.phases.len() {
            self.phases[self.current_phase].status = PhaseStatus::Active;
        }
    }

    /// Update progress of current phase
    pub fn update_progress(&mut self, progress: f64) {
        if self.current_phase < self.phases.len() {
            self.phases[self.current_phase].progress = progress.clamp(0.0, 1.0);
        }
    }

    /// Complete current phase and move to next
    pub fn complete_phase(&mut self) {
        if self.current_phase < self.phases.len() {
            self.phases[self.current_phase].status = PhaseStatus::Completed;
            self.phases[self.current_phase].progress = 1.0;
            self.current_phase += 1;
            if self.current_phase < self.phases.len() {
                self.phases[self.current_phase].status = PhaseStatus::Active;
            }
        }
    }

    /// Mark current phase as failed
    pub fn fail_phase(&mut self) {
        if self.current_phase < self.phases.len() {
            self.phases[self.current_phase].status = PhaseStatus::Failed;
        }
    }

    /// Get overall progress (0.0 to 1.0)
    pub fn overall_progress(&self) -> f64 {
        let completed: f64 = self
            .phases
            .iter()
            .map(|p| match p.status {
                PhaseStatus::Completed => 1.0,
                PhaseStatus::Active => p.progress,
                _ => 0.0,
            })
            .sum();
        completed / self.phases.len() as f64
    }

    /// Estimate remaining time based on elapsed time and progress
    pub fn estimated_remaining(&self) -> Option<std::time::Duration> {
        let progress = self.overall_progress();
        if progress > 0.01 {
            let elapsed = self.start_time.elapsed();
            let estimated_total = elapsed.as_secs_f64() / progress;
            let remaining = estimated_total - elapsed.as_secs_f64();
            if remaining > 0.0 {
                return Some(std::time::Duration::from_secs_f64(remaining));
            }
        }
        None
    }

    /// Render the progress display
    pub fn render(&self) -> String {
        use colored::Colorize;
        use super::theme::current_theme;

        let theme = current_theme();
        let mut result = String::new();

        for (i, phase) in self.phases.iter().enumerate() {
            let (icon, color) = match phase.status {
                PhaseStatus::Pending => ("○", theme.muted),
                PhaseStatus::Active => ("●", theme.accent),
                PhaseStatus::Completed => ("✓", theme.success),
                PhaseStatus::Failed => ("✗", theme.error),
                PhaseStatus::Skipped => ("◌", theme.muted),
            };

            let phase_num = format!("{}/{}", i + 1, self.phases.len());
            let progress_str = if phase.status == PhaseStatus::Active && phase.progress > 0.0 {
                format!(" [{:.0}%]", phase.progress * 100.0)
            } else {
                String::new()
            };

            result.push_str(&format!(
                "{} Phase {}: {}{}",
                icon.custom_color(color),
                phase_num.custom_color(theme.muted),
                phase.name.custom_color(if phase.status == PhaseStatus::Active {
                    theme.primary
                } else {
                    theme.muted
                }),
                progress_str.custom_color(theme.accent)
            ));
            result.push('\n');
        }

        // Add ETA if available
        if let Some(remaining) = self.estimated_remaining() {
            let secs = remaining.as_secs();
            let eta = if secs >= 60 {
                format!("~{}m {}s", secs / 60, secs % 60)
            } else {
                format!("~{}s", secs)
            };
            result.push_str(&format!(
                "\n{} {}",
                "ETA:".custom_color(theme.muted),
                eta.custom_color(theme.accent)
            ));
        }

        result
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::const_is_empty)]
mod tests {
    use super::*;

    #[test]
    fn test_animator_creation() {
        let animator = Animator::new(10);
        assert_eq!(animator.frame_rate(), 10);
        assert_eq!(animator.tick(), 0);
    }

    #[test]
    fn test_animator_default() {
        let animator = Animator::default();
        assert_eq!(animator.frame_rate(), 10);
    }

    #[test]
    fn test_animator_reset() {
        let mut animator = Animator::new(10);
        animator.tick = 100;
        animator.reset();
        assert_eq!(animator.tick(), 0);
    }

    #[test]
    fn test_animator_set_frame_rate() {
        let mut animator = Animator::new(10);
        animator.set_frame_rate(30);
        assert_eq!(animator.frame_rate(), 30);

        // Should not allow 0
        animator.set_frame_rate(0);
        assert_eq!(animator.frame_rate(), 1);
    }

    #[test]
    fn test_spinner_animation_dots() {
        let spinner = SpinnerAnimation::dots();
        assert_eq!(spinner.frame(0), "⠋");
        assert_eq!(spinner.frame(1), "⠙");
        assert_eq!(spinner.frame(10), "⠋"); // Wraps
    }

    #[test]
    fn test_spinner_animation_with_message() {
        let spinner = SpinnerAnimation::dots().with_message("Loading");
        let frame = spinner.frame(0);
        assert!(frame.contains("Loading"));
        assert!(frame.contains("⠋"));
    }

    #[test]
    fn test_spinner_variants() {
        let _braille = SpinnerAnimation::braille();
        let _arrows = SpinnerAnimation::arrows();
        let _bounce = SpinnerAnimation::bounce();
        let _garden = SpinnerAnimation::garden();
        let _line = SpinnerAnimation::line();
        let _arc = SpinnerAnimation::arc();
    }

    #[test]
    fn test_wave_animation() {
        let wave = WaveAnimation::new(10);
        let frame = wave.frame(0);
        assert_eq!(frame.chars().count(), 10);
    }

    #[test]
    fn test_wave_animation_changes() {
        let wave = WaveAnimation::new(5);
        let frame1 = wave.frame(0);
        let frame2 = wave.frame(1);
        assert_ne!(frame1, frame2);
    }

    #[test]
    fn test_progress_animation() {
        let mut progress = ProgressAnimation::new(20);
        progress.set_progress(0.5);
        assert_eq!(progress.progress(), 0.5);

        let frame = progress.frame(0);
        assert!(frame.contains("50%"));
    }

    #[test]
    fn test_progress_animation_clamping() {
        let mut progress = ProgressAnimation::new(20);
        progress.set_progress(1.5);
        assert_eq!(progress.progress(), 1.0);

        progress.set_progress(-0.5);
        assert_eq!(progress.progress(), 0.0);
    }

    #[test]
    fn test_progress_animation_complete() {
        let mut progress = ProgressAnimation::new(10);
        progress.set_progress(0.5);
        assert!(!progress.is_complete(0));

        progress.set_progress(1.0);
        assert!(progress.is_complete(0));
    }

    #[test]
    fn test_progress_animation_variants() {
        let blocks = ProgressAnimation::new(10).with_blocks();
        let _ = blocks.frame(0);

        let ascii = ProgressAnimation::new(10).with_ascii();
        let _ = ascii.frame(0);

        let no_percent = ProgressAnimation::new(10).hide_percentage();
        let frame = no_percent.frame(0);
        assert!(!frame.contains('%'));
    }

    #[test]
    fn test_progress_worm_animation() {
        let worm = ProgressWormAnimation::new(20);
        let frame1 = worm.frame(0);
        let frame2 = worm.frame(1);
        assert_ne!(frame1, frame2);
    }

    #[test]
    fn test_progress_worm_with_length() {
        let worm = ProgressWormAnimation::new(20).with_length(5);
        assert_eq!(worm.worm_length, 5);
    }

    #[test]
    fn test_pulse_animation() {
        let pulse = PulseAnimation::new();
        let frame = pulse.frame(0);
        assert!(!frame.is_empty());
    }

    #[test]
    fn test_pulse_default() {
        let pulse = PulseAnimation::default();
        assert!(!pulse.chars.is_empty());
    }

    #[test]
    fn test_sparkle_animation() {
        let sparkle = SparkleAnimation::new(20);
        let frame = sparkle.frame(0);
        assert_eq!(frame.chars().count(), 20);
    }

    #[test]
    fn test_sparkle_density() {
        let sparkle = SparkleAnimation::new(100).with_density(0.5);
        assert_eq!(sparkle.density, 0.5);
    }

    #[test]
    fn test_fire_animation() {
        let fire = FireAnimation::new(10, 5);
        let frame = fire.frame(0);
        let lines: Vec<&str> = frame.lines().collect();
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_matrix_rain_animation() {
        let matrix = MatrixRainAnimation::new(10, 5);
        let frame = matrix.frame(0);
        let lines: Vec<&str> = frame.lines().collect();
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_color_rgb() {
        let color = Color::rgb(255, 128, 64);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 128);
        assert_eq!(color.b, 64);
    }

    #[test]
    fn test_color_codes() {
        let color = Color::rgb(255, 0, 0);
        let fg = color.fg_code();
        assert!(fg.contains("255"));
        assert!(fg.contains("38;2"));

        let bg = color.bg_code();
        assert!(bg.contains("255"));
        assert!(bg.contains("48;2"));
    }

    #[test]
    fn test_color_blend() {
        let c1 = Color::rgb(0, 0, 0);
        let c2 = Color::rgb(255, 255, 255);

        let mid = Color::blend(c1, c2, 0.5);
        assert!(mid.r > 100 && mid.r < 150);
        assert!(mid.g > 100 && mid.g < 150);
        assert!(mid.b > 100 && mid.b < 150);
    }

    #[test]
    fn test_color_blend_edges() {
        let c1 = Color::rgb(100, 100, 100);
        let c2 = Color::rgb(200, 200, 200);

        let start = Color::blend(c1, c2, 0.0);
        assert_eq!(start.r, c1.r);

        let end = Color::blend(c1, c2, 1.0);
        assert_eq!(end.r, c2.r);
    }

    #[test]
    fn test_cycle_mode_default() {
        assert_eq!(CycleMode::default(), CycleMode::Loop);
    }

    #[test]
    fn test_color_cycler() {
        let cycler = ColorCycler::from_palette(palettes::SUNSET);
        let color = cycler.color_at(0);
        assert_eq!(color.r, 212); // AMBER
    }

    #[test]
    fn test_color_cycler_loop() {
        let cycler = ColorCycler::new(vec![Color::rgb(255, 0, 0), Color::rgb(0, 255, 0)]);
        assert_eq!(cycler.color_at(0).r, 255);
        assert_eq!(cycler.color_at(1).g, 255);
        assert_eq!(cycler.color_at(2).r, 255); // Loops
    }

    #[test]
    fn test_color_cycler_bounce() {
        let cycler = ColorCycler::new(vec![
            Color::rgb(255, 0, 0),
            Color::rgb(0, 255, 0),
            Color::rgb(0, 0, 255),
        ])
        .with_mode(CycleMode::Bounce);

        // Forward
        assert_eq!(cycler.color_at(0).r, 255);
        assert_eq!(cycler.color_at(1).g, 255);
        assert_eq!(cycler.color_at(2).b, 255);
        // Backward
        assert_eq!(cycler.color_at(3).g, 255);
        assert_eq!(cycler.color_at(4).r, 255);
    }

    #[test]
    fn test_color_cycler_speed() {
        let cycler =
            ColorCycler::new(vec![Color::rgb(255, 0, 0), Color::rgb(0, 255, 0)]).with_speed(2);

        assert_eq!(cycler.color_at(0).r, 255);
        assert_eq!(cycler.color_at(1).r, 255); // Still first color
        assert_eq!(cycler.color_at(2).g, 255); // Now second
    }

    #[test]
    fn test_color_cycler_smooth() {
        let cycler = ColorCycler::new(vec![Color::rgb(0, 0, 0), Color::rgb(255, 255, 255)]);

        let mid = cycler.smooth_color_at(5, 10);
        assert!(mid.r > 100 && mid.r < 150);
    }

    #[test]
    fn test_color_cycler_empty() {
        let cycler = ColorCycler::new(vec![]);
        let color = cycler.color_at(0);
        assert_eq!(color.r, 255); // Default white
    }

    #[test]
    fn test_animated_status() {
        let status = AnimatedStatus::new("Processing");
        let frame = status.frame(0);
        assert!(frame.contains("Processing"));
        assert!(frame.contains("s]")); // Elapsed time
    }

    #[test]
    fn test_animated_status_hide_elapsed() {
        let status = AnimatedStatus::new("Test").hide_elapsed();
        let frame = status.frame(0);
        assert!(!frame.contains('['));
    }

    #[test]
    fn test_animated_status_with_spinner() {
        let status = AnimatedStatus::new("Test").with_spinner(SpinnerAnimation::garden());
        let frame = status.frame(0);
        assert!(frame.contains("🌱"));
    }

    #[test]
    fn test_animated_status_set_message() {
        let mut status = AnimatedStatus::new("Old");
        status.set_message("New");
        let frame = status.frame(0);
        assert!(frame.contains("New"));
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
        assert_eq!(format_duration(Duration::from_secs(5)), "5.000s");
        assert_eq!(format_duration(Duration::from_secs(65)), "1m 05s");
        assert_eq!(format_duration(Duration::from_secs(3665)), "1h 01m 05s");
    }

    #[test]
    fn test_simple_progress_bar() {
        let bar = simple_progress_bar(0.5, 10);
        assert!(bar.contains("50%"));
        assert!(bar.contains('█'));
        assert!(bar.contains('░'));
    }

    #[test]
    fn test_gradient_progress_bar() {
        let start = Color::rgb(255, 0, 0);
        let end = Color::rgb(0, 255, 0);
        let bar = gradient_progress_bar(0.5, 10, start, end);
        assert!(bar.contains("50%"));
        assert!(bar.contains("\x1b[")); // ANSI codes
    }

    #[test]
    fn test_animation_is_complete_default() {
        let spinner = SpinnerAnimation::dots();
        assert!(!spinner.is_complete(100));
    }

    #[test]
    fn test_animation_frame_rate_default() {
        let spinner = SpinnerAnimation::dots();
        assert_eq!(spinner.frame_rate(), 10);
    }

    #[test]
    fn test_spinner_presets_exist() {
        assert!(!SPINNER_DOTS.is_empty());
        assert!(!SPINNER_BRAILLE.is_empty());
        assert!(!SPINNER_ARROWS.is_empty());
        assert!(!SPINNER_BOUNCE.is_empty());
        assert!(!SPINNER_CLOCK.is_empty());
        assert!(!SPINNER_GARDEN.is_empty());
        assert!(!SPINNER_MOON.is_empty());
        assert!(!SPINNER_BOX.is_empty());
        assert!(!SPINNER_LINE.is_empty());
        assert!(!SPINNER_ARC.is_empty());
    }

    #[test]
    fn test_progress_presets_exist() {
        assert!(!PROGRESS_BLOCKS.is_empty());
        assert!(!PROGRESS_SHADES.is_empty());
        assert!(!PROGRESS_SIMPLE.is_empty());
        assert!(!PROGRESS_ASCII.is_empty());
        assert!(!PROGRESS_DOTS.is_empty());
    }

    #[test]
    fn test_wave_presets_exist() {
        assert!(!WAVE_BARS.is_empty());
        assert!(!WAVE_SINE.is_empty());
        assert!(!WAVE_WATER.is_empty());
    }

    #[test]
    fn test_palettes_exist() {
        assert!(!palettes::SUNSET.is_empty());
        assert!(!palettes::OCEAN.is_empty());
        assert!(!palettes::FIRE.is_empty());
        assert!(!palettes::ICE.is_empty());
        assert!(!palettes::RAINBOW.is_empty());
    }

    #[test]
    fn test_wave_with_custom_chars() {
        let wave = WaveAnimation::new(5).with_chars(WAVE_SINE);
        let frame = wave.frame(0);
        assert_eq!(frame.chars().count(), 5);
    }

    #[test]
    fn test_pulse_with_custom_chars() {
        let pulse = PulseAnimation::new().with_chars(vec!["A", "B", "C"]);
        assert_eq!(pulse.frame(0), "A");
        assert_eq!(pulse.frame(1), "B");
        assert_eq!(pulse.frame(2), "C");
    }
}
