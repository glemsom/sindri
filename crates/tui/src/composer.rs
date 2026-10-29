//! The composer: the prompt being typed, as rows and a cursor. Enter sends,
//! Shift+Enter breaks the line, and a cancel leaves the draft where it was
//! (ADR-0011 §10).

/// The fewest rows the composer is drawn with, and the most.
const MIN_ROWS: u16 = 1;
const MAX_ROWS: u16 = 6;

/// The prompt as it is being typed.
#[derive(Debug, Clone, Default)]
pub struct Composer {
    rows: Vec<String>,
    row: usize,
    column: usize,
}

impl Composer {
    /// An empty composer, holding one empty row.
    pub fn new() -> Self {
        Composer::default()
    }

    /// The rows as they will be drawn.
    pub fn rows(&self) -> &[String] {
        &self.rows
    }

    /// Where the cursor is: (column, row) inside the composer's text.
    pub fn cursor(&self) -> (u16, u16) {
        (self.column as u16, self.row as u16)
    }

    /// The whole draft, its rows joined the way it will be sent.
    pub fn text(&self) -> String {
        self.rows.join("\n")
    }

    /// Whether there is nothing to send.
    pub fn is_blank(&self) -> bool {
        self.text().trim().is_empty()
    }

    /// Takes the draft, leaving an empty composer.
    pub fn take(&mut self) -> String {
        let text = self.text();
        self.clear();
        text
    }

    /// Empties the composer.
    pub fn clear(&mut self) {
        self.rows.clear();
        self.rows.push(String::new());
        self.row = 0;
        self.column = 0;
    }

    /// How tall the composer is drawn, borders included: at least two rows and
    /// never more than eight (ADR-0011 §4).
    pub fn height(&self) -> u16 {
        let rows = self.rows.len().max(1) as u16;
        rows.clamp(MIN_ROWS, MAX_ROWS) + 2
    }

    /// One character, where the cursor is.
    pub fn insert(&mut self, ch: char) {
        self.ensure_row();
        let column = self.column;
        if let Some(row) = self.rows.get_mut(self.row) {
            let at = byte_index(row, column);
            row.insert(at, ch);
            self.column = column + 1;
        }
    }

    /// A new line, splitting the row at the cursor.
    pub fn newline(&mut self) {
        self.ensure_row();
        if let Some(row) = self.rows.get_mut(self.row) {
            let at = byte_index(row, self.column);
            let rest = row.split_off(at);
            self.row += 1;
            self.column = 0;
            self.rows.insert(self.row, rest);
        }
    }

    /// Deletes the character before the cursor, joining rows at the start of
    /// one.
    pub fn backspace(&mut self) {
        if self.column > 0 {
            if let Some(row) = self.rows.get_mut(self.row) {
                let at = byte_index(row, self.column - 1);
                row.remove(at);
                self.column -= 1;
            }
            return;
        }
        if self.row > 0 {
            let rest = self.rows.remove(self.row);
            self.row -= 1;
            if let Some(row) = self.rows.get_mut(self.row) {
                self.column = row.chars().count();
                row.push_str(&rest);
            }
        }
    }

    /// One column left.
    pub fn left(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        }
    }

    /// One column right.
    pub fn right(&mut self) {
        let end = self.rows.get(self.row).map_or(0, |row| row.chars().count());
        if self.column < end {
            self.column += 1;
        }
    }

    /// One row up.
    pub fn up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.clamp_column();
        }
    }

    /// One row down.
    pub fn down(&mut self) {
        if self.row + 1 < self.rows.len() {
            self.row += 1;
            self.clamp_column();
        }
    }

    fn clamp_column(&mut self) {
        let end = self.rows.get(self.row).map_or(0, |row| row.chars().count());
        self.column = self.column.min(end);
    }

    fn ensure_row(&mut self) {
        if self.rows.is_empty() {
            self.rows.push(String::new());
            self.row = 0;
            self.column = 0;
        }
    }
}

/// The byte offset of the `column`th character, so multi-byte text is never cut
/// in half.
fn byte_index(row: &str, column: usize) -> usize {
    row.char_indices()
        .nth(column)
        .map_or(row.len(), |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::Composer;

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
