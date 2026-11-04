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
//! its line, not a second kind of row.

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
