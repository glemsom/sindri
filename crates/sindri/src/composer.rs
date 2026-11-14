use sindri_core::config::Touched;

use crate::pen;

pub const MIN_ROWS: u16 = 2;

pub const MAX_ROWS: u16 = 6;

/// The band's top row is its edge and not draft, so a band of `MIN_ROWS` paints one row.
pub const HAIRLINE_ROWS: u16 = 1;

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
    Home,
    End,
    PageUp,
    PageDown,
    Copy,
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

/// What the band says an empty draft can do there, in the dim ink: it leaves on the first glyph a
/// reader types. It names no key, because the footer is where a key is named.
pub const PLACEHOLDER: &str = "what should the run do?";

/// One row of the draft as it is drawn: the bytes of it the reader's words cover.
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
    /// The columns the frame gives the draft's words, and `None` until a frame has measured it.
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

    /// The run loop sets this when a run starts and when it stops, and nothing else in the
    /// frame may.
    pub fn set_state(&mut self, state: State) {
        self.state = state;
    }

    /// The columns the frame gives the draft's words: the band's width less the gutter its sigil
    /// stands in. The frame hands it over before the band is placed, so a draft wraps at the width
    /// it is drawn at and at no other.
    pub fn set_room(&mut self, room: u16) {
        self.room = Some(room);
    }

    pub fn rows(&self) -> u16 {
        self.laid().1
    }

    pub fn visible_rows(&self) -> u16 {
        self.rows().saturating_sub(HAIRLINE_ROWS)
    }

    /// The first row the band draws: the scroll the caret gave the draft, and never past the last
    /// row the band has room for.
    pub fn scroll(&self) -> usize {
        self.laid().2
    }

    pub fn lines(&self) -> Vec<&str> {
        let (drawn, rows, first) = self.laid();
        let visible = usize::from(rows.saturating_sub(HAIRLINE_ROWS));
        drawn
            .iter()
            .skip(first)
            .take(visible)
            .map(|row| &self.text[row.start..row.end])
            .collect()
    }

    /// The row of the band the caret waits in, and its column in cells.
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

    /// The draft as the band draws it: the rows its words wrap into, the rows the band takes for
    /// them with its one edge row counted, and the first of them the scroll reaches.
    fn laid(&self) -> (Vec<Row>, u16, usize) {
        let drawn = self.drawn();
        let rows = u16::try_from(drawn.len())
            .unwrap_or(u16::MAX)
            .saturating_add(HAIRLINE_ROWS)
            .clamp(MIN_ROWS, MAX_ROWS);
        let visible = usize::from(rows.saturating_sub(HAIRLINE_ROWS));
        let first = self.scroll.min(drawn.len().saturating_sub(visible));
        (drawn, rows, first)
    }

    /// The rows the draft is drawn as, top to bottom: a row ends at the last cell that does not cut
    /// a glyph, and after the last blank before it, so no word is broken. Unlike `rows::wrap_under`,
    /// a blank the reader typed is neither trimmed nor closed.
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

    /// The breaks the reader typed, and never the wraps the room gave them.
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

    /// The row the offset `at` waits in: the first row that reaches it, so a caret at a row's last
    /// cell is drawn there and not at the first cell of the row below.
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
            // The mark's three keys and the transcript's three are the frame's: a box has no mark to
            // change and no rows to move.
            Key::ShiftUp
            | Key::ShiftDown
            | Key::Copy
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
        let (drawn, rows, _) = self.laid();
        let row = self.row_at(&drawn, self.cursor);
        let visible = usize::from(rows.saturating_sub(HAIRLINE_ROWS));
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll + visible {
            self.scroll = row + 1 - visible;
        }
    }

    /// The column the cursor sits in, counted in cells from the row's first one: a wide glyph moves
    /// it two.
    fn column(&self, row: Row) -> usize {
        usize::from(pen::cells(&self.text[row.start..self.cursor]))
    }

    /// The offset in `row` nearest to `column` without passing it: a wide glyph the column lands
    /// inside is not taken, so the cursor keeps the side of it it came from.
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
mod tests {
    use super::*;

    fn holding(text: &str) -> Composer {
        let mut composer = Composer::new();
        composer.paste(text);
        composer
    }

    fn numbered(lines: usize) -> Composer {
        let draft: Vec<String> = (1..=lines).map(|line| line.to_string()).collect();
        holding(&draft.join("\n"))
    }

