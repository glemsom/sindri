//! One dark theme, in truecolour. There is no light mode and no 256-colour
//! fallback to maintain (ADR-0011).

use ratatui::style::{Color, Modifier, Style};

pub const HEADING: Color = Color::Rgb(0xc0, 0x9a, 0xf0);
pub const FAINT: Color = Color::Rgb(0x6b, 0x74, 0x83);
pub const BAD: Color = Color::Rgb(0xe0, 0x6c, 0x75);
pub const GOOD: Color = Color::Rgb(0x6a, 0xc9, 0x7a);
pub const PROMPT: Color = Color::Rgb(0x61, 0xaf, 0xef);
pub const CODE: Color = Color::Rgb(0xe5, 0xc0, 0x7b);
pub const CODE_BG: Color = Color::Rgb(0x2b, 0x30, 0x3b);
pub const DIFF_ADDED: Color = Color::Rgb(0x6a, 0xc9, 0x7a);
pub const DIFF_REMOVED: Color = Color::Rgb(0xe0, 0x6c, 0x75);
pub const DIFF_HUNK: Color = Color::Rgb(0x61, 0xaf, 0xef);
pub const DIFF_FILE: Color = Color::Rgb(0x8b, 0x98, 0xa9);

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
