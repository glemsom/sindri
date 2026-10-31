//! A terminal's own memory, for the tests that have to read a screen rather than
//! a stream: a frame writes cells where a cursor points and erases what it
//! overwrites, so `visible`-style stripping cannot say what a reader is looking
//! at. This is the few sequences ratatui uses, and nothing more — plus the one
//! reading that memory cannot do on its own, `says`, which finds a phrase a
//! frame wrapped across rows.

#[derive(Debug)]
pub struct Screen {
    cells: std::collections::BTreeMap<(u16, u16), char>,
    row: u16,
    col: u16,
    visible: bool,
    seen_while_visible: Vec<(u16, u16)>,
    /// The bytes of a character a read ended inside, kept for the next read: a
    /// frame arrives in whatever pieces the terminal hands over, and half a
    /// character is not a cell.
    waiting: Vec<u8>,
    /// A sequence a read ended inside, kept for the next read: its tail drawn
    /// as text lands in cells, glues itself to a word, and the row stops saying
    /// what the frame said.
    sequence: String,
}

impl Default for Screen {
    fn default() -> Self {
        Screen {
            cells: std::collections::BTreeMap::new(),
            row: 0,
            col: 0,
            // Nothing has hidden it yet: a terminal starts with the cursor in
            // sight, which is why the first frame has to hide it.
            visible: true,
            seen_while_visible: Vec::new(),
            waiting: Vec::new(),
            sequence: String::new(),
        }
    }
}

impl Screen {
    /// A read, as the terminal handed it over: bytes, because a read can end in
    /// the middle of a character, and the pieces it has not finished say
    /// nothing yet.
    pub fn feed(&mut self, read: &[u8]) {
        let mut bytes = std::mem::take(&mut self.waiting);
        bytes.extend_from_slice(read);
        let (said, waiting) = whole_characters(&bytes);
        self.waiting = waiting;
        let mut chars: Vec<char> = std::mem::take(&mut self.sequence).chars().collect();
        chars.extend(said.chars());
        self.parse(&chars);
    }

    /// Walks the characters the reads made whole. A sequence that ends past the
    /// last of them is kept, whole, for the read that carries the rest of it.
    fn parse(&mut self, chars: &[char]) {
        let mut next = 0;
        while let Some(&ch) = chars.get(next) {
            next += 1;
            match ch {
                '\u{1b}' => match self.sequence_ends(chars, next) {
                    Some(end) => next = end,
                    None => {
                        self.sequence = chars[next - 1..].iter().collect();
                        return;
                    }
                },
                '\r' => self.col = 0,
                '\n' => self.row = self.row.saturating_add(1),
                ch if !ch.is_control() => {
                    if self.visible {
                        self.seen_while_visible.push((self.row, self.col));
                    }
                    self.cells.insert((self.row, self.col), ch);
                    self.col = self.col.saturating_add(1);
                }
                _ => {}
            }
        }
    }

    /// Where the sequence whose introducer sits at `from - 1` ends — applying
    /// it on the way — or `None` where the text stops inside it.
    fn sequence_ends(&mut self, text: &[char], from: usize) -> Option<usize> {
        match text.get(from) {
            // CSI: parameters and intermediates, then one final byte.
            Some('[') => {
                let mut index = from + 1;
                let mut params = String::new();
                while let Some(&ch) = text.get(index) {
                    index += 1;
                    if ('@'..='~').contains(&ch) {
                        self.control(&params, ch);
                        return Some(index);
                    }
                    params.push(ch);
                }
                None
            }
            // OSC: a picture or a title, up to a bell or a string terminator.
            // Nothing a frame test asserts on.
            Some(']') => {
                let mut index = from + 1;
                while let Some(&ch) = text.get(index) {
                    index += 1;
                    match ch {
                        '\u{7}' => return Some(index),
                        '\u{1b}' => {
                            return match text.get(index) {
                                Some('\\') => Some(index + 1),
                                // Not the terminator's own byte: the escape
                                // ends here, and what follows is the text's.
                                Some(_) => Some(index),
                                None => None,
                            };
                        }
                        _ => {}
                    }
                }
                None
            }
            Some(_) => Some(from + 1),
            None => None,
        }
    }