    fn sent(outcome: Outcome) -> String {
        match outcome {
            Outcome::Send(prompt) => prompt.text().to_owned(),
            other => panic!("the draft should have been sent, and {other:?} came instead"),
        }
    }

    #[test]
    fn typing_extends_the_draft_at_the_cursor() {
        let mut composer = Composer::new();
        for glyph in "hello".chars() {
            assert_eq!(composer.key(Key::Char(glyph)), Outcome::Edited);
        }

        assert_eq!(composer.text(), "hello");
        assert_eq!(composer.cursor(), 5);
    }

    #[test]
    fn moving_between_the_lines_of_a_draft_keeps_the_caret_in_the_cells_it_was_in() {
        let mut composer = holding("日x\nab");
        assert_eq!(
            composer.caret(),
            (1, 2),
            "the second line is two cells wide"
        );

        composer.key(Key::Up);

        assert_eq!(
            composer.caret(),
            (0, 2),
            "the caret comes back to the cell it left from, and never past the last glyph of \
             the line it lands in"
        );
    }

    #[test]
    fn typing_between_two_glyphs_leaves_them_apart() {
        let mut composer = holding("ac");
        composer.key(Key::Left);
        composer.key(Key::Char('b'));

        assert_eq!(composer.text(), "abc");
    }

    #[test]
    fn enter_sends_the_draft_and_leaves_the_box_empty() {
        let mut composer = holding("why did the retry test fail");

        assert_eq!(
            sent(composer.key(Key::Enter)),
            "why did the retry test fail",
            "the draft left the box as a prompt"
        );
        assert_eq!(composer.text(), "", "and the box is empty again");
        assert_eq!(
            composer.rows(),
            MIN_ROWS,
            "so the band is back at its floor"
        );
    }

    #[test]
    fn a_box_with_nothing_in_it_has_nothing_to_send() {
        let mut composer = Composer::new();
        assert_eq!(composer.key(Key::Enter), Outcome::Unchanged);

        let mut spaces = holding("   ");
        assert_eq!(
            spaces.key(Key::Enter),
            Outcome::Unchanged,
            "spaces are not a prompt"
        );
        assert_eq!(spaces.text(), "   ", "and the box keeps them");
    }

    #[test]
    fn shift_enter_and_ctrl_j_break_the_line_rather_than_sending() {
        let mut composer = holding("first");

        assert_eq!(composer.key(Key::Newline), Outcome::Edited);
        composer.key(Key::Char('s'));
        assert_eq!(composer.key(Key::Newline), Outcome::Edited);
        composer.key(Key::Char('t'));

        assert_eq!(composer.text(), "first\ns\nt");
        assert_eq!(composer.cursor(), 9);
    }

    #[test]
    fn backspace_removes_the_glyph_before_the_cursor() {
        let mut composer = holding("ab");

        assert_eq!(composer.key(Key::Backspace), Outcome::Edited);
        assert_eq!(composer.text(), "a");
        assert_eq!(composer.key(Key::Backspace), Outcome::Edited);
        assert_eq!(composer.text(), "");
        assert_eq!(
            composer.key(Key::Backspace),
            Outcome::Unchanged,
            "an empty box has nothing left to take back"
        );
    }

    #[test]
    fn backspace_eats_a_whole_glyph_however_many_bytes_it_took() {
        let mut composer = holding("✓ ok");

        for _ in 0..3 {
            composer.key(Key::Backspace);
        }
        assert_eq!(
            composer.text(),
            "✓",
            "the tick is still there, not taken half at a time"
        );

        composer.key(Key::Backspace);
        assert_eq!(composer.text(), "", "and one more takes it whole");
        assert_eq!(composer.cursor(), 0);
    }

    #[test]
    fn an_empty_draft_takes_the_two_row_floor() {
        assert_eq!(Composer::new().rows(), 2);
        assert_eq!(Composer::new().visible_rows(), 1);
        assert_eq!(
            holding("one line").rows(),
            2,
            "one line still takes the hairline and one row"
        );
    }

