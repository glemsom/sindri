//! The composer: what is being typed, in a box, with the cursor where it
//! belongs, and a border lit from the rail's side. Where the terminal speaks
//! Kitty graphics the box is a picture — `border.rs`'s stroke, made for the
//! composer's own rect and ink, placed under the words and held by
//! `render/border.rs` — and the cells it covers are blank surface with
//! ` prompt ` the one word on them. Everywhere else the frame is the rounded
//! glyphs it has always been. The picture is a layer, not a cell: the draft, the
//! placeholder, the `missing` line and the wrapped rows are drawn the same way
//! on both paths, and the cursor is given the same point. The contract is the
//! same one: a rect, an `App`, a `Buffer` in and a `Position` out — the `App`
//! mutably, because the picture it holds is made once and reused while it
//! holds.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};

use crate::app::App;
use crate::border::Ink;
use crate::render::stroke::{self, TITLE};
use crate::theme;

/// The border's walk: the top and bottom rules are lit from the rail's side, so
/// the panel's right-hand end is the seam's ink and its left-hand end is the ink
/// it has when it is quiet, or when it holds the keyboard. Only the rules are
/// walked, and the corners take the walk's own ends, so the silhouette is
/// brightest where the light lands and the panel reads as part of the rail's
/// column rather than a box beside it.
///
/// The light lands because `render::draw` gives the composer the transcript's
/// width: the panel ends one column short of the margin the seam's strip is
/// drawn in, so the corner this walk brightens is the one the light stands
/// beside, not one thirty-three columns past it.
///
/// The ` prompt ` title and the two vertical rules keep one ink: a walk across
/// one cell says nothing, and the title's letters are a word, not a rule.
fn lit(area: Rect, buf: &mut Buffer, ink: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    // The walk is a function of the column, so its step is the panel's width:
    // the last column is the far end, and a panel one column wide is all of it.
    let span = f32::from(area.width.saturating_sub(1)).max(1.0);
    for y in [area.y, area.y + area.height.saturating_sub(1)] {
        for x in area.x..area.x + area.width {
            let cell = &mut buf[(x, y)];
            if !matches!(cell.symbol(), "─" | "╭" | "╮" | "╰" | "╯") {
                continue;
            }
            let t = f32::from(x - area.x) / span;
            cell.set_fg(theme::ramp(ink, theme::SEAM, t));
        }
    }
}

/// What the composer says while it holds nothing: the three ways a prompt can
/// begin — asked, a path, or a command — so the first thing typed is not a
/// guess. It is drawn only over a composer with no text in at
/// all, so it never sits in front of a draft.
pub const PLACEHOLDER: &str = "Ask, or drop a log here · @ path · / command";

pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) -> Position {
    // The composer is the one thing on screen that is always the user's, so
    // it floats on its own panel: the border is the prompt's colour while it
    // holds the focus, and steps back to faint while a run has it. The two
    // states are the two inks a picture of this border is made in.
    let (border, ink) = if app.is_running() {
        (theme::faint(), Ink::Faint)
    } else {
        (theme::prompt(), Ink::Prompt)
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .style(theme::surface())
        .border_style(border)
        .title(Span::styled(TITLE, border));
    // The inner rect is computed the same way on both paths — from the block,
    // drawn or not — so the first glyph of the draft lands on the same cell and
    // the cursor is given the same point. A border that shifts the cursor by one
    // column is worse than any font's glyph.
    let inner = block.inner(area);
    let lines: Vec<Line<'static>> = match app.missing {
        // What this composer cannot do, where the prompt would be: a run with
        // no key cannot send one, and saying so is the frame's.
        Some(missing) if app.composer().is_blank() => {
            vec![Line::from(Span::styled(missing.composer(), theme::faint()))]
        }
        // An empty composer invites the first keystroke; a blank one is not
        // the same thing — a space is a draft, and a draft is never shadowed.
        None if app.composer().text().is_empty() => {
            vec![Line::from(Span::styled(PLACEHOLDER, theme::faint()))]
        }
        _ => app
            .composer()
            .rows()
            .iter()
            .map(|row| Line::from(row.clone()))
            .collect(),
    };
    let (column, row) = app.composer().cursor();
    let cursor = Position::new(
        inner.x.saturating_add(column),
        inner
            .y
            .saturating_add(row)
            .min(inner.y + inner.height.saturating_sub(1)),
    );

    let painted = match app.border.as_mut() {
        Some(picture) => {
            // The picture's air is the panel's: the cells it covers are the
            // surface the composer floats on, blank because the stroke is the
            // border now.
            buf.set_style(area, theme::surface());
            picture.draw(area, ink, buf)
        }
        None => false,
    };
    Paragraph::new(lines)
        .style(theme::surface())
        .render(inner, buf);
    if painted {
        // The image claims every cell it covers, so the words go in after it and
        // the cells they land in are handed back to the terminal: a glyph drawn
        // into a covered cell is never sent.
        stroke::label(border, area, buf);
        stroke::shown(inner, buf);
        stroke::shown(stroke::label_cells(area), buf);
    } else {
        block.render(area, buf);
        lit(area, buf, ink.colour());
    }
    cursor
}

/// The composer seen as a frame: the two paths side by side, and the cells
/// a picture is allowed to change.
#[cfg(test)]
mod tests;
