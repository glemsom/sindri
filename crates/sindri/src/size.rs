//! The runs of a row a terminal draws at more than one cell: the cells the sizing leaves, which is
//! the width the row is measured by, and the words it is written as, which is what it copies as.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::Look;
use crate::link;
use crate::palette::Palette;
use crate::pen::{self, Clip};

/// The one scale this product asks a terminal for: the two highest headings at twice the cell.
pub const SCALE: u16 = 2;

/// Whether a terminal draws text at more than one cell, read off the names it gives itself: one
/// that names none is handed the heading it is handed today, because a heading spaced out by a
/// terminal that reads no sizing is worse than a small one.
pub fn speaks(term: Option<&str>, program: Option<&str>) -> bool {
    link::named(term, program, &["kitty", "ghostty"])
}

/// One run of a row's cells drawn at `scale` times the cell: the columns it covers on its row, the
/// words it is drawn as, and the ink they are read in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sized {
    left: u16,
    width: u16,
    scale: u16,
    words: String,
    look: Look,
}

impl Sized {
    /// A run of `words` drawn at `scale`, beginning `left` columns into the row it is on: the
    /// footprint is the cells the words take, each of them `scale` wide.
    pub fn new(left: u16, scale: u16, words: impl Into<String>, look: Look) -> Sized {
        let words = words.into();
        Sized {
            left,
            width: pen::cells(&words).saturating_mul(scale),
            scale,
            words,
            look,
        }
    }

    /// Whether the columns of this run cover the column `at` of the row it is drawn on.
    pub fn covers(&self, at: u16) -> bool {
        self.left <= at && at < self.left.saturating_add(self.width)
    }

    /// The same run `by` columns further right: a run is measured from the left edge of the row it
    /// was laid on, so a row set further right moves it with the cells it covers.
    pub(crate) fn shifted(&self, by: u16) -> Sized {
        Sized {
            left: self.left.saturating_add(by),
            ..self.clone()
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn scale(&self) -> u16 {
        self.scale
    }

    pub fn words(&self) -> &str {
        &self.words
    }

    /// The same words as the cells a scaled glyph leaves: every glyph followed by the columns the
    /// terminal draws it over, so the run measures as wide as it is drawn.
    pub fn spelling(&self) -> String {
        let mut spelling = String::with_capacity(self.words.len() * usize::from(self.scale));
        for glyph in self.words.chars() {
            spelling.push(glyph);
            spelling.push_str(&pen::blank(
                pen::cell(glyph).saturating_mul(self.scale.saturating_sub(1)),
            ));
        }
        spelling
    }
}

/// One run laid on the frame: the row and column it was drawn at, and the run itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Laid {
    row: u16,
    left: u16,
    run: Sized,
}

impl Laid {
    pub fn row(&self) -> u16 {
        self.row
    }

    pub fn left(&self) -> u16 {
        self.left
    }

    pub fn run(&self) -> &Sized {
        &self.run
    }

    /// The columns of a frame row this run covers: the cells it is drawn in on the row it was laid
    /// on, and — the block being a square of cells — the same columns on the row under it.
    fn covers(&self, row: u16, column: u16) -> bool {
        self.row <= row
            && row < self.row.saturating_add(self.run.scale)
            && self.left <= column
            && column < self.left.saturating_add(self.run.width)
    }
}

/// The sized runs a frame draws, laid on the rows it drew: what travels beside the cells, because
/// a cell is a glyph and a look and has no room for the cells the sizing leaves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sizes {
    laid: Vec<Laid>,
}

impl Sizes {
    pub fn none() -> Sizes {
        Sizes::default()
    }

    /// Lays the runs a row names at the frame's row `row`, from the column `left` it was drawn at.
    pub fn lay(&mut self, row: u16, left: u16, runs: &[Sized]) {
        self.laid.extend(runs.iter().map(|run| Laid {
            row,
            left: left.saturating_add(run.left),
            run: run.clone(),
        }));
    }

    /// Keeps the runs whose block stands whole in `band`, and writes each of them as the cells the
    /// sizing leaves: a run with no row of the band under it is drawn as the words it says.
    pub fn settle(&mut self, band: Rect, buffer: &mut Buffer, palette: &Palette) {
        self.laid.retain(|laid| {
            let fits = laid.row.saturating_add(laid.run.scale).saturating_sub(1) < band.bottom();
            if fits {
                pen::write(
                    buffer,
                    Rect {
                        x: laid.left,
                        y: laid.row,
                        width: laid.run.width,
                        height: 1,
                    },
                    &[(laid.run.spelling(), laid.run.look)],
                    Clip::Edge,
                    palette,
                );
            }
            fits
        });
    }

    /// The run covering the cell at `row`'s `column`: a block is written on its first row, and the
    /// cells a row under it holds are the block's and drawn by it. Nothing it does not cover.
    pub fn covers(&self, row: u16, column: u16) -> Option<&Laid> {
        self.laid.iter().find(|laid| laid.covers(row, column))
    }

    pub fn laid(&self) -> &[Laid] {
        &self.laid
    }
}

#[cfg(test)]
mod tests;
