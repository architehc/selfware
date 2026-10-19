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
pub use sticky_bar::StickyBar;
pub use style::{Glyphs, SelfwareStyle};
pub use task_display::TaskDisplay;
pub use theme::{current_theme, set_theme, ThemeId};

// TUI and demo modules
#[cfg(feature = "tui")]
pub mod demo;
#[cfg(feature = "tui")]
pub mod tui;
