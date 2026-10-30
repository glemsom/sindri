//! The composer: the prompt being typed, as rows and a cursor. Enter
//! sends, Shift+Enter breaks the line, and a cancel leaves the draft where
//! it was (ADR-0011).

const MIN_ROWS: u16 = 2;
const MAX_ROWS: u16 = 6;

#[derive(Debug, Clone, Default)]
pub struct Composer {
    rows: Vec<String>,
    row: usize,
    column: usize,
}

impl Composer {
    pub fn new() -> Self {
        Composer::default()
    }

    pub fn rows(&self) -> &[String] {
        &self.rows
    }

    pub fn cursor(&self) -> (u16, u16) {
        (self.column as u16, self.row as u16)
    }

    pub fn row(&self) -> &str {
        let row = self.row.min(self.rows.len().saturating_sub(1));
        self.rows.get(row).map_or("", String::as_str)
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

    pub fn take(&mut self) -> String {
        let text = self.text();
        self.clear();
        text
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.rows.push(String::new());
        self.row = 0;
        self.column = 0;
    }

    pub fn height(&self) -> u16 {
        let rows = self.rows.len().max(1) as u16;
        rows.clamp(MIN_ROWS, MAX_ROWS) + 2
    }

    pub fn insert(&mut self, ch: char) {
        self.ensure_row();
        let column = self.column;
        if let Some(row) = self.rows.get_mut(self.row) {
            let at = byte_index(row, column);
            row.insert(at, ch);
            self.column = column + 1;
        }
    }

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

    pub fn left(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        }
    }

    pub fn right(&mut self) {
        let end = self.rows.get(self.row).map_or(0, |row| row.chars().count());
        if self.column < end {
            self.column += 1;
        }
    }

    pub fn up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.clamp_column();
        }
    }

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
