//! One dark theme, in truecolour. There is no light mode and no 256-colour
//! fallback to maintain. The theme owns every colour the frame
//! paints, and the three surfaces that make nearness legible: the terminal's
//! own canvas, the panel a composer or a code fence sits on, and the raised
//! tier above it.

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
/// two hues apart.
pub const DIFF_ADDED_BG: Color = Color::Rgb(0x1c, 0x2b, 0x22);
pub const DIFF_REMOVED_BG: Color = Color::Rgb(0x2f, 0x20, 0x23);

/// The surfaces, in the order they read as nearer: the canvas is the
/// terminal's own black and nothing paints it, a panel is a composer, a
/// fence or the rail, and raised is a step above a panel.
pub const PANEL: Color = Color::Rgb(0x17, 0x1a, 0x21);
pub const RAISED: Color = Color::Rgb(0x23, 0x27, 0x33);
/// Inline code's tint, light enough to sit on a panel as well as the canvas.
pub const CODE_BG: Color = Color::Rgb(0x2b, 0x30, 0x3b);

/// A dot leader's dots: the dimmest ink the frame spends, because a run of
/// dots reads heavier than the label it bridges to and only has to be seen,
/// not read. Dimmer than FAINT, and still over the 3:1 a graphic needs on the
/// canvas, so the gap is visible rather than guessed at.
pub const LEADER: Color = Color::Rgb(0x55, 0x5e, 0x70);
/// The rail's seam: a step above FAINT, because a dot at one-eighth of a cell
/// loses apparent luminance and the spine has to hold the same edge the rule
/// and the facts hold.
pub const SEAM: Color = Color::Rgb(0xa8, 0xb3, 0xc6);

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
    LEADER,
    SEAM,
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

pub fn leader() -> Style {
    Style::new().fg(LEADER)
}

pub fn seam() -> Style {
    Style::new().fg(SEAM)
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

/// A walk between two named stops, `t` of the way across: what a rule lit from
/// one end spends, one cell at a time. The stops are the theme's, so a renderer
/// that walks still names no colour of its own — it interpolates two it was
/// given, and `RAMPS` says which pairs it may interpolate.
pub fn ramp(from: Color, to: Color, t: f32) -> Color {
    let (start, end) = (channels(from), channels(to));
    let t = t.clamp(0.0, 1.0);
    let [r, g, b] = [0, 1, 2].map(|channel| {
        let (begin, close) = (f32::from(start[channel]), f32::from(end[channel]));
        (begin + (close - begin) * t).round() as u8
    });
    Color::Rgb(r, g, b)
}

/// The segments a renderer may walk, and the only ones. The composer's border
/// is lit from the rail's side: it leaves the ink the panel has when it is
/// quiet, or the ink it has when it holds the keyboard, and arrives at the
/// seam's — the colour of the spine it is being lit by.
pub const RAMPS: &[(Color, Color)] = &[(FAINT, SEAM), (PROMPT, SEAM)];

/// Whether the frame may paint a colour: one `PALETTE` names, one a walk of
/// `RAMPS` arrives at, or nothing at all. `tests/theme.rs` asks this of every
/// cell — a renderer that invents a hue still fails, and the law it fails
/// against is now stated where the walk lives rather than in the test.
pub fn owns(colour: Color) -> bool {
    colour == Color::Reset
        || PALETTE.contains(&colour)
        || RAMPS.iter().any(|(from, to)| on_ramp(colour, *from, *to))
}

/// Whether `colour` sits on the segment `from`–`to`, within the step a float
/// walk rounded to: the cell carries bytes and the walk computes in floats, so
/// a step either way is the same step. The law is that no renderer invents a
/// hue, never that it rounds the way this one does.
fn on_ramp(colour: Color, from: Color, to: Color) -> bool {
    let Color::Rgb(r, g, b) = colour else {
        return false;
    };
    let point = [r, g, b].map(f32::from);
    let (start, end) = (channels(from), channels(to));
    let walk: [f32; 3] =
        [0, 1, 2].map(|channel| f32::from(end[channel]) - f32::from(start[channel]));
    // The widest channel says how far along the walk a point sits; the other
    // two follow it, because one step of the walk moves all three together.
    let widest = (0..3)
        .max_by(|a, b| walk[*a].abs().total_cmp(&walk[*b].abs()))
        .unwrap_or(0);
    let t = if walk[widest].abs() < f32::EPSILON {
        0.0
    } else {
        (point[widest] - f32::from(start[widest])) / walk[widest]
    };
    (-0.01..=1.01).contains(&t)
        && (0..3).all(|channel| {
            let along = f32::from(start[channel]) + walk[channel] * t;
            (point[channel] - along).abs() <= 1.0
        })
}

/// A stop's truecolour bytes: a walk between two colours that are not
/// truecolour is a module that panics rather than a gradient nobody can see.
const fn channels(colour: Color) -> [u8; 3] {
    match colour {
        Color::Rgb(r, g, b) => [r, g, b],
        _ => panic!("a ramp is walked between two truecolour stops"),
    }
}
