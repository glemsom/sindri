//! The composer: the draft a prompt is typed into, as a value rather than a widget —
//! its text and its cursor, the key map that edits it, the two-to-six rows the box
//! takes and how far it scrolls when the draft outgrows them (§12).
//!
//! Nothing here knows what a terminal is (ADR-0005): the terminal hands in a key or a
//! paste and paints what the value holds, so every rule §12 names — the height, the
//! scroll offset, what `Ctrl-C` means — is a function of the draft and is proved by a
//! unit test rather than by an eyeball.
//!
//! `State` is the one fact `Ctrl-C`'s rule reads, and whoever owns the run sets it
//! (#43), so the composer decides what a cancel means and the terminal decides what
//! to do about it. `Outcome::Send` is the other seam: it carries the draft that just
//! left the box, and the terminal answers it — by starting the run it asks for, or, with
//! a run already in flight, by queueing it above the box (#42).
//!
//! A paste has no path to `Outcome::Send` at all: `Composer::paste` returns nothing a
//! run could take, so every line break a clipboard carried stays a line break — which
//! is the whole of "a pasted prompt arrives as one draft with its line breaks kept".

use sindri_core::config::Touched;

/// The fewest rows the composer's band takes: the hairline and one row of draft.
pub const MIN_ROWS: u16 = 2;

/// The most rows the band takes. A draft taller than this scrolls instead of growing,
/// because the transcript above it is the band a run grows into (§12).
pub const MAX_ROWS: u16 = 6;

/// The band's top row is its edge rather than draft — the hairline a draft is typed
/// under — so the rows a line of draft can be painted on are this many fewer.
pub const HAIRLINE_ROWS: u16 = 1;

/// Whether a run is live, which is the only fact `Ctrl-C`'s rule reads: a key that
/// moves nothing is not a change the frame has to repaint.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    /// No run: `Ctrl-C` has nothing to cancel, so it clears the draft.
    #[default]
    Idle,
    /// A run, streaming or asking for a tool: `Ctrl-C` cancels it and the draft stays.
    Running,
}

/// A key this frame's map answers, decoded from the bytes a terminal sends by whoever
/// owns the terminal: `Shift+Enter` is one of these and not four bytes (§17).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A glyph typed, counted as one keypress however many bytes it took.
    Char(char),
    /// `Enter`: the draft becomes the prompt a run takes.
    Enter,
    /// `Shift+Enter` or `Ctrl-J`: a line break inside the draft, never a send.
    Newline,
    /// Backspace: the glyph before the cursor goes.
    Backspace,
    /// The arrow keys, which move the cursor — `Up` on the first line being where the
    /// queue's recall sits, which is the queue's decision and not this map's (§12).
    Left,
    Right,
    Up,
    Down,
    /// `Shift+↑`/`Shift+↓`: the mark grows a whole row up the transcript, or down it
    /// (§12). The frame's, not the box's: a draft has no mark to grow.
    ShiftUp,
    ShiftDown,
    /// The keys that travel the cursor to either end of its line.
    Home,
    End,
    /// `Ctrl-Y`: the words the mark covers leave the terminal, or the newest answer does
    /// when nothing is marked (§12). A key of the frame's rather than the box's.
    Copy,
    /// `Ctrl-Q`: the launch ends and the terminal is given back.
    Quit,
    /// `Ctrl-C`: cancels a run and keeps the draft, or clears it when nothing runs.
    Cancel,
    /// `Ctrl-X`: drops the newest queued prompt. Not a key of this map's — the queue is
    /// the frame's and so is the rule (§12) — and listed here because this is the one
    /// vocabulary the keyboard is decoded into.
    Drop,
    /// `Ctrl-U`: empties the value a settings row holds, which unsets the key it names. Not
    /// a key of this map's either — a draft is cleared by `Ctrl-C` when nothing runs — and
    /// listed here for the same reason `Ctrl-X` is (§12).
    Clear,
    /// `Tab` and `Shift+Tab`: the row below and the row above at a screen whose rows are
    /// keys, and the end of the list wrapping round to its other end.
    Tab,
    BackTab,
    /// `Esc`: closes the screen over the draft. A draft has nothing to close, so the
    /// composer answers it by leaving the draft alone.
    Escape,
}

/// A prompt that has left the composer: what `Enter` makes of a draft, and the value
/// the run loop will take when #43 wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    /// The draft as it was typed, line breaks and all.
    text: String,
}

