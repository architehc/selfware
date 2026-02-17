//! Selfware Theme System
//!
//! Configurable color themes for terminal output.
//! Supports multiple built-in themes and custom theme loading.

use colored::CustomColor;
use std::sync::atomic::{AtomicU8, Ordering};

/// Global theme selection (0 = Amber, 1 = Ocean, 2 = Minimal, 3 = HighContrast)
static CURRENT_THEME: AtomicU8 = AtomicU8::new(0);

/// Theme identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeId {
    #[default]
    Amber,
    Ocean,
    Minimal,
    HighContrast,
    Dracula,
    Monokai,
    SolarizedDark,
    SolarizedLight,
    Nord,
    Gruvbox,
}

impl ThemeId {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => ThemeId::Amber,
            1 => ThemeId::Ocean,
            2 => ThemeId::Minimal,
            3 => ThemeId::HighContrast,
            4 => ThemeId::Dracula,
            5 => ThemeId::Monokai,
            6 => ThemeId::SolarizedDark,
            7 => ThemeId::SolarizedLight,
            8 => ThemeId::Nord,
            9 => ThemeId::Gruvbox,
            _ => ThemeId::Amber,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            ThemeId::Amber => 0,
            ThemeId::Ocean => 1,
            ThemeId::Minimal => 2,
            ThemeId::HighContrast => 3,
            ThemeId::Dracula => 4,
            ThemeId::Monokai => 5,
            ThemeId::SolarizedDark => 6,
            ThemeId::SolarizedLight => 7,
            ThemeId::Nord => 8,
            ThemeId::Gruvbox => 9,
        }
    }
}

/// A complete color theme
#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    /// Primary action color (titles, emphasis)
    pub primary: CustomColor,
    /// Success/growth color
    pub success: CustomColor,
    /// Warning/attention color
    pub warning: CustomColor,
    /// Error/frost color
    pub error: CustomColor,
    /// Muted/secondary text
    pub muted: CustomColor,
    /// Accent color for highlights
    pub accent: CustomColor,
    /// Tool/action name color
    pub tool: CustomColor,
    /// Path/filename color
    pub path: CustomColor,
}

impl ThemeColors {
    /// Warm amber theme (default) - like aged paper, wood grain, and amber resin
    pub const AMBER: ThemeColors = ThemeColors {
        primary: CustomColor {
            r: 212,
            g: 163,
            b: 115,
        }, // #D4A373 - Warm amber
        success: CustomColor {
            r: 144,
            g: 190,
            b: 109,
        }, // #90BE6D - Fresh growth
        warning: CustomColor {
            r: 188,
            g: 108,
            b: 37,
        }, // #BC6C25 - Soil brown
        error: CustomColor {
            r: 100,
            g: 100,
            b: 120,
        }, // #646478 - Frost
        muted: CustomColor {
            r: 128,
            g: 128,
            b: 128,
        }, // #808080 - Stone gray
        accent: CustomColor {
            r: 184,
            g: 115,
            b: 51,
        }, // #B87333 - Copper
        tool: CustomColor {
            r: 184,
            g: 115,
            b: 51,
        }, // #B87333 - Copper
        path: CustomColor {
            r: 143,
            g: 151,
            b: 121,
        }, // #8F9779 - Sage
    };

    /// Cool ocean theme - blues and teals like deep water
    pub const OCEAN: ThemeColors = ThemeColors {
        primary: CustomColor {
            r: 100,
            g: 149,
            b: 237,
        }, // #6495ED - Cornflower blue
        success: CustomColor {
            r: 32,
            g: 178,
            b: 170,
        }, // #20B2AA - Light sea green
        warning: CustomColor {
            r: 255,
            g: 165,
            b: 0,
        }, // #FFA500 - Orange
        error: CustomColor {
            r: 255,
            g: 99,
            b: 71,
        }, // #FF6347 - Tomato
        muted: CustomColor {
            r: 119,
            g: 136,
            b: 153,
        }, // #778899 - Light slate gray
        accent: CustomColor {
            r: 0,
            g: 206,
            b: 209,
        }, // #00CED1 - Dark turquoise
        tool: CustomColor {
            r: 72,
            g: 209,
            b: 204,
        }, // #48D1CC - Medium turquoise
        path: CustomColor {
            r: 176,
            g: 196,
            b: 222,
        }, // #B0C4DE - Light steel blue
    };

