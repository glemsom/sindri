//! One dark theme, in truecolour. There is no light mode and no 256-colour
//! fallback to maintain (ADR-0011 §13).

use ratatui::style::{Color, Modifier, Style};

/// Headings, and anything else that names a section.
pub const HEADING: Color = Color::Rgb(0xc0, 0x9a, 0xf0);
/// What the frame says quietly: timings, hints, the rail's labels.
pub const FAINT: Color = Color::Rgb(0x6b, 0x74, 0x83);
/// A failure, wherever it appears.
pub const BAD: Color = Color::Rgb(0xe0, 0x6c, 0x75);
/// A command that exited zero.
pub const GOOD: Color = Color::Rgb(0x6a, 0xc9, 0x7a);
/// The prompt marker and the composer's own text.
pub const PROMPT: Color = Color::Rgb(0x61, 0xaf, 0xef);
/// Inline code's text.
pub const CODE: Color = Color::Rgb(0xe5, 0xc0, 0x7b);
/// Inline code's ground.
pub const CODE_BG: Color = Color::Rgb(0x2b, 0x30, 0x3b);

/// Quiet, for the things the eye should find second.
pub fn faint() -> Style {
    Style::new().fg(FAINT)
}

/// A failure.
pub fn bad() -> Style {
    Style::new().fg(BAD)
}

/// A command that exited zero.
pub fn good() -> Style {
    Style::new().fg(GOOD)
}

/// Inline code.
pub fn code() -> Style {
    Style::new().fg(CODE).bg(CODE_BG)
}

/// The prompt marker.
pub fn prompt() -> Style {
    Style::new().fg(PROMPT).add_modifier(Modifier::BOLD)
}
