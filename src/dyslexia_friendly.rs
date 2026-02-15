//! Dyslexia-Friendly Mode
//!
//! Accessibility features for users with dyslexia:
//! - Font preferences (OpenDyslexic, etc.)
//! - Text spacing options
//! - Color overlays
//! - Text-to-speech integration
//! - Reading aids

use std::collections::HashMap;

// ============================================================================
// Font Settings
// ============================================================================

/// Dyslexia-friendly font family
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum DyslexiaFont {
    /// OpenDyslexic font
    #[default]
    OpenDyslexic,
    /// Lexie Readable
    LexieReadable,
    /// Dyslexie font
    Dyslexie,
    /// Comic Sans (commonly used)
    ComicSans,
    /// Arial (clean sans-serif)
    Arial,
    /// Verdana (wide letter spacing)
    Verdana,
    /// Trebuchet MS
    TrebuchetMs,
    /// Century Gothic
    CenturyGothic,
    /// Custom font name
    Custom(String),
}

impl DyslexiaFont {
    pub fn as_str(&self) -> &str {
        match self {
            DyslexiaFont::OpenDyslexic => "OpenDyslexic",
            DyslexiaFont::LexieReadable => "Lexie Readable",
            DyslexiaFont::Dyslexie => "Dyslexie",
            DyslexiaFont::ComicSans => "Comic Sans MS",
            DyslexiaFont::Arial => "Arial",
            DyslexiaFont::Verdana => "Verdana",
            DyslexiaFont::TrebuchetMs => "Trebuchet MS",
            DyslexiaFont::CenturyGothic => "Century Gothic",
            DyslexiaFont::Custom(name) => name,
        }
    }

    /// Get CSS font-family string
    pub fn css_font_family(&self) -> String {
        format!("'{}', sans-serif", self.as_str())
    }
}

/// Font weight
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontWeight {
    Light,
    #[default]
    Regular,
    Medium,
    SemiBold,
    Bold,
}

impl FontWeight {
    pub fn css_value(&self) -> u16 {
        match self {
            FontWeight::Light => 300,
            FontWeight::Regular => 400,
            FontWeight::Medium => 500,
            FontWeight::SemiBold => 600,
            FontWeight::Bold => 700,
        }
    }
}

// ============================================================================
// Spacing Settings
// ============================================================================

/// Letter spacing level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LetterSpacing {
    /// Normal spacing
    Normal,
    /// Slightly increased (0.05em)
    #[default]
    Slight,
    /// Moderate increase (0.1em)
    Moderate,
    /// Large increase (0.15em)
    Large,
    /// Extra large (0.2em)
    ExtraLarge,
}

impl LetterSpacing {
    pub fn em_value(&self) -> f32 {
        match self {
            LetterSpacing::Normal => 0.0,
            LetterSpacing::Slight => 0.05,
            LetterSpacing::Moderate => 0.1,
            LetterSpacing::Large => 0.15,
            LetterSpacing::ExtraLarge => 0.2,
        }
    }

    pub fn css_value(&self) -> String {
        format!("{}em", self.em_value())
    }
}

/// Word spacing level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordSpacing {
    /// Normal spacing
    Normal,
    /// Slight increase (0.1em)
    #[default]
    Slight,
    /// Moderate increase (0.2em)
    Moderate,
    /// Large increase (0.35em)
    Large,
    /// Extra large (0.5em)
    ExtraLarge,
}

impl WordSpacing {
    pub fn em_value(&self) -> f32 {
        match self {
            WordSpacing::Normal => 0.0,
            WordSpacing::Slight => 0.1,
            WordSpacing::Moderate => 0.2,
            WordSpacing::Large => 0.35,
            WordSpacing::ExtraLarge => 0.5,
        }
    }

    pub fn css_value(&self) -> String {
        format!("{}em", self.em_value())
    }
}

/// Line height level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineHeight {
    /// Normal (1.4)
    Normal,
    /// Comfortable (1.6)
    #[default]
    Comfortable,
    /// Relaxed (1.8)
    Relaxed,
    /// Loose (2.0)
    Loose,
    /// Very loose (2.2)
    VeryLoose,
}

impl LineHeight {
    pub fn value(&self) -> f32 {
        match self {
            LineHeight::Normal => 1.4,
            LineHeight::Comfortable => 1.6,
            LineHeight::Relaxed => 1.8,
            LineHeight::Loose => 2.0,
            LineHeight::VeryLoose => 2.2,
        }
    }
}

/// Paragraph spacing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParagraphSpacing {
    /// Normal (1em)
    Normal,
    /// Increased (1.5em)
    #[default]
    Increased,
    /// Large (2em)
    Large,
    /// Extra large (2.5em)
    ExtraLarge,
}

impl ParagraphSpacing {
    pub fn em_value(&self) -> f32 {
        match self {
            ParagraphSpacing::Normal => 1.0,
            ParagraphSpacing::Increased => 1.5,
            ParagraphSpacing::Large => 2.0,
            ParagraphSpacing::ExtraLarge => 2.5,
        }
    }
}

