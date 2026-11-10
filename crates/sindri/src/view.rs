//! A frame is drawn through one seam, `View::paint`, onto whatever buffer it is handed:
//! the terminal's screen or a test's headless backend, so a golden fixture and the
//! picture a reader sees are produced by the same code.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::config::{Config, Touched};
use sindri_core::provider::Deltas;
use sindri_core::provider::served::Listing;
use sindri_core::records::{Entry, SessionDocument};
use sindri_core::run::{Load, Loaded};
use sindri_core::skills::{Catalog, Skill};

use crate::commands::{self, Command};
use crate::composer::{Composer, HAIRLINE_ROWS, Key, Outcome, Prompt, State};
use crate::graphics::CellSize;
use crate::ink::Ink;
use crate::keys::{Act, Mode, Stack};
use crate::mark::{Cell, Mark, Span};
use crate::models::Models;
use crate::palette::{Band, Palette};
use crate::picker::Picker;
use crate::picture::Surface;
use crate::pictures::Pictures;
use crate::pulse::Pulse;
use crate::queue::Queue;
use crate::rail::Facts;
use crate::rows::{Row, Rows, Scroll};
use crate::settings::Settings;
use crate::stats::Stats;
use crate::stream::Stream;

/// The hints in survival order, most valuable first: the order a narrower footer gives
/// them up in, from the end, and longer than any width draws.
const HINTS: [&str; 8] = [
    "Enter send",
    "? keys",
    "Ctrl-C cancel",
    "Ctrl-Q quit",
    "/ commands",
    "@ files",
    "↑ queue",
    "Shift+Enter newline",
];

const GAP: &str = " · ";

const STEPS: [(u16, usize); 2] = [(120, 7), (100, 5)];

/// The first two of `HINTS`: held at every width and given up last.
const FLOOR: usize = 2;

const HAIRLINE: char = '─';

const KEYS_MARGIN: u16 = 2;

const KEYS_GAP: usize = 2;

const PICKER_TITLE: &str = "@ files";

const PICKER_QUERY: &str = "query: ";

const PICKER_EMPTY: &str = "no file answers";

/// The glyph before the row a key acts on, in the picker and in the settings modal both:
/// one mark, because both screens say the same thing with it.
const ROW_MARK: &str = "\u{203a} ";
const ROW_GAP: &str = "  ";

const SETTINGS_TITLE: &str = "settings";

const SETTINGS_GAP: usize = 2;

const MODELS_TITLE: &str = "/model";

const STATS_TITLE: &str = "/stats";

const MODELS_QUERY: &str = "query: ";

const MODELS_ASKING: &str = "asking the wire what it serves";

const MODELS_EMPTY: &str = "no served name answers";

const NAMES_GAP: usize = 2;

#[derive(Debug, Default, Clone)]
pub struct View {
    rows: Rows,
    scroll: Scroll,
    composer: Composer,
    stream: Stream,
    queue: Queue,
    stack: Stack,
    pulse: Option<Pulse>,
    rail: Facts,
    record: Option<SessionDocument>,
    mark: Option<Mark>,
    marked: Span,
    workspace: Option<PathBuf>,
    picker: Picker,
    models: Models,
    loaded: Loaded,
    home: Option<PathBuf>,
    settings: Settings,
    pictures: Pictures,
    cell_size: CellSize,
    palette: Palette,
}

impl View {
    pub fn boot() -> View {
        View::default()
    }

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

    pub fn typing(composer: Composer) -> View {
        View {
            composer,
            ..View::default()
        }
    }

    pub fn composer(&self) -> &Composer {
        &self.composer
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn enqueue(&mut self, prompt: Prompt) {
        self.queue.push(prompt);
    }

    pub fn dequeue(&mut self) -> Option<Prompt> {
        self.queue.next_to_run()
    }

    pub fn stream(&self) -> &Stream {
        &self.stream
    }

    /// The turn in flight is over: what it said is written down as entries, so the live entry
    /// it was drawn as goes. A frame that kept the stream would draw the same answer twice.
    pub fn settle(&mut self) {
        self.stream = Stream::new();
    }

    pub fn set_workspace(&mut self, workspace: PathBuf) {
        self.workspace = Some(workspace);
    }

    pub fn picker(&self) -> &Picker {
        &self.picker
    }

    pub fn models(&self) -> &Models {
        &self.models
    }

    pub fn set_served(&mut self, listing: Listing) {
        self.models.served(listing);
    }

    pub fn loaded(&self) -> &Loaded {
        &self.loaded
    }

    pub fn set_home(&mut self, home: Option<PathBuf>) {
        self.home = home;
    }

    pub fn cell_size(&self) -> CellSize {
        self.cell_size
    }

    pub fn set_cell_size(&mut self, cell_size: CellSize) {
        self.cell_size = cell_size;
    }

    /// The colours this frame is drawn in, resolved once at boot: a frame that read them itself
    /// would ask the terminal a question its reader has already answered.
    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }

