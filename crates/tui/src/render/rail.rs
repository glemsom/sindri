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
    let (lines, session) = facts(app, width);
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
    let drawn = rows.y.saturating_sub(area.y) as usize;
    let plain = lines
        .iter()
        .map(|line| {
            let row = crate::render::transcript::plain(line);
            // The column's own edge is not part of what a row says: a copy of
            // the session id is the id, not the box it was drawn in.
            row.strip_prefix("│ ").unwrap_or(&row).to_string()
        })
        .collect();
    app.set_rail_view(
        rows,
        plain,
        session.start.saturating_sub(drawn)..session.end.saturating_sub(drawn),
    );
    Paragraph::new(lines).render(rows, buf);
}

/// The rail's rows, and where the session fact sits among them.
fn facts(app: &App, width: usize) -> (Vec<Line<'static>>, std::ops::Range<usize>) {
    let rule = || Line::from(Span::styled("│", theme::faint()));
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut session = 0..0;
    for (label, value) in app.rail().facts() {
        let is_session = label == "session";
        if is_session {
            lines.push(rule());
            session.start = lines.len();
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
        if is_session {
            session.end = lines.len();
        }
    }
    (lines, session)
}

pub fn strip(app: &App, area: Rect, buf: &mut Buffer) {
    let text = app.rail().strip(area.width as usize);
    let shown: String = text.chars().take(area.width as usize).collect();
    let line = Line::from(Span::styled(shown, theme::faint()));
    Paragraph::new(line).render(area, buf);
}
