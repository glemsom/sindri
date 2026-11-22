//! A row of the composer's band: the gutter it opens with, and the runs it is drawn as.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen::{self, Clip};
use crate::rows::{SIGIL, SPINE};

/// The gutter a row of the band opens with: the spine a prompt's words are quoted by and the sigil
/// the row the draft opens on wears — the accent's while the band holds the keys, the dim ink's on
/// a row the caret waits elsewhere in.
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
    let (sigil, ink) = match gutter {
        Gutter::Lit => (SIGIL.trim_end(), Ink::Accent),
        Gutter::Open => (SIGIL.trim_end(), Ink::Dim),
        Gutter::Blank => (" ", Ink::Dim),
    };
    let mut spans: Vec<(&str, Ink)> = Vec::with_capacity(runs.len().saturating_add(2));
    // The prompt's own two columns, so the draft's words start where a sent prompt's do.
    spans.push((SPINE, Ink::Dim));
    spans.push((sigil, ink));
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
