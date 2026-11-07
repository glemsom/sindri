//! What one frame holds and where each part of it goes: the transcript above, the
//! composer's band — the hairline a draft is typed under and the draft itself — and
//! the footer's hints along the bottom edge (§12).
//!
//! `View::paint` is the one seam a frame is drawn through. The terminal calls it on
//! the frame the real screen is painted from, and a test calls it on a frame going to
//! ratatui's headless backend, so the picture a fixture holds and the picture a reader
//! sees are produced by the same code — the point of ADR-0012, and the reason a
//! visual decision can be graded by an agent at all.
//!
//! A frame is a stack of modes, and the mode on top is what is drawn: the composer at
//! the base, and the `?` screen over it, which replaces the bands with one list of the
//! keys in force painted from the tables that answer them (#45). Where a screen is up
//! there is no draft on the frame and no caret, because a key pressed then would not
//! reach one.
//!
//! The transcript's rows are `rows`' to lay out and this file's to draw: a frame is
//! handed the entries a run has written and where the reader is scrolled to, and the
//! band paints the window of them the frame reaches, which is §12's rule that only
//! on-screen rows are laid out (ADR-0008). The turn in flight is drawn under those
//! entries as one more of them — the live one, the answer and the reasoning of the
//! newest turn as they land — so a run's picture is the facts it has written and the
//! turn it is still in, and this file is the sink the wire's fragments land in. The
//! composer's band is painted from the `Composer` it is handed, and the rows the box
//! takes are the `Composer`'s to decide: this file asks rather than counts (#41). The
//! rail draws into the columns it takes from the transcript's band, and how many columns
//! those are — and what it gives up when there are fewer — is `rail`'s to decide and not
//! this file's (#46): `paint` asks it for the split and hands it the facts it was given.
//! The pulse is #39's, and marking and copy are #50's. Each paints into a band of this
//! frame and changes its fixture deliberately in the commit that changes it.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::provider::Deltas;
use sindri_core::records::Entry;

use crate::composer::{Composer, HAIRLINE_ROWS, Key, Outcome, State};
use crate::ink::Ink;
use crate::keys::{Act, Mode, Stack};
use crate::picker::Picker;
use crate::rail::Facts;
use crate::rows::{Rows, Scroll};
use crate::stream::Stream;

/// The hints §12 never takes away, whichever way the frame narrows: the key that sends
/// a prompt and the key that says which keys there are.
const FLOOR_HINTS: [(&str, Ink); 3] = [
    ("Enter send", Ink::Muted),
    (" · ", Ink::Dim),
    ("? keys", Ink::Muted),
];

/// The hairline a draft is typed under: one glyph, so the composer's top edge is a row
/// of them, drawn in `dim` because it is a fact about the frame rather than a thing said.
const HAIRLINE: char = '─';

/// The columns the keys screen leaves empty at its left edge, so its rows read as one
/// block rather than as the frame's own prose.
const KEYS_MARGIN: u16 = 2;

/// The gap between a key's name and what it does on the keys screen: two columns, so the
/// descriptions of every mode line up in one column an eye can run down.
const KEYS_GAP: usize = 2;

/// What the `@` picker's screen heads itself with, in the accent ink the keys screen heads
/// its own with.
const PICKER_TITLE: &str = "@ files";

/// The label the picker's query sits behind, so a line of glyphs under the title reads as
/// what the list is narrowed by rather than as a row of it.
const PICKER_QUERY: &str = "query: ";

/// What a picker whose rows are all gone says: a screen with a query and nothing under it
/// has to say the query is what emptied it.
const PICKER_EMPTY: &str = "no file answers";

/// The glyph before the row the selection is on, and the two cells every other row keeps
/// empty so the paths line up in one column.
const PICKER_MARK: &str = "\u{203a} ";
const PICKER_NO_MARK: &str = "  ";

