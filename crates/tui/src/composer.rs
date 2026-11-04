//! The composer: the prompt being typed, as rows and a cursor. Enter
//! sends, Shift+Enter and Ctrl-J break the line, a paste arrives as the rows it
//! was copied in, and a cancel leaves the draft where it was. The cursor is held
//! as a character index, because an edit is expressed in characters, and
//! reported in cells, because the terminal draws cells and a glyph may be two of
//! them wide. The panel it is drawn in has at most six rows — `MIN_ROWS` the
//! floor a short draft is given, `MAX_ROWS` the ceiling it stops at — so a taller
//! draft is seen through a window that follows the cursor: the rows drawn are the
//! ones the cursor is in, and the row the cursor is reported on is the panel's,
//! never the draft's when the two differ.

use unicode_width::UnicodeWidthStr;

const MIN_ROWS: u16 = 2;
const MAX_ROWS: u16 = 6;

#[derive(Debug, Clone, Default)]
pub struct Composer {
    rows: Vec<String>,
    row: usize,
    column: usize,
    /// The draft's row the panel's top row is: the window's one piece of state,
    /// moved by `settle` whenever `row` or the draft's rows change, so a draft
    /// no taller than the panel keeps it at nothing and is drawn as it always
    /// was.
    top: usize,
}

impl Composer {
    pub fn new() -> Self {
        Composer::default()
    }

    /// The rows the panel draws: the window of at most `MAX_ROWS` rows that the
    /// cursor is in, and every row of a draft no taller than the panel.
    pub fn visible(&self) -> &[String] {
        let start = self.window_top();
        let end = (start + self.window()).min(self.rows.len());
        &self.rows[start..end]
    }

    /// Where the terminal puts the cursor: the column of cells the row up to
    /// the character index occupies, and the row of the panel it is drawn on.
    /// The index itself stays in characters — arrows, backspace and splice all
    /// count them — so a wide glyph moves the cursor two columns and the index
    /// one, and the row is the window's rather than the draft's.
    pub fn cursor(&self) -> (u16, u16) {
        (
            cell_column(self.row(), self.column) as u16,
            self.row_in_panel() as u16,
        )
    }

    pub fn row(&self) -> &str {
        self.rows.get(self.draft_row()).map_or("", String::as_str)
    }

    /// The draft's own row the cursor is on, which is the panel's row only while
    /// the panel is not scrolled. The keys that walk the draft count these, so a
    /// cursor at the top of a scrolled panel still has rows above it. Held to a
    /// row the draft has, because an index past the draft is drawn on its last
    /// row rather than nowhere.
    pub fn draft_row(&self) -> usize {
        self.row.min(self.rows.len().saturating_sub(1))
    }

    pub fn splice(&mut self, from: usize, text: &str) {
        self.ensure_row();
        if let Some(row) = self.rows.get_mut(self.row) {
            let start = byte_index(row, from);
            let rest = row[start..].to_string();
            let end = rest
                .find(char::is_whitespace)
                .map(|at| start + at)
                .unwrap_or(row.len());
            let tail = row[end..].to_string();
            row.truncate(start);
            row.push_str(text);
            self.column = row.chars().count();
            row.push_str(&tail);
        }
    }

    pub fn text(&self) -> String {
        self.rows.join("\n")
    }

    pub fn is_blank(&self) -> bool {
        self.text().trim().is_empty()
    }

    /// What is typed becomes `text`, the cursor at its end: a queued prompt
    /// taken back arrives above the draft it was typed before.
    pub fn replace(&mut self, text: &str) {
        self.rows = text.split('\n').map(str::to_string).collect();
        if self.rows.is_empty() {
            self.rows.push(String::new());
        }
        self.row = self.rows.len() - 1;
        self.column = self.rows[self.row].chars().count();
        self.settle();
    }

    pub fn take(&mut self) -> String {
        let text = self.text();
        self.clear();
        text
    }

    /// Empties the draft, and says whether there was one: a `Ctrl-C` over a
    /// composer with nothing in it has taken nothing away.
    pub fn clear(&mut self) -> bool {
        let drafted = !self.is_blank() || self.rows.len() > 1;
        self.rows.clear();
        self.rows.push(String::new());
        self.row = 0;
        self.column = 0;
        self.top = 0;
        drafted
    }

    pub fn height(&self) -> u16 {
        self.window() as u16 + 2
    }

    /// One character at the cursor. A character is always a draft with one more
    /// character in it, so it always says something.
    pub fn insert(&mut self, ch: char) -> bool {
        self.ensure_row();
        let column = self.column;
        if let Some(row) = self.rows.get_mut(self.row) {
            let at = byte_index(row, column);
            row.insert(at, ch);
            self.column = column + 1;
        }
        true
    }

