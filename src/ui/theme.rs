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
}

impl ThemeId {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => ThemeId::Amber,
            1 => ThemeId::Ocean,
            2 => ThemeId::Minimal,
            3 => ThemeId::HighContrast,
            _ => ThemeId::Amber,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            ThemeId::Amber => 0,
            ThemeId::Ocean => 1,
            ThemeId::Minimal => 2,
            ThemeId::HighContrast => 3,
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
    match current_theme_id() {
        ThemeId::Amber => ThemeColors::AMBER,
        ThemeId::Ocean => ThemeColors::OCEAN,
        ThemeId::Minimal => ThemeColors::MINIMAL,
        ThemeId::HighContrast => ThemeColors::HIGH_CONTRAST,
    }
}

/// Get theme colors by ID
pub fn theme_colors(id: ThemeId) -> ThemeColors {
    match id {
        ThemeId::Amber => ThemeColors::AMBER,
        ThemeId::Ocean => ThemeColors::OCEAN,
        ThemeId::Minimal => ThemeColors::MINIMAL,
        ThemeId::HighContrast => ThemeColors::HIGH_CONTRAST,
    }
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
