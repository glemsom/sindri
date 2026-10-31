//! Selecting rendered rows with the mouse, and the text that leaves with
//! them: exactly the rows the frame drew, with every style dropped.
//! The selection is a pair of screen cells, resolved against the
//! rows the last frame drew, so a copy is always of what the user saw — not
//! of the model's markdown source, and not of a style run a terminal would
//! have to interpret.

use std::path::PathBuf;

use ratatui::layout::{Position, Rect};

use crate::picture::Worth;

#[derive(Debug, Clone, Default)]
pub struct Viewport {
    pub area: Rect,
    pub rows: Vec<String>,
    pub spill: Vec<(usize, PathBuf)>,
    /// The transcript rows that are an expandable entry's own row, and which
    /// entry that is: a click there opens it.
    pub expand: Vec<(usize, usize)>,
    /// The pictures the frame drew: where each block of rows is, and the words
    /// it stands for. A picture is the third of these maps — a row to something
    /// that is not a character — and it is what a repaint and a copy both read.
    pub pictures: Vec<Placed>,
}

/// A picture the frame drew: the drawn row its block starts at, how many rows
/// the block covers (the graphic's and its caption's), the cells the graphic
/// was given, and the words it copies as. Nothing but the
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
    /// The words the block copies as: a caption, or a diagram's fence body.
    pub copy: String,
    /// What the store measured it as: what a repaint is built from.
    pub worth: Worth,
    /// Whether the whole graphic is in the window. A block partly scrolled out
    /// is placed all the same — its rows are here, and its caption — but it is
    /// drawn nowhere.
    pub drawn: bool,
    /// What the block says instead of a graphic it can never be drawn as: the
    /// rows it asks for against the window's, for a block taller than any
    /// scroll can make whole. `None` where the graphic is drawn, or where the
    /// reader simply scrolled past it.
    pub reason: Option<String>,
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

    /// The picture a row is a row of: the block a drag reaching that row has
    /// taken. Its first row holds the words the block copies as and the rest
    /// carry none, because a graphic is drawn there or the words are above it.
    pub fn block(&self, row: usize) -> Option<&Placed> {
        self.pictures
            .iter()
            .find(|picture| row >= picture.at && row < picture.at + picture.rows)
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

    /// Moves the selection by `rows` of the screen, as the rows under it moved:
    /// what a drag covered is a place in the thread and not on the glass, so a
    /// note that arrives at the tail moves the mark with the text rather than
    /// leaving it over a neighbour's rows.
    pub fn translate(&mut self, rows: isize) {
        for at in [&mut self.anchor, &mut self.head] {
            let rows = rows.clamp(isize::from(i16::MIN), isize::from(i16::MAX)) as i16;
            at.y = at.y.saturating_add_signed(rows);
        }
    }

    /// Pulls both ends into the rows the window holds: a drag that ran off the
    /// edge took the rows that were at the edge, and the mark stands on those
    /// rather than on cells no frame has.
    pub fn clip(&mut self, area: Rect) {
        let last = area.y.saturating_add(area.height.saturating_sub(1));
        for at in [&mut self.anchor, &mut self.head] {
            at.y = at.y.clamp(area.y, last);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// The words a drawn row copies as: what a picture's block stands for — its
/// caption, or the fence body a diagram hides — on the row the block starts at,
/// which is where the frame puts them and why a drag that lands anywhere in a
/// block still takes them; the row itself everywhere else.
pub fn words(viewport: &Viewport, row: usize) -> &str {
    match viewport.block(row) {
        Some(picture) if picture.at == row => &picture.copy,
        _ => viewport.rows.get(row).map_or("", String::as_str),
    }
}

/// The text a selection takes: the rows it covers, the first from the column
/// the drag started on, the last up to the column it ended on, and every row
/// between in full. Rows are joined with one newline each, and nothing else
/// is added — a copy is not a paraphrase.
pub fn copied(viewport: &Viewport, selection: &Selection) -> Option<String> {
    let mut lines = Vec::new();
    for (at, covered) in crate::mark::span(viewport, selection)
        .into_iter()
        .enumerate()
    {
        // A block's rows past its first are lit and not taken: the words are
        // the block's, and they are taken once.
        let Some(chars) = covered.chars else {
            continue;
        };
        let row = words(viewport, covered.row);
        // The text a row yields is the characters the span names, cut at their
        // end: a selection of characters, never of cells.
        let taken: String = row
            .chars()
            .skip(chars.start)
            .take(chars.len())
            .collect::<String>()
            .trim_end()
            .to_string();
        // Only rows that are blank are dropped: the indentation of the first row
        // taken is part of what the user selected.
        if !taken.is_empty() || at != 0 {
            lines.push(taken);
        }
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    let text = lines.join("\n");
    (!text.trim().is_empty()).then_some(text)
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
    /// the frame puts there, so that a copy is the caption once.
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
            copy: "a chart ‹plot.png›".to_string(),
            worth: crate::picture::Worth::Refused(crate::picture::Refusal::Missing),
            drawn: true,
            reason: None,
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
    fn a_diagrams_block_copies_as_the_source_it_hides_and_never_its_caption() {
        let mut viewport = pictured();
        // A diagram is the same block with its fence body as its words: mermaid
        // is what a reader pastes, and `‹mermaid›` is what they cannot.
        for picture in &mut viewport.pictures {
            picture.copy = "stateDiagram-v2\n    [*] --> Resting".to_string();
        }
        let taken = copied(&viewport, &drag((0, 5), (60, 6)));
        assert_eq!(
            taken.as_deref(),
            Some("stateDiagram-v2\n    [*] --> Resting"),
            "the block's words are the source, once, however deep in it the drag starts"
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
        assert_eq!(
            copied(&rows, &drag((3, 2), (4, 2))).as_deref(),
            Some("日本"),
            "and a drag that starts on a wide glyph's second cell starts at it"
        );
    }

    #[test]
    fn a_selection_is_the_characters_of_the_rows_and_no_byte_of_anything_else() {
        let rows = viewport(&["bold plain", "│ quoted"]);
        let taken = copied(&rows, &drag((0, 2), (20, 3))).unwrap_or_default();
        assert_eq!(taken, "bold plain\n│ quoted");
    }
}