/// Text spacing settings
#[derive(Debug, Clone, Default)]
pub struct SpacingSettings {
    pub letter_spacing: LetterSpacing,
    pub word_spacing: WordSpacing,
    pub line_height: LineHeight,
    pub paragraph_spacing: ParagraphSpacing,
}

impl SpacingSettings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set letter spacing
    pub fn with_letter_spacing(mut self, spacing: LetterSpacing) -> Self {
        self.letter_spacing = spacing;
        self
    }

    /// Builder: set word spacing
    pub fn with_word_spacing(mut self, spacing: WordSpacing) -> Self {
        self.word_spacing = spacing;
        self
    }

    /// Builder: set line height
    pub fn with_line_height(mut self, height: LineHeight) -> Self {
        self.line_height = height;
        self
    }

    /// Builder: set paragraph spacing
    pub fn with_paragraph_spacing(mut self, spacing: ParagraphSpacing) -> Self {
        self.paragraph_spacing = spacing;
        self
    }

    /// Create maximum spacing settings
    pub fn maximum() -> Self {
        Self {
            letter_spacing: LetterSpacing::ExtraLarge,
            word_spacing: WordSpacing::ExtraLarge,
            line_height: LineHeight::VeryLoose,
            paragraph_spacing: ParagraphSpacing::ExtraLarge,
        }
    }
}

// ============================================================================
// Color Overlays
// ============================================================================

/// Overlay color for reading assistance
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayColor {
    pub name: String,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl OverlayColor {
    pub fn new(name: impl Into<String>, red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            name: name.into(),
            red,
            green,
            blue,
            alpha,
        }
    }

    /// Yellow overlay (commonly used)
    pub fn yellow() -> Self {
        Self::new("Yellow", 255, 255, 0, 30)
    }

    /// Blue overlay
    pub fn blue() -> Self {
        Self::new("Blue", 173, 216, 230, 30)
    }

    /// Pink overlay
    pub fn pink() -> Self {
        Self::new("Pink", 255, 192, 203, 30)
    }

    /// Green overlay
    pub fn green() -> Self {
        Self::new("Green", 144, 238, 144, 30)
    }

    /// Peach overlay
    pub fn peach() -> Self {
        Self::new("Peach", 255, 218, 185, 30)
    }

    /// Mint overlay
    pub fn mint() -> Self {
        Self::new("Mint", 189, 252, 201, 30)
    }

    /// Lavender overlay
    pub fn lavender() -> Self {
        Self::new("Lavender", 230, 230, 250, 30)
    }

    /// Cream overlay
    pub fn cream() -> Self {
        Self::new("Cream", 255, 253, 208, 40)
    }

    /// Get CSS rgba value
    pub fn css_rgba(&self) -> String {
        let alpha = self.alpha as f32 / 255.0;
        format!(
            "rgba({}, {}, {}, {:.2})",
            self.red, self.green, self.blue, alpha
        )
    }

    /// Get hex color (without alpha)
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }

    /// Adjust opacity
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.alpha = (opacity.clamp(0.0, 1.0) * 255.0) as u8;
        self
    }

    /// Get all predefined overlays
    pub fn all_presets() -> Vec<Self> {
        vec![
            Self::yellow(),
            Self::blue(),
            Self::pink(),
            Self::green(),
            Self::peach(),
            Self::mint(),
            Self::lavender(),
            Self::cream(),
        ]
    }
}

/// Background color settings
#[derive(Debug, Clone, Default)]
pub struct BackgroundSettings {
    /// Use overlay
    pub overlay_enabled: bool,
    /// Overlay color
    pub overlay: Option<OverlayColor>,
    /// Use dark mode
    pub dark_mode: bool,
    /// Custom background color
    pub custom_background: Option<String>,
    /// Custom text color
    pub custom_text_color: Option<String>,
}

impl BackgroundSettings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable overlay with color
    pub fn with_overlay(mut self, overlay: OverlayColor) -> Self {
        self.overlay_enabled = true;
        self.overlay = Some(overlay);
        self
    }

    /// Enable dark mode
    pub fn with_dark_mode(mut self) -> Self {
        self.dark_mode = true;
        self
    }

    /// Set custom background
    pub fn with_background(mut self, color: impl Into<String>) -> Self {
        self.custom_background = Some(color.into());
        self
    }

    /// Set custom text color
    pub fn with_text_color(mut self, color: impl Into<String>) -> Self {
        self.custom_text_color = Some(color.into());
        self
    }
}

// ============================================================================
// Reading Aids
// ============================================================================

/// Reading line guide style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineGuideStyle {
    /// No line guide
    #[default]
    None,
    /// Underline current line
    Underline,
    /// Highlight current line
    Highlight,
    /// Focus mode (dim other lines)
    Focus,
    /// Reading ruler
    Ruler,
}

/// Text masking style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextMaskStyle {
    /// No masking
    #[default]
    None,
    /// Show only one paragraph at a time
    Paragraph,
    /// Show only a few lines at a time
    Window,
    /// Gradual reveal
    Reveal,
}

/// Word highlighting mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordHighlightMode {
    /// No highlighting
    #[default]
    None,
    /// Highlight current word during TTS
    Current,
    /// Highlight sentence during TTS
    Sentence,
    /// Karaoke-style following
    Karaoke,
}

