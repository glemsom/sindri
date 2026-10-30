//! The frame's state: the transcript, the composer, the queue and the rail.
//! It knows nothing of the model, the sandbox or the terminal: it is told
//! what happened and it knows how to show it (ADR-0011).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::layout::Rect;
use sindri_core::cancel::Cancel;

use crate::completion::Completion;
use crate::composer::Composer;
use crate::copy::{Clipboard, System};
use crate::driver::Side;
use crate::entry::{Entry, Note};
use crate::face::Face;
use crate::overlay::Overlay;
use crate::rail::{Launch, Rail};
use crate::selection::{Selection, Viewport};
use crate::settings::Missing;

/// Where `/new` has got to. Two states would not do: the frame marks the
/// request, the loop hands it to the front end once, and nothing may run
/// until an answer arrives either way (ADR-0015).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Renew {
    #[default]
    Idle,
    Asked,
    Pending,
}

const DEFAULT_PAGE: usize = 10;

/// The rows the rail last drew, so a click on one copies what was drawn there.
/// The session id is the fact a user takes out of the frame by hand — it is
/// what `sindri sessions show` takes — and a wrapped id is copied whole, never
/// half of it (ADR-0006, issue #51).
#[derive(Debug, Clone, Default)]
pub struct RailView {
    pub area: Rect,
    pub rows: Vec<String>,
    /// Where the session fact sits in `rows`.
    pub session: std::ops::Range<usize>,
}

impl RailView {
    pub fn row_at(&self, at: ratatui::layout::Position) -> Option<usize> {
        self.area
            .contains(at)
            .then(|| (at.y - self.area.y) as usize)
            .filter(|row| *row < self.rows.len())
    }
}

pub struct App {
    pub(crate) rail: Rail,
    pub(crate) entries: Vec<Entry>,
    pub(crate) composer: Composer,
    pub(crate) queue: VecDeque<String>,
    pub(crate) running: Option<Cancel>,
    /// The one flag of the launch: a key or a signal raises it, the
    /// frame puts it down between prompts, and every run is given a
    /// clone (ADR-0015).
    pub(crate) cancel: Cancel,
    pub(crate) sent: bool,
    pub(crate) early: Vec<(Side, String)>,
    pub(crate) quit: bool,
    face: Option<Face>,
    pub(crate) scroll: usize,
    /// The rows one page is worth to whatever has the screen. The transcript
    /// reports its own every frame; an open overlay reports what its box
    /// showed instead, so `PgUp`/`PgDn` move the box that covers the frame and
    /// never the frame beneath it (ADR-0013, issue #49).
    pub(crate) page: usize,
    pub(crate) viewport: Viewport,
    /// The entry the reader last opened or clicked, which is what `Enter` on
    /// an empty composer toggles: a frame with no cursor of its own has one
    /// honest answer to "which call?", and it is the one just touched
    /// (ADR-0011 §9, issue #44).
    pub(crate) chosen: Option<usize>,
    /// One row count per entry at the width it was measured at, dropped
    /// whenever an entry changes, so the window never lays out a row it does
    /// not show (ADR-0011).
    pub(crate) heights: Vec<Option<usize>>,
    pub(crate) width: Option<u16>,
    pub(crate) selection: Selection,
    pub(crate) clipboard: Box<dyn Clipboard>,
    pub(crate) completion: Option<Completion>,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) walking: Option<(u64, String)>,
    pub(crate) renew: Renew,
    pub(crate) selecting: usize,
    pub(crate) generation: u64,
    pub(crate) config: Option<PathBuf>,
    pub(crate) reload: bool,
    /// What the front door found absent when it opened the frame itself: the
    /// run it opened for cannot send a prompt until it is filled in, and the
    /// composer says so rather than pretending (ADR-0004).
    pub(crate) missing: Option<Missing>,
    pub(crate) rail_view: RailView,
    /// Where the records live: `/sessions` and `/export` read this directory
    /// and write nothing into it (ADR-0006).
    pub(crate) sessions: Option<PathBuf>,
    /// The directory a default export lands in: the workspace the frame was
    /// launched in.
    pub(crate) workspace: Option<PathBuf>,
    /// A path `/export` refused to overwrite, waiting for the second ask.
    pub(crate) export_pending: Option<PathBuf>,
    /// How long the run in flight has been going: the one thing the frame is
    /// told by the clock rather than by the run (ADR-0011 §4). It starts at
    /// zero with every prompt and stops when the run does.
    pub(crate) now: Duration,
}

impl App {
    pub fn new(launch: Launch, face: Option<Face>) -> Self {
        App::with_clipboard(launch, face, Box::new(System::detect()))
    }

    pub fn with_clipboard(
        launch: Launch,
        face: Option<Face>,
        clipboard: Box<dyn Clipboard>,
    ) -> Self {
        App {
            rail: Rail::new(launch),
            entries: Vec::new(),
            composer: Composer::new(),
            queue: VecDeque::new(),
            running: None,
            cancel: Cancel::new(),
            sent: false,
            early: Vec::new(),
            quit: false,
            face,
            scroll: 0,
            page: DEFAULT_PAGE,
            viewport: Viewport::default(),
            chosen: None,
            heights: Vec::new(),
            width: None,
            selection: Selection::default(),
            clipboard,
            completion: None,
            overlay: None,
            walking: None,
            renew: Renew::Idle,
            selecting: 0,
            generation: 0,
            config: None,
            reload: false,
            missing: None,
            rail_view: RailView::default(),
            sessions: None,
            workspace: None,
            export_pending: None,
            now: Duration::ZERO,
        }
    }

    pub fn with_cancel(mut self, cancel: Cancel) -> Self {
        self.cancel = cancel;
        self
    }