    /// Minimal grayscale theme - clean and simple
    pub const MINIMAL: ThemeColors = ThemeColors {
        primary: CustomColor {
            r: 220,
            g: 220,
            b: 220,
        }, // #DCDCDC - Gainsboro
        success: CustomColor {
            r: 180,
            g: 180,
            b: 180,
        }, // #B4B4B4 - Silver
        warning: CustomColor {
            r: 160,
            g: 160,
            b: 160,
        }, // #A0A0A0 - Dark gray
        error: CustomColor {
            r: 140,
            g: 140,
            b: 140,
        }, // #8C8C8C - Gray
        muted: CustomColor {
            r: 100,
            g: 100,
            b: 100,
        }, // #646464 - Dim gray
        accent: CustomColor {
            r: 200,
            g: 200,
            b: 200,
        }, // #C8C8C8 - Light gray
        tool: CustomColor {
            r: 180,
            g: 180,
            b: 180,
        }, // #B4B4B4 - Silver
        path: CustomColor {
            r: 160,
            g: 160,
            b: 160,
        }, // #A0A0A0 - Dark gray
    };

    /// Dracula theme - purple and pink on dark
    pub const DRACULA: ThemeColors = ThemeColors {
        primary: CustomColor {
            r: 189,
            g: 147,
            b: 249,
        }, // #BD93F9 - Purple
        success: CustomColor {
            r: 80,
            g: 250,
            b: 123,
        }, // #50FA7B - Green
        warning: CustomColor {
            r: 241,
            g: 250,
            b: 140,
        }, // #F1FA8C - Yellow
        error: CustomColor {
            r: 255,
            g: 85,
            b: 85,
        }, // #FF5555 - Red
        muted: CustomColor {
            r: 98,
            g: 114,
            b: 164,
        }, // #6272A4 - Comment
        accent: CustomColor {
            r: 255,
            g: 121,
            b: 198,
        }, // #FF79C6 - Pink
        tool: CustomColor {
            r: 139,
            g: 233,
            b: 253,
        }, // #8BE9FD - Cyan
        path: CustomColor {
            r: 241,
            g: 250,
            b: 140,
        }, // #F1FA8C - Yellow
    };

    /// Monokai theme - warm and vibrant
    pub const MONOKAI: ThemeColors = ThemeColors {
        primary: CustomColor {
            r: 102,
            g: 217,
            b: 239,
        }, // #66D9EF - Blue
        success: CustomColor {
            r: 166,
            g: 226,
            b: 46,
        }, // #A6E22E - Green
        warning: CustomColor {
            r: 253,
            g: 151,
            b: 31,
        }, // #FD971F - Orange
        error: CustomColor {
            r: 249,
            g: 38,
            b: 114,
        }, // #F92672 - Red/Pink
        muted: CustomColor {
            r: 117,
            g: 113,
            b: 94,
        }, // #75715E - Comment
        accent: CustomColor {
            r: 174,
            g: 129,
            b: 255,
        }, // #AE81FF - Purple
        tool: CustomColor {
            r: 166,
            g: 226,
            b: 46,
        }, // #A6E22E - Green
        path: CustomColor {
            r: 230,
            g: 219,
            b: 116,
        }, // #E6DB74 - Yellow
    };

    /// Solarized Dark theme
    pub const SOLARIZED_DARK: ThemeColors = ThemeColors {
        primary: CustomColor {
            r: 38,
            g: 139,
            b: 210,
        }, // #268BD2 - Blue
        success: CustomColor {
            r: 133,
            g: 153,
            b: 0,
        }, // #859900 - Green
        warning: CustomColor {
            r: 181,
            g: 137,
            b: 0,
        }, // #B58900 - Yellow
        error: CustomColor {
            r: 220,
            g: 50,
            b: 47,
        }, // #DC322F - Red
        muted: CustomColor {
            r: 88,
            g: 110,
            b: 117,
        }, // #586E75 - Base01
        accent: CustomColor {
            r: 108,
            g: 113,
            b: 196,
        }, // #6C71C4 - Violet
        tool: CustomColor {
            r: 42,
            g: 161,
            b: 152,
        }, // #2AA198 - Cyan
        path: CustomColor {
            r: 147,
            g: 161,
            b: 161,
        }, // #93A1A1 - Base1
    };