    /// A paste, inserted at the cursor as the rows it holds rather than as the
    /// characters a key would have sent: `insert` and `newline` are one
    /// character's doors, and a clipboard arrives with hundreds at once. A line
    /// break is sent three ways — `\n`, `\r\n` and a lone `\r` — and each is
    /// one row, so a pasted prompt keeps the shape it was copied in.
    pub fn insert_str(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        self.ensure_row();
        let lines = text.replace("\r\n", "\n").replace('\r', "\n");
        for (index, line) in lines.split('\n').enumerate() {
            if index > 0 {
                self.newline();
            }
            for ch in line.chars() {
                self.insert(ch);
            }
        }
        true
    }

    /// Breaks the row the cursor is on. Even at the end of a row it is a row the
    /// frame did not have, so it always says something.
    pub fn newline(&mut self) -> bool {
        self.ensure_row();
        if let Some(row) = self.rows.get_mut(self.row) {
            let at = byte_index(row, self.column);
            let rest = row.split_off(at);
            self.row += 1;
            self.column = 0;
            self.rows.insert(self.row, rest);
        }
        self.settle();
        true
    }

    /// One character back, or a row joined to the one above it. At the head of
    /// an empty draft there is nothing behind the cursor to take.
    pub fn backspace(&mut self) -> bool {
        if self.column > 0 {
            if let Some(row) = self.rows.get_mut(self.row) {
                let at = byte_index(row, self.column - 1);
                row.remove(at);
                self.column -= 1;
            }
            return true;
        }
        if self.row > 0 {
            let rest = self.rows.remove(self.row);
            self.row -= 1;
            if let Some(row) = self.rows.get_mut(self.row) {
                self.column = row.chars().count();
                row.push_str(&rest);
            }
            self.settle();
            return true;
        }
        false
    }

    /// The cursor one cell back, and whether it moved: the frame draws where the
    /// cursor is, so an arrow at the head of the draft says nothing.
    pub fn left(&mut self) -> bool {
        if self.column > 0 {
            self.column -= 1;
            return true;
        }
        false
    }

    pub fn right(&mut self) -> bool {
        let end = self.rows.get(self.row).map_or(0, |row| row.chars().count());
        if self.column < end {
            self.column += 1;
            return true;
        }
        false
    }

    pub fn up(&mut self) -> bool {
        if self.row > 0 {
            self.row -= 1;
            self.clamp_column();
            self.settle();
            return true;
        }
        false
    }

    pub fn down(&mut self) -> bool {
        if self.row + 1 < self.rows.len() {
            self.row += 1;
            self.clamp_column();
            self.settle();
            return true;
        }
        false
    }

    fn clamp_column(&mut self) {
        let end = self.rows.get(self.row).map_or(0, |row| row.chars().count());
        self.column = self.column.min(end);
    }

    /// How many rows the panel has: the draft's own count, never fewer than
    /// `MIN_ROWS` and never more than `MAX_ROWS`.
    fn window(&self) -> usize {
        let rows = self.rows.len().max(1);
        rows.clamp(MIN_ROWS as usize, MAX_ROWS as usize)
    }

    /// The panel's row the cursor is on.
    fn row_in_panel(&self) -> usize {
        self.draft_row().saturating_sub(self.window_top())
    }

    /// The draft's row the window opens on, settled from where the cursor is: a
    /// row broken under a full panel takes its top with it, a cursor walked above
    /// the window brings the window up to it, and a draft that shrank under a
    /// scrolled window brings the window back inside the draft. A clause at
    /// either edge, so the window never scrolls past the row the cursor is on.
    ///
    /// Read on every draw and written back by `settle`, because one rule should
    /// answer a window that is stale as readily as one that was just moved: a
    /// draft emptied under a scrolled window can never be asked for rows it does
    /// not have.
    fn window_top(&self) -> usize {
        let window = self.window();
        let row = self.draft_row();
        self.top
            .min(row)
            .max((row + 1).saturating_sub(window))
            .min(self.rows.len().saturating_sub(window))
    }

    /// Announces the window `window_top` has already decided: the one place the
    /// window moves, and the only thing a mutation has to remember.
    fn settle(&mut self) {
        self.top = self.window_top();
    }

    fn ensure_row(&mut self) {
        if self.rows.is_empty() {
            self.rows.push(String::new());
            self.row = 0;
            self.column = 0;
        }
    }
}

fn byte_index(row: &str, column: usize) -> usize {
    row.char_indices()
        .nth(column)
        .map_or(row.len(), |(index, _)| index)
}

/// How many cells the first `column` characters of `row` take up: the offset
/// the cursor is drawn at, and never the count of characters the cursor is held
/// at.
fn cell_column(row: &str, column: usize) -> usize {
    UnicodeWidthStr::width(&row[..byte_index(row, column)])
}

#[cfg(test)]
mod tests {
    use super::Composer;

