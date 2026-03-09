//! Selfware UI System
//!
//! A personal, artisanal terminal interface for your local AI workshop.
//! Built around the philosophy: software you own, software that knows you,
//! software that lasts.

pub mod animations;
pub mod banners;
pub mod components;
pub mod diff_viewer;
pub mod garden;
pub mod input_handler;
pub mod loading_phrases;
pub mod mascot;
pub mod selections;
pub mod spinner;
pub mod style;
pub mod swarm_viz;
pub mod task_display;
pub mod theme;

// TUI and demo modules
#[cfg(feature = "tui")]
pub mod demo;
#[cfg(feature = "tui")]
pub mod tui;
