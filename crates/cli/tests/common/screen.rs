//! A terminal's own memory, for the tests that have to read a screen rather than
//! a stream: a frame writes cells where a cursor points and erases what it
//! overwrites, so `visible`-style stripping cannot say what a reader is looking
//! at. This is the few sequences ratatui uses, and nothing more — plus the one
//! reading that memory cannot do on its own, `says`, which finds a phrase a
//! frame wrapped across rows.

/// A terminal's own memory: the cells a frame put somewhere, and the cursor it
/// put them from. Enough of a terminal to say what a reader is looking at, and
/// nothing more.
#[derive(Debug, Default)]
pub struct Screen {
    cells: std::collections::BTreeMap<(u16, u16), char>,
    row: u16,
    col: u16,
}

impl Screen {
    /// Applies one read: printable characters land where the cursor is, and the
    /// control sequences a frame uses to put them there.
    pub fn feed(&mut self, text: &str) {
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '\u{1b}' => match chars.next() {
                    Some('[') => self.control(&mut chars),
                    // OSC: a picture or a title, up to a bell or a string
                    // terminator. Nothing a frame test asserts on.
                    Some(']') => {
                        for next in chars.by_ref() {
                            if next == '\u{7}' || next == '\u{1b}' {
                                break;
                            }
                        }
                    }
                    _ => {}
                },
                '\r' => self.col = 0,
                '\n' => self.row = self.row.saturating_add(1),
                ch if !ch.is_control() => {
                    self.cells.insert((self.row, self.col), ch);
                    self.col = self.col.saturating_add(1);
                }
                _ => {}
            }
        }
    }

    /// One CSI sequence: its parameters, then the final byte that says what to
    /// do with them.
    fn control(&mut self, chars: &mut std::str::Chars<'_>) {
        let mut params = String::new();
        let mut action = None;
        for ch in chars.by_ref() {
            if ('@'..='~').contains(&ch) {
                action = Some(ch);
                break;
            }
            params.push(ch);
        }
        let numbers: Vec<u16> = params
            .trim_start_matches('?')
            .split(';')
            .map(|number| number.parse().unwrap_or(0))
            .collect();
        let first = numbers.first().copied().unwrap_or(0).max(1);
        match action {
            // Where the cursor goes: `H`/`f` name a cell, the arrows move from
            // it, and `G` names a column.
            Some('H') | Some('f') => {
                self.row = numbers.first().copied().unwrap_or(1).saturating_sub(1);
                self.col = numbers.get(1).copied().unwrap_or(1).saturating_sub(1);
            }
            Some('A') => self.row = self.row.saturating_sub(first),
            Some('B') => self.row = self.row.saturating_add(first),
            Some('C') => self.col = self.col.saturating_add(first),
            Some('D') => self.col = self.col.saturating_sub(first),
            Some('G') => self.col = first - 1,
            // What it erases: `2J` is the whole screen, `J` is this row, and
            // `K` is the rest of it.
            Some('J') if first == 2 => self.cells.clear(),
            Some('J') => self.cells.retain(|(row, _), _| *row != self.row),
            Some('K') => self
                .cells
                .retain(|(row, col), _| *row != self.row || *col < self.col),
            _ => {}
        }
    }

    /// The rows that have anything on them, oldest first.
    pub fn text(&self) -> String {
        let mut rows: std::collections::BTreeMap<u16, Vec<char>> =
            std::collections::BTreeMap::new();
        for ((row, col), ch) in &self.cells {
            let line = rows.entry(*row).or_default();
            while line.len() < usize::from(*col) {
                line.push(' ');
            }
            line.push(*ch);
        }
        rows.values()
            .map(|line| line.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Whether the screen says `phrase`, as words rather than as a row's
/// substring: the frame wraps a sentence, repeats a note's marker on every row
/// it takes, hangs the sentence's punctuation off its last word, and fills the
/// row's right-hand columns with the rail. So a phrase is matched word by word
/// — a row may start it anywhere, and the row below must carry on with it from
/// its first word, which is where a wrap lands.
pub fn says(text: &str, phrase: &str) -> bool {
    let want = words(phrase);
    let mut from = 0;
    for row in text.lines() {
        let row = words(row);
        // The rest of a phrase is only ever at the start of the next row: what
        // follows a row's own words is the rail, and the rail is not speech.
        let starts = if from == 0 { 0..row.len() } else { 0..1 };
        let mut carried = from;
        for start in starts {
            let mut taken = from;
            while taken < want.len()
                && start + taken - from < row.len()
                && row[start + taken - from] == want[taken]
            {
                taken += 1;
            }
            if taken == want.len() {
                return true;
            }
            carried = carried.max(taken);
        }
        from = carried;
    }
    false
}

/// A row, or a phrase, as the words a reader would read: a marker stands on a
/// row of its own as far as matching is concerned, and it is dropped, and a
/// word is compared without the punctuation that hangs off it — `finished:`
/// is the word `finished`.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|ch: char| !ch.is_alphanumeric())
                .to_string()
        })
        .filter(|word| !word.is_empty())
        .collect()
}
