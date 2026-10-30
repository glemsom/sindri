//! Selecting rendered rows with the mouse, and the text that leaves with
//! them: exactly the rows the frame drew, with every style dropped
//! (ADR-0011). The selection is a pair of screen cells, resolved against the
//! rows the last frame drew, so a copy is always of what the user saw — not
//! of the model's markdown source, and not of a style run a terminal would
//! have to interpret.

use std::path::PathBuf;

use ratatui::layout::{Position, Rect};
use unicode_width::UnicodeWidthChar;

use crate::picture::Worth;

#[derive(Debug, Clone, Default)]
pub struct Viewport {
    pub area: Rect,
    pub rows: Vec<String>,
    pub spill: Vec<(usize, PathBuf)>,
    /// The transcript rows that are an expandable entry's own row, and which
    /// entry that is: a click there opens it (ADR-0011 §9, issue #44).
    pub expand: Vec<(usize, usize)>,
    /// The pictures the frame drew: where each block of rows is, and the words
    /// it stands for. A picture is the third of these maps — a row to something
    /// that is not a character — and it is what a repaint and a copy both read
    /// (ADR-0016 §6, §9).
    pub pictures: Vec<Placed>,
}

/// A picture the frame drew: the drawn row its block starts at, how many rows
/// the block covers (the graphic's and its caption's), the cells the graphic
/// was given, and the caption it copies as (ADR-0016 §6, §9). Nothing but the
/// placements is kept: what the buffer can hold is where a picture went, not
/// the pixels that went there.
#[derive(Debug, Clone)]
pub struct Placed {
    /// The drawn row the block starts at.
    pub at: usize,
    /// How many drawn rows the block covers.
    pub rows: usize,
    /// The cells the graphic is drawn into, in the frame's own coordinates.
    pub area: Rect,
    /// The words the block copies as.
    pub caption: String,
    /// What the store measured it as: what a repaint is built from.
    pub worth: Worth,
    /// Whether the whole graphic is in the window. A block partly scrolled out
    /// is placed all the same — its rows are here, and its caption — but it is
    /// drawn nowhere (ADR-0016 §6).
    pub drawn: bool,
}

impl Viewport {
    pub fn holds(&self, at: Position) -> bool {
        at.x >= self.area.x
            && at.x < self.area.x + self.area.width
            && at.y >= self.area.y
            && at.y < self.area.y + self.area.height
    }

    pub fn spilled(&self, row: usize) -> Option<&PathBuf> {
        self.spill
            .iter()
            .find(|(at, _)| *at == row)
            .map(|(_, path)| path)
    }

    pub fn expandable(&self, row: usize) -> Option<usize> {
        self.expand
            .iter()
            .find(|(at, _)| *at == row)
            .map(|(_, entry)| *entry)
    }

    /// The picture a row is filler for: one of its block's rows past the first,
    /// which carries no words because the graphic is drawn there or the caption
    /// is above it. A copy takes the caption once and none of these
    /// (ADR-0016 §9).
    pub fn filler(&self, row: usize) -> Option<&Placed> {
        self.pictures
            .iter()
            .find(|picture| row > picture.at && row < picture.at + picture.rows)
    }