/// Reading aid settings
#[derive(Debug, Clone, Default)]
pub struct ReadingAidSettings {
    /// Line guide style
    pub line_guide: LineGuideStyle,
    /// Text mask style
    pub text_mask: TextMaskStyle,
    /// Word highlighting during TTS
    pub word_highlight: WordHighlightMode,
    /// Show syllable breaks
    pub syllable_breaks: bool,
    /// Show bionic reading formatting
    pub bionic_reading: bool,
    /// Number of visible lines for window mask
    pub window_lines: u32,
}

impl ReadingAidSettings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set line guide
    pub fn with_line_guide(mut self, style: LineGuideStyle) -> Self {
        self.line_guide = style;
        self
    }

    /// Builder: set text mask
    pub fn with_text_mask(mut self, style: TextMaskStyle) -> Self {
        self.text_mask = style;
        self
    }

    /// Builder: set word highlight mode
    pub fn with_word_highlight(mut self, mode: WordHighlightMode) -> Self {
        self.word_highlight = mode;
        self
    }

    /// Builder: enable syllable breaks
    pub fn with_syllable_breaks(mut self) -> Self {
        self.syllable_breaks = true;
        self
    }

    /// Builder: enable bionic reading
    pub fn with_bionic_reading(mut self) -> Self {
        self.bionic_reading = true;
        self
    }

    /// Builder: set window lines
    pub fn with_window_lines(mut self, lines: u32) -> Self {
        self.window_lines = lines;
        self
    }
}

// ============================================================================
// Text-to-Speech Settings
// ============================================================================

/// TTS voice gender
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceGender {
    Male,
    #[default]
    Female,
    Neutral,
}

/// TTS settings
#[derive(Debug, Clone)]
pub struct TtsSettings {
    /// Enable TTS
    pub enabled: bool,
    /// Speech rate (0.5 - 2.0, 1.0 is normal)
    pub rate: f32,
    /// Pitch (0.5 - 2.0, 1.0 is normal)
    pub pitch: f32,
    /// Volume (0.0 - 1.0)
    pub volume: f32,
    /// Preferred voice gender
    pub voice_gender: VoiceGender,
    /// Preferred voice name (if available)
    pub voice_name: Option<String>,
    /// Pause between sentences (ms)
    pub sentence_pause_ms: u32,
    /// Pause between paragraphs (ms)
    pub paragraph_pause_ms: u32,
    /// Auto-start on new content
    pub auto_start: bool,
    /// Highlight words during speech
    pub highlight_words: bool,
}

impl Default for TtsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            rate: 0.9, // Slightly slower is often better for dyslexia
            pitch: 1.0,
            volume: 1.0,
            voice_gender: VoiceGender::default(),
            voice_name: None,
            sentence_pause_ms: 300,
            paragraph_pause_ms: 600,
            auto_start: false,
            highlight_words: true,
        }
    }
}

impl TtsSettings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: enable TTS
    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Builder: set rate
    pub fn with_rate(mut self, rate: f32) -> Self {
        self.rate = rate.clamp(0.5, 2.0);
        self
    }

    /// Builder: set pitch
    pub fn with_pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch.clamp(0.5, 2.0);
        self
    }

    /// Builder: set volume
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume.clamp(0.0, 1.0);
        self
    }

    /// Builder: set voice gender
    pub fn with_voice_gender(mut self, gender: VoiceGender) -> Self {
        self.voice_gender = gender;
        self
    }

    /// Builder: set voice name
    pub fn with_voice_name(mut self, name: impl Into<String>) -> Self {
        self.voice_name = Some(name.into());
        self
    }

    /// Builder: set sentence pause
    pub fn with_sentence_pause(mut self, ms: u32) -> Self {
        self.sentence_pause_ms = ms;
        self
    }

    /// Builder: set paragraph pause
    pub fn with_paragraph_pause(mut self, ms: u32) -> Self {
        self.paragraph_pause_ms = ms;
        self
    }

    /// Builder: enable auto-start
    pub fn with_auto_start(mut self) -> Self {
        self.auto_start = true;
        self
    }

    /// Builder: enable word highlighting
    pub fn with_word_highlighting(mut self) -> Self {
        self.highlight_words = true;
        self
    }
}

// ============================================================================
// Text Processing
// ============================================================================

/// Bionic reading processor
pub struct BionicProcessor;

impl BionicProcessor {
    /// Apply bionic reading formatting
    /// Bolds the first part of each word to aid reading flow
    pub fn process(text: &str) -> String {
        let mut result = String::new();
        let mut in_word = false;
        let mut word_start = 0;

        for (i, c) in text.char_indices() {
            if c.is_alphabetic() {
                if !in_word {
                    in_word = true;
                    word_start = i;
                }
            } else if in_word {
                // End of word - process it
                let word = &text[word_start..i];
                result.push_str(&Self::format_word(word));
                result.push(c);
                in_word = false;
            } else {
                result.push(c);
            }
        }

        // Handle last word
        if in_word {
            let word = &text[word_start..];
            result.push_str(&Self::format_word(word));
        }

        result
    }