    /// Solarized Light theme
    pub const SOLARIZED_LIGHT: ThemeColors = ThemeColors {
        primary: CustomColor {
            r: 38,
            g: 139,
            b: 210,
        }, // #268BD2 - Blue
        success: CustomColor {
            r: 133,
            g: 153,
            b: 0,
        }, // #859900 - Green
        warning: CustomColor {
            r: 181,
            g: 137,
            b: 0,
        }, // #B58900 - Yellow
        error: CustomColor {
            r: 220,
            g: 50,
            b: 47,
        }, // #DC322F - Red
        muted: CustomColor {
            r: 101,
            g: 123,
            b: 131,
        }, // #657B83 - Base00
        accent: CustomColor {
            r: 108,
            g: 113,
            b: 196,
        }, // #6C71C4 - Violet
        tool: CustomColor {
            r: 42,
            g: 161,
            b: 152,
        }, // #2AA198 - Cyan
        path: CustomColor {
            r: 88,
            g: 110,
            b: 117,
        }, // #586E75 - Base01
    };

    /// Nord theme - arctic, north-bluish palette
    pub const NORD: ThemeColors = ThemeColors {
        primary: CustomColor {
            r: 136,
            g: 192,
            b: 208,
        }, // #88C0D0 - Nord8
        success: CustomColor {
            r: 163,
            g: 190,
            b: 140,
        }, // #A3BE8C - Nord14
        warning: CustomColor {
            r: 235,
            g: 203,
            b: 139,
        }, // #EBCB8B - Nord13
        error: CustomColor {
            r: 191,
            g: 97,
            b: 106,
        }, // #BF616A - Nord11
        muted: CustomColor {
            r: 76,
            g: 86,
            b: 106,
        }, // #4C566A - Nord3
        accent: CustomColor {
            r: 180,
            g: 142,
            b: 173,
        }, // #B48EAD - Nord15
        tool: CustomColor {
            r: 129,
            g: 161,
            b: 193,
        }, // #81A1C1 - Nord9
        path: CustomColor {
            r: 143,
            g: 188,
            b: 187,
        }, // #8FBCBB - Nord7
    };

    /// Gruvbox theme - retro groove color scheme
    pub const GRUVBOX: ThemeColors = ThemeColors {
        primary: CustomColor {
            r: 131,
            g: 165,
            b: 152,
        }, // #83A598 - Blue
        success: CustomColor {
            r: 184,
            g: 187,
            b: 38,
        }, // #B8BB26 - Green
        warning: CustomColor {
            r: 250,
            g: 189,
            b: 47,
        }, // #FABD2F - Yellow
        error: CustomColor {
            r: 251,
            g: 73,
            b: 52,
        }, // #FB4934 - Red
        muted: CustomColor {
            r: 146,
            g: 131,
            b: 116,
        }, // #928374 - Gray
        accent: CustomColor {
            r: 211,
            g: 134,
            b: 155,
        }, // #D3869B - Purple
        tool: CustomColor {
            r: 142,
            g: 192,
            b: 124,
        }, // #8EC07C - Aqua
        path: CustomColor {
            r: 254,
            g: 128,
            b: 25,
        }, // #FE8019 - Orange
    };