/// One frame's worth of facts, in the shape the terminal draws them. Boot knows none
/// of them — no run, no prompt, no transcript row, nothing loaded — so the frame is its
/// chrome and nothing else; a run hands over the entries it has written and where the
/// transcript is scrolled to, a keystroke hands over the draft, and this type is where
/// the tickets after this one put the facts they do have.
#[derive(Debug, Default, Clone)]
pub struct View {
    /// The transcript's rows, one entry at a time, at the width the band is drawn at.
    rows: Rows,
    /// Where the transcript's window sits: on the newest row, or held some rows above
    /// it while facts land below (#50's arrivals count from here).
    scroll: Scroll,
    /// The draft a prompt is typed into: the composer's band is painted from this
    /// value and from nothing else (#41).
    composer: Composer,
    /// The turn in flight: the partial answer and the partial reasoning of the run's
    /// newest turn, painted under the settled entries and laid out again alone as each
    /// fragment lands (§4, §12). Empty is the case of no turn in flight, which is every
    /// frame until #43 feeds the run's deltas in here.
    stream: Stream,
    /// The modes this frame is in, the composer at the base and any screen opened over
    /// it: what answers a key and, while a screen is up, the whole of what is painted.
    stack: Stack,
    /// The facts the rail draws down the frame's right-hand columns: what the core knows
    /// about the run and what a caller hands in for the facts nothing produces yet, in
    /// the values `rail` spells them with (#46).
    rail: Facts,
    /// The directory the `@` picker walks: the one the launch stands in, which is the
    /// workspace a run's commands run in. `None` is a frame that was told no directory,
    /// and it offers no file rather than inventing one (§12).
    workspace: Option<PathBuf>,
    /// The files the picker walks up when `@` is pressed, narrowed by the query typed into
    /// the screen: empty until a screen is opened, and walked afresh each time it is.
    picker: Picker,
}

impl View {
    /// The frame a launch opens on: the screen has just been taken, no run has started
    /// and nothing has been typed, so the transcript is empty, the composer shows its
    /// hairline over one empty row and the footer shows the hints never given up.
    pub fn boot() -> View {
        View::default()
    }

    /// The frame a run is in: the entries it has written, in order, and where the
    /// reader is standing in them. What a row of an entry looks like is `rows`'
    /// business, so a caller says what happened and never how wide or in which ink.
    pub fn of(entries: impl IntoIterator<Item = Entry>, scroll: Scroll) -> View {
        let mut rows = Rows::new();
        for entry in entries {
            rows.push(entry);
        }
        View {
            rows,
            scroll,
            ..View::default()
        }
    }

    /// The frame a draft somebody typed is painted on. A fixture builds one this way,
    /// so the accepted picture of a six-row composer is made through the same value a
    /// reader's six rows are.
    pub fn typing(composer: Composer) -> View {
        View {
            composer,
            ..View::default()
        }
    }

    /// The draft this frame is drawn around.
    pub fn composer(&self) -> &Composer {
        &self.composer
    }

    /// The turn in flight: the runs of answer and reasoning that have landed since the
    /// run's newest turn started, in the order they landed.
    pub fn stream(&self) -> &Stream {
        &self.stream
    }

    /// The turn in flight is over: what it said is written down as entries by then
    /// (ADR-0007), so the live entry it was drawn as goes and the facts take its place.
    /// A run calls this when it settles a turn and pushes that turn's entries; a frame
    /// that kept the stream would draw the same answer twice.
    pub fn settle(&mut self) {
        self.stream = Stream::new();
    }

    /// Hands this frame the directory it stands in: the workspace `@` walks, and the one
    /// the run's own commands are given. A launch reads the same directory off the process
    /// (`Launch::open`), so the picker and the run cannot disagree about where they are.
    pub fn set_workspace(&mut self, workspace: PathBuf) {
        self.workspace = Some(workspace);
    }

    /// The files the picker last walked and the query narrowing them: what the screen
    /// paints, and what a selection takes its path from.
    pub fn picker(&self) -> &Picker {
        &self.picker
    }

    /// The mode on top of the stack: what this frame is showing and what will answer
    /// the next key.
    pub fn mode(&self) -> Mode {
        self.stack.top()
    }

