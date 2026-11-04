//! What covers the transcript when something has to: the list of every
//! binding `?` opens, a command's untruncated output, the settings modal, and
//! `/model`'s screen. Each is a box in the middle of the frame on the raised
//! tier, over a frame dimmed so what it covers recedes; it closes with `Esc`,
//! and the modal and the screen size themselves to what they have to say.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget};

use crate::app::App;
use crate::markdown::text;
use crate::overlay::{Overlay, help_rows};
use crate::render::{model, settings};
use crate::theme;

const KEYS: usize = 20;

/// Which edges of a box run past what it shows: what it marks so a reader
/// knows there is more without scrolling to find out.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct More {
    pub above: bool,
    pub below: bool,
}

/// Draws the overlay that has the screen, over everything else. The overlay
/// is borrowed rather than cloned: a spill of a megabyte is a window per
/// frame, never a copy of itself. The answer is the cell the terminal's cursor
/// belongs in when the overlay is one a reader types into — the settings
/// modal's row — and `None` for every overlay drawn read-only.
pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) -> Option<Position> {
    let overlay = app.overlay.as_mut()?;
    // A modal is a modal: everything it leaves uncovered recedes, so what
    // covers and what is covered read as one frame and not a hole in it.
    // The overlay's own rows are drawn after `Clear`, at full
    // strength, so this pass costs nothing to undo.
    dim(area, buf);
    // The box's outside, then the columns its inside has: a box's rows are
    // clipped to what the box will hold, not to the terminal.
    let width = area.width.saturating_sub(8).clamp(20, 110);
    let inside = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y,
        width,
        height: area.height,
    };
    let content = Block::bordered().inner(inside);
    let (height, title, marks, lines) = match &mut *overlay {
        Overlay::Settings(settings) => {
            // A page moves the modal by what its box shows, not by what the
            // transcript it covers happens to be showing.
            let (page, cursor) = settings::draw(settings, &app.served, area, buf);
            app.window.page = page.max(1);
            return cursor;
        }
        Overlay::Model(model) => {
            // The screen sizes itself and says which way its list runs past the
            // box, so a page moves it by the names its box showed.
            app.window.page = model::draw(model, &app.served, area, buf).max(1);
            return None;
        }
        Overlay::Help(help) => {
            // The box is as tall as the rows the list wrapped into, and no
            // taller than the terminal: what does not fit is reached with the
            // page keys, and the box says where the reader is.
            let lines = help_lines(content.width as usize);
            let height = (lines.len() as u16).saturating_add(2).min(area.height);
            let visible = height.saturating_sub(2) as usize;
            help.scroll.settled(lines.len(), visible);
            app.window.page = visible.max(1);
            let window = help.scroll.window();
            (
                height,
                help.title(),
                More {
                    above: help.scroll.above(),
                    below: help.scroll.below(),
                },
                lines[window].to_vec(),
            )
        }
        Overlay::Spill(spill) => {
            let visible = spill_window(area).height as usize;
            spill.scroll.settled(spill.lines.len(), visible);
            app.window.page = visible.max(1);
            let title = format!("{} · {} lines", spill.title, spill.lines.len());
            let lines = spill
                .window()
                .iter()
                .map(String::as_str)
                .map(text_line)
                .collect();
            (area.height, title, More::default(), lines)
        }
    };
    let boxed = Rect {
        y: area.y + (area.height.saturating_sub(height)) / 2,
        height,
        ..inside
    };
    let inner = Block::bordered().inner(boxed);
    render(buf, boxed, inner, &title, marks, lines);
    None
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

/// One line of a box that holds plain text: a spill's file is read in the
/// faint ink that is not the frame speaking.
fn text_line(line: &str) -> Line<'_> {
    Line::from(Span::styled(line, theme::faint()))
}

fn spill_window(area: Rect) -> Rect {
    Rect {
        height: area.height.saturating_sub(2),
        ..area
    }
}

fn render(
    buf: &mut Buffer,
    boxed: Rect,
    inner: Rect,
    title: &str,
    marks: More,
    lines: Vec<Line<'_>>,
) {
    Clear.render(boxed, buf);
    Paragraph::new(lines)
        .style(theme::raised())
        .render(inner, buf);
    paged(title, marks).render(boxed, buf);
}

/// The same box, saying which way it can move: a marker on the edge the list
/// runs past, on the right of the border it shares with the title, so a reader
/// knows there is more without scrolling to find out.
pub(super) fn paged(title: &str, More { above, below }: More) -> Block<'static> {
    let mark = |mark: &'static str| Line::from(Span::styled(mark, theme::faint())).right_aligned();
    let mut block = block(title);
    if above {
        block = block.title_top(mark(" ↑ more "));
    }
    if below {
        block = block.title_bottom(mark(" ↓ more "));
    }
    block
}

/// The box an overlay is drawn in: a step above a panel, because it is drawn
/// over one, and rounded because the frame's boxes are.
fn block(title: &str) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .style(theme::raised())
        .border_style(theme::faint())
        .title(Span::styled(format!(" {title} "), theme::faint()))
}

/// Every binding as the box draws it: the keys in their column and the
/// description wrapped under it, so a description is read rather than clipped
/// at the border and the box is sized from the rows that wrapping made. A name
/// longer than the column — the command row is one — is not cut to fit it: it
/// keeps a row of its own and the description wraps beneath it.
fn help_lines(columns: usize) -> Vec<Line<'static>> {
    let rest = columns.saturating_sub(KEYS).max(1);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (keys, what) in help_rows() {
        let over = text::clip(&keys, KEYS) != keys;
        if over {
            lines.push(Line::from(Span::styled(keys.clone(), theme::prompt())));
        }
        let rows = text::wrap(
            text::glyphs(&[Span::styled(what, theme::faint())]),
            "",
            theme::faint(),
            rest,
        );
        for (index, row) in rows.into_iter().enumerate() {
            let head = match (over, index) {
                (false, 0) => Span::styled(format!("{keys:<KEYS$}"), theme::prompt()),
                _ => Span::styled(" ".repeat(KEYS), theme::faint()),
            };
            let mut spans = vec![head];
            spans.extend(row.spans);
            lines.push(Line::from(spans));
        }
    }
    lines
}
