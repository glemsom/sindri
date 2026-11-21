//! A row of the composer's band: the gutter it opens with, and the runs it is drawn as.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen::{self, Clip};
use crate::rows::{INDENT, SIGIL};

/// The gutter a row of the band opens with: `Lit` is the sigil of the row the caret waits in, `Open`
/// the sigil of a row a prompt's words begin on, and `Blank` the indent under one. Two columns every
/// row spends, so the draft's words and a queued prompt's begin where an answer's prose does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gutter {
    Lit,
    Open,
    Blank,
}

/// Writes one row of the composer's band: the gutter it opens with, then each run in order in the ink
/// it carries, cut at the row's edge.
pub fn row(
    buffer: &mut Buffer,
    area: Rect,
    gutter: Gutter,
    runs: &[(&str, Ink)],
    palette: &Palette,
) {
    let sigil = match gutter {
        Gutter::Lit => Some(Ink::Accent),
        Gutter::Open => Some(Ink::Dim),
        Gutter::Blank => None,
    };
    let mut spans: Vec<(&str, Ink)> = Vec::with_capacity(runs.len().saturating_add(2));
    match sigil {
        Some(ink) => {
            // The blank column and then the sigil, which is where a prompt's own sigil stands.
            spans.push((" ", Ink::Dim));
            spans.push((SIGIL.trim_end(), ink));
        }
        None => spans.push((INDENT, Ink::Dim)),
    }
    spans.extend_from_slice(runs);
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