    pub fn row_of(&self, at: Position) -> Option<usize> {
        self.holds(at).then(|| (at.y - self.area.y) as usize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
    pub dragging: bool,
}

impl Selection {
    pub fn start(at: Position) -> Self {
        Selection {
            anchor: at,
            head: at,
            dragging: true,
        }
    }

    pub fn finish(&mut self) {
        self.dragging = false;
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// The text a selection takes: the rows it covers, the first from the column
/// the drag started on, the last up to the column it ended on, and every row
/// between in full. Rows are joined with one newline each, and nothing else
/// is added — a copy is not a paraphrase.
pub fn copied(viewport: &Viewport, selection: &Selection) -> Option<String> {
    if selection.is_empty() || viewport.rows.is_empty() {
        return None;
    }
    let (start, end) = ordered(selection.anchor, selection.head);
    let area = viewport.area;
    let first = row_in(viewport, start)?;
    let last = row_in(viewport, end)?;
    let below = end.y >= area.y + viewport.rows.len() as u16;
    let mut lines = Vec::new();
    for index in first..=last {
        // A picture's block copies as its caption once: the rows past its first
        // are a graphic's — or a caption's, already carried whole on that first
        // row — so a copy of them would be a run of blank lines (ADR-0016 §9).
        // A drag that starts below the block's first row still takes the words.
        let inside = viewport.filler(index);
        let words;
        let row: &str = match inside {
            Some(_) if index != first => continue,
            Some(picture) => {
                words = picture.caption.clone();
                &words
            }
            None => &viewport.rows[index],
        };
        let from = if index == first {
            char_at(row, (start.x.saturating_sub(area.x)) as usize)
        } else {
            0
        };
        let to = if index != last || below {
            row.chars().count()
        } else {
            char_after(row, (end.x.saturating_sub(area.x)) as usize)
        };
        let taken: String = row
            .chars()
            .skip(from)
            .take(to.saturating_sub(from))
            .collect::<String>()
            .trim_end()
            .to_string();
        if !taken.is_empty() || index != first {
            lines.push(taken);
        }
    }
    // Only rows that are blank are dropped: the indentation of the first row
    // taken is part of what the user selected.
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    let text = lines.join("\n");
    (!text.trim().is_empty()).then_some(text)
}

/// The character a screen column falls on. A cell is not a character: a wide
/// glyph takes two, and a selection takes it whole rather than half of it.
fn char_at(row: &str, column: usize) -> usize {
    let mut used = 0;
    for (index, ch) in row.chars().enumerate() {
        if used >= column {
            return index;
        }
        used += UnicodeWidthChar::width(ch).unwrap_or(0);
    }
    row.chars().count()
}

/// The character just past the one covering `column`, so a selection that
/// ends on a wide glyph includes all of it.
fn char_after(row: &str, column: usize) -> usize {
    let mut used = 0;
    for (index, ch) in row.chars().enumerate() {
        used += UnicodeWidthChar::width(ch).unwrap_or(0);
        if used > column {
            return index + 1;
        }
    }
    row.chars().count()
}

fn ordered(one: Position, other: Position) -> (Position, Position) {
    if (one.y, one.x) <= (other.y, other.x) {
        (one, other)
    } else {
        (other, one)
    }
}

fn row_in(viewport: &Viewport, at: Position) -> Option<usize> {
    let row = at.y.checked_sub(viewport.area.y)? as usize;
    let last = viewport.rows.len().checked_sub(1)?;
    Some(row.min(last))
}

#[cfg(test)]
mod tests {
    use super::{Selection, Viewport, copied};
    use ratatui::layout::{Position, Rect};

    fn viewport(rows: &[&str]) -> Viewport {
        Viewport {
            area: Rect::new(0, 2, 60, rows.len() as u16),
            rows: rows.iter().map(|row| row.to_string()).collect(),
            spill: Vec::new(),
            expand: Vec::new(),
            pictures: Vec::new(),
        }
    }

    fn drag(from: (u16, u16), to: (u16, u16)) -> Selection {
        let mut selection = Selection::start(Position::new(from.0, from.1));
        selection.head = Position::new(to.0, to.1);
        selection.finish();
        selection
    }

    /// A frame with a picture in it: the three rows it is drawn on, its caption
    /// under them, and the words of the block on its first row — which is what
    /// the frame puts there, so that a copy is the caption once (ADR-0016 §9).
    fn pictured() -> Viewport {
        let mut viewport = viewport(&[
            "  ✓ 1.2s",
            "a chart ‹plot.png›",
            "",
            "",
            "",
            "",
            "the next block",
        ]);
        viewport.pictures.push(super::Placed {
            at: 1,
            rows: 4,
            area: Rect::new(2, 3, 13, 3),
            caption: "a chart ‹plot.png›".to_string(),
            worth: crate::picture::Worth::Refused(crate::picture::Refusal::Missing),
            drawn: true,
        });
        viewport
    }

    #[test]
    fn a_pictures_block_copies_as_its_caption_once_and_never_its_blank_rows() {
        let viewport = pictured();
        let taken = copied(&viewport, &drag((0, 2), (60, 8)));
        assert_eq!(
            taken.as_deref(),
            Some("  ✓ 1.2s\na chart ‹plot.png›\n\nthe next block"),
            "the caption is the block's words, and its rows are not blank lines"
        );
    }

    #[test]
    fn a_drag_that_starts_on_a_pictures_rows_still_takes_its_words() {
        let viewport = pictured();
        let taken = copied(&viewport, &drag((0, 5), (60, 5)));
        assert_eq!(
            taken.as_deref(),
            Some("a chart ‹plot.png›"),
            "a selection inside the block is the block's words, once"
        );
    }

    #[test]
    fn a_drag_takes_exactly_the_rows_it_covers() {
        let rows = viewport(&["# Title", "hello world", "  indented"]);
        let taken = copied(&rows, &drag((0, 2), (60, 3)));
        assert_eq!(taken.as_deref(), Some("# Title\nhello world"));
    }

    #[test]
    fn the_first_and_last_rows_are_cut_at_the_columns_the_drag_reached() {
        let rows = viewport(&["alpha", "bravo", "charlie"]);
        let taken = copied(&rows, &drag((2, 2), (3, 3)));
        assert_eq!(taken.as_deref(), Some("pha\nbrav"));
    }

    #[test]
    fn a_drag_from_the_bottom_upwards_reads_the_same() {
        let rows = viewport(&["alpha", "bravo"]);
        let up = copied(&rows, &drag((0, 3), (1, 2)));
        let down = copied(&rows, &drag((1, 2), (0, 3)));
        assert_eq!(up, down, "a selection has no direction");
        assert_eq!(up.as_deref(), Some("lpha\nb"));
    }

    #[test]
    fn a_click_is_not_a_selection_and_a_drag_off_the_rows_is_clamped() {
        assert_eq!(copied(&viewport(&["alpha"]), &drag((3, 2), (3, 2))), None);
        let rows = viewport(&["alpha", "bravo"]);
        assert_eq!(
            copied(&rows, &drag((0, 2), (0, 40))).as_deref(),
            Some("alpha\nbravo"),
            "a drag past the last row stops at it"
        );
    }

    #[test]
    fn the_first_rows_indentation_is_part_of_what_was_selected() {
        let rows = viewport(&["    indented", "plain"]);
        assert_eq!(
            copied(&rows, &drag((0, 2), (40, 3))).as_deref(),
            Some("    indented\nplain"),
            "a copy is the rows, leading space and all"
        );
    }

    #[test]
    fn a_wide_glyph_is_taken_whole_rather_than_cut_in_half() {
        let rows = viewport(&["日本語"]);
        let taken = copied(&rows, &drag((0, 2), (4, 2)));
        assert_eq!(
            taken.as_deref(),
            Some("日本語"),
            "a column in the middle of a wide glyph is still that glyph"
        );
        let rows = viewport(&["ab日本"]);
        assert_eq!(
            copied(&rows, &drag((2, 2), (3, 2))).as_deref(),
            Some("日"),
            "two cells of a wide glyph are the glyph"
        );
    }

    #[test]
    fn a_selection_is_the_characters_of_the_rows_and_no_byte_of_anything_else() {
        let rows = viewport(&["bold plain", "│ quoted"]);
        let taken = copied(&rows, &drag((0, 2), (20, 3))).unwrap_or_default();
        assert_eq!(taken, "bold plain\n│ quoted");
    }
}
