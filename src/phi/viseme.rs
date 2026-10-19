//! Phi Fox 10-viseme mouth shape & speech lip-sync engine.
//!
//! Shared across the native ratatui TUI (`src/ui/tui/phi_arena.rs`), the Web
//! Policy Twin (`src/evolve/web/phi/phi_rig.js`), and `scripts/phi_tmux_game.py`.

/// 10-viseme mouth shapes matching the Web `PhiVisemeEngine` and `scripts/phi_tmux_game.py`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