    /// Hands the frame one key and says what the terminal is to do about it: paint the
    /// frame again, send the draft, quit, or leave a run cancelled and the draft where
    /// it was. The mode on top decides — one row of its table, or its fallback — and the
    /// terminal acts (ADR-0005).
    pub fn key(&mut self, key: Key) -> Outcome {
        let Some(act) = self.stack.act(key) else {
            // A screen that names no row for the key has nothing to say about it, which
            // is how a glyph typed at a screen stays out of the draft under it.
            return Outcome::Unchanged;
        };
        match act {
            Act::Draft => self.composer.key(key),
            Act::Up => self.up(),
            Act::Keys => self.keys(key),
            Act::Files => self.at(key),
            Act::Query => self.query(key),
            Act::Previous => self.step(Step::Previous),
            Act::Next => self.step(Step::Next),
            Act::Pick => self.pick(),
            Act::Close => self.close(),
        }
    }

    /// `?`: the screen that lists the keys in force.
    ///
    /// It is a key only while the draft is empty, for the reason `↑` is the queue's only
    /// on the first line: the composer is a text box, and a prompt that could not hold a
    /// question mark would be a composer that cannot ask a question. With a draft the
    /// key is a glyph like every other, and the frame's keys are a `Ctrl-C` away.
    fn keys(&mut self, key: Key) -> Outcome {
        if !self.composer.text().is_empty() {
            return self.composer.key(key);
        }
        self.stack.open(Mode::Keys);
        Outcome::Edited
    }

    /// `@`: the files of the workspace, walked afresh and offered as one screen (§12).
    ///
    /// It is a key only where it starts a word — the head of the draft, or after the
    /// space, tab or line break that leaves one — for the reason `?` is a key only while
    /// the draft is empty: this is a text box, and a prompt that could not hold
    /// `someone@host` would be a prompt that cannot name an address. An `@` inside a word
    /// is a glyph like every other, so the two ways to want both are a space before the
    /// key and typing the glyph on.
    ///
    /// A frame that was told no workspace offers no file rather than the process's own
    /// directory: what a launch stands in is handed in (`View::set_workspace`), and a
    /// picker walking somewhere else would be offering paths no run could read.
    fn at(&mut self, key: Key) -> Outcome {
        if !self.composer.at_word_start() {
            return self.composer.key(key);
        }
        let Some(root) = self.workspace.clone() else {
            return Outcome::Unchanged;
        };
        self.picker = Picker::walk(&root);
        self.stack.open(Mode::Files);
        Outcome::Edited
    }

    /// A glyph typed at the picker, or the backspace that takes one back: the query is the
    /// picker's own filter box, and it answers in the picker's own map (`picker::Picker`).
    fn query(&mut self, key: Key) -> Outcome {
        if self.picker.key(key) {
            Outcome::Edited
        } else {
            Outcome::Unchanged
        }
    }

