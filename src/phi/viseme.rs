//! Phi Fox 10-viseme mouth shape & speech lip-sync engine.
//!
//! Shared across the native ratatui TUI (`src/ui/tui/phi_arena.rs`), the Web
//! Policy Twin (`src/evolve/web/phi/phi_rig.js`), and `scripts/phi_tmux_game.py`.

/// 10-viseme mouth shapes matching the Web `PhiVisemeEngine` and `scripts/phi_tmux_game.py`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Viseme {
    Rest,
    Mbp,
    Etc,
    Ai,
    E,
    O,
    U,
    Fv,
    LTh,
    Wq,
}

impl Viseme {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Rest => "rest",
            Self::Mbp => "mbp",
            Self::Etc => "etc",
            Self::Ai => "ai",
            Self::E => "e",
            Self::O => "o",
            Self::U => "u",
            Self::Fv => "fv",
            Self::LTh => "l_th",
            Self::Wq => "wq",
        }
    }

    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Rest => "───",
            Self::Mbp => "═══",
            Self::Etc => "─▫─",
            Self::Ai => "◁◯▷",
            Self::E => "◁═▷",
            Self::O => "(◯)",
            Self::U => "(▫)",
            Self::Fv => "─▿─",
            Self::LTh => "─△─",
            Self::Wq => "∘▫∘",
        }
    }
}

/// Map a single word to its primary viseme mouth shape.
pub fn word_to_viseme(word: &str) -> Viseme {
    let cleaned: String = word
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if cleaned.is_empty() {
        return Viseme::Rest;
    }
    if cleaned.starts_with('m') || cleaned.starts_with('b') || cleaned.starts_with('p') {
        return Viseme::Mbp;
    }
    if cleaned.starts_with('f') || cleaned.starts_with('v') {
        return Viseme::Fv;
    }
    if cleaned.starts_with('l') || cleaned.starts_with("th") {
        return Viseme::LTh;
    }
    if cleaned.starts_with('w') || cleaned.starts_with("qu") || cleaned.starts_with('q') {
        return Viseme::Wq;
    }
    for ch in cleaned.chars() {
        match ch {
            'a' => return Viseme::Ai,
            'e' => return Viseme::E,
            'o' => return Viseme::O,
            'u' => return Viseme::U,
            _ => {}
        }
    }
    Viseme::Etc
}

/// Half-open time window `[start_ms, end_ms)` paired with a mouth [`Viseme`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VisemeSpan {
    pub start_ms: u64,
    pub end_ms: u64,
    pub viseme: Viseme,
}

impl VisemeSpan {
    /// Returns `true` iff `elapsed_ms` lies in the half-open interval `[start_ms, end_ms)`.
    #[inline]
    pub const fn contains(&self, elapsed_ms: u64) -> bool {
        self.start_ms <= elapsed_ms && elapsed_ms < self.end_ms
    }
}

/// Look up the active [`Viseme`] at `elapsed_ms` across a timed schedule,
/// falling back to [`Viseme::Rest`] whenever `elapsed_ms` is outside all `[start_ms, end_ms)` spans.
pub fn active_viseme_at(schedule: &[VisemeSpan], elapsed_ms: u64) -> Viseme {
    for span in schedule {
        if span.contains(elapsed_ms) {
            return span.viseme;
        }
    }
    Viseme::Rest
}

/// Convert spoken text into a bounded sequence of viseme frames ending at `Viseme::Rest`.
pub fn text_to_viseme_frames(text: &str, max_frames: usize) -> Vec<Viseme> {
    if max_frames <= 1 {
        return vec![Viseme::Rest];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![Viseme::Rest];
    }
    let cap = max_frames - 1;
    let mut frames: Vec<Viseme> = words.into_iter().take(cap).map(word_to_viseme).collect();
    frames.push(Viseme::Rest);
    frames
}
