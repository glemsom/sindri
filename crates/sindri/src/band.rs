//! A row of the composer's band: the gutter it opens with, and the runs it is drawn as.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen::{self, Clip};
use crate::rows::INDENT;

/// The sigil that opens the draft, standing in the gutter an answer is set from — `rows::INDENT`
/// columns wide, inside the box the band closes into.
pub(crate) const SIGIL: &str = "\u{203a} ";

/// One stretch of a row drawn in one ink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run<'a> {
    text: &'a str,
    ink: Ink,
}

impl<'a> Run<'a> {
    pub const fn in_ink(text: &'a str, ink: Ink) -> Run<'a> {
        Run { text, ink }
    }

    pub fn text(&self) -> &'a str {
        self.text
    }

    pub fn ink(&self) -> Ink {
        self.ink
    }
}

/// Writes one row of the composer's band. The sigil opens the row a prompt's words begin on and
/// every other row keeps the gutter blank under it, so a line of the draft and a waited prompt both
/// begin in the answer's first column.
pub fn row(buffer: &mut Buffer, area: Rect, opens: bool, runs: &[Run<'_>], palette: &Palette) {
    let gutter = match opens {
        true => SIGIL,
        false => INDENT,
    };
    let mut spans: Vec<(&str, Ink)> = Vec::with_capacity(runs.len().saturating_add(1));
    spans.push((gutter, Ink::Dim));
    spans.extend(runs.iter().map(|run| (run.text(), run.ink())));
    pen::write(
        buffer,
        Rect {
            y: area.y,
            height: 1,
            ..area
        },
        &spans,
        Clip::Edge,
        palette,
    );
}

#[cfg(test)]
mod tests;