    #[test]
    fn a_line_per_row_grows_the_box_to_its_ceiling_and_no_further() {
        for lines in 1..MAX_ROWS {
            assert_eq!(
                numbered(usize::from(lines)).rows(),
                lines + HAIRLINE_ROWS,
                "{lines} lines take the hairline and one row each"
            );
        }
        assert_eq!(numbered(5).rows(), MAX_ROWS);
        assert_eq!(numbered(6).rows(), MAX_ROWS, "the sixth line scrolls");
        assert_eq!(numbered(9).rows(), MAX_ROWS);
    }

    #[test]
    fn a_draft_past_the_ceiling_scrolls_instead_of_growing() {
        let composer = numbered(9);

        assert_eq!(
            composer.scroll(),
            4,
            "nine lines, five rows: the window starts at the fifth"
        );
        assert_eq!(composer.lines(), vec!["5", "6", "7", "8", "9"]);
        assert_eq!(
            composer.caret(),
            (4, 1),
            "the cursor is on the last painted row"
        );
    }

    #[test]
    fn the_window_follows_the_cursor_back_up_the_draft() {
        let mut composer = numbered(9);

        for _ in 0..4 {
            assert_eq!(composer.key(Key::Up), Outcome::Edited);
        }
        assert_eq!(
            composer.caret(),
            (0, 1),
            "four lines up is the window's first row"
        );
        assert_eq!(composer.scroll(), 4, "and nothing has scrolled yet");

        composer.key(Key::Up);
        assert_eq!(
            composer.scroll(),
            3,
            "one more line up, and the window follows"
        );
        assert_eq!(composer.caret(), (0, 1));
    }

    #[test]
    fn the_first_line_is_where_the_queue_may_take_the_up_arrow() {
        let mut composer = holding("one\ntwo");

        assert!(!composer.on_first_line());
        composer.key(Key::Up);
        assert!(composer.on_first_line());
        assert_eq!(
            composer.key(Key::Up),
            Outcome::Unchanged,
            "there is no line above the first to move to"
        );
    }

    #[test]
    fn a_recalled_prompt_is_the_draft_again_line_for_line() {
        let mut composer = Composer::new();
        composer.recall(Prompt {
            text: String::from("the first line\nthe second"),
        });

        assert_eq!(
            composer.text(),
            "the first line\nthe second",
            "the prompt came back whole, its line break included"
        );
        assert_eq!(composer.caret(), (1, 10), "and the caret follows it");
    }

    #[test]
    fn a_prompt_recalled_into_a_draft_leaves_what_was_typed_there() {
        let mut composer = holding("fix: ");
        composer.recall(Prompt {
            text: String::from("the retry"),
        });

        assert_eq!(
            composer.text(),
            "fix: the retry",
            "a recall puts the prompt where the caret was and takes nothing away"
        );
    }

    #[test]
    fn the_cursor_changes_line_at_the_column_the_line_allows() {
        let mut composer = holding("long line\nx");

        composer.key(Key::Up);
        assert_eq!(
            composer.cursor(),
            1,
            "one glyph in, which is the column it came from"
        );
        composer.key(Key::End);
        assert_eq!(composer.cursor(), 9);
        composer.key(Key::Down);
        assert_eq!(
            composer.cursor(),
            11,
            "the second line has no column nine, so the caret stops at its end"
        );
        composer.key(Key::Up);
        assert_eq!(
            composer.cursor(),
            1,
            "and comes back up from the column the short line left the caret in"
        );
        composer.key(Key::Home);
        assert_eq!(
            composer.cursor(),
            0,
            "column zero is the first line's start"
        );
    }

    #[test]
    fn a_draft_wider_than_the_room_wraps_where_its_words_end() {
        let mut composer = Composer::new();
        composer.set_room(10);
        composer.paste("one two three four");

        assert_eq!(
            composer.lines(),
            vec!["one two ", "three four"],
            "ten columns hold neither `three` nor the blank before it on the first row, so the \
             draft wraps on the blank rather than inside a word"
        );
        assert_eq!(
            composer.rows(),
            3,
            "two rows of draft and the band's one row of edge"
        );
    }

    #[test]
    fn the_caret_wraps_onto_the_row_below_it_when_the_room_runs_out() {
        let mut composer = Composer::new();
        composer.set_room(10);
        composer.paste("one two three four");

        assert_eq!(
            composer.caret(),
            (1, 10),
            "the draft's last glyph is ten cells into the row the wrap gave it"
        );
        for _ in 0..3 {
            composer.key(Key::Left);
        }
        assert_eq!(
            composer.caret(),
            (1, 7),
            "and the caret walks back along that row"
        );
        composer.key(Key::Home);
        composer.key(Key::Up);
        composer.key(Key::End);
        assert_eq!(
            composer.caret(),
            (0, 8),
            "a row of the draft ends where its words do, and not where the draft does"
        );
    }

