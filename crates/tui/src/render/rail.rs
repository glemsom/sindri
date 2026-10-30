//! The rail as a column, and as one strip when the terminal is too narrow for
//! one. The face's space is taken first, so its absence closes the rail up
//! rather than leaving a hole (ADR-0011).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::face;
use crate::markdown::text;
use crate::rail::{Block, Fact, Fit};
use crate::theme;

const FACE_FLOOR: u16 = face::ROWS;
/// The label field: nine columns, so `workspace` fits whole and every value
/// starts beside it at the same column.
const LABEL: usize = 9;

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
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut session = 0..0;
    let mut ruled = false;
    // A value is never wrapped: it is shortened to the column beside its
    // label, so a fact is one row however long it is.
    let room = width.saturating_sub(LABEL + 1);
    for fact in app.rail().facts(app.now()) {
        if fact.block() != Block::Spend && !ruled {
            // The rule opens the identity block: the numbers above it, what
            // the run *is* below it.
            lines.push(rule());
            ruled = true;
        }
        if fact.copies_whole() {
            session = lines.len()..lines.len() + 1;
        }
        lines.push(row(&fact, room));
    }
    (lines, session)
}

fn row(fact: &Fact, room: usize) -> Line<'static> {
    match fact.block() {
        Block::Posture => Line::from(vec![
            Span::styled("│", theme::faint()),
            Span::styled(format!(" ⬢ {} ", fact.value()), chip()),
        ]),
        _ => Line::from(vec![
            Span::styled("│ ", theme::faint()),
            Span::styled(format!("{:<LABEL$} ", fact.label()), theme::faint()),
            // A figure that has just grown is pointed at for a moment, so the
            // motion is visible rather than inferred (ADR-0011 §4).
            Span::styled(
                value(fact, room),
                if fact.fresh() {
                    theme::prompt()
                } else {
                    Style::new()
                },
            ),
        ]),
    }
}

/// The value, shortened to the column the rail has for it: an id or a path
/// keeps both its ends, so nothing is ever broken mid-token across rows.
fn value(fact: &Fact, room: usize) -> String {
    match fact.fit() {
        Fit::Middle => text::middle(fact.value(), room),
        Fit::End => text::clip(fact.value(), room),
    }
}

fn rule() -> Line<'static> {
    Line::from(Span::styled("│", theme::faint()))
}

/// A posture is a chip: stated on the raised tier, so it reads as the one
/// thing on the rail that is not a label and a value. The first frame's
/// posture line borrows the same one (ADR-0011 §21).
pub(crate) fn chip() -> ratatui::style::Style {
    theme::good().patch(theme::raised())
}

pub fn strip(app: &App, area: Rect, buf: &mut Buffer) {
    let text = app.rail().strip(area.width as usize, app.now());
    let shown: String = text.chars().take(area.width as usize).collect();
    let line = Line::from(Span::styled(shown, theme::faint()));
    Paragraph::new(line).render(area, buf);
}