    /// A draft of `rows` rows, `row 01` to `row NN`, the cursor at its end.
    fn drafted(rows: usize) -> Composer {
        let mut composer = Composer::new();
        let lines: Vec<String> = (1..=rows).map(|n| format!("row {n:02}")).collect();
        composer.insert_str(&lines.join("\n"));
        composer
    }

    #[test]
    fn a_multi_byte_character_is_one_column_and_is_never_split() {
        let mut composer = Composer::new();
        composer.insert('é');
        composer.insert('x');
        assert_eq!(composer.text(), "éx");
        composer.backspace();
        composer.backspace();
        assert_eq!(composer.text(), "");
    }

    #[test]
    fn a_wide_glyph_moves_the_cursor_by_both_its_cells() {
        let mut composer = Composer::new();
        for ch in "漢字".chars() {
            composer.insert(ch);
        }
        assert_eq!(composer.cursor(), (4, 0), "two characters, four cells");
        composer.backspace();
        assert_eq!(composer.cursor(), (2, 0), "an edit is still one character");
    }

    #[test]
    fn a_splice_replaces_one_token_and_leaves_the_rest_of_the_row() {
        let mut composer = Composer::new();
        for ch in "look at @src/ma please".chars() {
            composer.insert(ch);
        }
        composer.splice(8, "src/main.rs");
        assert_eq!(composer.text(), "look at src/main.rs please");
        assert_eq!(composer.cursor(), (19, 0), "the cursor sits after the path");
    }

    #[test]
    fn a_draft_taller_than_the_panel_is_drawn_from_the_row_the_cursor_is_in() {
        // Twenty rows through a panel of six: the window is the six that end
        // at the cursor's row, so the row the cursor is drawn on is the draft's
        // own and not the panel's last.
        let composer = drafted(20);

        assert_eq!(
            composer.visible(),
            ["row 15", "row 16", "row 17", "row 18", "row 19", "row 20"],
            "the panel's six rows are the draft's last six"
        );
        assert_eq!(
            composer.cursor(),
            (6, 5),
            "and the cursor is on the last of them, past the row it ends"
        );
    }

    #[test]
    fn a_draft_that_fits_the_panel_is_drawn_from_its_first_row() {
        let composer = drafted(3);

        assert_eq!(
            composer.visible(),
            ["row 01", "row 02", "row 03"],
            "a draft the panel holds whole is untouched by the window"
        );
        assert_eq!(
            composer.cursor(),
            (6, 2),
            "the cursor keeps the row it had: the third of three"
        );
    }

    #[test]
    fn the_window_moves_by_one_row_only_once_the_cursor_leaves_it() {
        let mut composer = drafted(20);
        for _ in 0..5 {
            composer.up();
        }
        assert_eq!(
            composer.visible()[0],
            "row 15",
            "a cursor walked up inside the window leaves it where it is"
        );
        assert_eq!(composer.cursor(), (6, 0), "and is drawn on its top row");

        composer.up();
        assert_eq!(
            composer.visible()[0],
            "row 14",
            "the row above the window takes the panel's top with it, by one"
        );
        assert_eq!(composer.cursor(), (6, 0), "the cursor stays on the top row");
    }

    #[test]
    fn a_row_broken_at_the_top_of_a_long_draft_is_drawn_where_the_cursor_is() {
        let mut composer = drafted(20);
        for _ in 0..19 {
            composer.up();
        }
        composer.newline();

        assert_eq!(
            composer.cursor(),
            (0, 1),
            "the cursor is on the row the break made, and not off the panel"
        );
        assert_eq!(composer.visible()[1], "", "the row it broke off is empty");
        assert_eq!(
            composer.visible()[0],
            "row 01",
            "above the row that was broken"
        );
    }

    #[test]
    fn a_draft_replaced_by_a_shorter_one_is_drawn_from_its_first_row() {
        let mut composer = drafted(20);
        composer.replace("one\ntwo\nthree\nfour");

        assert_eq!(
            composer.visible(),
            ["one", "two", "three", "four"],
            "a window that was scrolled comes back to the rows the shorter draft has"
        );
        assert_eq!(composer.cursor(), (4, 3), "the cursor is on its last row");
    }

    #[test]
    fn a_draft_emptied_under_the_window_leaves_the_panel_on_its_first_row() {
        let mut composer = drafted(20);
        composer.clear();

        assert_eq!(
            composer.visible(),
            [""],
            "an emptied draft is one empty row"
        );
        assert_eq!(composer.cursor(), (0, 0), "and the cursor at its head");
    }

    #[test]
    fn shift_enter_splits_the_row_the_cursor_is_on() {
        let mut composer = Composer::new();
        for ch in "one two".chars() {
            composer.insert(ch);
        }
        composer.left();
        composer.left();
        composer.left();
        composer.newline();
        assert_eq!(composer.text(), "one \ntwo");
    }
}