    /// `↑` and `↓` at the picker: the selection walks the rows and stops at the ends, so a
    /// key that moved nothing is a frame that is not painted.
    fn step(&mut self, step: Step) -> Outcome {
        let moved = match step {
            Step::Previous => self.picker.up(),
            Step::Next => self.picker.down(),
        };
        match moved {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// `Enter` at the picker: the chosen path goes into the draft as one shell word and the
    /// screen closes. A list the query left empty has no row to take, so there is nothing
    /// to insert and the screen stays open to be widened again — the alternative is a
    /// screen closing on a key that did nothing.
    fn pick(&mut self) -> Outcome {
        let Some(word) = self.picker.word() else {
            return Outcome::Unchanged;
        };
        self.composer.insert(&word);
        self.stack.close();
        Outcome::Edited
    }

    /// `Esc`: the screen on top closes and the mode under it has the keys again. The
    /// composer is the base of the stack and never closes, so there the key does
    /// nothing and nothing is repainted.
    fn close(&mut self) -> Outcome {
        let closed = self.stack.top();
        self.stack.close();
        if self.stack.top() == closed {
            Outcome::Unchanged
        } else {
            Outcome::Edited
        }
    }

    /// `↑` — one key with two homes, and this is the only place it is decided (A9).
    ///
    /// On the composer's first line the thing above the caret is the queue rather than
    /// another row of the draft, so the arrow recalls the newest queued prompt; anywhere
    /// else it moves the caret a row up, which is the composer's own map. The queue is
    /// #42's and is not here yet, so on that line there is nothing to recall and nothing
    /// to repaint — and #42 adds its queue to this arm rather than to a keymap of its own.
    fn up(&mut self) -> Outcome {
        if self.composer.on_first_line() {
            return Outcome::Unchanged;
        }
        self.composer.key(Key::Up)
    }

    /// Hands the draft a pasted prompt: one draft, its line breaks kept, and no path
    /// back to something a run could take. A screen has no draft, so what a clipboard
    /// holds while one is open is dropped rather than typed where nobody can see it.
    pub fn paste(&mut self, text: &str) {
        if self.stack.modal().is_some() {
            return;
        }
        self.composer.paste(text);
    }

    /// Hands this frame the facts its rail draws: the model, the turns against the cap,
    /// the two meters, the workspace and its branch, the skills in force, the sandbox
    /// posture, and the session's GUID and temp. They are the core's own values, so the
    /// rail cannot say about a run what the run was never handed (#46).
    pub fn set_facts(&mut self, facts: Facts) {
        self.rail = facts;
    }

    /// Says whether a run is live, which is the one fact `Ctrl-C`'s rule reads: the run
    /// loop says so when it starts and when it stops (#43), and nothing else may.
    pub fn set_state(&mut self, state: State) {
        self.composer.set_state(state);
    }

    /// Paints this frame into `area`: every band of it, in the ink each band is drawn
    /// in, from the top of the screen to the bottom edge.
    ///
    /// Painting is what tells the row cache how wide the transcript is, so a resize
    /// invalidates it here and the same pass reflows: the width a row was laid out at is
    /// the width a band drew it at, and no caller gets to say otherwise (ADR-0008). The
    /// composer's rows are read from the draft, so the band a draft grows into is
    /// measured when the frame is drawn and never cached.
    pub fn paint(&mut self, area: Rect, buffer: &mut Buffer) {
        // A screen is the whole frame: the bands under it are not painted at all, so
        // nothing of a draft a key cannot reach is on the screen pretending otherwise.
        if let Some(modal) = self.stack.modal() {
            match modal {
                Mode::Files => self.files_screen(area, buffer),
                // The `?` screen lists the keys of every mode in the stack, so it is what a
                // screen with no picture of its own is drawn as.
                Mode::Composer | Mode::Keys => self.keys_screen(area, buffer),
            }
            return;
        }
        let bands = bands(area, self.composer.rows());
        // The rail takes its columns from the transcript's band before the transcript is
        // laid out in what is left: a row's width is the width it is drawn at (ADR-0008),
        // so the rail's share is decided here and nowhere else (#46).
        let share = self.rail.split(bands.transcript);
        if let Some((shape, rail)) = share.rail {
            self.rail.paint(shape, rail, buffer);
        }
        self.transcript(share.transcript, buffer);
        self.composer_band(bands.composer, buffer);
        self.footer(bands.footer, buffer);
    }

    /// The transcript: what the run has said so far, newest last, painted from the rows
    /// the cache holds for the window this band reaches and for no other entry. Where a
    /// reader scrolled back is told what arrived below by #50; the pulse at the head of
    /// the newest answer is #39's.
    ///
    /// The turn in flight is the last of the entries the window reaches and the only one
    /// laid out again as the frame is painted, which is the whole of what makes an answer
    /// arrive on screen with no relaid history around it (ADR-0008).
    fn transcript(&mut self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Ink::Plain.style());
        self.rows.resize(area.width);
        self.rows.live(&self.stream);
        let window = self.rows.window(self.scroll, area.height);
        let mut y = area.y;
        for piece in window.pieces() {
            let rows = self.rows.rows(piece.entry);
            let first = usize::from(piece.skip);
            for row in &rows[first..first + usize::from(piece.rows)] {
                row.paint(
                    Rect {
                        height: 1,
                        y,
                        ..area
                    },
                    buffer,
                );
                y += 1;
            }
        }
    }

    /// The composer: the hairline across the band's top row and the draft's lines
    /// below it — the rows the box takes, from the line it has scrolled to. Where the
    /// cursor sits inside a line is not painted here but handed to the terminal (see
    /// `View::cursor`), because a cell of a frame is a glyph in an ink and a caret is
    /// neither.
    fn composer_band(&self, area: Rect, buffer: &mut Buffer) {
        let hairline = Rect { height: 1, ..area };
        for x in hairline.x..hairline.right() {
            if let Some(cell) = buffer.cell_mut((x, hairline.y)) {
                cell.set_char(HAIRLINE);
                cell.set_style(Ink::Dim.style());
            }
        }

        let draft = Rect {
            y: area.y.saturating_add(HAIRLINE_ROWS),
            height: area.height.saturating_sub(HAIRLINE_ROWS),
            ..area
        };
        for (row, line) in self.composer.lines().iter().enumerate() {
            let Some(offset) = u16::try_from(row).ok() else {
                break;
            };
            let Some(y) = draft.y.checked_add(offset) else {
                break;
            };
            if y >= draft.bottom() {
                break;
            }
            draft_line(buffer, Rect { y, ..draft }, line);
        }
    }

    /// Where the terminal's own cursor belongs, so a reader sees where typing lands:
    /// the caret's cell on its row of the draft, and nothing when the caret has run
    /// past the band's edge.
    ///
    /// The caret is the terminal's cursor and never a cell of the frame. An ink names a
    /// foreground and nothing else (ADR-0012), so a caret drawn as a cell would be a
    /// letter tinted `accent` — invisible on the space it usually sits on — and a
    /// reverse-video block cannot be written into a fixture at all.
    pub fn cursor(&self, area: Rect) -> Option<(u16, u16)> {
        // No draft is on screen while one is open, so there is no cell for the caret to
        // wait on: it would be a cursor floating over a list.
        if self.stack.modal().is_some() {
            return None;
        }
        let bands = bands(area, self.composer.rows());
        let (row, column) = self.composer.caret();
        let y = bands
            .composer
            .y
            .checked_add(HAIRLINE_ROWS)?
            .checked_add(row)?;
        let x = area.x.checked_add(column)?;
        (column < area.width && y < bands.composer.bottom()).then_some((x, y))
    }

    /// The `?` screen: every key in force, the mode that answers first at the top and
    /// the modes under it after — because the top one is where the reader is, and the
    /// rest are why their keys are not doing anything.
    ///
    /// It is painted from the tables themselves rather than from prose about them, so
    /// the screen and the keys cannot disagree, and a mode added later is a variant, a
    /// table and a heading here with nothing else to remember (A7).
    fn keys_screen(&self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Ink::Plain.style());
        let modes: Vec<Mode> = self.stack.modes().collect();
        let name_width = modes
            .iter()
            .flat_map(|mode| mode.table())
            .map(|binding| binding.name.chars().count() + KEYS_GAP)
            .max()
            .unwrap_or(0);
        let mut y = area.y;
        screen_row(buffer, area, y, &[("? keys", Ink::Accent)]);
        for mode in modes {
            // A blank row before each heading, so a mode's keys read as one block and
            // the heading is not taken for the last key of the block above it.
            y = y.saturating_add(1);
            if y >= area.bottom() {
                return;
            }
            screen_row(buffer, area, y, &[(mode.name(), Ink::Dim)]);
            for binding in mode.table() {
                y = y.saturating_add(1);
                if y >= area.bottom() {
                    return;
                }
                let name = format!("{:<name_width$}", binding.name);
                screen_row(
                    buffer,
                    area,
                    y,
                    &[(&name, Ink::Muted), (binding.does, Ink::Plain)],
                );
            }
        }
    }