impl Prompt {
    /// The prompt the run is to answer.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// What a key did, in the words the terminal acts on: paint, send, quit, cancel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The key moved nothing — a key this map has no use for, or a backspace in an
    /// empty box — so there is nothing to repaint.
    Unchanged,
    /// The frame changed — the draft, or the screen over it — so what it holds is no
    /// longer what it did and the terminal has a frame to paint.
    Edited,
    /// `Enter`, with a draft worth sending in it: the prompt is the terminal's now.
    Send(Prompt),
    /// `Ctrl-Q`: the launch ends.
    Quit,
    /// `Ctrl-C` during a run: the run is the thing cancelled, and the draft stays.
    Cancel,
    /// `Ctrl-Y`: the words that leave the frame, for the terminal to write out by the
    /// sequence that carries them out of it — the core names no escape (§12, §14).
    Copy(String),
    /// A model name `Enter` or a `/model <name>` line chose: the terminal writes it into
    /// the config through the one owner-only save (ADR-0009), because a frame holds a
    /// value and the file is not one (§11, §12).
    Model(String),
    /// The `/model` screen is up and has no names yet: the terminal asks the wire what it
    /// serves and hands the listing back, because asking is a request and a frame makes
    /// none (§10, ADR-0005).
    Served,
    /// `Enter` at the settings modal: the keys a reader touched, for the terminal to write
    /// through the core's one owner-only save (ADR-0009). A frame opens no file.
    Save(Touched),
    /// `/settings`: the screen is the terminal's to open, because its rows are the config's
    /// and the frame holds no config — the terminal seeds it from the one it launched with
    /// (§11, §12).
    Settings,
}

/// The draft, where the cursor is in it, how far the band has scrolled and whether a
/// run is live.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Composer {
    /// The draft as it was typed, line breaks and all: one draft, never one prompt a
    /// line.
    text: String,
    /// Where the next glyph lands, as a byte offset into `text` that is always on a
    /// glyph boundary.
    cursor: usize,
    /// The first row of the draft the band paints, counted in lines: nothing scrolls
    /// away while the whole draft fits.
    scroll: usize,
    /// Whether a run is live.
    state: State,
}

impl Composer {
    /// An empty draft, cursor at the start, nothing scrolled: the box a launch opens.
    pub fn new() -> Composer {
        Composer::default()
    }

    /// The draft as it stands.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Where the cursor is, as a byte offset into the draft.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Whether a run is live, which is what `Ctrl-C` reads.
    pub fn state(&self) -> State {
        self.state
    }

    /// Says whether a run is live: the run loop sets this when it starts and when it
    /// stops, and nothing else in the frame may.
    pub fn set_state(&mut self, state: State) {
        self.state = state;
    }

    /// The rows the band takes: one per painted line plus the hairline, never fewer
    /// than `MIN_ROWS` and never more than `MAX_ROWS`.
    pub fn rows(&self) -> u16 {
        let lines = u16::try_from(self.line_count()).unwrap_or(u16::MAX);
        lines
            .saturating_add(HAIRLINE_ROWS)
            .clamp(MIN_ROWS, MAX_ROWS)
    }

    /// The draft rows that fit below the hairline: what a taller draft scrolls.
    pub fn visible_rows(&self) -> u16 {
        self.rows().saturating_sub(HAIRLINE_ROWS)
    }

    /// The first draft row painted, counted in lines: nothing while the whole draft
    /// fits, and a window that follows the cursor once it does not.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// The draft's lines the band can paint, from `scroll` down: what the frame draws,
    /// top row first.
    pub fn lines(&self) -> Vec<&str> {
        let visible = usize::from(self.visible_rows());
        self.text
            .split('\n')
            .skip(self.scroll)
            .take(visible)
            .collect()
    }

    /// Where the cursor is, as the row of `lines` it is on and the column of glyphs
    /// into that row: the cursor is always on a painted row, because every edit and
    /// every move follows it.
    pub fn caret(&self) -> (u16, u16) {
        let row = self.row(self.cursor);
        let column = self.text[self.line_start(row)..self.cursor].chars().count();
        (
            u16::try_from(row.saturating_sub(self.scroll)).unwrap_or(u16::MAX),
            u16::try_from(column).unwrap_or(u16::MAX),
        )
    }

    /// Whether the cursor waits on the draft's first line — the fact the queue's `↑`
    /// turns on, because on that line and with a queue the arrow recalls rather than
    /// moves (§12).
    pub fn on_first_line(&self) -> bool {
        self.row(self.cursor) == 0
    }

    /// Whether the cursor starts a word — the head of the draft, or a space, a tab or a
    /// line break behind it. It is the fact `@` turns on, and it is read here rather than
    /// in a second count of glyphs: a key that opens a screen has to be told from a glyph
    /// somewhere, and the draft is what knows where the words are.
    pub fn at_word_start(&self) -> bool {
        self.text[..self.cursor]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
    }

    /// A pasted prompt in one edit: one draft, its line breaks kept. A terminal puts
    /// `\r\n` or a bare `\r` on the wire for the break a clipboard calls `\n`, so both
    /// become the `\n` the draft is measured in — and not one of them is the `Enter`
    /// that sends, because this hands back nothing a run could take.
    pub fn paste(&mut self, text: &str) {
        let kept = text.replace("\r\n", "\n").replace('\r', "\n");
        self.insert(&kept);
    }

    /// `↑` on the first line: a queued prompt is the draft again, whole, put in where the
    /// caret waits so that nothing the reader had already typed is thrown away (§12).
    ///
    /// A recalled prompt is a paste that took a detour through the queue: every line
    /// break it was typed with comes back as one, because a prompt is one entry however
    /// many lines it carries (§12).
    pub fn recall(&mut self, prompt: Prompt) {
        self.insert(&prompt.text);
    }

