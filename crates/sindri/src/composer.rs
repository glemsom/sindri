use sindri_core::config::Touched;

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
    Copy,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Composer {
    text: String,
    cursor: usize,
    scroll: usize,
    state: State,
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

    pub fn rows(&self) -> u16 {
        let lines = u16::try_from(self.line_count()).unwrap_or(u16::MAX);
        lines
            .saturating_add(HAIRLINE_ROWS)
            .clamp(MIN_ROWS, MAX_ROWS)
    }

    pub fn visible_rows(&self) -> u16 {
        self.rows().saturating_sub(HAIRLINE_ROWS)
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn lines(&self) -> Vec<&str> {
        let visible = usize::from(self.visible_rows());
        self.text
            .split('\n')
            .skip(self.scroll)
            .take(visible)
            .collect()
    }

    pub fn caret(&self) -> (u16, u16) {
        let row = self.row(self.cursor);
        let column = self.text[self.line_start(row)..self.cursor].chars().count();
        (
            u16::try_from(row.saturating_sub(self.scroll)).unwrap_or(u16::MAX),
            u16::try_from(column).unwrap_or(u16::MAX),
        )
    }

    pub fn on_first_line(&self) -> bool {
        self.row(self.cursor) == 0
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
            // The mark's three keys are the frame's: a box has no mark to change.
            Key::ShiftUp | Key::ShiftDown | Key::Copy => Outcome::Unchanged,
            Key::Quit => Outcome::Quit,
            Key::Cancel => self.cancel(),
            // `Ctrl-X` is the queue's and `Ctrl-U`, `Tab` and `Shift+Tab` the settings
            // screen's: the frame answers them before the draft ever sees them.
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
        let row = self.row(self.cursor);
        if row == 0 {
            return false;
        }
        self.cursor = self.at_column(row - 1, self.column(row));
        self.follow();
        true
    }

    fn down(&mut self) -> bool {
        let row = self.row(self.cursor);
        if row + 1 >= self.line_count() {
            return false;
        }
        self.cursor = self.at_column(row + 1, self.column(row));
        self.follow();
        true
    }

    fn home(&mut self) -> bool {
        let row = self.row(self.cursor);
        let start = self.line_start(row);
        let changed = start != self.cursor;
        self.cursor = start;
        changed
    }

    fn end(&mut self) -> bool {
        let row = self.row(self.cursor);
        let end = self.line_end(row);
        let changed = end != self.cursor;
        self.cursor = end;
        self.follow();
        changed
    }

    fn follow(&mut self) {
        let row = self.row(self.cursor);
        let visible = usize::from(self.visible_rows());
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll + visible {
            self.scroll = row + 1 - visible;
        }
    }

    fn line_count(&self) -> usize {
        self.text.matches('\n').count() + 1
    }

    fn row(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())]
            .matches('\n')
            .count()
    }

    fn line_start(&self, row: usize) -> usize {
        let mut start = 0;
        for _ in 0..row {
            match self.text[start..].find('\n') {
                Some(index) => start += index + 1,
                None => return self.text.len(),
            }
        }
        start
    }

    fn line_end(&self, row: usize) -> usize {
        match self.text[self.line_start(row)..].find('\n') {
            Some(index) => self.line_start(row) + index,
            None => self.text.len(),
        }
    }

    fn column(&self, row: usize) -> usize {
        self.text[self.line_start(row)..self.cursor].chars().count()
    }

    fn at_column(&self, row: usize, column: usize) -> usize {
        let end = self.line_end(row);
        let mut at = self.line_start(row);
        for _ in 0..column {
            match self.text[at..end].chars().next() {
                Some(glyph) => at += glyph.len_utf8(),
                None => break,
            }
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