    /// The `@` picker: the query the list is narrowed by, then the rows it leaves nearest
    /// first, the selection marked. The marker is drawn rather than the row tinted alone,
    /// because a row painted the accent ink tells a reader that something here is chosen and
    /// not which row it is on a list of eight.
    ///
    /// A path wider than the frame is cut at its right edge instead of dropping the row:
    /// §12's "whole or it is not drawn" is a rule about hints and facts, and a reader
    /// pointing at a path wants the row it is nearest to, not silence.
    fn files_screen(&self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Ink::Plain.style());
        let mut y = area.y;
        screen_row(buffer, area, y, &[(PICKER_TITLE, Ink::Accent)]);
        y = y.saturating_add(1);
        if y < area.bottom() {
            screen_row(
                buffer,
                area,
                y,
                &[(PICKER_QUERY, Ink::Dim), (self.picker.query(), Ink::Muted)],
            );
        }
        let rows = self.picker.rows();
        if rows.is_empty() {
            y = y.saturating_add(1);
            if y < area.bottom() {
                screen_row(buffer, area, y, &[(PICKER_EMPTY, Ink::Dim)]);
            }
            return;
        }
        for (at, path) in rows.iter().enumerate() {
            y = y.saturating_add(1);
            if y >= area.bottom() {
                return;
            }
            let (marker, ink) = match at == self.picker.selected() {
                true => (PICKER_MARK, Ink::Accent),
                false => (PICKER_NO_MARK, Ink::Plain),
            };
            cut_row(buffer, screen_band(area), y, &[(marker, ink), (path, ink)]);
        }
    }

    /// The footer: the hints §12 keeps at every width, on the bottom edge. The five
    /// more a 120-column frame shows are the chrome's ticket; these three spans are the
    /// floor it degrades to, so the golden frame holds what no width can take away.
    fn footer(&self, area: Rect, buffer: &mut Buffer) {
        write(buffer, area, &FLOOR_HINTS);
    }
}