    /// Hands the map one key and says what the terminal is to do about it.
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
            // The three keys a mark is taken with are the frame's: a box has no mark, so
            // it has nothing to say about them and nothing to repaint.
            Key::ShiftUp | Key::ShiftDown | Key::Copy => Outcome::Unchanged,
            Key::Quit => Outcome::Quit,
            Key::Cancel => self.cancel(),
            // `Ctrl-X` is the queue's, and the frame answers it before the draft ever
            // sees it: a draft reaching here changes nothing (§12).
            // `Ctrl-U`, `Tab` and `Shift+Tab` are the settings screen's, and are the
            // frame's to answer: a draft reaching here changes nothing.
            Key::Drop | Key::Escape | Key::Clear | Key::Tab | Key::BackTab => Outcome::Unchanged,
        }
    }

    /// `Enter`: the draft becomes the prompt a run takes and the box is empty again. A
    /// draft of nothing but spaces is not a prompt, so the box keeps it and nothing is
    /// repainted.
    ///
    /// What becomes of a prompt when a run is already in flight is not this value's
    /// business either: the terminal answers the outcome, and a prompt it cannot run now
    /// waits its turn in the frame's queue rather than being dropped (§12).
    fn send(&mut self) -> Outcome {
        if self.text.trim().is_empty() {
            return Outcome::Unchanged;
        }
        let text = std::mem::take(&mut self.text);
        self.cursor = 0;
        self.scroll = 0;
        Outcome::Send(Prompt { text })
    }

    /// `Ctrl-C`: with a run live it is the run that is cancelled and the draft stays,
    /// because a prompt half-written when a run dies is the one the reader was
    /// writing. With no run there is nothing to cancel, so the key clears what is
    /// typed — and a box already empty is not a change to repaint.
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

    /// The outcome an edit leaves behind: a key that moved nothing is not a frame to
    /// paint again, and §12's idle frame is never drawn.
    fn edited(changed: bool) -> Outcome {
        if changed {
            Outcome::Edited
        } else {
            Outcome::Unchanged
        }
    }

    /// Puts `text` in at the cursor and follows it: one path for a typed glyph, for a
    /// whole pasted prompt, and for the word a picked path is, so each of them is an edit
    /// like any other and none of them can leave the caret behind.
    pub fn insert(&mut self, text: &str) {
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.follow();
    }

    /// Takes back the glyph before the cursor, whole: a byte would leave a glyph half
    /// there.
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

    /// Up a line, keeping the column: the cursor's glyph column is what a reader reads
    /// as "the same place", and a shorter line ends it.
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

    /// Keeps the cursor on a painted row, scrolling the window as little as it can: it
    /// moves only when the cursor would otherwise leave the band, so a draft being
    /// typed does not jump a line at a time.
    fn follow(&mut self) {
        let row = self.row(self.cursor);
        let visible = usize::from(self.visible_rows());
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll + visible {
            self.scroll = row + 1 - visible;
        }
    }

    /// How many lines the draft has, the empty draft having one: an empty box still
    /// paints one row of nothing below its hairline.
    fn line_count(&self) -> usize {
        self.text.matches('\n').count() + 1
    }

    /// The line a byte offset is on, counted from the top.
    fn row(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())]
            .matches('\n')
            .count()
    }

    /// Where the byte offset a line starts at.
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

    /// One past the last byte of a line, which is the offset of its break.
    fn line_end(&self, row: usize) -> usize {
        match self.text[self.line_start(row)..].find('\n') {
            Some(index) => self.line_start(row) + index,
            None => self.text.len(),
        }
    }

    /// The glyph column the cursor is in, counted in glyphs rather than bytes so that
    /// a line of ticks and a line of letters line up as a reader sees them.
    fn column(&self, row: usize) -> usize {
        self.text[self.line_start(row)..self.cursor].chars().count()
    }

    /// The offset of `column` glyphs into a line, or its end when the line is shorter.
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
    //! The composer's own rules, one test each: the height two lines make and six
    //! cap, the window a taller draft scrolls, the paste that keeps its line breaks
    //! and the `Ctrl-C` that reads the run's state. None of them needs a terminal,
    //! which is the point of the value (§12, ADR-0005).

    use super::*;

    /// A composer holding `text`, cursor at the end: what a draft pasted in one go is,
    /// and what each test starts from unless it says otherwise.
    fn holding(text: &str) -> Composer {
        let mut composer = Composer::new();
        composer.paste(text);
        composer
    }

    /// A draft of `lines` numbered lines, cursor at the end of the last: a draft the
    /// band cannot hold whole.
    fn numbered(lines: usize) -> Composer {
        let draft: Vec<String> = (1..=lines).map(|line| line.to_string()).collect();
        holding(&draft.join("\n"))
    }

    /// The prompt an outcome carries, or a failure naming what came instead.
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
