//! A picture in an answer: the block of rows it takes, the caption it keeps, and
//! where the writer places one — an image tag has its block here, and a
//! `mermaid` fence borrows it. The store answers what a source is worth; this
//! module says what the writer does with that answer and what the caption
//! reads. The graphic itself is drawn later, by the renderer, over the rows
//! reserved here.

use ratatui::text::{Line, Span};

use crate::markdown::{Cut, text};
use crate::picture::{Refusal, Worth};
use crate::theme;

use super::Writer;

/// An image between its start and its end: the two things its block is made of.
pub(super) struct Image {
    pub(super) src: String,
    pub(super) alt: String,
}

/// One markdown source as the frame draws it: the rows, and the pictures among
/// them. A picture is a block, so where its block starts is a row of these.
pub struct Written {
    /// The rows asked for: every row of the answer, or the range of them a
    /// window holds, re-based to the first.
    pub rows: Vec<Line<'static>>,
    pub pictures: Vec<Picture>,
    /// How many rows the whole answer is, whatever part of it `rows` holds: the
    /// count a frame places a window against.
    pub total: usize,
    /// Where the answer's last block begins and the rows above it: what a delta
    /// landing in the answer is counted from, so the rows above it are not read
    /// again.
    pub cut: Cut,
}

/// A picture the writer placed: the row its block starts at, the source as the
/// model wrote it, and the caption under it. The block is the rows the store
/// measured followed by the caption; a source that cannot be drawn is the
/// caption alone, so `at` is the caption's own row and `worth` says why.
/// The renderer draws `worth` over those rows where the
/// terminal can.
pub struct Picture {
    /// Where the block starts, as an index into [`Written::rows`]: a caller that
    /// puts rows of its own above these — a turn's marker, a gutter — adds them
    /// to it before it knows where on the screen the picture goes.
    pub at: usize,
    /// How many rows of [`Written::rows`] the block is: the graphic's rows and
    /// the caption's under them, which is what a copy takes as one row of words.
    pub rows: usize,
    /// The columns the block is indented by, inside the width it was measured
    /// at: a quote's `│ ` sits over the picture as it sits over its caption, so
    /// the graphic is drawn where the block's own edge is.
    pub inset: u16,
    /// The source as the model wrote it — a target, or a fence's own name.
    pub src: String,
    pub caption: String,
    /// What a copy of the block yields: the caption where the caption is the
    /// words, and the fence body where it is not. `‹mermaid›` is a name, and a
    /// block that copies as a name copies as nothing a reader can paste.
    pub copy: String,
    /// What the source is worth: the rows it takes, or the reason it takes
    /// none. A frame with no store at all is refused by the terminal itself.
    pub worth: Worth,
}

/// The words a picture is captioned with: the alt text's words and the target
/// in `‹›` as a link keeps its own, or the source and the reason it takes no
/// rows where nothing is drawn. A picture whose words are
/// absent is its target alone.
fn caption(src: &str, alt: &str, worth: &Worth) -> String {
    let target = match worth {
        // A block nothing has refused says its source and no reason: a fence
        // waiting for its drawing is its panel and never reaches a caption.
        Worth::Drawn { .. } | Worth::Pending => format!("‹{src}›"),
        Worth::Refused(refusal) => format!("‹{src}: {}›", refusal.said()),
    };
    let words = alt.trim();
    if words.is_empty() {
        target
    } else {
        format!("{words} {target}")
    }
}

impl Writer<'_> {
    /// The alt text of the image being written, kept as its caption, or `None`
    /// where no image is open: the one place the two inline events ask.
    pub(super) fn alt(&mut self) -> Option<&mut String> {
        self.image.as_mut().map(|image| &mut image.alt)
    }

    /// Whether this place in the answer holds words rather than blocks: a table
    /// cell, a list item and a heading are none of them a place a graphic goes.
    pub(super) fn inline(&self) -> bool {
        !self.lists.is_empty() || self.table.is_some() || self.heading.is_some()
    }

    /// The marker a block's rows hang off, and the columns left inside it: a
    /// quote's `│ ` sits over a picture as it sits over its caption.
    pub(super) fn block_margin(&self) -> (String, usize) {
        let prefix = "│ ".repeat(self.quote);
        let columns = self.width.saturating_sub(prefix.chars().count()).max(1);
        (prefix, columns)
    }

    /// A picture the store drew, as a block of the answer: the rows it was
    /// measured at, the caption under them, and the paragraph it interrupted
    /// going on below. A source that could not be drawn reserves no rows, so it
    /// is its caption alone rather than a hole where a box would have been.
    /// `copy` is what a copy of the block yields, which is the caption for
    /// everything whose caption is its words and the fence body for a diagram.
    pub(super) fn place(&mut self, src: String, caption: String, worth: Worth, copy: String) {
        self.flush();
        let (prefix, columns) = self.block_margin();
        let taken = usize::from(worth.rows());
        if self.apart && self.counted > 0 {
            self.keep(vec![Line::default()]);
        }
        self.apart = false;
        let at = self.counted;
        self.keep(vec![Line::default(); taken]);
        let words = [Span::styled(caption.clone(), theme::faint())];
        let rows_of_caption = text::wrap(text::glyphs(&words), &prefix, theme::faint(), columns);
        let rows = taken + rows_of_caption.len();
        self.keep(rows_of_caption);
        self.pictures.push(Picture {
            at,
            rows,
            inset: prefix.chars().count() as u16,
            src,
            caption,
            copy,
            worth,
        });
    }

    /// An image is a block: it ends the row it sits in, reserves the rows the
    /// store measured and captions them, and the paragraph it interrupted goes
    /// on below the caption.
    ///
    /// Inside a table cell, a list item or a heading it is not a block and is
    /// not drawn — none of the three is a place a graphic goes — so it stays the
    /// words it was given, and that cell, item or title says what it said
    /// before.
    pub(super) fn image_end(&mut self) {
        let Some(image) = self.image.take() else {
            return;
        };
        if self.inline() {
            // The words as the writer would have drawn them in that place: a
            // cell takes plain text, and an item the style it is already in.
            let words = if self.table.is_some() {
                Span::raw(image.alt)
            } else {
                let style = self.style();
                Span::styled(image.alt, style)
            };
            self.pending.push(words);
            return;
        }
        let worth = match self.store {
            Some(store) => store.measure(&image.src, self.block_margin().1 as u16),
            None => Worth::Refused(Refusal::NoGraphics),
        };
        let caption = caption(&image.src, &image.alt, &worth);
        // An image copies as its caption, which is the words the alt text gave.
        self.place(image.src, caption.clone(), worth, caption);
    }
}