/// The frame as the sink of the turn in flight: a fragment of answer or of reasoning
/// lands in this frame's stream, and the next paint draws it (§4). This is the door #43
/// takes — the run is handed the frame and streams into it — and it is why a fragment
/// never has to be written down to be seen (ADR-0007).
impl Deltas for View {
    fn text(&mut self, fragment: &str) {
        self.stream.text(fragment);
    }

    fn reasoning(&mut self, fragment: &str) {
        self.stream.reasoning(fragment);
    }
}

/// Which way `↑` and `↓` move the picker's selection, said once so the two rows of its
/// table are one arm of the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// The row above the selection.
    Previous,
    /// The row below it.
    Next,
}

/// The bands a frame is cut into, top to bottom.
struct Bands {
    /// Everything above the composer: the transcript's rows, and the columns `rail`
    /// takes from them, or the row it takes below a hundred columns (#46).
    transcript: Rect,
    /// The composer's rows of draft, with the hairline on the first of them.
    composer: Rect,
    /// The footer's single row of hints, on the bottom edge.
    footer: Rect,
}

/// Cuts `area` into its three bands. The chrome is measured up from the bottom edge and
/// the transcript takes what is left, because the footer holds one row and the composer
/// as many as the draft asks for (two to six), while the transcript is the band a run
/// grows into (§12).
fn bands(area: Rect, composer_rows: u16) -> Bands {
    let footer = area.height.min(1);
    let composer = composer_rows.min(area.height.saturating_sub(footer));
    let transcript = area.height.saturating_sub(footer).saturating_sub(composer);
    Bands {
        transcript: Rect {
            height: transcript,
            ..area
        },
        composer: Rect {
            y: area.y.saturating_add(transcript),
            height: composer,
            ..area
        },
        footer: Rect {
            y: area.y.saturating_add(transcript).saturating_add(composer),
            height: footer,
            ..area
        },
    }
}

/// Writes one row of a screen at the screen's margin, each span in its own ink. Like the
/// footer's hints, a span that would run past the frame's right edge is not drawn at
/// all: a screen's row is whole or it is not there.
fn screen_row(buffer: &mut Buffer, area: Rect, y: u16, spans: &[(&str, Ink)]) {
    write(
        buffer,
        Rect {
            y,
            ..screen_band(area)
        },
        spans,
    );
}

