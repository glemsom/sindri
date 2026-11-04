//! One row of the modal as a field: the value it holds, where the next
//! character lands in that value, and what a save makes of it. Every row is
//! this one field — the address, the model name, the header table and a secret
//! — so a keystroke means the same thing in each of them and no row carries an
//! editing path of its own. A row of lines is the same field as a row of one
//! line: the breaks it holds are characters in its value, which is why a paste
//! lands at the cursor there too.
//!
//! The cursor is an index into the value's characters and never into the cells
//! those characters are drawn in: a glyph may be two cells wide, and a row too
//! narrow for its value is scrolled to the cursor where it is drawn. What the
//! frame shows is therefore a reading of this index, and a narrow row costs the
//! field nothing.
//!
//! The breaks of a row of lines are characters like any other, so editing one
//! line of the header table is already a cursor moved to the start of that line
//! and a key taken there: what a line-level edit needs is a cursor that knows
//! its line, not a second kind of row. So a row of lines knows its own: `line`
//! is the line the cursor is in, and the four edits a table is built with — add
//! a blank line, remove one, empty one, stand at the end of one — are the
//! characters moved rather than a second buffer beside the value. The fold that
//! draws the table holds no cursor of its own, which is why the mark it draws
//! and the cell the terminal's cursor stands on cannot disagree.

use sindri_core::config::Key;

/// One row of the modal: the key it edits, the text it holds, the cursor in
/// that text, and what a save makes of it.
#[derive(Debug, Clone)]
pub struct Field {
    pub key: Key,
    pub text: String,
    /// Where the next character lands: a character index into `text`, in
    /// `0..=text.chars().count()`. It rests at the end of the value the file
    /// held, because appending to a value nobody typed is the one edit every
    /// row starts able to make.
    at: usize,
    /// What a save makes of this field. An untouched field is written as
    /// nothing — a secret the file holds is never handed back, so writing one
    /// empty would erase it — and an empty field is two things rather than
    /// one: the value it does not hold, and the absence `Ctrl-U` makes of it.
    pub edit: Edit,
}

/// What one save does with one field, so two flags cannot disagree about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// No keystroke has been in this field: the file keeps what it had.
    Untouched,
    /// The field's text is the value, the empty one included — which for the
    /// writable paths is the explicit `[]`, and not the floor.
    Value,
    /// `Ctrl-U`: the key leaves the file, so the environment or the run's
    /// default speaks instead — the shipped floor, for the writable paths.
    Unset,
}

impl Field {
    /// A field holding `text`, its cursor at the end of it.
    pub fn new(key: Key, text: String) -> Field {
        let at = text.chars().count();
        Field {
            key,
            text,
            at,
            edit: Edit::Untouched,
        }
    }

    /// Where the next character lands, as an index into the value's characters.
    pub fn cursor(&self) -> usize {
        self.at
    }

    /// One character at the cursor, which moves past it.
    pub fn insert(&mut self, ch: char) {
        let at = self.offset();
        self.text.insert(at, ch);
        self.at += 1;
    }

    /// Text at the cursor, which moves past it: a paste is one edit of the
    /// field and never a key. On a row of lines the text brings its own breaks,
    /// so they land where the cursor is and the list stays the list.
    pub fn insert_str(&mut self, text: &str) {
        let at = self.offset();
        self.text.insert_str(at, text);
        self.at += text.chars().count();
    }

    /// The character before the cursor, removed: the cursor is left on the cell
    /// that character stood on, which is where a reader's next one lands. The
    /// composer's word for the same key, because the two are the same edit.
    pub fn backspace(&mut self) -> bool {
        let Some(before) = self.at.checked_sub(1) else {
            return false;
        };
        let at = self.place(before);
        self.text.remove(at);
        self.at = before;
        true
    }

    /// The character the cursor stands on, removed: the value closes over it,
    /// so the cursor stays where it is and now holds what followed.
    pub fn delete(&mut self) -> bool {
        let at = self.offset();
        match at < self.text.len() {
            true => {
                self.text.remove(at);
                true
            }
            false => false,
        }
    }

    /// The cursor one character towards the start of the value.
    pub fn left(&mut self) {
        self.at = self.at.saturating_sub(1);
    }

    /// The cursor one character towards its end.
    pub fn right(&mut self) {
        self.at = (self.at + 1).min(self.chars());
    }

    /// The cursor to the start of the value, which is the start of the row.
    pub fn home(&mut self) {
        self.at = 0;
    }

    /// The cursor past the last character of the value.
    pub fn end(&mut self) {
        self.at = self.chars();
    }

    /// The value replaced whole — a name a list took, a toggle flipped — with
    /// the cursor at its end, because the reader's next keystroke appends to
    /// what a key put there rather than landing inside it.
    pub fn set(&mut self, text: String) {
        self.at = text.chars().count();
        self.text = text;
    }