    #[test]
    fn an_arrow_moves_between_the_rows_the_wrap_gave_the_draft() {
        let mut composer = Composer::new();
        composer.set_room(10);
        composer.paste("one two three four");

        assert_eq!(composer.key(Key::Up), Outcome::Edited);
        assert_eq!(
            composer.caret(),
            (0, 8),
            "the row above holds eight cells, so the caret stops at its end rather than \
             leaving the draft"
        );
        assert_eq!(
            composer.key(Key::Down),
            Outcome::Edited,
            "and comes back down to the row the wrap gave the draft"
        );
        assert_eq!(
            composer.caret(),
            (1, 8),
            "in the column it left from, not the one the long row ended in"
        );
    }

    #[test]
    fn a_word_wider_than_the_room_is_broken_where_a_glyph_ends() {
        let mut composer = Composer::new();
        composer.set_room(5);
        composer.paste("日本語です");

        assert_eq!(
            composer.lines(),
            vec!["日本", "語で", "す"],
            "two wide glyphs fill five columns and the fifth is left blank, so no glyph is drawn \
             half on one row and half on the next"
        );
        assert_eq!(composer.rows(), 4);
    }

    #[test]
    fn a_draft_taller_than_the_band_scrolls_by_the_rows_the_wrap_gave_it() {
        let mut composer = Composer::new();
        composer.set_room(4);
        composer.paste("abcdefghijklmnopqrstuvwxyz");

        assert_eq!(
            composer.rows(),
            MAX_ROWS,
            "seven rows of draft are drawn in the five the band has room for"
        );
        assert_eq!(
            composer.scroll(),
            2,
            "so the band shows the rows from the third on"
        );
        assert_eq!(
            composer.lines(),
            vec!["ijkl", "mnop", "qrst", "uvwx", "yz"],
            "which are the rows the caret's own is the last of"
        );
    }

    #[test]
    fn a_pasted_prompt_is_one_draft_with_its_line_breaks_kept() {
        let mut composer = holding("fix this: ");

        composer.paste("the first line\r\nthe second\rand the third\n");

        assert_eq!(
            composer.text(),
            "fix this: the first line\nthe second\nand the third\n",
            "every break the clipboard carried is still a break"
        );
        assert_eq!(
            composer.lines(),
            vec![
                "fix this: the first line",
                "the second",
                "and the third",
                ""
            ],
            "one draft of four lines, not four prompts"
        );
        assert_eq!(composer.rows(), 5);
    }

    #[test]
    fn a_pasted_prompt_goes_in_at_the_cursor() {
        let mut composer = holding("two");
        composer.key(Key::Home);

        composer.paste("one\rthree\r");

        assert_eq!(composer.text(), "one\nthree\ntwo");
    }

    #[test]
    fn ctrl_c_during_a_run_keeps_the_draft() {
        let mut composer = holding("half a prompt");
        composer.set_state(State::Running);

        assert_eq!(composer.key(Key::Cancel), Outcome::Cancel);
        assert_eq!(
            composer.text(),
            "half a prompt",
            "the draft is still the reader's"
        );
        assert_eq!(composer.cursor(), 13);
        assert_eq!(
            composer.state(),
            State::Running,
            "and the run is still the run"
        );
    }

    #[test]
    fn ctrl_c_when_idle_clears_the_draft() {
        let mut composer = holding("half a prompt");

        assert_eq!(composer.key(Key::Cancel), Outcome::Edited);
        assert_eq!(composer.text(), "");
        assert_eq!(composer.cursor(), 0);
        assert_eq!(composer.rows(), MIN_ROWS);
        assert_eq!(
            composer.key(Key::Cancel),
            Outcome::Unchanged,
            "an empty box has nothing left to clear"
        );
    }

    #[test]
    fn ctrl_q_asks_for_the_launch_to_end_and_leaves_the_draft_alone() {
        let mut composer = holding("a draft nobody sent");

        assert_eq!(composer.key(Key::Quit), Outcome::Quit);
        assert_eq!(composer.text(), "a draft nobody sent");
    }
}