    /// High contrast theme - accessibility focused
    pub const HIGH_CONTRAST: ThemeColors = ThemeColors {
        primary: CustomColor {
            r: 255,
            g: 255,
            b: 255,
        }, // #FFFFFF - White
        success: CustomColor { r: 0, g: 255, b: 0 }, // #00FF00 - Lime
        warning: CustomColor {
            r: 255,
            g: 255,
            b: 0,
        }, // #FFFF00 - Yellow
        error: CustomColor { r: 255, g: 0, b: 0 },   // #FF0000 - Red
        muted: CustomColor {
            r: 192,
            g: 192,
            b: 192,
        }, // #C0C0C0 - Silver
        accent: CustomColor {
            r: 0,
            g: 255,
            b: 255,
        }, // #00FFFF - Cyan
        tool: CustomColor {
            r: 255,
            g: 0,
            b: 255,
        }, // #FF00FF - Magenta
        path: CustomColor {
            r: 0,
            g: 255,
            b: 255,
        }, // #00FFFF - Cyan
    };
}

/// Set the global theme
pub fn set_theme(theme: ThemeId) {
    CURRENT_THEME.store(theme.to_u8(), Ordering::SeqCst);
}

/// Get the current theme ID
pub fn current_theme_id() -> ThemeId {
    ThemeId::from_u8(CURRENT_THEME.load(Ordering::SeqCst))
}

/// Get the current theme colors
pub fn current_theme() -> ThemeColors {
    theme_colors(current_theme_id())
}

/// Get theme colors by ID
pub fn theme_colors(id: ThemeId) -> ThemeColors {
    match id {
        ThemeId::Amber => ThemeColors::AMBER,
        ThemeId::Ocean => ThemeColors::OCEAN,
        ThemeId::Minimal => ThemeColors::MINIMAL,
        ThemeId::HighContrast => ThemeColors::HIGH_CONTRAST,
        ThemeId::Dracula => ThemeColors::DRACULA,
        ThemeId::Monokai => ThemeColors::MONOKAI,
        ThemeId::SolarizedDark => ThemeColors::SOLARIZED_DARK,
        ThemeId::SolarizedLight => ThemeColors::SOLARIZED_LIGHT,
        ThemeId::Nord => ThemeColors::NORD,
        ThemeId::Gruvbox => ThemeColors::GRUVBOX,
    }
}

/// Get a theme by name (case-insensitive)
pub fn theme_from_name(name: &str) -> Option<ThemeId> {
    match name.to_lowercase().as_str() {
        "amber" => Some(ThemeId::Amber),
        "ocean" => Some(ThemeId::Ocean),
        "minimal" => Some(ThemeId::Minimal),
        "highcontrast" | "high-contrast" | "high_contrast" => Some(ThemeId::HighContrast),
        "dracula" => Some(ThemeId::Dracula),
        "monokai" => Some(ThemeId::Monokai),
        "solarized-dark" | "solarizeddark" | "solarized_dark" => Some(ThemeId::SolarizedDark),
        "solarized-light" | "solarizedlight" | "solarized_light" => Some(ThemeId::SolarizedLight),
        "nord" => Some(ThemeId::Nord),
        "gruvbox" => Some(ThemeId::Gruvbox),
        _ => None,
    }
}

/// Get a list of all available theme names
pub fn available_themes() -> Vec<&'static str> {
    vec![
        "amber",
        "ocean",
        "minimal",
        "high-contrast",
        "dracula",
        "monokai",
        "solarized-dark",
        "solarized-light",
        "nord",
        "gruvbox",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_switching() {
        set_theme(ThemeId::Amber);
        assert_eq!(current_theme_id(), ThemeId::Amber);

        set_theme(ThemeId::Ocean);
        assert_eq!(current_theme_id(), ThemeId::Ocean);

        set_theme(ThemeId::Minimal);
        assert_eq!(current_theme_id(), ThemeId::Minimal);

        set_theme(ThemeId::HighContrast);
        assert_eq!(current_theme_id(), ThemeId::HighContrast);
    }

    #[test]
    fn test_theme_colors() {
        let amber = theme_colors(ThemeId::Amber);
        assert_eq!(amber.primary.r, 212);
        assert_eq!(amber.primary.g, 163);
        assert_eq!(amber.primary.b, 115);

        let ocean = theme_colors(ThemeId::Ocean);
        assert_eq!(ocean.primary.r, 100);
        assert_eq!(ocean.primary.g, 149);
        assert_eq!(ocean.primary.b, 237);
    }
}
