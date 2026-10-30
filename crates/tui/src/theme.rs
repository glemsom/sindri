//! One dark theme, in truecolour. There is no light mode and no 256-colour
//! fallback to maintain (ADR-0011). The theme owns every colour the frame
//! paints, and the three surfaces that make nearness legible: the terminal's
//! own canvas, the panel a composer or a code fence sits on, and the raised
//! tier above it (ADR-0011 §13).

use ratatui::style::{Color, Modifier, Style};

pub const HEADING: Color = Color::Rgb(0xc0, 0x9a, 0xf0);
/// Grey enough to recede, bright enough to clear 4.5:1 on every surface.
pub const FAINT: Color = Color::Rgb(0x8f, 0x98, 0xa8);
pub const BAD: Color = Color::Rgb(0xe0, 0x6c, 0x75);
pub const GOOD: Color = Color::Rgb(0x6a, 0xc9, 0x7a);
pub const PROMPT: Color = Color::Rgb(0x61, 0xaf, 0xef);
pub const CODE: Color = Color::Rgb(0xe5, 0xc0, 0x7b);
pub const DIFF_ADDED: Color = Color::Rgb(0x6a, 0xc9, 0x7a);
pub const DIFF_REMOVED: Color = Color::Rgb(0xe0, 0x6c, 0x75);
pub const DIFF_HUNK: Color = Color::Rgb(0x61, 0xaf, 0xef);
pub const DIFF_FILE: Color = Color::Rgb(0x8b, 0x98, 0xa9);
/// A diff row's wash: the green and the red again, at panel weight, so an
/// added row and a removed one still differ in a terminal that cannot tell the
/// two hues apart (ADR-0011 §24).
pub const DIFF_ADDED_BG: Color = Color::Rgb(0x1c, 0x2b, 0x22);
pub const DIFF_REMOVED_BG: Color = Color::Rgb(0x2f, 0x20, 0x23);

/// The surfaces, in the order they read as nearer: the canvas is the
/// terminal's own black and nothing paints it, a panel is a composer, a
/// fence or the rail, and raised is a step above a panel.
pub const PANEL: Color = Color::Rgb(0x17, 0x1a, 0x21);
pub const RAISED: Color = Color::Rgb(0x23, 0x27, 0x33);
/// Inline code's tint, light enough to sit on a panel as well as the canvas.
pub const CODE_BG: Color = Color::Rgb(0x2b, 0x30, 0x3b);

/// Every colour the frame may paint. A renderer names one of these or nothing;
/// `tests/theme.rs` holds the frame to it.
pub const PALETTE: &[Color] = &[
    HEADING,
    FAINT,
    BAD,
    GOOD,
    PROMPT,
    CODE,
    PANEL,
    RAISED,
    CODE_BG,
    DIFF_ADDED,
    DIFF_REMOVED,
    DIFF_HUNK,
    DIFF_FILE,
    DIFF_ADDED_BG,
    DIFF_REMOVED_BG,
];

pub fn faint() -> Style {
    Style::new().fg(FAINT)
}

pub fn bad() -> Style {
    Style::new().fg(BAD)
}

pub fn good() -> Style {
    Style::new().fg(GOOD)
}

pub fn code() -> Style {
    Style::new().fg(CODE).bg(CODE_BG)
}

pub fn prompt() -> Style {
    Style::new().fg(PROMPT).add_modifier(Modifier::BOLD)
}

/// A panel: the surface a composer, a fence or the rail sits on.
pub fn surface() -> Style {
    Style::new().bg(PANEL)
}

/// The step above a panel, for a thing drawn on top of one.
pub fn raised() -> Style {
    Style::new().bg(RAISED)
}
