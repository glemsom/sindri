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
    for row in app.rail().rows() {
        lines.extend(text::wrap(
            text::glyphs(&spans(&row)),
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

/// One fact: its name quietly, its value plainly.
fn spans(row: &str) -> Vec<Span<'static>> {
    match row.split_once(' ') {
        Some((label, value)) if !value.is_empty() => vec![
            Span::styled(format!("{label} "), theme::faint()),
            Span::styled(value.to_string(), ratatui::style::Style::default()),
        ],
        _ => vec![Span::styled(row.to_string(), theme::faint())],
    }
}