/// The columns a screen's rows are drawn in: the frame inset by the margin that makes them
/// read as one block rather than as the frame's own prose.
fn screen_band(area: Rect) -> Rect {
    let margin = KEYS_MARGIN.min(area.width);
    Rect {
        x: area.x.saturating_add(margin),
        width: area.width.saturating_sub(margin),
        height: 1,
        ..area
    }
}

/// Writes one draft line across a row from the band's left edge, cut at its right
/// edge: a line wider than the frame is prose that runs on, not a hint that has to be
/// whole — §12's "whole or it is not drawn" is a rule about chrome — so it is cut
/// rather than dropped.
fn draft_line(buffer: &mut Buffer, area: Rect, line: &str) {
    cut_row(buffer, area, area.y, &[(line, Ink::Plain)]);
}

/// Writes spans of text across one row from its left edge, each in its own ink, and cuts the
/// last of them at the row's right edge — the writer a line of prose gets, where `write` is
/// the one chrome gets.
fn cut_row(buffer: &mut Buffer, area: Rect, y: u16, spans: &[(&str, Ink)]) {
    let mut x = area.x;
    for (text, ink) in spans {
        for glyph in text.chars() {
            if x >= area.right() {
                return;
            }
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_char(glyph);
                cell.set_style(ink.style());
            }
            x += 1;
        }
    }
}

/// Writes spans of text across a row from its left edge, each in its own ink, and never
/// half of one: §12 gives a hint up whole or not at all, so a span that would not fit in
/// what is left of the row is not drawn rather than cut.
fn write(buffer: &mut Buffer, area: Rect, spans: &[(&str, Ink)]) {
    let mut x = area.x;
    for (text, ink) in spans {
        let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
        if x.saturating_add(width) > area.right() {
            return;
        }
        for glyph in text.chars() {
            if let Some(cell) = buffer.cell_mut((x, area.y)) {
                cell.set_char(glyph);
                cell.set_style(ink.style());
            }
            x += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    //! The one thing a golden frame cannot hold: the cell the terminal's own cursor is
    //! put on. A fixture is a buffer of glyphs and inks (ADR-0012) and a caret is
    //! neither, so where the caret goes is proved here and painted by the terminal.

    use super::*;

    /// A screen `cols` by `rows`, at the origin: the area a frame is painted into.
    fn screen(cols: u16, rows: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: cols,
            height: rows,
        }
    }

    #[test]
    fn the_terminal_cursor_lands_on_the_carets_row_of_the_draft() {
        let composer = {
            let mut composer = Composer::new();
            composer.paste("first line\nsecond line");
            composer
        };
        let view = View::typing(composer);
        let area = screen(40, 12);

        // Twelve rows: the footer takes the last, and the three-row band — a hairline
        // and two lines of draft — sits on rows 8 to 10. The caret is eleven glyphs
        // into the second line.
        assert_eq!(view.composer().rows(), 3);
        assert_eq!(view.cursor(area), Some((11, 10)));
    }

    #[test]
    fn a_screen_has_no_caret_for_the_terminal_to_put_anywhere() {
        let mut view = View::boot();
        let area = screen(40, 12);
        assert!(
            view.cursor(area).is_some(),
            "the empty draft's caret has a cell before the screen opens"
        );

        view.key(Key::Char('?'));

        assert_eq!(
            view.cursor(area),
            None,
            "a screen is the whole frame and has no draft in it, so the caret is not left \
             floating over a list"
        );
    }

    #[test]
    fn a_caret_run_past_the_bands_edge_has_no_cell_to_wait_on() {
        let view = View::typing({
            let mut composer = Composer::new();
            composer.paste(&"x".repeat(60));
            composer
        });

        assert_eq!(
            view.cursor(screen(40, 12)),
            None,
            "column 60 is off a 40-column frame"
        );
        assert_eq!(view.cursor(screen(80, 12)), Some((60, 10)));
    }
}
