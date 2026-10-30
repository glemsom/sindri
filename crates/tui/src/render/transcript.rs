//! The transcript column — the hero. A conversation is laid out as the
//! document it is: two rows between turns and none inside one, a turn's body
//! hanging two columns off its own edge, and the prompt at full strength on a
//! band of its own (ADR-0016).
//!
//! Only the rows that fit are laid out, so an unbounded conversation still has
//! a bounded render tree (ADR-0011). The window has a place in the thread and
//! not an offset from its tail: a reader on the tail is shown the newest rows
//! and follows them, and a reader who scrolls back is held to the row they are
//! reading while the run appends below them (ADR-0013, issue #45).

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::render::hello;
use crate::render::layout::{Frame, drawn, offset_of, place, resolve};
use crate::theme;

pub struct View {
    pub rows: Vec<Line<'static>>,
    pub plain: Vec<String>,
    pub laid_out: usize,
    /// Where the window starts in the thread, as an entry, and how many
    /// entries it shows: what the position marker is drawn from.
    pub top: usize,
    pub cover: usize,
    /// Whether the thread runs past the window at either end.
    pub taller: bool,
    /// How many entries the frame had to measure or lay out: the cost a long
    /// thread must not pay for a page (ADR-0013).
    pub measured: usize,
    pub spill: Vec<(usize, PathBuf)>,
    /// The drawn rows that are an expandable entry's own row, and the entry
    /// behind each: a click there opens it (ADR-0011 §9, issue #44).
    pub expand: Vec<(usize, usize)>,
}

pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    // A thread with nothing in it is the first frame: the one screen that can
    // say what the run is and what can be typed (issue #46).
    if app.entries().is_empty() {
        let rows = hello::rows(app, area.width);
        app.set_viewport(area, hello::plain_rows(&rows), Vec::new(), Vec::new());
        Paragraph::new(rows).render(area, buf);
        return;
    }
    let view = window(app, area.width, area.height as usize);
    // A thread shorter than the column is written on the floor of it, so the
    // newest row is the one above the composer rather than the top row of a
    // screenful of blank ones (issue #46).
    let at = floor(area, view.rows.len());
    app.set_viewport(at, view.plain, view.spill.clone(), view.expand.clone());
    let (top, cover, taller) = (view.top, view.cover, view.taller);
    Paragraph::new(view.rows).render(at, buf);
    if taller {
        mark(top, cover, app.entries().len(), area, buf);
    }
}

/// The rows a short thread is drawn on: the bottom of the column, so the
/// newest row sits where the next one will arrive from.
fn floor(area: Rect, rows: usize) -> Rect {
    let height = (rows as u16).min(area.height);
    Rect {
        y: area.y + area.height - height,
        height,
        ..area
    }
}

/// The thread is longer than the window: the column's last cell carries a thin
/// bar over the part of the thread the window shows, so a reader knows both
/// that there is more and where they are in it (issue #45). It is placed by
/// entry and not by row, because the thread's rows are never counted: what the
/// window can afford to know is which entries it is showing (ADR-0013).
fn mark(top: usize, cover: usize, of: usize, area: Rect, buf: &mut Buffer) {
    let of = of.max(1);
    let height = area.height as usize;
    if height == 0 || area.width == 0 {
        return;
    }
    let cover = cover.clamp(1, of);
    let len = (height * cover / of).max(1).min(height);
    // The ends are pinned: a window on the first turn marks the top of the
    // column and a window on the tail marks the bottom of it, whatever the
    // entries between them happen to be worth.
    let at = if top == 0 {
        0
    } else if top + cover >= of {
        height - len
    } else {
        (height * top / of).min(height - len)
    };
    let column = area.x + area.width - 1;
    for row in at..at + len {
        let cell = &mut buf[(column, area.y + row as u16)];
        cell.set_symbol("┃");
        cell.set_style(theme::faint());
    }
}

pub fn window(app: &mut App, width: u16, height: usize) -> View {
    let height = height.max(1);
    let width = width.max(1);
    let count = app.entries().len();
    app.note_width(width);

    // What the reader asked for is answered against the thread as it is now,
    // and only then laid out.
    let ask = app.window.take_ask();
    let mut follow = app.window.follow;
    resolve(app, width, count, ask, &mut follow);
    let held = app.window.at;

    let mut frame = Frame::new(app, width);
    let mut first = count;
    let mut below = 0;
    if !follow && held.0 < count {
        first = held.0;
        below = frame.hold(held.0, held.1, count, height);
    }
    if frame.rows.len() < height {
        // Nothing is held, or the tail came into view with room to spare: the
        // window fills from the newest rows rather than leaving the column half
        // empty, and it follows them from then on.
        follow = true;
        below = 0;
        frame = Frame::new(app, width);
        first = frame.tail(count, height);
    }

    let visible = height.min(frame.rows.len());
    // A window on the tail shows the last rows it built, and one held to a
    // place shows the first of them.
    let start = if follow {
        frame.rows.len().saturating_sub(visible)
    } else {
        0
    };
    let (top, cover) = place(&frame.marks, frame.rows.len(), start, visible);
    // A held window keeps its place, rows into its entry included: that is what
    // stops the text moving when rows arrive below it. A window on the tail
    // names its place again every frame, because the tail is not a row.
    let at = if follow {
        (top, start.saturating_sub(offset_of(&frame.marks, top)))
    } else {
        held
    };
    let taller = below > 0 || at.0 > 0 || at.1 > 0;
    let laid_out = count.saturating_sub(first);
    let measured = frame.measured;

    let Frame {
        app,
        mut rows,
        spill,
        expand,
        ..
    } = frame;
    rows.drain(..start);
    rows.truncate(visible);
    let plain: Vec<String> = rows.iter().map(plain).collect();
    app.window.settled(ask, at, follow, below, plain.len());
    let spill = drawn(spill, start, plain.len());
    let expand = drawn(expand, start, plain.len());
    View {
        rows,
        plain,
        laid_out,
        top,
        cover,
        taller,
        measured,
        spill,
        expand,
    }
}

pub fn plain(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::plain;
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};

    #[test]
    fn a_styled_row_copies_as_its_characters_with_no_style_in_them() {
        let line = Line::from(vec![
            Span::styled(
                "bold",
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" plain"),
        ]);
        assert_eq!(
            plain(&line),
            "bold plain",
            "a copy is the characters, never the styles around them"
        );
    }

    #[test]
    fn trailing_space_is_not_something_anyone_copies() {
        let line = Line::from("a  ");
        assert_eq!(plain(&line), "a");
    }
}