    /// Format a single word with bionic reading
    fn format_word(word: &str) -> String {
        let len = word.chars().count();
        if len <= 1 {
            return format!("**{}**", word);
        }

        let bold_len = match len {
            2..=3 => 1,
            4..=6 => 2,
            7..=9 => 3,
            _ => (len as f32 * 0.35).ceil() as usize,
        };

        let chars: Vec<char> = word.chars().collect();
        let bold_part: String = chars[..bold_len].iter().collect();
        let rest: String = chars[bold_len..].iter().collect();

        format!("**{}**{}", bold_part, rest)
    }
}

/// Syllable processor
pub struct SyllableProcessor {
    /// Common syllable patterns
    patterns: HashMap<String, Vec<usize>>,
}

impl Default for SyllableProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SyllableProcessor {
    pub fn new() -> Self {
        Self {
            patterns: HashMap::new(),
        }
    }

    /// Split word into syllables (simplified English rules)
    pub fn split(&self, word: &str) -> Vec<String> {
        // Check cache first
        if let Some(breaks) = self.patterns.get(word) {
            return self.apply_breaks(word, breaks);
        }

        // Simple syllable splitting based on vowel patterns
        self.simple_split(word)
    }

    /// Apply known break points
    fn apply_breaks(&self, word: &str, breaks: &[usize]) -> Vec<String> {
        let chars: Vec<char> = word.chars().collect();
        let mut syllables = Vec::new();
        let mut start = 0;

        for &end in breaks {
            if end <= chars.len() {
                syllables.push(chars[start..end].iter().collect());
                start = end;
            }
        }

        if start < chars.len() {
            syllables.push(chars[start..].iter().collect());
        }

        syllables
    }

    /// Simple vowel-based syllable splitting
    fn simple_split(&self, word: &str) -> Vec<String> {
        let vowels = ['a', 'e', 'i', 'o', 'u', 'y'];
        let chars: Vec<char> = word.to_lowercase().chars().collect();

        if chars.len() <= 2 {
            return vec![word.to_string()];
        }

        let mut syllables = Vec::new();
        let mut current = String::new();
        let mut prev_vowel = false;

        for (i, &c) in chars.iter().enumerate() {
            let is_vowel = vowels.contains(&c);

            current.push(word.chars().nth(i).unwrap_or(c));

            if prev_vowel && !is_vowel && i < chars.len() - 1 {
                // Consonant after vowel, potential break
                let remaining_has_vowel = chars[i + 1..].iter().any(|&ch| vowels.contains(&ch));
                if remaining_has_vowel && current.len() > 1 {
                    syllables.push(current.clone());
                    current.clear();
                }
            }

            prev_vowel = is_vowel;
        }

        if !current.is_empty() {
            syllables.push(current);
        }

        if syllables.is_empty() {
            syllables.push(word.to_string());
        }

        syllables
    }

    /// Format word with syllable breaks
    pub fn format(&self, word: &str, separator: &str) -> String {
        self.split(word).join(separator)
    }
}

// ============================================================================
// Dyslexia-Friendly Configuration
// ============================================================================

/// Complete dyslexia-friendly configuration
#[derive(Debug, Clone)]
pub struct DyslexiaConfig {
    /// Font settings
    pub font: DyslexiaFont,
    /// Font size multiplier
    pub font_size_multiplier: f32,
    /// Font weight
    pub font_weight: FontWeight,
    /// Spacing settings
    pub spacing: SpacingSettings,
    /// Background settings
    pub background: BackgroundSettings,
    /// Reading aids
    pub reading_aids: ReadingAidSettings,
    /// TTS settings
    pub tts: TtsSettings,
    /// Maximum line width (in characters)
    pub max_line_width: Option<u32>,
    /// Avoid italics
    pub avoid_italics: bool,
    /// Avoid all caps
    pub avoid_all_caps: bool,
    /// Use left alignment (avoid justified)
    pub left_align: bool,
    /// Bullet point style for lists
    pub list_bullet: String,
}

impl Default for DyslexiaConfig {
    fn default() -> Self {
        Self {
            font: DyslexiaFont::OpenDyslexic,
            font_size_multiplier: 1.2,
            font_weight: FontWeight::Regular,
            spacing: SpacingSettings::default(),
            background: BackgroundSettings::default(),
            reading_aids: ReadingAidSettings::default(),
            tts: TtsSettings::default(),
            max_line_width: Some(70),
            avoid_italics: true,
            avoid_all_caps: true,
            left_align: true,
            list_bullet: "•".to_string(),
        }
    }
}

