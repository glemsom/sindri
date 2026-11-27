use sindri_core::config::Touched;

use crate::pen;

pub const MIN_ROWS: u16 = 3;

pub const MAX_ROWS: u16 = DRAFT_ROWS + 2 * (AIR_ROWS + 1);

pub const AIR_ROWS: u16 = 1;

pub const DRAFT_ROWS: u16 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ends {
    pub above: u16,
    pub below: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Idle,
    Running,
}

/// A key decoded from the bytes a terminal sends, so `Ctrl-J` and `Shift+Enter` are one key
/// here, and a glyph is one press however many bytes it took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Newline,
    Backspace,
    Left,
    Right,
    Up,
    Down,
    ShiftUp,
    ShiftDown,
    ShiftLeft,
    ShiftRight,
    Home,
    End,
    PageUp,
    PageDown,
    Copy,
    Fold,
    Forward,
    Quit,
    Cancel,
    Drop,
    Clear,
    Tab,
    BackTab,
    Escape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    text: String,
}

impl Prompt {
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Unchanged,
    Edited,
    Send(Prompt),
    Quit,
    Cancel,
    Copy(String),
    Model(String),
    Served,
    Save(Touched),
    Settings,
}

pub const PLACEHOLDER: &str = "what should the run do?";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Row {
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Composer {
    text: String,
    cursor: usize,
    scroll: usize,
    state: State,
    room: Option<u16>,
}

impl Composer {
    pub fn new() -> Composer {
        Composer::default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn set_state(&mut self, state: State) {
        self.state = state;
    }

    pub fn set_room(&mut self, room: u16) {
        self.room = Some(room);
    }

    pub fn rows(&self) -> u16 {
        self.laid().1
    }

    pub fn ends(&self) -> Ends {
        Composer::ends_at(self.drawn().len())
    }

    fn ends_at(drawn: usize) -> Ends {
        match drawn > 1 {
            true => Ends {
                above: 1 + AIR_ROWS,
                below: 1 + AIR_ROWS,
            },
            false => Ends { above: 1, below: 1 },
        }
    }

    pub fn visible_rows(&self) -> u16 {
        Composer::visible(self.drawn().len())
    }

    pub fn kept_back(&self) -> Option<u16> {
        let drawn = u16::try_from(self.drawn().len()).unwrap_or(u16::MAX);
        let visible = Composer::visible(usize::from(drawn));
        (drawn > visible).then(|| drawn - visible)
    }

    fn visible(drawn: usize) -> u16 {
        let room = u16::try_from(drawn).unwrap_or(u16::MAX).min(DRAFT_ROWS);
        match usize::from(room) < drawn {
            true => room.saturating_sub(1),
            false => room,
        }
    }

    pub fn scroll(&self) -> usize {
        self.laid().2
    }

    pub fn lines(&self) -> Vec<&str> {
        let (drawn, _, first) = self.laid();
        let visible = usize::from(Composer::visible(drawn.len()));
        drawn
            .iter()
            .skip(first)
            .take(visible)
            .map(|row| &self.text[row.start..row.end])
            .collect()
    }

    pub fn caret(&self) -> (u16, u16) {
        let (drawn, _, first) = self.laid();
        let row = self.row_at(&drawn, self.cursor);
        let column = pen::cells(&self.text[drawn[row].start..self.cursor]);
        (
            u16::try_from(row.saturating_sub(first)).unwrap_or(u16::MAX),
            column,
        )
    }

    pub fn on_first_line(&self) -> bool {
        self.row_at(&self.drawn(), self.cursor) == 0
    }

    fn laid(&self) -> (Vec<Row>, u16, usize) {
        let drawn = self.drawn();
        let ends = Composer::ends_at(drawn.len());
        let rows = u16::try_from(drawn.len())
            .unwrap_or(u16::MAX)
            .saturating_add(ends.above)
            .saturating_add(ends.below)
            .clamp(MIN_ROWS, MAX_ROWS);
        let visible = usize::from(Composer::visible(drawn.len()));
        let first = self.scroll.min(drawn.len().saturating_sub(visible));
        (drawn, rows, first)
    }

    fn drawn(&self) -> Vec<Row> {
        let room = usize::from(self.room.unwrap_or(u16::MAX).max(1));
        let mut drawn: Vec<Row> = Vec::new();
        for (start, line_end) in self.hard_lines() {
            let mut at = start;
            loop {
                let end = self.row_end(at, line_end, room);
                drawn.push(Row { start: at, end });
                at = end;
                if at >= line_end {
                    break;
                }
            }
        }
        drawn
    }

    fn hard_lines(&self) -> Vec<(usize, usize)> {
        let mut lines = Vec::new();
        let mut start = 0;
        for (at, glyph) in self.text.char_indices() {
            if glyph == '\n' {
                lines.push((start, at));
                start = at + 1;
            }
        }
        lines.push((start, self.text.len()));
        lines
    }

    fn row_end(&self, at: usize, line_end: usize, room: usize) -> usize {
        let mut width = 0;
        let mut blank = None;
        let mut stop = at;
        for (offset, glyph) in self.text[at..line_end].char_indices() {
            let wide = usize::from(pen::cell(glyph));
            if width + wide > room {
                break;
            }
            width += wide;
            stop = at + offset + glyph.len_utf8();
            if glyph == ' ' {
                blank = Some(stop);
            }
        }
        match blank {
            Some(blank) if stop < line_end => blank,
            _ if stop > at => stop,
            // A glyph wider than the whole room is taken anyway, or no row would ever advance.
            _ => {
                at + self.text[at..line_end]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8)
            }
        }
    }

    fn row_at(&self, drawn: &[Row], at: usize) -> usize {
        drawn
            .iter()
            .position(|row| at <= row.end && row.start <= at)
            .unwrap_or_else(|| drawn.len().saturating_sub(1))
    }

    pub fn at_word_start(&self) -> bool {
        self.text[..self.cursor]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
    }

    /// A pasted prompt in one edit: a terminal puts `\r\n` or a bare `\r` on the wire for the
    /// break a clipboard calls `\n`, so both become the `\n` the draft is measured in — and
    /// neither of them is the `Enter` that sends.
    pub fn paste(&mut self, text: &str) {
        let kept = text.replace("\r\n", "\n").replace('\r', "\n");
        self.insert(&kept);
    }

    pub fn recall(&mut self, prompt: Prompt) {
        self.insert(&prompt.text);
    }

    pub fn key(&mut self, key: Key) -> Outcome {
        match key {
            Key::Char(glyph) if glyph.is_control() => Outcome::Unchanged,
            Key::Char(glyph) => {
                let mut bytes = [0u8; 4];
                self.insert(glyph.encode_utf8(&mut bytes));
                Outcome::Edited
            }
            Key::Newline => {
                self.insert("\n");
                Outcome::Edited
            }
            Key::Enter => self.send(),
            Key::Backspace => Composer::edited(self.backspace()),
            Key::Left => Composer::edited(self.left()),
            Key::Right => Composer::edited(self.right()),
            Key::Up => Composer::edited(self.up()),
            Key::Down => Composer::edited(self.down()),
            Key::Home => Composer::edited(self.home()),
            Key::End => Composer::edited(self.end()),
            // The mark's three keys and the transcript's three are the frame's: a draft has no mark
            // to change and no rows to move.
            Key::ShiftUp
            | Key::ShiftDown
            | Key::ShiftLeft
            | Key::ShiftRight
            | Key::Copy
            | Key::Fold
            | Key::PageUp
            | Key::PageDown
            | Key::Forward => Outcome::Unchanged,
            Key::Quit => Outcome::Quit,
            Key::Cancel => self.cancel(),
            // `Ctrl-X` is the queue's, `Ctrl-U` the transcript's, and `Tab` and `Shift+Tab` the
            // settings screen's: the frame answers them before the draft ever sees them.
            Key::Drop | Key::Escape | Key::Clear | Key::Tab | Key::BackTab => Outcome::Unchanged,
        }
    }

    fn send(&mut self) -> Outcome {
        if self.text.trim().is_empty() {
            return Outcome::Unchanged;
        }
        let text = std::mem::take(&mut self.text);
        self.cursor = 0;
        self.scroll = 0;
        Outcome::Send(Prompt { text })
    }

    fn cancel(&mut self) -> Outcome {
        match self.state {
            State::Running => Outcome::Cancel,
            State::Idle => {
                let typed = !self.text.is_empty();
                self.text.clear();
                self.cursor = 0;
                self.scroll = 0;
                Composer::edited(typed)
            }
        }
    }

    fn edited(changed: bool) -> Outcome {
        if changed {
            Outcome::Edited
        } else {
            Outcome::Unchanged
        }
    }

    pub fn rewrite(&mut self, text: &str) {
        self.replace(text, text.len());
    }

    pub fn replace(&mut self, text: &str, caret: usize) {
        self.text.clear();
        self.text.push_str(text);
        self.cursor = caret.min(self.text.len());
        self.follow();
    }

    pub fn insert(&mut self, text: &str) {
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.follow();
    }

    fn backspace(&mut self) -> bool {
        let before = self.text[..self.cursor].chars().next_back();
        match before {
            Some(glyph) => {
                self.cursor -= glyph.len_utf8();
                self.text.remove(self.cursor);
                self.follow();
                true
            }
            None => false,
        }
    }

    fn left(&mut self) -> bool {
        let before = self.text[..self.cursor].chars().next_back();
        match before {
            Some(glyph) => {
                self.cursor -= glyph.len_utf8();
                true
            }
            None => false,
        }
    }

    fn right(&mut self) -> bool {
        let after = self.text[self.cursor..].chars().next();
        match after {
            Some(glyph) => {
                self.cursor += glyph.len_utf8();
                true
            }
            None => false,
        }
    }

    fn up(&mut self) -> bool {
        let drawn = self.drawn();
        let row = self.row_at(&drawn, self.cursor);
        if row == 0 {
            return false;
        }
        let column = self.column(drawn[row]);
        self.cursor = self.at_column(drawn[row - 1], column);
        self.follow();
        true
    }

    fn down(&mut self) -> bool {
        let drawn = self.drawn();
        let row = self.row_at(&drawn, self.cursor);
        if row + 1 >= drawn.len() {
            return false;
        }
        let column = self.column(drawn[row]);
        self.cursor = self.at_column(drawn[row + 1], column);
        self.follow();
        true
    }

    fn home(&mut self) -> bool {
        let drawn = self.drawn();
        let row = self.row_at(&drawn, self.cursor);
        let start = drawn[row].start;
        let changed = start != self.cursor;
        self.cursor = start;
        changed
    }

    fn end(&mut self) -> bool {
        let drawn = self.drawn();
        let row = self.row_at(&drawn, self.cursor);
        let end = drawn[row].end;
        let changed = end != self.cursor;
        self.cursor = end;
        self.follow();
        changed
    }

    fn follow(&mut self) {
        let drawn = self.drawn();
        let row = self.row_at(&drawn, self.cursor);
        let visible = usize::from(Composer::visible(drawn.len()));
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll + visible {
            self.scroll = row + 1 - visible;
        }
    }

    fn column(&self, row: Row) -> usize {
        usize::from(pen::cells(&self.text[row.start..self.cursor]))
    }

    fn at_column(&self, row: Row, column: usize) -> usize {
        let mut at = row.start;
        let mut reached = 0;
        while reached < column {
            let Some(glyph) = self.text[at..row.end].chars().next() else {
                break;
            };
            let width = usize::from(pen::cell(glyph));
            if reached + width > column {
                break;
            }
            reached += width;
            at += glyph.len_utf8();
        }
        at
    }
}

#[cfg(test)]
mod tests;
