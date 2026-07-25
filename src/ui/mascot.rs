//! Selfware ASCII Mascot System
//!
//! A friendly fox mascot that reacts to agent state with different poses and moods.
//! Adds personality to the terminal experience.

use super::theme::current_theme;
use colored::Colorize;

/// Mascot mood/state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MascotMood {
    /// Friendly greeting
    #[default]
    Greeting,
    /// Deep in thought
    Thinking,
    /// Hard at work
    Working,
    /// Task completed successfully
    Success,
    /// Something went wrong
    Error,
    /// Taking a break
    Idle,
}

/// ASCII art frames for the fox mascot
pub struct FoxMascot;

impl FoxMascot {
    /// Greeting fox - friendly wave
    pub const GREETING: &'static [&'static str] = &[
        r"   /\___/\  ",
        r"  ( o   o ) ",
        r"  (  =^=  ) ",
        r"   )     (  ",
        r"  (       ) ",
        r" ( |     | )",
        r"  \|     |/ ",
    ];

    /// Thinking fox - contemplative pose
    pub const THINKING: &'static [&'static str] = &[
        r"   /\___/\   ",
        r"  ( -   - )  ",
        r"  (  =^=  )  ",
        r"   ) hmm (   ",
        r"  (  \./  )  ",
        r"   \     /   ",
    ];

    /// Thinking animation frames
    pub const THINKING_FRAMES: &'static [&'static [&'static str]] = &[
        &[
            r"   /\___/\   ",
            r"  ( -   - )  ",
            r"  (  =^=  )  ",
            r"   )  .  (   ",
            r"  (       )  ",
        ],
        &[
            r"   /\___/\   ",
            r"  ( -   - )  ",
            r"  (  =^=  )  ",
            r"   ) ..  (   ",
            r"  (       )  ",
        ],
        &[
            r"   /\___/\   ",
            r"  ( -   - )  ",
            r"  (  =^=  )  ",
            r"   ) ... (   ",
            r"  (       )  ",
        ],
    ];

    /// Working fox - typing/coding pose
    pub const WORKING: &'static [&'static str] = &[
        r"   /\___/\    ",
        r"  ( o   o ) []",
        r"  (  =^=  )   ",
        r"   )_||_(     ",
        r"  /|    |\    ",
    ];

    /// Success fox - celebrating
    pub const SUCCESS: &'static [&'static str] = &[
        r"   /\___/\  *",
        r"  ( ^   ^ ) ",
        r"  (  =^=  )*",
        r"   )\\|//(  ",
        r"  (       ) ",
        r"   \\   //  ",
    ];

    /// Error fox - concerned look
    pub const ERROR: &'static [&'static str] = &[
        r"   /\___/\  ",
        r"  ( ;   ; ) ",
        r"  (  =^=  ) ",
        r"   )  ~  (  ",
        r"  (       ) ",
    ];

    /// Idle fox - relaxed
    pub const IDLE: &'static [&'static str] = &[
        r"   /\___/\  ",
        r"  ( -   - ) ",
        r"  (  =^=  )z",
        r"   )     ( z",
        r"  (  ___  ) ",
    ];

    /// Small inline fox (single line)
    pub const INLINE: &'static str = r"/\___/\ ";
}

/// Render the mascot with the given mood
pub fn render_mascot(mood: MascotMood) -> String {
    let frames = match mood {
        MascotMood::Greeting => FoxMascot::GREETING,
        MascotMood::Thinking => FoxMascot::THINKING,
        MascotMood::Working => FoxMascot::WORKING,
        MascotMood::Success => FoxMascot::SUCCESS,
        MascotMood::Error => FoxMascot::ERROR,
        MascotMood::Idle => FoxMascot::IDLE,
    };

    let theme = current_theme();
    let color = match mood {
        MascotMood::Greeting | MascotMood::Idle => theme.primary,
        MascotMood::Thinking | MascotMood::Working => theme.accent,
        MascotMood::Success => theme.success,
        MascotMood::Error => theme.error,
    };

    frames
        .iter()
        .map(|line| format!("  {}", line.custom_color(color)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render a small inline mascot indicator
pub fn render_inline_mascot(mood: MascotMood) -> String {
    let theme = current_theme();
    let (icon, color) = match mood {
        MascotMood::Greeting => ("🦊", theme.primary),
        MascotMood::Thinking => ("💭", theme.accent),
        MascotMood::Working => ("🔧", theme.tool),
        MascotMood::Success => ("🎉", theme.success),
        MascotMood::Error => ("😟", theme.error),
        MascotMood::Idle => ("💤", theme.muted),
    };
    format!("{} {}", icon, FoxMascot::INLINE.custom_color(color))
}

/// Get a thinking animation frame
pub fn thinking_frame(tick: usize) -> &'static [&'static str] {
    let idx = tick % FoxMascot::THINKING_FRAMES.len();
    FoxMascot::THINKING_FRAMES[idx]
}

#[cfg(test)]
#[path = "../../tests/unit/ui/mascot/mascot_test.rs"]
mod tests;