    /// The value emptied, the cursor with it.
    pub fn clear(&mut self) {
        self.text.clear();
        self.at = 0;
    }

    /// Whether the cursor stands at the end of the value: what tells a paste
    /// that ends the value from one dropped inside it.
    pub fn at_end(&self) -> bool {
        self.at >= self.chars()
    }

    /// The line the cursor is in: what the fold's mark stands on, what a key
    /// typed into a line is a key of that line, and where a line's own check
    /// hangs — one line's sentence is the sentence for the line the reader is
    /// in.
    pub fn line(&self) -> usize {
        let at = self.offset();
        self.spans()
            .iter()
            .position(|(_, end)| at <= *end)
            .unwrap_or_default()
    }

    /// How many lines the value holds: one per break in it, so a value holding
    /// nothing is the one blank line a header is typed on, and a break at the
    /// end of the value is the blank line it ends with.
    pub fn line_count(&self) -> usize {
        self.spans().len()
    }

    /// A line of the value as it is written, which is the line a check is run on
    /// and the line a reader is typing. A line past the last of them is no text.
    pub fn line_text(&self, line: usize) -> &str {
        let Some((start, end)) = self.spans().get(line).copied() else {
            return "";
        };
        &self.text[start..end]
    }

    /// The cursor to the end of a line: a line taken for writing is typed on
    /// from its end, exactly as a value nobody has typed is.
    pub fn onto(&mut self, line: usize) {
        let Some((_, end)) = self.spans().get(line).copied() else {
            return;
        };
        self.at = self.text[..end].chars().count();
    }

    /// A blank line at the end of the value, for a header not yet written, and
    /// its number. A table already ending in a blank line stands on that one
    /// rather than stacking them: one silent line to type on is enough.
    pub fn add_line(&mut self) -> usize {
        let last = self.line_count() - 1;
        if self.line_text(last).is_empty() {
            self.onto(last);
            return last;
        }
        self.text.push('\n');
        self.onto(last + 1);
        last + 1
    }

    /// A line removed whole, the break that carries it with it, so the lines
    /// either side keep the characters they had — and the cursor onto the line
    /// that took its place.
    pub fn remove_line(&mut self, line: usize) {
        let Some((start, end)) = self.spans().get(line).copied() else {
            return;
        };
        let (from, to) = match self.text[end..].starts_with('\n') {
            true => (start, end + 1),
            false => (start.saturating_sub(1), end),
        };
        self.text.replace_range(from..to, "");
        self.onto(line.min(self.line_count() - 1));
    }

    /// A line's characters taken and its break kept, so the line is blank rather
    /// than gone: what `Ctrl-U` makes of one line of a table.
    pub fn empty_line(&mut self, line: usize) {
        let Some((start, end)) = self.spans().get(line).copied() else {
            return;
        };
        self.text.replace_range(start..end, "");
        self.at = self.text[..start].chars().count();
    }

    /// The byte span of each line of the value — where its characters start and
    /// where they end — the break after a line belonging to neither.
    fn spans(&self) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut start = 0;
        for line in self.text.split('\n') {
            let end = start + line.len();
            spans.push((start, end));
            start = end + 1;
        }
        spans
    }

    /// How many characters the value holds.
    fn chars(&self) -> usize {
        self.text.chars().count()
    }

    /// The byte index the cursor stands at: a character index read as a place
    /// in a `String`, which is the only way a value of any alphabet is edited
    /// without splitting a glyph.
    fn offset(&self) -> usize {
        self.place(self.at)
    }

    /// The byte index of a character index, and the value's end for one past
    /// its last character — the cursor may sit there, and half a glyph is never
    /// a place to stand.
    fn place(&self, at: usize) -> usize {
        self.text
            .char_indices()
            .nth(at)
            .map_or(self.text.len(), |(byte, _)| byte)
    }
}

/// What a paste is on a row rather than in the composer: one line. A row of
/// the modal is a single value — the address a wire is asked at, the credential
/// it is asked with — so the line breaks a clipboard carries are dropped, and a
/// buffer copied with its trailing newline is the address and nothing else.
pub(crate) fn one_line(text: &str) -> String {
    text.chars()
        .filter(|ch| *ch != '\n' && *ch != '\r')
        .collect()
}

/// What a paste is on a row that holds a list of lines: the breaks kept, since
/// one line is one entry there, and the trailing whitespace a copied block
/// carries dropped, so a buffer ending in a line ending is its lines and
/// nothing else. `\r` goes with it: a row and a file carry one line ending.
///
/// A block dropped before the end of the value keeps that last ending, because
/// there it is the break between the block's final line and the line it was
/// dropped in front of — a paste that runs two headers together is not the table
/// the clipboard held. `at_end` is the cursor's, and only a paste that ends the
/// value spends the trim.
pub(crate) fn whole(text: &str, at_end: bool) -> String {
    let kept = match at_end {
        true => text.trim_end(),
        false => text,
    };
    kept.replace('\r', "")
}
