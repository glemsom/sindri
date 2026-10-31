//! The transcript column — the hero. A conversation is laid out as the
//! document it is: two rows between turns and none inside one, a turn's body
//! hanging two columns off its own edge, and the prompt at full strength on a
//! band of its own. Only the rows that fit are laid out, so an unbounded
//! conversation still has a bounded render tree, and a reader who scrolls back
//! is held to the row they are reading while the run appends below them.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::render::hello;
use crate::render::layout::{Frame, drawn, offset_of, place, resolve};
use crate::render::pictures::{self, Shown};
use crate::selection::Viewport;

pub struct View {
    pub rows: Vec<Line<'static>>,
    pub plain: Vec<String>,
    pub laid_out: usize,
    /// How many entries the frame had to measure or lay out: the cost a long
    /// thread must not pay for a page.
    pub measured: usize,
    pub spill: Vec<(usize, PathBuf)>,
    /// The drawn rows that are an expandable entry's own row, and the entry
    /// behind each: a click there opens it.
    pub expand: Vec<(usize, usize)>,
    /// The pictures the window shows, re-based to its rows, each with whether
    /// its graphic is whole in them: the third of the frame's row maps, and what
    /// is drawn over the rows the markdown reserved.
    pub(crate) pictures: Vec<Shown>,
}

pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    // A thread with nothing in it is the first frame: the one screen that can
    // say what the run is and what can be typed.
    if app.entries().is_empty() {
        let rows = hello::rows(app, area.width);
        app.set_viewport(Viewport {
            area,
            rows: hello::plain_rows(&rows),
            ..Viewport::default()
        });
        Paragraph::new(rows).render(area, buf);
        return;
    }
    let view = window(app, area.width, area.height as usize);
    // A thread shorter than the column is written on the floor of it, so the
    // newest row is the one above the composer rather than the top row of a
    // screenful of blank ones.
    let at = floor(area, view.rows.len());
    let placed = pictures::placed(&view.pictures, at);
    let (rows, plain) = (view.rows, view.plain);
    app.set_viewport(Viewport {
        area: at,
        rows: plain,
        spill: view.spill,
        expand: view.expand,
        pictures: placed.clone(),
    });
    Paragraph::new(rows).render(at, buf);
    // A picture is drawn where its rows are, after the paragraph that reserved
    // them — and never behind an overlay, which is cells and not a graphic.
    if app.overlay().is_none() {
        for picture in placed.iter().filter(|picture| picture.drawn) {
            pictures::paint(app, picture, buf);
        }
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
    // A hold the reader asked for is theirs until a key takes the tail, so a
    // column the thread is too short to fill does not hand them back to it.
    let engaged = !follow && app.window.engaged(ask);

    let mut frame = Frame::new(app, width);
    let mut first = count;
    let mut below = 0;
    if !follow && held.0 < count {
        first = held.0;
        below = frame.hold(held.0, held.1, count, height);
    }
    if frame.rows.len() < height && !engaged {
        // Nothing is held, or a scroll-back reached the tail with room to
        // spare: the window fills from the newest rows rather than leaving the
        // column half empty, and it follows them from then on.
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
    let top = place(&frame.marks, start);
    // A held window keeps its place, rows into its entry included: that is what
    // stops the text moving when rows arrive below it. A window on the tail
    // names its place again every frame, because the tail is not a row.
    let at = if follow {
        (top, start.saturating_sub(offset_of(&frame.marks, top)))
    } else {
        held
    };
    let laid_out = count.saturating_sub(first);
    let measured = frame.measured;

    let Frame {
        app,
        mut rows,
        spill,
        expand,
        pictures,
        ..
    } = frame;
    rows.drain(..start);
    rows.truncate(visible);
    let mut plain: Vec<String> = rows.iter().map(plain).collect();
    let pictures = pictures::on_screen(pictures, start, visible);
    // A picture's rows are drawn on, so they carry no words of their own: a copy
    // of the block is its caption once, on the block's first row, and nothing on
    // the rest — drawn or not, because the blank rows are the window's either
    // way.
    for picture in &pictures {
        let laid = &picture.laid;
        let end = laid.end().min(plain.len());
        for row in plain.iter_mut().take(end).skip(laid.row) {
            row.clear();
        }
        if let Some(first) = plain.get_mut(laid.row) {
            *first = laid.caption.clone();
        }
    }
    app.window.settled(ask, at, follow, below, plain.len());
    let spill = drawn(spill, start, plain.len());
    let expand = drawn(expand, start, plain.len());
    View {
        rows,
        plain,
        laid_out,
        measured,
        spill,
        expand,
        pictures,
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