impl DyslexiaConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set font
    pub fn with_font(mut self, font: DyslexiaFont) -> Self {
        self.font = font;
        self
    }

    /// Builder: set font size multiplier
    pub fn with_font_size(mut self, multiplier: f32) -> Self {
        self.font_size_multiplier = multiplier.clamp(0.5, 3.0);
        self
    }

    /// Builder: set font weight
    pub fn with_font_weight(mut self, weight: FontWeight) -> Self {
        self.font_weight = weight;
        self
    }

    /// Builder: set spacing settings
    pub fn with_spacing(mut self, spacing: SpacingSettings) -> Self {
        self.spacing = spacing;
        self
    }

    /// Builder: set background settings
    pub fn with_background(mut self, background: BackgroundSettings) -> Self {
        self.background = background;
        self
    }

    /// Builder: set reading aids
    pub fn with_reading_aids(mut self, aids: ReadingAidSettings) -> Self {
        self.reading_aids = aids;
        self
    }

    /// Builder: set TTS settings
    pub fn with_tts(mut self, tts: TtsSettings) -> Self {
        self.tts = tts;
        self
    }

    /// Builder: set max line width
    pub fn with_max_line_width(mut self, width: u32) -> Self {
        self.max_line_width = Some(width);
        self
    }

    /// Builder: allow italics
    pub fn allow_italics(mut self) -> Self {
        self.avoid_italics = false;
        self
    }

    /// Builder: allow all caps
    pub fn allow_all_caps(mut self) -> Self {
        self.avoid_all_caps = false;
        self
    }

    /// Create a preset for mild dyslexia
    pub fn preset_mild() -> Self {
        Self {
            font: DyslexiaFont::Arial,
            font_size_multiplier: 1.1,
            spacing: SpacingSettings::new()
                .with_letter_spacing(LetterSpacing::Slight)
                .with_line_height(LineHeight::Comfortable),
            ..Default::default()
        }
    }

    /// Create a preset for moderate dyslexia
    pub fn preset_moderate() -> Self {
        Self {
            font: DyslexiaFont::OpenDyslexic,
            font_size_multiplier: 1.3,
            spacing: SpacingSettings::new()
                .with_letter_spacing(LetterSpacing::Moderate)
                .with_word_spacing(WordSpacing::Moderate)
                .with_line_height(LineHeight::Relaxed),
            background: BackgroundSettings::new().with_overlay(OverlayColor::cream()),
            reading_aids: ReadingAidSettings::new().with_line_guide(LineGuideStyle::Highlight),
            ..Default::default()
        }
    }

    /// Create a preset for severe dyslexia
    pub fn preset_severe() -> Self {
        Self {
            font: DyslexiaFont::OpenDyslexic,
            font_size_multiplier: 1.5,
            font_weight: FontWeight::Medium,
            spacing: SpacingSettings::maximum(),
            background: BackgroundSettings::new().with_overlay(OverlayColor::yellow()),
            reading_aids: ReadingAidSettings::new()
                .with_line_guide(LineGuideStyle::Focus)
                .with_bionic_reading()
                .with_syllable_breaks(),
            tts: TtsSettings::new()
                .enabled()
                .with_rate(0.8)
                .with_word_highlighting(),
            max_line_width: Some(50),
            ..Default::default()
        }
    }

    /// Generate CSS styles for this configuration
    pub fn to_css(&self) -> String {
        let mut css = String::new();

        // Font settings
        css.push_str(&format!("font-family: {};\n", self.font.css_font_family()));
        css.push_str(&format!("font-size: {}em;\n", self.font_size_multiplier));
        css.push_str(&format!("font-weight: {};\n", self.font_weight.css_value()));

        // Spacing
        css.push_str(&format!(
            "letter-spacing: {};\n",
            self.spacing.letter_spacing.css_value()
        ));
        css.push_str(&format!(
            "word-spacing: {};\n",
            self.spacing.word_spacing.css_value()
        ));
        css.push_str(&format!(
            "line-height: {};\n",
            self.spacing.line_height.value()
        ));

        // Text alignment
        if self.left_align {
            css.push_str("text-align: left;\n");
        }

        // Max line width
        if let Some(width) = self.max_line_width {
            css.push_str(&format!("max-width: {}ch;\n", width));
        }

        // Background overlay
        if self.background.overlay_enabled {
            if let Some(ref overlay) = self.background.overlay {
                css.push_str(&format!("background-color: {};\n", overlay.css_rgba()));
            }
        }

        css
    }
}

// ============================================================================
// Text Formatter
// ============================================================================

/// Text formatter for dyslexia-friendly output
pub struct TextFormatter {
    config: DyslexiaConfig,
    syllable_processor: SyllableProcessor,
}

impl TextFormatter {
    pub fn new(config: DyslexiaConfig) -> Self {
        Self {
            config,
            syllable_processor: SyllableProcessor::new(),
        }
    }

    /// Format text according to dyslexia-friendly settings
    pub fn format(&self, text: &str) -> String {
        let mut result = text.to_string();

        // Convert all caps to lowercase if needed
        if self.config.avoid_all_caps {
            result = self.remove_all_caps(&result);
        }

        // Convert italics to regular if needed
        if self.config.avoid_italics {
            result = self.remove_italics(&result);
        }

        // Apply bionic reading if enabled
        if self.config.reading_aids.bionic_reading {
            result = BionicProcessor::process(&result);
        }

        // Apply syllable breaks if enabled
        if self.config.reading_aids.syllable_breaks {
            result = self.apply_syllable_breaks(&result);
        }

        // Wrap lines if max width is set
        if let Some(max_width) = self.config.max_line_width {
            result = self.wrap_lines(&result, max_width as usize);
        }

        result
    }

