//! The rail as a column, and as one strip when the terminal is too narrow for
//! one. The face's space is taken first, so its absence closes the rail up
//! rather than leaving a hole (ADR-0011).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::face;
use crate::markdown::text;
use crate::theme;

const FACE_FLOOR: u16 = face::ROWS;

pub fn column(app: &mut App, area: Rect, buf: &mut Buffer) {
    // The rail is a panel: the facts sit on a surface, one tier above the
    // canvas the transcript is written on (ADR-0016).
    buf.set_style(area, theme::surface());
    let width = area.width.saturating_sub(2) as usize;
    let lines = facts(app, width);
    let spare = area.height.saturating_sub(lines.len() as u16);
    let mut rows = area;
    if spare >= FACE_FLOOR
        && let Some(face) = app.face_mut()
    {
        // The rows the picture takes, not the rows the rail would allow: a
        // square crop in a rail of tall cells is shorter than it is wide, and
        // reserving the rest would leave a hole under the face.
        let height = face.rows(Rect {
            height: spare,
            ..area
        });
        face.render(Rect { height, ..area }, buf);
        rows = Rect {
            y: area.y + height,
            height: area.height - height,
            ..area
        };
    }
    if rows.height == 0 || rows.width == 0 {
        return;
    }
    Paragraph::new(lines).render(rows, buf);
}

fn facts(app: &App, width: usize) -> Vec<Line<'static>> {
    let rule = || Line::from(Span::styled("│", theme::faint()));
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (label, value) in app.rail().facts() {
        if label == "session" {
            lines.push(rule());
        }
        lines.extend(text::wrap(
            text::glyphs(&[
                Span::styled(label, theme::faint()),
                Span::raw(" "),
                Span::raw(value),
            ]),
            "│ ",
            theme::faint(),
            width,
        ));
    }
    lines
}

pub fn strip(app: &App, area: Rect, buf: &mut Buffer) {
    let text = app.rail().strip(area.width as usize);
    let shown: String = text.chars().take(area.width as usize).collect();
    let line = Line::from(Span::styled(shown, theme::faint()));
    Paragraph::new(line).render(area, buf);
}
