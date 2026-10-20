//! Selfware UI System
//!
//! A personal, artisanal terminal interface for your local AI workshop.
//! Built around the philosophy: software you own, software that knows you,
//! software that lasts.

#![allow(unused_imports)]

pub mod animations;
pub mod banners;
pub mod components;
pub mod garden;
pub mod loading_phrases;
pub mod mascot;
pub mod spinner;
pub mod sticky_bar;
pub mod style;
pub mod task_display;
pub mod theme;

pub use components::{render_header, render_task_complete, render_task_start, WorkshopContext};
pub use garden::{
    bed_display_name, build_garden_from_path, garden_status_short, render_plant, scan_directory,
    DigitalGarden, GardenBed, GardenPlant, GrowthStage, PlantType, Season,
};
pub use spinner::{stop_icon, supports_ansi, supports_color, StopKind, TerminalSpinner};
pub use sticky_bar::{
    active_bash_count, bash_finished, bash_started, fmt_elapsed, fmt_tokens,
    is_active as is_sticky_bar_active, BashGuard, StickyBar, StickyState,
};
pub use style::{Glyphs, SelfwareStyle};
pub use task_display::TaskDisplay;
pub use theme::{
    available_themes, current_theme, current_theme_id, set_theme, theme_colors, theme_from_name,
    ThemeColors, ThemeId,
};

// TUI and demo modules
#[cfg(feature = "tui")]
pub mod demo;
#[cfg(feature = "tui")]
pub mod tui;