    /// Remove all caps text
    fn remove_all_caps(&self, text: &str) -> String {
        let mut result = String::new();
        let mut word = String::new();

        for c in text.chars() {
            if c.is_whitespace() || c.is_ascii_punctuation() {
                if word
                    .chars()
                    .all(|ch| ch.is_uppercase() || !ch.is_alphabetic())
                    && word.len() > 1
                {
                    // All caps word, convert to title case
                    let mut chars = word.chars();
                    if let Some(first) = chars.next() {
                        result.push(first);
                        result.push_str(&chars.as_str().to_lowercase());
                    }
                } else {
                    result.push_str(&word);
                }
                result.push(c);
                word.clear();
            } else {
                word.push(c);
            }
        }

        // Handle last word
        if word
            .chars()
            .all(|ch| ch.is_uppercase() || !ch.is_alphabetic())
            && word.len() > 1
        {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                result.push(first);
                result.push_str(&chars.as_str().to_lowercase());
            }
        } else {
            result.push_str(&word);
        }

        result
    }

    /// Remove italic markdown formatting
    fn remove_italics(&self, text: &str) -> String {
        // Simple replacement of *text* and _text_ patterns
        let mut result = text.to_string();

        // Replace single asterisks/underscores for italics
        // Be careful not to replace ** or __
        let chars: Vec<char> = result.chars().collect();
        let mut new_result = String::new();
        let mut i = 0;

        while i < chars.len() {
            if (chars[i] == '*' || chars[i] == '_')
                && (i + 1 >= chars.len() || chars[i + 1] != chars[i])
                && (i == 0 || chars[i - 1] != chars[i])
            {
                // Single marker - skip it
                i += 1;
            } else {
                new_result.push(chars[i]);
                i += 1;
            }
        }

        result = new_result;
        result
    }

    /// Apply syllable breaks to text
    fn apply_syllable_breaks(&self, text: &str) -> String {
        let mut result = String::new();
        let mut word = String::new();

        for c in text.chars() {
            if c.is_alphabetic() {
                word.push(c);
            } else {
                if !word.is_empty() {
                    result.push_str(&self.syllable_processor.format(&word, "·"));
                    word.clear();
                }
                result.push(c);
            }
        }

        if !word.is_empty() {
            result.push_str(&self.syllable_processor.format(&word, "·"));
        }

        result
    }

    /// Wrap lines to max width
    fn wrap_lines(&self, text: &str, max_width: usize) -> String {
        let mut result = String::new();
        let mut current_width = 0;

        for word in text.split_whitespace() {
            let word_len = word.chars().count();

            if current_width + word_len + 1 > max_width && current_width > 0 {
                result.push('\n');
                current_width = 0;
            }

            if current_width > 0 {
                result.push(' ');
                current_width += 1;
            }

            result.push_str(word);
            current_width += word_len;
        }

        result
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Font Tests

    #[test]
    fn test_dyslexia_font_as_str() {
        assert_eq!(DyslexiaFont::OpenDyslexic.as_str(), "OpenDyslexic");
        assert_eq!(DyslexiaFont::ComicSans.as_str(), "Comic Sans MS");
        assert_eq!(
            DyslexiaFont::Custom("MyFont".to_string()).as_str(),
            "MyFont"
        );
    }

    #[test]
    fn test_dyslexia_font_css() {
        let font = DyslexiaFont::Arial;
        assert_eq!(font.css_font_family(), "'Arial', sans-serif");
    }

    #[test]
    fn test_font_weight_css() {
        assert_eq!(FontWeight::Regular.css_value(), 400);
        assert_eq!(FontWeight::Bold.css_value(), 700);
    }

    // Spacing Tests

    #[test]
    fn test_letter_spacing_em() {
        assert_eq!(LetterSpacing::Normal.em_value(), 0.0);
        assert_eq!(LetterSpacing::Moderate.em_value(), 0.1);
    }

    #[test]
    fn test_letter_spacing_css() {
        assert_eq!(LetterSpacing::Moderate.css_value(), "0.1em");
    }

    #[test]
    fn test_word_spacing_em() {
        assert_eq!(WordSpacing::Normal.em_value(), 0.0);
        assert_eq!(WordSpacing::Large.em_value(), 0.35);
    }

    #[test]
    fn test_line_height_value() {
        assert_eq!(LineHeight::Normal.value(), 1.4);
        assert_eq!(LineHeight::Loose.value(), 2.0);
    }

    #[test]
    fn test_spacing_settings_builder() {
        let settings = SpacingSettings::new()
            .with_letter_spacing(LetterSpacing::Large)
            .with_word_spacing(WordSpacing::Moderate)
            .with_line_height(LineHeight::Relaxed);

        assert_eq!(settings.letter_spacing, LetterSpacing::Large);
        assert_eq!(settings.word_spacing, WordSpacing::Moderate);
        assert_eq!(settings.line_height, LineHeight::Relaxed);
    }

    #[test]
    fn test_spacing_maximum() {
        let settings = SpacingSettings::maximum();
        assert_eq!(settings.letter_spacing, LetterSpacing::ExtraLarge);
        assert_eq!(settings.word_spacing, WordSpacing::ExtraLarge);
        assert_eq!(settings.line_height, LineHeight::VeryLoose);
    }

    // Color Overlay Tests

    #[test]
    fn test_overlay_color_creation() {
        let color = OverlayColor::new("Test", 255, 128, 64, 128);
        assert_eq!(color.name, "Test");
        assert_eq!(color.red, 255);
        assert_eq!(color.green, 128);
        assert_eq!(color.blue, 64);
        assert_eq!(color.alpha, 128);
    }

    #[test]
    fn test_overlay_color_presets() {
        let yellow = OverlayColor::yellow();
        assert_eq!(yellow.name, "Yellow");
        assert_eq!(yellow.red, 255);
        assert_eq!(yellow.green, 255);

        let presets = OverlayColor::all_presets();
        assert_eq!(presets.len(), 8);
    }

    #[test]
    fn test_overlay_css_rgba() {
        let color = OverlayColor::new("Test", 255, 0, 0, 128);
        let rgba = color.css_rgba();
        assert!(rgba.contains("255"));
        assert!(rgba.contains("0.50")); // 128/255 ≈ 0.50
    }

    #[test]
    fn test_overlay_hex() {
        let color = OverlayColor::new("Test", 255, 0, 128, 255);
        assert_eq!(color.hex(), "#ff0080");
    }

    #[test]
    fn test_overlay_with_opacity() {
        let color = OverlayColor::yellow().with_opacity(0.5);
        assert_eq!(color.alpha, 127); // 0.5 * 255 = 127.5 → 127
    }

    // Reading Aid Tests

    #[test]
    fn test_reading_aid_builder() {
        let aids = ReadingAidSettings::new()
            .with_line_guide(LineGuideStyle::Highlight)
            .with_bionic_reading()
            .with_syllable_breaks();

        assert_eq!(aids.line_guide, LineGuideStyle::Highlight);
        assert!(aids.bionic_reading);
        assert!(aids.syllable_breaks);
    }

    // TTS Tests

    #[test]
    fn test_tts_settings_default() {
        let tts = TtsSettings::default();
        assert!(!tts.enabled);
        assert_eq!(tts.rate, 0.9);
        assert_eq!(tts.pitch, 1.0);
    }

    #[test]
    fn test_tts_settings_builder() {
        let tts = TtsSettings::new()
            .enabled()
            .with_rate(0.7)
            .with_pitch(1.2)
            .with_volume(0.8);

        assert!(tts.enabled);
        assert_eq!(tts.rate, 0.7);
        assert_eq!(tts.pitch, 1.2);
        assert_eq!(tts.volume, 0.8);
    }

    #[test]
    fn test_tts_rate_clamping() {
        let tts = TtsSettings::new().with_rate(5.0);
        assert_eq!(tts.rate, 2.0); // Clamped to max

        let tts = TtsSettings::new().with_rate(0.1);
        assert_eq!(tts.rate, 0.5); // Clamped to min
    }

    // Bionic Reading Tests

    #[test]
    fn test_bionic_processor_short_word() {
        let result = BionicProcessor::process("hi");
        assert_eq!(result, "**h**i");
    }

    #[test]
    fn test_bionic_processor_medium_word() {
        let result = BionicProcessor::process("hello");
        assert_eq!(result, "**he**llo");
    }

    #[test]
    fn test_bionic_processor_sentence() {
        let result = BionicProcessor::process("The quick brown fox");
        // "The" is 3 letters, so 1 char is bolded -> **T**he
        assert!(result.contains("**T**he"));
        // "quick" is 5 letters, so 2 chars are bolded -> **qu**ick
        assert!(result.contains("**qu**ick"));
        // "brown" is 5 letters, so 2 chars are bolded -> **br**own
        assert!(result.contains("**br**own"));
    }

    #[test]
    fn test_bionic_processor_preserves_punctuation() {
        let result = BionicProcessor::process("Hello, world!");
        assert!(result.contains(","));
        assert!(result.contains("!"));
    }

    // Syllable Processor Tests

    #[test]
    fn test_syllable_split_simple() {
        let processor = SyllableProcessor::new();
        let syllables = processor.split("hello");
        assert!(!syllables.is_empty());
    }

    #[test]
    fn test_syllable_split_short() {
        let processor = SyllableProcessor::new();
        let syllables = processor.split("hi");
        assert_eq!(syllables.len(), 1);
        assert_eq!(syllables[0], "hi");
    }

    #[test]
    fn test_syllable_format() {
        let processor = SyllableProcessor::new();
        let formatted = processor.format("testing", "·");
        assert!(formatted.contains("·") || formatted == "testing");
    }

    // Config Tests

    #[test]
    fn test_dyslexia_config_default() {
        let config = DyslexiaConfig::default();
        assert_eq!(config.font, DyslexiaFont::OpenDyslexic);
        assert_eq!(config.font_size_multiplier, 1.2);
        assert!(config.avoid_italics);
        assert!(config.avoid_all_caps);
        assert!(config.left_align);
    }

    #[test]
    fn test_dyslexia_config_builder() {
        let config = DyslexiaConfig::new()
            .with_font(DyslexiaFont::Arial)
            .with_font_size(1.5)
            .with_font_weight(FontWeight::Bold)
            .with_max_line_width(60)
            .allow_italics();

        assert_eq!(config.font, DyslexiaFont::Arial);
        assert_eq!(config.font_size_multiplier, 1.5);
        assert_eq!(config.font_weight, FontWeight::Bold);
        assert_eq!(config.max_line_width, Some(60));
        assert!(!config.avoid_italics);
    }

    #[test]
    fn test_preset_mild() {
        let config = DyslexiaConfig::preset_mild();
        assert_eq!(config.font, DyslexiaFont::Arial);
        assert_eq!(config.font_size_multiplier, 1.1);
    }

    #[test]
    fn test_preset_moderate() {
        let config = DyslexiaConfig::preset_moderate();
        assert_eq!(config.font, DyslexiaFont::OpenDyslexic);
        assert!(config.background.overlay_enabled);
    }

    #[test]
    fn test_preset_severe() {
        let config = DyslexiaConfig::preset_severe();
        assert_eq!(config.font_size_multiplier, 1.5);
        assert!(config.tts.enabled);
        assert!(config.reading_aids.bionic_reading);
    }

    #[test]
    fn test_config_to_css() {
        let config = DyslexiaConfig::new()
            .with_font(DyslexiaFont::Arial)
            .with_spacing(SpacingSettings::new().with_letter_spacing(LetterSpacing::Moderate));

        let css = config.to_css();
        assert!(css.contains("Arial"));
        assert!(css.contains("letter-spacing"));
        assert!(css.contains("text-align: left"));
    }

    // Text Formatter Tests

    #[test]
    fn test_formatter_creation() {
        let config = DyslexiaConfig::new();
        let formatter = TextFormatter::new(config);
        assert!(formatter.format("test").len() > 0);
    }

    #[test]
    fn test_formatter_all_caps_removal() {
        let config = DyslexiaConfig::new();
        let formatter = TextFormatter::new(config);

        let result = formatter.format("HELLO WORLD");
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_formatter_preserves_normal_text() {
        let config = DyslexiaConfig::new();
        let formatter = TextFormatter::new(config);

        let result = formatter.format("Hello World");
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_formatter_bionic_reading() {
        let config = DyslexiaConfig::new()
            .with_reading_aids(ReadingAidSettings::new().with_bionic_reading());
        let formatter = TextFormatter::new(config);

        let result = formatter.format("hello");
        assert!(result.contains("**"));
    }

    #[test]
    fn test_formatter_line_wrapping() {
        let config = DyslexiaConfig::new().with_max_line_width(20);
        let formatter = TextFormatter::new(config);

        let result = formatter.format("The quick brown fox jumps over the lazy dog");
        assert!(result.contains('\n'));
    }

    // Background Settings Tests

    #[test]
    fn test_background_settings_default() {
        let bg = BackgroundSettings::default();
        assert!(!bg.overlay_enabled);
        assert!(bg.overlay.is_none());
        assert!(!bg.dark_mode);
    }

    #[test]
    fn test_background_settings_builder() {
        let bg = BackgroundSettings::new()
            .with_overlay(OverlayColor::yellow())
            .with_dark_mode();

        assert!(bg.overlay_enabled);
        assert!(bg.overlay.is_some());
        assert!(bg.dark_mode);
    }

    // Default implementations

    #[test]
    fn test_defaults() {
        assert_eq!(DyslexiaFont::default(), DyslexiaFont::OpenDyslexic);
        assert_eq!(FontWeight::default(), FontWeight::Regular);
        assert_eq!(LetterSpacing::default(), LetterSpacing::Slight);
        assert_eq!(WordSpacing::default(), WordSpacing::Slight);
        assert_eq!(LineHeight::default(), LineHeight::Comfortable);
        assert_eq!(ParagraphSpacing::default(), ParagraphSpacing::Increased);
        assert_eq!(LineGuideStyle::default(), LineGuideStyle::None);
        assert_eq!(TextMaskStyle::default(), TextMaskStyle::None);
        assert_eq!(WordHighlightMode::default(), WordHighlightMode::None);
        assert_eq!(VoiceGender::default(), VoiceGender::Female);
    }

    #[test]
    fn test_paragraph_spacing_em() {
        assert_eq!(ParagraphSpacing::Normal.em_value(), 1.0);
        assert_eq!(ParagraphSpacing::ExtraLarge.em_value(), 2.5);
    }

    #[test]
    fn test_syllable_processor_default() {
        let processor = SyllableProcessor::default();
        let syllables = processor.split("computer");
        assert!(!syllables.is_empty());
    }

    #[test]
    fn test_tts_voice_name() {
        let tts = TtsSettings::new().with_voice_name("Alex");
        assert_eq!(tts.voice_name, Some("Alex".to_string()));
    }

    #[test]
    fn test_formatter_italics_removal() {
        let config = DyslexiaConfig::new();
        let formatter = TextFormatter::new(config);

        let result = formatter.format("This is *emphasized* text");
        assert!(!result.contains('*'));
    }

    #[test]
    fn test_formatter_syllable_breaks() {
        let config = DyslexiaConfig::new()
            .with_reading_aids(ReadingAidSettings::new().with_syllable_breaks());
        let formatter = TextFormatter::new(config);

        let result = formatter.format("computer");
        // Should contain syllable separator or original word
        assert!(result.contains("·") || result == "computer");
    }
}
