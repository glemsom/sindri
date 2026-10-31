//! The card's drawing: what a broken boot paints instead of a half-drawn
//! interface. The words are wrapped and the block is sized from
//! the rows they made, so nothing is cut mid-sentence; what does not fit is
//! reached with the page keys and marked on the border.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::card::{Card, View};
use crate::markdown::text;
use crate::theme;

const WHOLE: &str = "any key leaves · run sindri doctor to check again";
const PAGED: &str = "PgUp/PgDn scroll · any other key leaves · run sindri doctor to check again";

pub fn draw(card: &Card, view: &mut View, area: Rect, buf: &mut Buffer) {
    let width = area.width.saturating_sub(2).max(1) as usize;
    // What failed, then what said so, then the doctor's own report, each line
    // of it on the row it was written on. Three weights, so the title, the
    // check and the description read as three things and not one paragraph.
    let mut content = wrapped(
        &card.title,
        theme::bad().add_modifier(Modifier::BOLD),
        width,
    );
    content.push(Line::default());
    content.extend(text::wrap(
        text::glyphs(&[Span::styled(card.check.clone(), Style::default())]),
        "check: ",
        theme::faint(),
        width,
    ));
    content.push(Line::default());
    content.extend(wrapped(&card.detail, theme::faint(), width));

    // The hint is the card's last row and is never scrolled away: the way out
    // and the command that checks again are readable from every page. Whether
    // the page keys are worth naming is only known once the rows are counted.
    let room = area.height.saturating_sub(2) as usize;
    let whole = wrapped(WHOLE, theme::faint(), width);
    let hint = if content.len() + whole.len() <= room {
        whole
    } else {
        wrapped(PAGED, theme::faint(), width)
    };
    // A terminal with fewer rows than the hint needs keeps its border and the
    // first row of the hint, and shows the report on the pages it does have.
    let hint: Vec<Line<'static>> = hint.into_iter().take(room).collect();
    let visible = room.saturating_sub(hint.len());
    view.settled(content.len(), visible);
    let window = view.window();

    // The block is as tall as the report it holds, and no taller than the
    // screen: a card that fits sits in the middle of the frame instead of
    // leaving six rows' worth of nothing under it.
    let height = window
        .len()
        .saturating_add(hint.len())
        .saturating_add(2)
        .min(area.height as usize) as u16;
    let boxed = Rect {
        y: area.y + (area.height.saturating_sub(height)) / 2,
        height,
        ..area
    };
    let inner = Block::bordered().inner(boxed);
    let body = Rect {
        height: (window.len() as u16).min(inner.height),
        ..inner
    };
    let foot = Rect {
        y: inner.bottom().saturating_sub(hint.len() as u16),
        height: (hint.len() as u16).min(inner.height),
        ..inner
    };
    // The box is wiped and then filled with the canvas, not left at the
    // terminal's background: `Clear` empties the cells of the frame under the
    // card, and the fill under them is the one `draw_card` painted.
    Clear.render(boxed, buf);
    super::canvas(boxed, buf);
    Paragraph::new(content[window].to_vec()).render(body, buf);
    Paragraph::new(hint).render(foot, buf);
    block(view).render(boxed, buf);
}

/// The card's frame: the failure's colour, and the mark on the edge the report
/// runs past, so a reader knows there is more without paging to find out.
fn block(view: &View) -> Block<'static> {
    let mark = |mark: &'static str| Line::from(Span::styled(mark, theme::faint())).right_aligned();
    let mut block = Block::bordered()
        .border_style(theme::bad())
        .title(Span::styled(" sindri ", theme::bad()));
    if view.above() {
        block = block.title_top(mark(" ↑ more "));
    }
    if view.below() {
        block = block.title_bottom(mark(" ↓ more "));
    }
    block
}

fn wrapped(text_of: &str, style: Style, width: usize) -> Vec<Line<'static>> {
    text::wrap(
        text::glyphs(&[Span::styled(text_of.to_string(), style)]),
        "",
        style,
        width,
    )
}
