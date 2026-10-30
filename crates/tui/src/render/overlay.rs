//! What covers the transcript when something has to: the list of every
//! binding `?` opens, a command's untruncated output, and the settings modal
//! (ADR-0011). Each is a box in the middle of the frame on the raised tier,
//! over a frame dimmed so what it covers recedes (ADR-0016); it closes with
//! `Esc`, and the modal sizes itself to what it has to say.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget};

use crate::app::App;
use crate::markdown::text;
use crate::overlay::{Overlay, help_rows};
use crate::render::settings;
use crate::theme;

const KEYS: usize = 20;

/// Draws the overlay that has the screen, over everything else. The overlay
/// is borrowed rather than cloned: a spill of a megabyte is a window per
/// frame, never a copy of itself (ADR-0013).
pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    let Some(overlay) = app.overlay_mut() else {
        return;
    };
    // A modal is a modal: everything it leaves uncovered recedes, so what
    // covers and what is covered read as one frame and not a hole in it
    // (ADR-0016). The overlay's own rows are drawn after `Clear`, at full
    // strength, so this pass costs nothing to undo.
    dim(area, buf);
    let (height, title, lines) = match &mut *overlay {
        Overlay::Settings(settings) => {
            settings::draw(settings, area, buf);
            return;
        }
        Overlay::Help => (
            (help_rows().len() as u16 + 2).min(area.height),
            "keys".to_string(),
            help_rows()
                .into_iter()
                .map(|(keys, what)| help_row(keys, &what))
                .collect(),
        ),
        Overlay::Spill(spill) => {
            spill.settled(spill_window(area).height as usize);
            let title = format!(
                "output: {} · {} lines",
                spill.path.display(),
                spill.lines.len()
            );
            let lines = spill
                .window()
                .iter()
                .map(|line| Line::from(Span::styled(line.as_str(), theme::faint())))
                .collect();
            (area.height, title, lines)
        }
    };
    let width = area.width.saturating_sub(8).clamp(20, 110);
    let boxed = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    let inner = Block::bordered().inner(boxed);
    render(buf, boxed, inner, &title, lines);
}

/// One pass of `DIM` over every cell of the frame, glyphs and surfaces
/// alike: a modifier and not a colour, so it applies to whatever was drawn.
fn dim(area: Rect, buf: &mut Buffer) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].modifier.insert(Modifier::DIM);
        }
    }
}

fn spill_window(area: Rect) -> Rect {
    Rect {
        height: area.height.saturating_sub(2),
        ..area
    }
}

fn render(buf: &mut Buffer, boxed: Rect, inner: Rect, title: &str, lines: Vec<Line<'_>>) {
    Clear.render(boxed, buf);
    Paragraph::new(lines)
        .style(theme::raised())
        .render(inner, buf);
    block(title).render(boxed, buf);
}

/// The box an overlay is drawn in: a step above a panel, because it is drawn
/// over one, and rounded because the frame's boxes are (ADR-0016).
pub(super) fn block(title: &str) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .style(theme::raised())
        .border_style(theme::faint())
        .title(Span::styled(format!(" {title} "), theme::faint()))
}

fn help_row(keys: &str, what: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{keys:<KEYS$}"), theme::prompt()),
        Span::styled(text::clip(what, 200), theme::faint()),
    ])
}
