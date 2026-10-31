//! The frame's state: the transcript, the composer, the queue and the rail.
//! It knows nothing of the model, the sandbox or the terminal: it is told
//! what happened and it knows how to show it.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::layout::Rect;
use sindri_core::cancel::Cancel;

mod marks;
mod pictures;

use crate::completion::Completion;
use crate::composer::Composer;
use crate::copy::{Clipboard, System};
use crate::driver::Side;
use crate::entry::{Entry, Note};
use crate::face::Face;
use crate::graphics::Graphics;
use crate::heights::Heights;
use crate::overlay::Overlay;
use crate::picture::Store;
use crate::rail::{Launch, Rail, RailView};
use crate::render::seam::Seam;
use crate::selection::{Selection, Viewport};
use crate::settings::Missing;
use crate::window::Window;

/// Where `/new` has got to. Two states would not do: the frame marks the
/// request, the loop hands it to the front end once, and nothing may run
/// until an answer arrives either way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Renew {
    #[default]
    Idle,
    Asked,
    Pending,
}

pub(crate) const DEFAULT_PAGE: usize = 10;

pub struct App {
    pub(crate) rail: Rail,
    pub(crate) entries: Vec<Entry>,
    pub(crate) composer: Composer,
    pub(crate) queue: VecDeque<String>,
    pub(crate) running: Option<Cancel>,
    /// The one flag of the launch: a key or a signal raises it, the
    /// frame puts it down between prompts, and every run is given a
    /// clone.
    pub(crate) cancel: Cancel,
    pub(crate) sent: bool,
    pub(crate) early: Vec<(Side, String)>,
    pub(crate) quit: bool,
    face: Option<Face>,
    /// The glow in the margin: the seam's light strip, built from the same
    /// `graphics` the face and a picture read. A terminal that draws no
    /// graphics has no seam to draw.
    pub(crate) seam: Option<Seam>,
    /// What the terminal said it can draw, asked once at launch: the face draws
    /// from it, and a picture draws from the same answer.
    graphics: Option<Graphics>,
    /// What a picture's `src` is worth, and what it costs: the one thing that
    /// measures a source and reads its file, so a layout asks a store and never
    /// the disk.
    pictures: Option<Store>,
    /// Where the reader is in the thread, and what arrived below them: the
    /// window is the one thing that outlives a frame.
    pub(crate) window: Window,
    pub(crate) viewport: Viewport,
    /// The entry the reader last opened or clicked, which is what `Enter` on
    /// an empty composer toggles: a frame with no cursor of its own has one
    /// honest answer to "which call?", and it is the one just touched.
    pub(crate) chosen: Option<usize>,
    /// One row count per entry at the width it was measured at, dropped for the
    /// entries that change, so the window never lays out a row it does not
    /// show.
    pub(crate) heights: Heights,
    pub(crate) width: Option<u16>,
    pub(crate) selection: Selection,
    /// The window's place the mark's rows were last read against: the mark
    /// covers a place in the thread, so the note a copy leaves at the tail
    /// pushes the marked rows up and the mark has to go with them.
    mark_at: Option<(usize, usize)>,
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
    /// composer says so rather than pretending.
    pub(crate) missing: Option<Missing>,
    pub(crate) rail_view: RailView,
    /// Where the records live: `/export` reads this directory and writes
    /// nothing into it.
    pub(crate) sessions: Option<PathBuf>,
    /// The directory a default export lands in: the workspace the frame was
    /// launched in.
    pub(crate) workspace: Option<PathBuf>,
    /// A path `/export` refused to overwrite, waiting for the second ask.
    pub(crate) export_pending: Option<PathBuf>,
    /// How long the run in flight has been going: the one thing the frame is
    /// told by the clock rather than by the run. It starts at
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
            seam: None,
            graphics: None,
            pictures: None,
            window: Window::default(),
            viewport: Viewport::default(),
            chosen: None,
            heights: Heights::default(),
            width: None,
            selection: Selection::default(),
            mark_at: None,
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

    /// Where the records live: `/export` reads the frame's own
    /// record from it.
    pub fn with_sessions(mut self, root: impl Into<PathBuf>) -> Self {
        self.sessions = Some(root.into());
        self
    }

    /// Where a default `/export` lands: the workspace the frame is in.
    pub fn with_workspace(mut self, dir: impl Into<PathBuf>) -> Self {
        self.workspace = Some(dir.into());
        self.store()
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

    /// The frame the front door opens when the run has no key: the
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
    /// idle frame owes the clock nothing, so the loop is not woken for it.
    pub fn pulse(&mut self, delta: Duration) {
        if self.is_running() {
            self.now += delta;
        }
    }

    pub fn wants_quit(&self) -> bool {
        self.quit
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn completion(&self) -> Option<&Completion> {
        self.completion.as_ref()
    }

    pub fn overlay(&self) -> Option<&Overlay> {
        self.overlay.as_ref()
    }

    pub fn face_mut(&mut self) -> Option<&mut Face> {
        self.face.as_mut()
    }

    /// The terminal's one answer, held by the frame so that the face, the seam
    /// and a picture borrow it rather than asking again.
    pub fn with_graphics(mut self, graphics: Option<Graphics>) -> Self {
        self.seam = graphics.as_ref().and_then(Seam::new);
        self.graphics = graphics;
        self.store()
    }

    pub fn graphics(&self) -> Option<&Graphics> {
        self.graphics.as_ref()
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
                self.heights = Heights::default();
                self.chosen = None;
                self.walking = None;
                self.window = Window::default();
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

    /// How many rows entry `index` takes at `width`: the store's answer, kept
    /// per entry at the width it was measured at, so the same question is asked
    /// once and never costs a file read, while a resize is measured again from
    /// the store.
    pub fn measure(&mut self, index: usize, width: u16) -> usize {
        if let Some(rows) = self.heights.at(index, width) {
            return rows;
        }
        let rows = crate::render::rows::entry_height(
            &self.entries[index],
            width,
            self.now,
            self.pictures.as_ref(),
        );
        self.heights.keep(index, width, rows);
        rows
    }

    /// How many heights have been measured from scratch over the frame's whole
    /// life: a frame reports the difference as its own.
    pub(crate) fn measured(&self) -> usize {
        self.heights.measured()
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
}
