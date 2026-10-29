//! The rail as a column, and as one strip when the terminal is too narrow for
//! one. The face's space is taken first, so its absence closes the rail up
//! rather than leaving a hole (ADR-0011 §5).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::face;
use crate::markdown::text;
use crate::theme;

/// The rail as a 32-column column.
pub fn column(app: &mut App, area: Rect, buf: &mut Buffer) {
    let mut rows = area;
    if let Some(face) = app.face_mut() {
        let height = face::ROWS.min(area.height);
        let face_area = Rect { height, ..area };
        face.render(face_area, buf);
        rows = Rect {
            y: area.y + height,
            height: area.height - height,
            ..area
        };
    }
    if rows.height == 0 || rows.width == 0 {
        return;
    }
    let width = rows.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (label, value) in app.rail().facts() {
        // The session's facts are a group of their own: one blank row apart.
        if label == "session" {
            lines.push(Line::default());
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
    Paragraph::new(lines).render(rows, buf);
}

/// The rail as one line, for a terminal below 100 columns.
pub fn strip(app: &App, area: Rect, buf: &mut Buffer) {
    let text = app.rail().strip(area.width as usize);
    let shown: String = text.chars().take(area.width as usize).collect();
    let line = Line::from(Span::styled(shown, theme::faint()));
    Paragraph::new(line).render(area, buf);
}
