//! Where a reader is in something taller than the box holding it: the rows
//! there are, the rows the box has for them, and the place they are reading
//! from. The card and the help list both scroll, so both hold this and neither
//! keeps its own copy of the clamping: the box measures itself, hands the two
//! numbers over, and the keys move within them.

/// A box taller than what it holds, scrolled by a reader. It stops where its
/// last screenful is full, not at the last row: a page of one row of content
/// and nine of nothing is not a page anyone asked for.
#[derive(Debug, Clone, Default)]
pub struct Scroll {
    scroll: usize,
    total: usize,
    visible: usize,
}

impl Scroll {
    /// The rows the box drew, so a list short enough to fit does not scroll at
    /// all and a scrolled one keeps its place across a redraw. A box with no
    /// rows to give still moves by one, so `PgDn` is never a key that does
    /// nothing at all.
    pub fn settled(&mut self, total: usize, visible: usize) {
        self.total = total;
        self.visible = visible.max(1);
        self.scroll = self.scroll.min(self.last());
    }

    fn last(&self) -> usize {
        self.total.saturating_sub(self.visible)
    }

    /// The rows there are, so a box that states where the reader is can name
    /// the end of the list it states it in.
    pub fn total(&self) -> usize {
        self.total
    }

    /// The rows the box shows, from where the reader is.
    pub fn window(&self) -> std::ops::Range<usize> {
        let start = self.scroll.min(self.last());
        start..(start + self.visible).min(self.total)
    }

    /// `PgUp`/`PgDn` for a box that pages by what it shows: the card moves by
    /// its own rows, from the first to the last and no further.
    pub fn page(&mut self, up: bool) {
        self.page_by(self.visible, up);
    }

    /// The same key for a box laid over another: the help list moves by the
    /// rows it was given — the ones the box it covers shows — because what it
    /// covers is the page the reader is turning.
    pub fn page_by(&mut self, rows: usize, up: bool) {
        self.scroll = if up {
            self.scroll.saturating_sub(rows)
        } else {
            self.scroll.saturating_add(rows).min(self.last())
        };
    }

    /// Whether the content runs past the box above or below: what the border
    /// marks so a reader knows there is more without scrolling for it.
    pub fn above(&self) -> bool {
        self.window().start > 0
    }

    pub fn below(&self) -> bool {
        self.window().end < self.total
    }

    /// Whether there is anything to page at all. A box with nothing beyond its
    /// bottom is left by every key, including the page keys.
    pub fn more(&self) -> bool {
        self.total > self.visible
    }
}

#[cfg(test)]
mod tests {
    use super::Scroll;

    /// A list of `total` rows in a box with room for `visible` of them.
    fn scrolled(total: usize, visible: usize) -> Scroll {
        let mut scroll = Scroll::default();
        scroll.settled(total, visible);
        scroll
    }

    #[test]
    fn a_list_that_fits_shows_all_of_itself_and_marks_nothing() {
        let scroll = scrolled(4, 12);
        assert_eq!(scroll.window(), 0..4);
        assert!(!scroll.above() && !scroll.below(), "nothing is hidden");
        assert!(!scroll.more(), "and there is nothing to page");
    }

    #[test]
    fn a_list_taller_than_the_box_shows_the_rows_the_box_has() {
        let scroll = scrolled(50, 20);
        assert_eq!(scroll.window(), 0..20);
        assert!(!scroll.above(), "the reader starts at the first row");
        assert!(scroll.below(), "and the list runs past the bottom");
    }

    #[test]
    fn paging_down_stops_where_the_last_screenful_is_full() {
        let mut scroll = scrolled(50, 20);
        scroll.page(false);
        assert_eq!(scroll.window(), 20..40);
        scroll.page(false);
        assert_eq!(
            scroll.window(),
            30..50,
            "the last page ends on the last row, it does not start on it"
        );
        assert!(
            scroll.above() && !scroll.below(),
            "with the way back marked"
        );
    }

    #[test]
    fn paging_up_stops_at_the_first_row() {
        let mut scroll = scrolled(50, 20);
        scroll.page(false);
        scroll.page(true);
        scroll.page(true);
        assert_eq!(scroll.window(), 0..20, "a page up from the top stays put");
    }

    #[test]
    fn a_box_laid_over_another_pages_by_the_rows_it_was_given() {
        let mut scroll = scrolled(50, 20);
        scroll.page_by(5, false);
        assert_eq!(
            scroll.window(),
            5..25,
            "the page is the box underneath's, not this one's"
        );
        scroll.page_by(5, true);
        assert_eq!(scroll.window(), 0..20);
    }

    #[test]
    fn a_list_that_grows_again_keeps_the_reader_where_they_were_reading() {
        let mut scroll = scrolled(50, 20);
        scroll.page(false);
        scroll.settled(8, 20);
        assert_eq!(
            scroll.window(),
            0..8,
            "a row the reader was on is gone: they start again at the top"
        );
    }

    #[test]
    fn a_box_with_no_rows_to_give_still_moves_by_one() {
        let mut scroll = scrolled(10, 0);
        assert_eq!(scroll.window(), 0..1, "the floor is one row, never none");
        scroll.page(false);
        assert_eq!(scroll.window(), 1..2);
    }
}
