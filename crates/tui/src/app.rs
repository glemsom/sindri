//! The frame's state: the transcript, the composer, the queue and the rail. It
//! knows nothing of the model, the sandbox or the terminal: it is told what
//! happened and it knows how to show it (ADR-0011 §1).
//!
//! The hands — what a key or a mouse does to this state — live in [`keys`], so
//! this file stays a description of what the frame holds.

use std::collections::VecDeque;
use std::path::PathBuf;

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

/// A screenful is the default page, before a frame has said how tall it is.
const DEFAULT_PAGE: usize = 10;

/// Everything the frame draws, and everything it has been told.
pub struct App {
    pub(crate) rail: Rail,
    pub(crate) entries: Vec<Entry>,
    pub(crate) composer: Composer,
    /// Prompts typed while a run was in flight, in the order they were typed.
    pub(crate) queue: VecDeque<String>,
    /// The flag of the run in flight, if there is one.
    pub(crate) running: Option<Cancel>,
    /// Whether Enter has been pressed on the draft since it was last started:
    /// typing alone never launches a run.
    pub(crate) sent: bool,
    /// Output that arrived before the call it belongs to was announced.
    pub(crate) early: Vec<(Side, String)>,
    pub(crate) quit: bool,
    face: Option<Face>,
    /// How many rows of the thread are scrolled past above the window.
    pub(crate) scroll: usize,
    /// One screenful: what `PgUp` and `PgDn` move by.
    pub(crate) page: usize,
    /// What the transcript column last drew: what a selection resolves against.
    pub(crate) viewport: Viewport,
    /// One row count per entry at the width it was measured at, dropped
    /// whenever an entry changes, so the window never lays out a row it does
    /// not show (ADR-0011 §4).
    pub(crate) heights: Vec<Option<usize>>,
    /// The width `heights` was measured at.
    pub(crate) width: Option<u16>,
    /// The drag in progress, if any.
    pub(crate) selection: Selection,
    /// Where a copy goes: the terminal, the shell, or nowhere.
    pub(crate) clipboard: Box<dyn Clipboard>,
    /// The `@` completion being offered, if any.
    pub(crate) completion: Option<Completion>,
    /// The overlay that has the screen, if any.
    pub(crate) overlay: Option<Overlay>,
    /// The completion whose walk is still running, if any.
    pub(crate) walking: Option<(u64, String)>,
    /// Whether `/new` has asked the front end for a fresh session.
    pub(crate) renew: bool,
    /// How many rows a keyboard selection spans, from the newest row upwards.
    pub(crate) selecting: usize,
    /// A counter for the walks that answer a completion: a stale answer is
    /// never applied to a question that has moved on.
    pub(crate) generation: u64,
    /// The config file `/settings` edits. `None` is the file a run reads; a
    /// test points it at a temporary directory instead.
    pub(crate) config: Option<PathBuf>,
    /// Whether a saved setting is waiting for the front end to take it up,
    /// which is how a change applies without a restart (ADR-0014).
    pub(crate) reload: bool,
}

impl App {
    /// A frame that has run nothing yet, with the face when the terminal can
    /// show it.
    pub fn new(launch: Launch, face: Option<Face>) -> Self {
        App::with_clipboard(launch, face, Box::new(System::detect()))
    }