    /// Where the records live (ADR-0006): `/sessions` lists it, `/export`
    /// reads the frame's own record from it.
    pub fn with_sessions(mut self, root: impl Into<PathBuf>) -> Self {
        self.sessions = Some(root.into());
        self
    }

    /// Where a default `/export` lands: the workspace the frame is in.
    pub fn with_workspace(mut self, dir: impl Into<PathBuf>) -> Self {
        self.workspace = Some(dir.into());
        self
    }

    /// The workspace, asked of the process only when the frame was never told
    /// one: an export must land somewhere it can name.
    pub(crate) fn workspace_dir(&self) -> PathBuf {
        self.workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn with_config(mut self, path: impl Into<PathBuf>) -> Self {
        self.config = Some(path.into());
        self
    }

    /// The frame the front door opens when the run has no key (ADR-0004): the
    /// settings modal is staged on the row it has nothing for, the transcript
    /// says what is absent once, and the composer says what it cannot do until
    /// the modal is saved. A run that stops for a missing key never reaches a
    /// card or a line on stderr — a card has no field to type in.
    pub fn with_missing(mut self, missing: Missing) -> Self {
        self.missing = Some(missing);
        self.note_error(missing.said());
        self.open_settings(Some(missing));
        self
    }

    pub fn rail_guid(&self) -> String {
        self.rail.session()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn rail(&self) -> &Rail {
        &self.rail
    }

    pub fn composer(&self) -> &Composer {
        &self.composer
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// The run's clock: how long it has been going, or how long it went.
    pub fn now(&self) -> Duration {
        self.now
    }

    /// Lets the clock on by `delta`. Only a live run is told the time: an
    /// idle frame owes the clock nothing, so the loop is not woken for it
    /// (ADR-0011 §4).
    pub fn pulse(&mut self, delta: Duration) {
        if self.is_running() {
            self.now += delta;
        }
    }

    pub fn wants_quit(&self) -> bool {
        self.quit
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn completion(&self) -> Option<&Completion> {
        self.completion.as_ref()
    }

    pub fn overlay(&self) -> Option<&Overlay> {
        self.overlay.as_ref()
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn face_mut(&mut self) -> Option<&mut Face> {
        self.face.as_mut()
    }

    pub fn take_walk(&mut self) -> Option<(u64, String)> {
        self.walking.take()
    }

    pub fn take_renew(&mut self) -> bool {
        if self.renew == Renew::Asked {
            self.renew = Renew::Pending;
            return true;
        }
        false
    }

    pub fn take_reload(&mut self) -> bool {
        std::mem::take(&mut self.reload)
    }

    pub fn reloaded(&mut self, reloaded: Result<Launch, String>) {
        match reloaded {
            Ok(launch) => self.rail.reconfigured(launch),
            Err(error) => self.note_error(format!(
                "settings saved, but the next run cannot start: {error}"
            )),
        }
    }

    pub fn paths(&mut self, generation: u64, paths: Vec<String>, capped: bool) {
        if let Some(completion) = self.completion.as_mut()
            && completion.generation == generation
        {
            completion.items = crate::completion::offered(&paths, &completion.typed);
            completion.capped = capped;
        }
    }

    pub fn renewed(&mut self, renewed: Result<Launch, String>) {
        match renewed {
            Ok(launch) => {
                self.renew = Renew::Idle;
                let session = launch.session.clone();
                self.entries.clear();
                self.queue.clear();
                self.early.clear();
                self.heights.clear();
                self.chosen = None;
                self.walking = None;
                self.scroll = 0;
                self.rail = Rail::new(launch);
                // A refused export was a refusal about the thread that asked:
                // a new thread must ask again before it replaces that file.
                self.export_pending = None;
                self.dirty();
                self.note_info(format!("new session {session}"));
            }
            Err(error) => {
                self.renew = Renew::Idle;
                self.note_error(error);
            }
        }
    }

    pub fn note_info(&mut self, text: impl Into<String>) {
        self.entries.push(Entry::Note(Note::info(text)));
        self.dirty();
    }

    pub fn note_error(&mut self, text: impl Into<String>) {
        self.entries.push(Entry::Note(Note::error(text)));
        self.dirty();
    }

    pub(crate) fn note_width(&mut self, width: u16) {
        if self.width != Some(width) {
            self.width = Some(width);
            self.dirty();
        }
    }

    pub(crate) fn measure(&mut self, index: usize, width: u16) -> usize {
        if self.heights.len() < self.entries.len() {
            self.heights.resize(self.entries.len(), None);
        }
        if let Some(Some(rows)) = self.heights.get(index) {
            return *rows;
        }
        let rows =
            crate::render::transcript::entry_rows(&self.entries[index], width, self.now).len();
        if let Some(slot) = self.heights.get_mut(index) {
            *slot = Some(rows);
        }
        rows
    }

    pub(crate) fn settled(&mut self, scroll: usize, page: usize) {
        self.scroll = scroll;
        self.page = page.max(1);
    }

    pub(crate) fn set_rail_view(
        &mut self,
        area: Rect,
        rows: Vec<String>,
        session: std::ops::Range<usize>,
    ) {
        self.rail_view = RailView {
            area,
            rows,
            session,
        };
    }

    pub(crate) fn set_viewport(
        &mut self,
        area: Rect,
        rows: Vec<String>,
        spill: Vec<(usize, std::path::PathBuf)>,
        expand: Vec<(usize, usize)>,
    ) {
        self.viewport = Viewport {
            area,
            rows,
            spill,
            expand,
        };
    }

    pub(crate) fn dirty(&mut self) {
        self.heights.fill(None);
    }
}
