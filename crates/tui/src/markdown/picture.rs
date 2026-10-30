//! A picture in an answer: the block of rows it takes, the caption it keeps, and
//! how the writer handles an image tag (ADR-0016 §2). The store answers what a
//! source is worth; this module says what the writer does with that answer and
//! what the caption reads. The graphic itself is drawn later, by the renderer,
//! over the rows reserved here (ADR-0016 §6, issue #59).

use ratatui::text::{Line, Span};

use crate::markdown::text;
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
    pub rows: Vec<Line<'static>>,
    pub pictures: Vec<Picture>,
}

/// A picture the writer placed: the row its block starts at, the source as the
/// model wrote it, and the caption under it. The block is the rows the store
/// measured followed by the caption; a source that cannot be drawn is the
/// caption alone, so `at` is the caption's own row and `worth` says why
/// (ADR-0016 §8). The renderer draws `worth` over those rows where the
/// terminal can.
pub struct Picture {
    /// Where the block starts, as an index into [`Written::rows`]: a caller that
    /// puts rows of its own above these — a turn's marker, a gutter — adds them
    /// to it before it knows where on the screen the picture goes.
    pub at: usize,
    /// How many rows of [`Written::rows`] the block is: the graphic's rows and
    /// the caption's under them, which is what a copy takes as one row of words
    /// (ADR-0016 §9).
    pub rows: usize,
    /// The columns the block is indented by, inside the width it was measured
    /// at: a quote's `│ ` sits over the picture as it sits over its caption, so
    /// the graphic is drawn where the block's own edge is (ADR-0016 §9).
    pub inset: u16,
    pub src: String,
    pub caption: String,
    /// What the source is worth: the rows it takes, or the reason it takes
    /// none. A frame with no store at all is refused by the terminal itself
    /// (ADR-0016 §1, §8).
    pub worth: Worth,
}

/// The words a picture is captioned with: the alt text's words and the target
/// in `‹›` as a link keeps its own, or the source and the reason it takes no
/// rows where nothing is drawn (ADR-0016 §2, §8). A picture whose words are
/// absent is its target alone.
fn caption(src: &str, alt: &str, worth: &Worth) -> String {
    let target = match worth {
        Worth::Drawn { .. } => format!("‹{src}›"),
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

    /// An image is a block: it ends the row it sits in, reserves the rows the
    /// store measured and captions them, and the paragraph it interrupted goes
    /// on below the caption (ADR-0016 §2).
    ///
    /// Inside a table cell, a list item or a heading it is not a block and is
    /// not drawn — none of the three is a place a graphic goes — so it stays the
    /// words it was given, and that cell, item or title says what it said
    /// before (ADR-0016 §2).
    pub(super) fn image_end(&mut self) {
        let Some(image) = self.image.take() else {
            return;
        };
        if !self.lists.is_empty() || self.table.is_some() || self.heading.is_some() {
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
        // The sentence's row is closed first, so the block starts under it.
        self.flush();
        let prefix = "│ ".repeat(self.quote);
        let columns = self.width.saturating_sub(prefix.chars().count()).max(1);
        let worth = match self.store {
            Some(store) => store.measure(&image.src, columns as u16),
            None => Worth::Refused(Refusal::NoGraphics),
        };
        let caption = caption(&image.src, &image.alt, &worth);
        // A picture that cannot be drawn reserves no rows: it is its caption and
        // the reason, never a hole where a box would have been (ADR-0016 §8).
        let taken = usize::from(worth.rows());
        if self.apart && !self.lines.is_empty() {
            self.lines.push(Line::default());
        }
        self.apart = false;
        let at = self.lines.len();
        self.lines.extend((0..taken).map(|_| Line::default()));
        let words = [Span::styled(caption.clone(), theme::faint())];
        let rows_of_caption = text::wrap(text::glyphs(&words), &prefix, theme::faint(), columns);
        let rows = taken + rows_of_caption.len();
        self.lines.extend(rows_of_caption);
        self.pictures.push(Picture {
            at,
            rows,
            inset: prefix.chars().count() as u16,
            src: image.src,
            caption,
            worth,
        });
    }
}