    pub fn palette(&self) -> Palette {
        self.palette
    }

    pub fn set_pictures(&mut self, pictures: Pictures) {
        self.pictures = pictures;
    }

    pub fn pictures(&self) -> &Pictures {
        &self.pictures
    }

    fn surface(&self) -> Surface {
        Surface::of(
            self.workspace.as_deref(),
            self.cell_size,
            self.pictures.draws(),
        )
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn open_settings(&mut self, config: &Config) {
        self.settings = Settings::of(config);
        self.stack.open(Mode::Settings);
    }

    pub fn saved(&mut self) {
        if self.stack.top() == Mode::Settings {
            self.stack.close();
        }
    }

    pub fn refuse(&mut self, reason: &str) {
        self.settings.refuse(reason);
    }

    pub fn set_record(&mut self, document: &SessionDocument) {
        self.record = Some(document.clone());
    }

    pub fn mode(&self) -> Mode {
        self.stack.top()
    }

    pub fn key(&mut self, key: Key) -> Outcome {
        let Some(act) = self.stack.act(key) else {
            return Outcome::Unchanged;
        };
        match act {
            Act::Draft => self.draft(key),
            Act::Up => self.up(),
            Act::Copy => self.copy(),
            Act::ExtendDown => self.extend(true),
            Act::ExtendUp => self.extend(false),
            Act::Drop => self.drop_newest(),
            Act::Keys => self.keys(key),
            Act::Files => self.at(key),
            Act::Query => self.query(key),
            Act::Previous => self.step(Step::Previous),
            Act::Next => self.step(Step::Next),
            Act::Pick => self.pick(),
            Act::Choose => self.choose(),
            Act::Value => self.value(key),
            Act::Flip => self.flip(),
            Act::Clear => self.clear(),
            Act::Write => self.write(),
            Act::Close => self.close(),
        }
    }

    pub fn drag(&mut self, anchor: Cell, head: Cell) {
        self.mark = Some(Mark::drag(anchor, head));
    }

    pub fn mark(&self) -> Option<Mark> {
        self.mark
    }

    /// `Ctrl-Y`: the words the mark covers leave the frame, or the newest answer does when
    /// nothing is marked. What leaves is the span the last paint drew, not a second reading.
    fn copy(&self) -> Outcome {
        if !self.marked.is_empty() {
            return Outcome::Copy(self.marked.text());
        }
        match self.rows.last_answer() {
            Some(answer) => Outcome::Copy(answer),
            None => Outcome::Unchanged,
        }
    }

    fn extend(&mut self, down: bool) -> Outcome {
        let Some(mark) = self.mark else {
            return Outcome::Unchanged;
        };
        let grown = mark.grown(down);
        if grown == mark {
            return Outcome::Unchanged;
        }
        self.mark = Some(grown);
        Outcome::Edited
    }

    fn draft(&mut self, key: Key) -> Outcome {
        match self.composer.key(key) {
            Outcome::Send(prompt) => match commands::read(prompt.text()) {
                Some(command) => self.answer(command),
                None => Outcome::Send(prompt),
            },
            outcome => outcome,
        }
    }

    fn answer(&mut self, command: Command) -> Outcome {
        match command {
            Command::New => self.start_over(),
            Command::Skill(name) => self.skill(&name),
            Command::Model => self.model(),
            Command::ModelNamed(name) => Outcome::Model(name),
            Command::Settings => Outcome::Settings,
            Command::Help => self.screen(Mode::Keys),
            Command::Stats => self.screen(Mode::Stats),
            Command::Quit => Outcome::Quit,
        }
    }

    /// `/new`: the conversation starts again — rows, turn in flight, marker and mark — and the
    /// loaded-skill set goes with the history. A run in flight is superseded and never stopped,
    /// so its session keeps saying `running` and it reaches its own stop reason.
    fn start_over(&mut self) -> Outcome {
        self.rows = Rows::new();
        self.scroll = Scroll::Tail;
        self.stream = Stream::new();
        self.pulse = None;
        self.mark = None;
        self.marked = Span::default();
        self.loaded.clear();
        Outcome::Edited
    }

    /// `/skill:name` and `/skill name` are one line: one skill into the one set the conversation
    /// keeps, discovered afresh. A name put in this set by hand would answer `already_loaded`
    /// to a later model load for a body nothing had ever handed over.
    fn skill(&mut self, name: &str) -> Outcome {
        let Some(workspace) = self.workspace.clone() else {
            return Outcome::Unchanged;
        };
        let catalog = Catalog::discover(&workspace, self.home.as_deref());
        if !catalog.get(name).is_some_and(Skill::human_invocable) {
            return Outcome::Unchanged;
        }
        match self.loaded.load(name) {
            Load::Loaded => Outcome::Edited,
            Load::AlreadyLoaded => Outcome::Unchanged,
        }
    }

    fn model(&mut self) -> Outcome {
        self.models = Models::new();
        self.stack.open(Mode::Models);
        Outcome::Served
    }

    fn screen(&mut self, mode: Mode) -> Outcome {
        self.stack.open(mode);
        Outcome::Edited
    }

    fn choose(&mut self) -> Outcome {
        let Some(name) = self.models.chosen().map(str::to_owned) else {
            return Outcome::Unchanged;
        };
        self.stack.close();
        Outcome::Model(name)
    }

    /// `?`, a key only while the draft is empty: a composer that could not hold a question
    /// mark would be a composer that cannot ask a question.
    fn keys(&mut self, key: Key) -> Outcome {
        if !self.composer.text().is_empty() {
            return self.composer.key(key);
        }
        self.stack.open(Mode::Keys);
        Outcome::Edited
    }

    /// `@`, a key only where it starts a word, for the reason `?` is one only on an empty draft:
    /// a prompt that could not hold `someone@host` cannot name an address. A frame told no
    /// workspace offers no file rather than walking the process's own directory.
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

    fn query(&mut self, key: Key) -> Outcome {
        let changed = match self.stack.top() {
            Mode::Models => self.models.key(key),
            _ => self.picker.key(key),
        };
        match changed {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    fn step(&mut self, step: Step) -> Outcome {
        let moved = match (self.stack.top(), step) {
            (Mode::Models, Step::Previous) => self.models.up(),
            (Mode::Models, Step::Next) => self.models.down(),
            (Mode::Settings, Step::Previous) => self.settings.up(),
            (Mode::Settings, Step::Next) => self.settings.down(),
            (_, Step::Previous) => self.picker.up(),
            (_, Step::Next) => self.picker.down(),
        };
        match moved {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    fn pick(&mut self) -> Outcome {
        let Some(word) = self.picker.word() else {
            return Outcome::Unchanged;
        };
        self.composer.insert(&word);
        self.stack.close();
        Outcome::Edited
    }

    fn value(&mut self, key: Key) -> Outcome {
        match self.settings.key(key) {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// `Space` at the settings modal: a flip on a toggle and a glyph everywhere else, because a
    /// path or a header may hold a space.
    fn flip(&mut self) -> Outcome {
        match self.settings.space() {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// `Ctrl-U` at the settings modal empties the row, and an emptied value is what the save
    /// unsets: the environment speaks for that key again.
    fn clear(&mut self) -> Outcome {
        match self.settings.clear() {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    fn write(&mut self) -> Outcome {
        let touched: Touched = self.settings.touched();
        if touched.is_empty() {
            self.stack.close();
            return Outcome::Edited;
        }
        Outcome::Save(touched)
    }

    fn close(&mut self) -> Outcome {
        let closed = self.stack.top();
        self.stack.close();
        if self.stack.top() == closed {
            Outcome::Unchanged
        } else {
            Outcome::Edited
        }
    }

    fn up(&mut self) -> Outcome {
        if !self.composer.on_first_line() {
            return self.composer.key(Key::Up);
        }
        match self.queue.recall() {
            Some(prompt) => {
                self.composer.recall(prompt);
                Outcome::Edited
            }
            None => Outcome::Unchanged,
        }
    }

    fn drop_newest(&mut self) -> Outcome {
        if self.queue.recall().is_none() {
            return Outcome::Unchanged;
        }
        Outcome::Edited
    }

    pub fn paste(&mut self, text: &str) {
        if self.stack.modal().is_some() {
            return;
        }
        self.composer.paste(text);
    }

    pub fn set_facts(&mut self, facts: Facts) {
        self.rail = facts;
    }

    pub fn set_state(&mut self, state: State) {
        self.composer.set_state(state);
    }

    /// Hands the frame its newest answer's marker. The elapsed time is the caller's, because a
    /// frame holds no clock, and a settled marker keeps the number the run stopped at.
    pub fn set_pulse(&mut self, pulse: Option<Pulse>) {
        self.pulse = pulse;
    }

    /// Paints this frame into `area`. Painting is what tells the row cache how wide the
    /// transcript is, so a resize invalidates it here and the same pass reflows; the
    /// composer's rows are measured when the frame is drawn and never cached.
    pub fn paint(&mut self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, self.palette.ground(None));
        // A screen is the whole frame: the bands under it are not painted, so no draft a key
        // cannot reach is on the screen pretending otherwise.
        if let Some(modal) = self.stack.modal() {
            match modal {
                Mode::Files => self.files_screen(area, buffer),
                Mode::Models => self.models_screen(area, buffer),
                Mode::Settings => self.settings_screen(area, buffer),
                Mode::Stats => self.stats_screen(area, buffer),
                Mode::Composer | Mode::Keys => self.keys_screen(area, buffer),
            }
            return;
        }
        let bands = bands(area, self.composer.rows(), self.queue.rows());
        // The rail's share is taken from the transcript's band before the transcript is laid out
        // in what is left: a row's width is the width it is drawn at.
        let share = self.rail.split(bands.transcript);
        if let Some((shape, rail)) = share.rail {
            self.rail.paint(shape, rail, buffer, &self.palette);
        }
        self.transcript(share.transcript, buffer);
        self.queue_band(bands.queue, buffer);
        self.composer_band(bands.composer, buffer);
        self.footer(bands.footer, buffer);
    }

    /// The turn in flight is the one entry laid out again on every paint; every other row comes
    /// from the window of the cache this band reaches.
    fn transcript(&mut self, area: Rect, buffer: &mut Buffer) {
        self.rows.resize(area.width);
        // How many rows a picture takes is part of the rows an entry was laid out as, so the
        // surface reaches the cache before a window is measured.
        self.rows.retune(self.surface());
        // Laid into the rows it heads rather than painted over them: the answer's first row is
        // wrapped around it, and the prose it introduces is never cut by the mark.
        let head = self.pulse.as_ref().map(Pulse::head);
        self.rows.live(&self.stream, head.as_ref());
        let window = self.rows.window(self.scroll, area.height);
        let mut y = area.y;
        let mut lines = Vec::new();
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
                    &self.palette,
                );
                // Placed after the row's words, because it is drawn over them: a terminal that
                // places no picture leaves the reader the words.
                place(&mut self.pictures, self.cell_size, row, area, y, buffer);
                // Gathered only when there is a mark: a mark's cells are counted in the rows this
                // band draws.
                if self.mark.is_some() {
                    lines.push(row.glyphs());
                }
                y += 1;
            }
        }
        // What this band placed is all the cache keeps: a picture scrolled past is a raster
        // nobody is looking at.
        self.pictures.retain_shown();
        self.marked = match self.mark {
            Some(mark) => Span::of(mark, &lines),
            None => Span::default(),
        };
        self.highlight(area, buffer);
    }

    /// Paints the mark over the rows it covers, from the span `Ctrl-Y` hands out: what is
    /// marked and what leaves are one value.
    fn highlight(&self, area: Rect, buffer: &mut Buffer) {
        for marked in self.marked.rows() {
            let Some(y) = area.y.checked_add(marked.row()) else {
                continue;
            };
            if y >= area.bottom() {
                continue;
            }
            let left = area.x.saturating_add(marked.left());
            let right = left.saturating_add(marked.width()).min(area.right());
            for x in left..right {
                if let Some(cell) = buffer.cell_mut((x, y)) {
                    cell.set_style(Ink::Mark.style(&self.palette));
                }
            }
        }
    }

    /// The queue's row: one line saying how many prompts wait above the composer, and no band
    /// at all while none does, so the transcript takes the row.
    fn queue_band(&self, area: Rect, buffer: &mut Buffer) {
        let Some(label) = self.queue.label() else {
            return;
        };
        write(buffer, area, &[(&label, Ink::Dim)], &self.palette);
    }

    fn composer_band(&self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, self.palette.ground(Some(Band::Composer)));
        let hairline = Rect { height: 1, ..area };
        for x in hairline.x..hairline.right() {
            if let Some(cell) = buffer.cell_mut((x, hairline.y)) {
                cell.set_char(HAIRLINE);
                cell.set_style(Ink::Dim.style(&self.palette));
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
            draft_line(buffer, Rect { y, ..draft }, line, &self.palette);
        }
    }

    /// Where the terminal's own cursor belongs, and nothing when the caret has run past the
    /// band's edge. The caret is never a cell of the frame: an ink names a foreground only, so
    /// a caret drawn as a cell would be invisible on the space it usually sits on.
    pub fn cursor(&self, area: Rect) -> Option<(u16, u16)> {
        // No draft is on screen while a screen is open, so there is no cell for the caret to wait
        // on: it would be a cursor floating over a list.
        if self.stack.modal().is_some() {
            return None;
        }
        let bands = bands(area, self.composer.rows(), self.queue.rows());
        let (row, column) = self.composer.caret();
        let y = bands
            .composer
            .y
            .checked_add(HAIRLINE_ROWS)?
            .checked_add(row)?;
        let x = area.x.checked_add(column)?;
        (column < area.width && y < bands.composer.bottom()).then_some((x, y))
    }

    fn keys_screen(&self, area: Rect, buffer: &mut Buffer) {
        let modes: Vec<Mode> = self.stack.modes().collect();
        let name_width = modes
            .iter()
            .flat_map(|mode| mode.table())
            .map(|binding| binding.name.chars().count() + KEYS_GAP)
            .max()
            .unwrap_or(0);
        let mut y = area.y;
        screen_row(buffer, area, y, &[("? keys", Ink::Accent)], &self.palette);
        for mode in modes {
            y = y.saturating_add(1);
            if y >= area.bottom() {
                return;
            }
            screen_row(buffer, area, y, &[(mode.name(), Ink::Dim)], &self.palette);
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
                    &self.palette,
                );
            }
        }
    }

    /// The `@` picker: the query, then the rows it leaves, the selection marked. A path wider
    /// than the frame is cut at its right edge rather than dropped: whole-or-nothing is a rule
    /// about hints and facts, and a reader wants the path they are pointing at.
    fn files_screen(&self, area: Rect, buffer: &mut Buffer) {
        let mut y = area.y;
        screen_row(
            buffer,
            area,
            y,
            &[(PICKER_TITLE, Ink::Accent)],
            &self.palette,
        );
        y = y.saturating_add(1);
        if y < area.bottom() {
            screen_row(
                buffer,
                area,
                y,
                &[(PICKER_QUERY, Ink::Dim), (self.picker.query(), Ink::Muted)],
                &self.palette,
            );
        }
        let rows = self.picker.rows();
        if rows.is_empty() {
            y = y.saturating_add(1);
            if y < area.bottom() {
                screen_row(buffer, area, y, &[(PICKER_EMPTY, Ink::Dim)], &self.palette);
            }
            return;
        }
        for (at, path) in rows.iter().enumerate() {
            y = y.saturating_add(1);
            if y >= area.bottom() {
                return;
            }
            let (marker, ink) = match at == self.picker.selected() {
                true => (ROW_MARK, Ink::Accent),
                false => (ROW_GAP, Ink::Plain),
            };
            cut_row(
                buffer,
                screen_band(area),
                y,
                &[(marker, ink), (path, ink)],
                &self.palette,
            );
        }
    }

    /// A wire that has not answered is one line saying it was asked, and one that could not
    /// answer is a hint rather than a refusal: a model named by hand still runs.
    fn models_screen(&self, area: Rect, buffer: &mut Buffer) {
        let mut y = area.y;
        screen_row(
            buffer,
            area,
            y,
            &[(MODELS_TITLE, Ink::Accent)],
            &self.palette,
        );
        y = y.saturating_add(1);
        if y < area.bottom() {
            screen_row(
                buffer,
                area,
                y,
                &[(MODELS_QUERY, Ink::Dim), (self.models.query(), Ink::Muted)],
                &self.palette,
            );
        }
        let rows = self.models.rows();
        let said = match self.models.hint() {
            _ if self.models.waiting() => Some(MODELS_ASKING),
            Some(hint) => Some(hint),
            None if rows.is_empty() => Some(MODELS_EMPTY),
            None => None,
        };
        if let Some(said) = said {
            y = y.saturating_add(1);
            if y < area.bottom() {
                screen_row(buffer, area, y, &[(said, Ink::Dim)], &self.palette);
            }
            return;
        }
        let widest = rows
            .iter()
            .map(|(id, _)| id.chars().count() + NAMES_GAP)
            .max()
            .unwrap_or(0);
        for (at, (id, label)) in rows.iter().enumerate() {
            y = y.saturating_add(1);
            if y >= area.bottom() {
                return;
            }
            let (marker, ink) = match at == self.models.selected() {
                true => (ROW_MARK, Ink::Accent),
                false => (ROW_GAP, Ink::Plain),
            };
            let name = format!("{id:<widest$}");
            cut_row(
                buffer,
                screen_band(area),
                y,
                &[(marker, ink), (&name, ink), (label, Ink::Dim)],
                &self.palette,
            );
        }
    }

    /// A value too wide for the frame is cut at its right edge rather than dropped, as the
    /// picker's paths are, and a refusal under the list names the key to change.
    fn settings_screen(&self, area: Rect, buffer: &mut Buffer) {
        let key_width = Config::KEYS
            .iter()
            .map(|key| key.chars().count() + SETTINGS_GAP)
            .max()
            .unwrap_or(0);
        let mut y = area.y;
        screen_row(
            buffer,
            area,
            y,
            &[(SETTINGS_TITLE, Ink::Accent)],
            &self.palette,
        );
        for (at, (key, text)) in self.settings.rows().enumerate() {
            y = y.saturating_add(1);
            if y >= area.bottom() {
                return;
            }
            let (marker, name_ink, value_ink) = match at == self.settings.current() {
                true => (ROW_MARK, Ink::Accent, Ink::Accent),
                false => (ROW_GAP, Ink::Muted, Ink::Plain),
            };
            let name = format!("{:<key_width$}", key);
            cut_row(
                buffer,
                screen_band(area),
                y,
                &[(marker, name_ink), (&name, name_ink), (text, value_ink)],
                &self.palette,
            );
        }
        let Some(refusal) = self.settings.refusal() else {
            return;
        };
        // A blank row under the list, so the refusal reads as a verdict on the visit and not as the
        // last key's value.
        y = y.saturating_add(2);
        if y >= area.bottom() {
            return;
        }
        cut_row(
            buffer,
            screen_band(area),
            y,
            &[(refusal, Ink::Err)],
            &self.palette,
        );
    }

    fn stats_screen(&self, area: Rect, buffer: &mut Buffer) {
        screen_row(
            buffer,
            area,
            area.y,
            &[(STATS_TITLE, Ink::Accent)],
            &self.palette,
        );
        let body = Rect {
            y: area.y.saturating_add(1),
            height: area.height.saturating_sub(1),
            ..area
        };
        self.stats().paint(body, buffer, &self.palette);
    }

    fn stats(&self) -> Stats {
        let Some(document) = &self.record else {
            return Stats::default();
        };
        Stats::of(&self.rail, document, self.loaded.names())
    }

    fn footer(&self, area: Rect, buffer: &mut Buffer) {
        let mut spans: Vec<(&str, Ink)> = Vec::new();
        for (index, hint) in kept(area.width).into_iter().enumerate() {
            if index > 0 {
                spans.push((GAP, Ink::Dim));
            }
            spans.push((hint, Ink::Muted));
        }
        write(buffer, area, &spans, &self.palette);
    }
}

impl Deltas for View {
    fn text(&mut self, fragment: &str) {
        self.stream.text(fragment);
    }

    fn reasoning(&mut self, fragment: &str) {
        self.stream.reasoning(fragment);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Previous,
    Next,
}

struct Bands {
    transcript: Rect,
    queue: Rect,
    composer: Rect,
    footer: Rect,
}

/// Places the picture `row` stands on over the block of cells the transcript reserved. A
/// block the band does not hold whole is not placed at all: a placement writes into every
/// row of it, and half a picture would be drawn over the composer under it.
fn place(
    pictures: &mut Pictures,
    cell: CellSize,
    row: &Row,
    area: Rect,
    y: u16,
    buffer: &mut Buffer,
) {
    let Some(picture) = row.picture() else {
        return;
    };
    let block = Rect {
        x: area.x,
        y,
        width: picture.cols(),
        height: picture.rows(),
    };
    if block.right() > area.right() || block.bottom() > area.bottom() {
        return;
    }
    pictures.place(picture, cell, block, buffer);
}

fn bands(area: Rect, composer_rows: u16, queue_rows: u16) -> Bands {
    let footer = area.height.min(1);
    let above = area.height.saturating_sub(footer);
    let composer = composer_rows.min(above);
    let queue = queue_rows.min(above.saturating_sub(composer));
    let transcript = above.saturating_sub(composer).saturating_sub(queue);
    let queue_y = area.y.saturating_add(transcript);
    Bands {
        transcript: Rect {
            height: transcript,
            ..area
        },
        queue: Rect {
            y: queue_y,
            height: queue,
            ..area
        },
        composer: Rect {
            y: queue_y.saturating_add(queue),
            height: composer,
            ..area
        },
        footer: Rect {
            y: queue_y.saturating_add(queue).saturating_add(composer),
            height: footer,
            ..area
        },
    }
}

/// Writes one row of a screen, each span in its own ink. A span that would run past the
/// frame's right edge is not drawn at all: a screen's row is whole or it is not there.
fn screen_row(buffer: &mut Buffer, area: Rect, y: u16, spans: &[(&str, Ink)], palette: &Palette) {
    write(
        buffer,
        Rect {
            y,
            ..screen_band(area)
        },
        spans,
        palette,
    );
}

fn screen_band(area: Rect) -> Rect {
    let margin = KEYS_MARGIN.min(area.width);
    Rect {
        x: area.x.saturating_add(margin),
        width: area.width.saturating_sub(margin),
        height: 1,
        ..area
    }
}

/// Writes one draft line across a row, cut at its right edge: prose that runs on is cut
/// rather than dropped, where a hint or a fact is whole or not drawn.
fn draft_line(buffer: &mut Buffer, area: Rect, line: &str, palette: &Palette) {
    cut_row(buffer, area, area.y, &[(line, Ink::Plain)], palette);
}

fn cut_row(buffer: &mut Buffer, area: Rect, y: u16, spans: &[(&str, Ink)], palette: &Palette) {
    let mut x = area.x;
    for (text, ink) in spans {
        for glyph in text.chars() {
            if x >= area.right() {
                return;
            }
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_char(glyph);
                cell.set_style(ink.style(palette));
            }
            x += 1;
        }
    }
}

/// The hints a footer `width` columns wide draws, in survival order, stopping at the first that
/// does not fit, gap included: a later and shorter hint is never taken in its place, because the
/// order is the whole of what the table says.
fn kept(width: u16) -> Vec<&'static str> {
    let gap = GAP.chars().count();
    let mut kept: Vec<&'static str> = Vec::new();
    let mut taken = 0;
    for hint in HINTS.iter().take(wanted(width)) {
        let room = hint.chars().count() + if kept.is_empty() { 0 } else { gap };
        if taken + room > usize::from(width) {
            break;
        }
        taken += room;
        kept.push(hint);
    }
    kept
}

fn wanted(width: u16) -> usize {
    STEPS
        .iter()
        .find(|(from, _)| width >= *from)
        .map_or(FLOOR, |(_, hints)| *hints)
}

fn write(buffer: &mut Buffer, area: Rect, spans: &[(&str, Ink)], palette: &Palette) {
    let mut x = area.x;
    for (text, ink) in spans {
        let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
        if x.saturating_add(width) > area.right() {
            return;
        }
        for glyph in text.chars() {
            if let Some(cell) = buffer.cell_mut((x, area.y)) {
                cell.set_char(glyph);
                cell.set_style(ink.style(palette));
            }
            x += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    //! What a golden frame cannot hold: the cell the terminal's cursor is put on, and the bands
    //! under the cells, which a fixture reads by ink and never by background.

    use sindri_core::policy::Enforcement;
    use sindri_core::redact::SecretSet;

    use super::*;
    use crate::ink::Look;

    /// Enough facts that the ledger fills its column top to bottom.
    fn rail() -> Facts {
        Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from("claude-sonnet-4-5-20250929")),
            think: Some(String::from("high")),
            turns: Some((6, 250)),
            ctx: Some(crate::rail::Ctx {
                size: 48_200,
                window: Some(200_000),
            }),
            cache: Some(88),
            workspace: Some(PathBuf::from("/srv/workspace")),
            branch: Some(String::from("spec/1-capability")),
            skills: Some(2),
            sandbox: Some(Enforcement::Bubblewrap),
            session: Some(String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")),
            temp: Some(PathBuf::from("/srv/sessions/9f2c7a41/tmp")),
        }
    }

    /// A band is a panel only because a name is read from a foreground and a modifier.
    #[test]
    fn the_rail_the_composer_and_a_code_well_are_painted_over_their_own_surface() {
        let palette = Palette::TRUECOLOR;
        let mut view = View::of(
            vec![Entry::Assistant {
                turn: 1,
                text: SecretSet::default().redact(
                    "```rust\nfn main() {\n    fence.words();\n}\n```\n\nand the prose under it",
                ),
            }],
            Scroll::Tail,
        );
        view.set_palette(palette);
        view.set_facts(rail());
        view.paste("the draft the composer band is drawn from");

        let area = screen(120, 40);
        let mut buffer = Buffer::empty(area);
        view.paint(area, &mut buffer);

        let cells = |band: Band| -> Vec<(u16, u16)> {
            (area.y..area.bottom())
                .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
                .filter(|(x, y)| {
                    buffer
                        .cell((*x, *y))
                        .is_some_and(|cell| cell.bg == palette.band(band))
                })
                .collect()
        };

        for (band, at_least) in [(Band::Rail, 100), (Band::Composer, 100), (Band::Well, 100)] {
            let on = cells(band);
            assert!(
                on.len() >= at_least,
                "{band:?} paints a surface under its cells, and it covered {} of them",
                on.len()
            );
            for (x, y) in on {
                let cell = buffer.cell((x, y)).expect("a cell of the frame");
                assert!(
                    Look::of(cell, &palette).is_some(),
                    "row {y}, column {x} sits on {band:?} and reads back as no ink: in the \
                     background {:?}, the foreground {:?}",
                    cell.bg,
                    cell.fg
                );
            }
        }

        let last = area.bottom() - 1;
        for x in area.x..area.right() {
            let cell = buffer.cell((x, last)).expect("a cell of the footer");
            assert!(
                [Band::Rail, Band::Composer, Band::Well]
                    .into_iter()
                    .all(|band| cell.bg != palette.band(band)),
                "the footer sits on no band, and column {x} of it sits on one"
            );
        }
    }

    fn screen(cols: u16, rows: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: cols,
            height: rows,
        }
    }

    fn footer(cols: u16, rows: u16) -> String {
        let mut view = View::boot();
        let area = screen(cols, rows);
        let mut buffer = Buffer::empty(area);
        view.paint(area, &mut buffer);
        let y = area.bottom() - 1;
        (area.x..area.right())
            .map(|x| buffer.cell((x, y)).expect("a cell of the frame").symbol())
            .collect()
    }

    #[test]
    fn a_hundred_and_twenty_columns_hold_the_seven_hints_the_table_names() {
        assert_eq!(
            footer(120, 24).trim_end(),
            "Enter send · ? keys · Ctrl-C cancel · Ctrl-Q quit · / commands · @ files · ↑ queue",
            "§57's table gives a 120-column footer seven hints, in survival order and \
             separated by one gap"
        );
    }

    #[test]
    fn five_hints_are_kept_from_a_hundred_columns_and_ninety_nine_keeps_two() {
        assert_eq!(
            footer(100, 24).trim_end(),
            "Enter send · ? keys · Ctrl-C cancel · Ctrl-Q quit · / commands",
            "a hundred columns is the narrowest frame §57 gives five hints, so the two \
             that go are the queue's and the picker's"
        );
        assert_eq!(
            footer(99, 24).trim_end(),
            "Enter send · ? keys",
            "one column under it and the footer is back to the two the floor holds"
        );
    }

    #[test]
    fn sixty_columns_keep_the_two_hints_no_width_takes_away() {
        assert_eq!(
            footer(60, 24).trim_end(),
            "Enter send · ? keys",
            "the floor is the floor at sixty columns as it is at ninety-nine"
        );
    }

    #[test]
    fn no_hint_is_cut_mid_word_at_any_width_between_twenty_and_two_hundred() {
        for width in 20..=200u16 {
            let drawn = footer(width, 24);
            let row = drawn.trim_end();
            let parts: Vec<&str> = if row.is_empty() {
                Vec::new()
            } else {
                row.split(GAP).collect()
            };

            assert_eq!(
                parts.as_slice(),
                &HINTS[..parts.len()],
                "at {width} columns the footer reads {row:?}, which is not a run of whole \
                 hints from the front of the table"
            );
        }
    }

    #[test]
    fn the_first_two_hints_are_drawn_at_every_width_from_twenty_columns() {
        for width in 20..=200u16 {
            let row = footer(width, 24);
            let hints: Vec<&str> = row.trim_end().split(GAP).collect();

            assert_eq!(
                hints.get(..FLOOR),
                Some(&HINTS[..FLOOR]),
                "at {width} columns the footer reads {:?}: the two hints §12 never takes \
                 away are drawn whole at every width, and 60 is not a special case of that",
                row.trim_end()
            );
        }
    }

    #[test]
    fn a_hint_the_row_has_no_room_for_is_not_drawn_at_all() {
        // Narrower than any frame a footer is drawn at, the row wins over the floor: a hint is
        // whole or not drawn, and the gap that would have introduced it goes with it.
        assert_eq!(
            footer(15, 24).trim_end(),
            "Enter send",
            "fifteen columns hold the first hint and neither the gap nor the second"
        );
        assert_eq!(
            footer(8, 24).trim_end(),
            "",
            "eight columns hold no hint at all, which is what the floor gives way to"
        );
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
