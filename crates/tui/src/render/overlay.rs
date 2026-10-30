//! What covers the transcript when something has to: the list of every
//! binding `?` opens, a command's untruncated output, and the settings modal.
//! Each is a box in the middle of the frame on the raised tier,
//! over a frame dimmed so what it covers recedes; it closes with
//! `Esc`, and the modal sizes itself to what it has to say.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget};

use crate::app::App;
use crate::markdown::text;
use crate::overlay::{Overlay, help_rows};
use crate::render::{sessions, settings};
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
/// frame, never a copy of itself.
pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    let Some(overlay) = app.overlay.as_mut() else {
        return;
    };
    // A modal is a modal: everything it leaves uncovered recedes, so what
    // covers and what is covered read as one frame and not a hole in it.
    // The overlay's own rows are drawn after `Clear`, at full
    // strength, so this pass costs nothing to undo.
    dim(area, buf);
    // The box's outside, then the columns its inside has: a picker's rows are
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
            app.window.page = settings::draw(settings, area, buf).max(1);
            return;
        }
        Overlay::Help(help) => {
            // The box is as tall as the rows the list wrapped into, and no
            // taller than the terminal: what does not fit is reached with the
            // page keys, and the box says where the reader is.
            let lines = help_lines(content.width as usize);
            let height = (lines.len() as u16).saturating_add(2).min(area.height);
            let visible = height.saturating_sub(2) as usize;
            help.settled(lines.len(), visible);
            app.window.page = visible.max(1);
            let window = help.window();
            (
                height,
                help.title(),
                More {
                    above: help.above(),
                    below: help.below(),
                },
                lines[window].to_vec(),
            )
        }
        Overlay::Sessions(sessions) => {
            let (height, title, lines) = sessions::draw(sessions, content);
            app.window.page = lines.len().max(1);
            (height, title, More::default(), lines)
        }
        Overlay::Spill(spill) => {
            spill.settled(spill_window(area).height as usize);
            app.window.page = spill_window(area).height.max(1) as usize;
            let title = format!("{} · {} lines", spill.title, spill.lines.len());
            let lines = spill
                .window()
                .iter()
                .map(|line| Line::from(Span::styled(line.as_str(), theme::faint())))
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