    fn control(&mut self, params: &str, action: char) {
        let numbers: Vec<u16> = params
            .trim_start_matches('?')
            .split(';')
            .map(|number| number.parse().unwrap_or(0))
            .collect();
        let first = numbers.first().copied().unwrap_or(0).max(1);
        match action {
            // Where the cursor goes: `H`/`f` name a cell, the arrows move from
            // it, and `G` names a column.
            'H' | 'f' => {
                self.row = numbers.first().copied().unwrap_or(1).saturating_sub(1);
                self.col = numbers.get(1).copied().unwrap_or(1).saturating_sub(1);
            }
            'A' => self.row = self.row.saturating_sub(first),
            'B' => self.row = self.row.saturating_add(first),
            'C' => self.col = self.col.saturating_add(first),
            'D' => self.col = self.col.saturating_sub(first),
            'G' => self.col = first - 1,
            // What it erases: `2J` is the whole screen, `J` is this row, and
            // `K` is the rest of it.
            'J' if first == 2 => self.cells.clear(),
            'J' => self.cells.retain(|(row, _), _| *row != self.row),
            'K' => self
                .cells
                .retain(|(row, col), _| *row != self.row || *col < self.col),
            // The cursor's own visibility: `?25h` shows it and `?25l` hides it.
            'h' | 'l' if params.starts_with('?') && first == 25 => {
                self.visible = action == 'h';
            }
            _ => {}
        }
    }

    pub fn seen_while_visible(&self) -> &[(u16, u16)] {
        &self.seen_while_visible
    }

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

/// What is text in `read`, and what waits for a character's own tail. Bytes no
/// character is are not a tail to wait on: a terminal draws a replacement for
/// them, and keeping them would stall every read after them.
fn whole_characters(read: &[u8]) -> (std::borrow::Cow<'_, str>, Vec<u8>) {
    match std::str::from_utf8(read) {
        Ok(whole) => (whole.into(), Vec::new()),
        Err(error) if error.error_len().is_none() => (
            String::from_utf8_lossy(&read[..error.valid_up_to()]),
            read[error.valid_up_to()..].to_vec(),
        ),
        Err(_) => (String::from_utf8_lossy(read), Vec::new()),
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
    if want.is_empty() {
        return true;
    }
    let mut from = 0;
    for row in text.lines() {
        let row = words(row);
        // The rest of a phrase is only ever at the start of the next row: what
        // follows a row's own words is the rail, and the rail is not speech.
        let mut carried = 0;
        if from > 0 {
            carried = from + matched(&row, 0, &want[from..]);
        }
        // Every row starts the phrase afresh as well: a row that carries
        // nothing on must not leave the reader looking for a phrase that began
        // somewhere it did not — a rail's `tokens 0 in` is not a phrase's `in`.
        for start in 0..row.len() {
            carried = carried.max(matched(&row, start, &want));
        }
        if carried == want.len() {
            return true;
        }
        from = carried;
    }
    false
}

/// How many of `want`'s words `row` says from `start`, in order.
fn matched(row: &[String], start: usize, want: &[String]) -> usize {
    row.get(start..)
        .unwrap_or_default()
        .iter()
        .zip(want)
        .take_while(|(said, wanted)| said == wanted)
        .count()
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|ch: char| !ch.is_alphanumeric())
                .to_string()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame as a terminal draws it: the alternate screen, a lit row, a
    /// wrapped sentence, a border, a cursor move, and the cursor shown — the
    /// frame's last act.
    const FRAME: &str = "\u{1b}[?1049h\u{1b}[38;48;2;35;39;51m│api_key   from environment\u{1b}[39m─ continued here\n╭─ settings ─╮\u{1b}[2;3H\u{1b}[?25h";

    fn read_in(pieces: &[&[u8]]) -> Screen {
        let mut screen = Screen::default();
        for piece in pieces {
            screen.feed(piece);
        }
        screen
    }

    #[test]
    fn a_frame_that_arrives_in_pieces_is_the_frame_that_was_drawn() {
        let whole = read_in(&[FRAME.as_bytes()]).text();
        assert!(
            says(&whole, "api_key"),
            "the whole frame says its row: {whole:?}"
        );
        for at in 1..FRAME.len() {
            // A read can end anywhere: inside a character, inside a colour, or
            // inside the sequence that moves the cursor.
            let (before, after) = FRAME.as_bytes().split_at(at);
            let split = read_in(&[before, after]).text();
            assert_eq!(split, whole, "a frame split at {at} is the same frame");
            assert!(says(&split, "api_key"), "and says its row: {split:?}");
        }
    }
}
