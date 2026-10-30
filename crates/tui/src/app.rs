//! The frame's state: the transcript, the composer, the queue and the rail.
//! It knows nothing of the model, the sandbox or the terminal: it is told
//! what happened and it knows how to show it (ADR-0011).

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
    pub(crate) page: usize,
    pub(crate) viewport: Viewport,
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
        }
    }

    pub fn with_cancel(mut self, cancel: Cancel) -> Self {
        self.cancel = cancel;
        self
    }

    pub fn with_config(mut self, path: impl Into<PathBuf>) -> Self {
        self.config = Some(path.into());
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

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
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

    pub fn overlay_mut(&mut self) -> Option<&mut Overlay> {
        self.overlay.as_mut()
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
                self.walking = None;
                self.scroll = 0;
                self.rail = Rail::new(launch);
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
        let rows = crate::render::transcript::entry_rows(&self.entries[index], width).len();
        if let Some(slot) = self.heights.get_mut(index) {
            *slot = Some(rows);
        }
        rows
    }

    pub(crate) fn settled(&mut self, scroll: usize, page: usize) {
        self.scroll = scroll;
        self.page = page.max(1);
    }

    pub(crate) fn set_viewport(
        &mut self,
        area: Rect,
        rows: Vec<String>,
        spill: Vec<(usize, std::path::PathBuf)>,
    ) {
        self.viewport = Viewport { area, rows, spill };
    }

    pub(crate) fn dirty(&mut self) {
        self.heights.fill(None);
    }
}