    /// The same frame, with a clipboard of its own: a test never reaches the
    /// user's real one (ADR-0013).
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
            sent: false,
            early: Vec::new(),
            quit: false,
            face,
            scroll: 0,
            page: DEFAULT_PAGE,
            viewport: Viewport::default(),
            heights: Vec::new(),
            width: None,
            selection: Selection::default(),
            clipboard,
            completion: None,
            overlay: None,
            walking: None,
            renew: false,
            selecting: 0,
            generation: 0,
            config: None,
            reload: false,
        }
    }

    /// The same frame, editing the config file at `path`: the seam a test uses
    /// so that no test can write a real user's config (ADR-0014).
    pub fn with_config(mut self, path: impl Into<PathBuf>) -> Self {
        self.config = Some(path.into());
        self
    }

    /// The session GUID the rail is showing.
    pub fn rail_guid(&self) -> String {
        self.rail.session()
    }

    /// The transcript, in the order it happened.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The rail's facts.
    pub fn rail(&self) -> &Rail {
        &self.rail
    }

    /// The composer being typed into.
    pub fn composer(&self) -> &Composer {
        &self.composer
    }

    /// How many prompts are waiting for the run in flight to end.
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Whether a run is in flight.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Whether the user asked to leave.
    pub fn wants_quit(&self) -> bool {
        self.quit
    }

    /// How far up the thread the window is scrolled.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// The drag in progress, or the last one.
    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// The completion on offer, if any.
    pub fn completion(&self) -> Option<&Completion> {
        self.completion.as_ref()
    }

    /// The overlay that has the screen, if any.
    pub fn overlay(&self) -> Option<&Overlay> {
        self.overlay.as_ref()
    }

    /// The same, to draw: a spill's scroll and its window belong to the frame.
    pub fn overlay_mut(&mut self) -> Option<&mut Overlay> {
        self.overlay.as_mut()
    }

    /// What the transcript column last drew.
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// The face, when the terminal can show one.
    pub fn face_mut(&mut self) -> Option<&mut Face> {
        self.face.as_mut()
    }

    /// The walk a frame still has to start: the completion it answers, and the
    /// fragment to rank. Taken once, so no keystroke starts two walks.
    pub fn take_walk(&mut self) -> Option<(u64, String)> {
        self.walking.take()
    }

    /// Whether `/new` is waiting for a fresh session from the front end.
    pub fn take_renew(&mut self) -> bool {
        std::mem::take(&mut self.renew)
    }

    /// Whether a saved setting is waiting for the front end to read it again.
    pub fn take_reload(&mut self) -> bool {
        std::mem::take(&mut self.reload)
    }

    /// The front end has read the saved settings: the rail shows the limits the
    /// next run will actually use, and a config that cannot start a run is said
    /// where it happened (ADR-0011 §15).
    pub fn reloaded(&mut self, reloaded: Result<Launch, String>) {
        match reloaded {
            Ok(launch) => self.rail.reconfigured(launch),
            Err(error) => self.note_error(format!(
                "settings saved, but the next run cannot start: {error}"
            )),
        }
    }

    /// What one background walk found, for the completion it was started for.
    /// A walk whose completion has already moved on is not applied.
    pub fn paths(&mut self, generation: u64, paths: Vec<String>, capped: bool) {
        if let Some(completion) = self.completion.as_mut()
            && completion.generation == generation
        {
            completion.items = crate::completion::offered(&paths, &completion.typed);
            completion.capped = capped;
        }
    }

    /// A fresh session from the front end: the rail is replaced, the transcript
    /// is already empty, and the new GUID is said where it happened.
    pub fn renewed(&mut self, launch: Option<Launch>) {
        match launch {
            Some(launch) => {
                let session = launch.session.clone();
                self.rail = Rail::new(launch);
                self.note_info(format!("new session {session}"));
            }
            None => self.note_error("this front end cannot start a new session".to_string()),
        }
    }

    /// A line the frame has to say, said in the transcript (ADR-0011 §15).
    pub fn note_info(&mut self, text: impl Into<String>) {
        self.entries.push(Entry::Note(Note::info(text)));
        self.dirty();
    }

    /// A failure the frame has to say, said where it happened.
    pub fn note_error(&mut self, text: impl Into<String>) {
        self.entries.push(Entry::Note(Note::error(text)));
        self.dirty();
    }

    /// The rows the window must build for, at this width.
    pub(crate) fn note_width(&mut self, width: u16) {
        if self.width != Some(width) {
            self.width = Some(width);
            self.dirty();
        }
    }

    /// How many rows one entry takes at `width`, laid out once and remembered.
    pub(crate) fn measure(&mut self, index: usize, width: u16) -> usize {
        if self.heights.len() < self.entries.len() {
            self.heights.resize(self.entries.len(), None);
        }
        if let Some(Some(rows)) = self.heights.get(index) {
            return *rows;
        }
        let rows = crate::render::transcript::entry_rows(&self.entries[index], width).len();
        if let Some(slot) = self.heights.get_mut(index) {
            *slot = Some(rows);
        }
        rows
    }

    /// The window adopted its own clamped scroll: a thread has an end, and
    /// scrolling past it shows nothing new.
    pub(crate) fn settled(&mut self, scroll: usize, page: usize) {
        self.scroll = scroll;
        self.page = page.max(1);
    }

    /// What the transcript column drew, for the mouse and the clipboard.
    pub(crate) fn set_viewport(
        &mut self,
        area: Rect,
        rows: Vec<String>,
        spill: Vec<(usize, std::path::PathBuf)>,
    ) {
        self.viewport = Viewport { area, rows, spill };
    }

    /// Every entry changed: the row counts are stale. Cleared rather than
    /// recomputed, because only the window's own entries are ever measured.
    pub(crate) fn dirty(&mut self) {
        self.heights.fill(None);
    }
}
