//! The frame's state: the transcript, the composer, the queue and the rail.
//! It knows nothing of the model, the sandbox or the terminal: it is told
//! what happened and it knows how to show it.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::layout::Rect;
use sindri_core::cancel::Cancel;
use sindri_core::status::{Doing, State};

mod marks;
mod pictures;
mod skill;
mod spill;

use crate::completion::Completion;
use crate::composer::Composer;
use crate::copy::{Clipboard, System};
use crate::driver::Side;
use crate::entry::{Entry, Note};
use crate::face::Face;
use crate::graphics::Graphics;
use crate::overlay::Overlay;
use crate::picture::Store;
use crate::rail::{Launch, Rail, RailView};
use crate::render::seam::Seam;
use crate::selection::{Selection, Viewport};
use crate::settings::{Missing, Served, Serving};
use crate::thread::Thread;
use crate::window::Window;

pub use self::skill::Loading;

use self::spill::Spilling;

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
    pub(crate) thread: Thread,
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
    /// Where the skill the reader asked for has got to, and the prompt they
    /// wrote after it: a load is a task of its own, so the turn it precedes waits
    /// until the body is in the conversation it will be sent with. `armed` is
    /// what runs once the answer lands.
    pub(crate) loading: Loading,
    pub(crate) armed: Option<String>,
    /// Where the spill the reader asked for has got to: the file a thread is
    /// reading, so a key pressed while it is out is the same ask.
    spilling: Spilling,
    pub(crate) selecting: usize,
    pub(crate) generation: u64,
    pub(crate) config: Option<PathBuf>,
    pub(crate) reload: bool,
    /// What the front door found absent when it opened the frame itself: the
    /// run it opened for cannot send a prompt until it is filled in, and the
    /// composer says so rather than pretending.
    pub(crate) missing: Option<Missing>,
    /// The names the wire serves, what they answer for and whether the ask is
    /// still out: a fact about the wire rather than a draft of the modal, so it
    /// outlives the modal and the screen that state it.
    pub(crate) served: Served,
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
    /// What the run is doing, in the five words a report may carry: set by the
    /// event the run last sent, and `Thinking` again for every prompt.
    pub(crate) doing: Doing,
    /// The state the last run ended in, until the next one begins: one of
    /// `done`, `error` and `idle`, decided by `Stopped` and by nothing else.
    /// `None` before the first run, because a frame that has started none has
    /// nothing to tell its terminal.
    pub(crate) settled: Option<State>,
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
            thread: Thread::default(),
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
            width: None,
            selection: Selection::default(),
            mark_at: None,
            clipboard,
            completion: None,
            overlay: None,
            walking: None,
            renew: Renew::Idle,
            loading: Loading::Idle,
            armed: None,
            spilling: Spilling::default(),
            selecting: 0,
            generation: 0,
            config: None,
            reload: false,
            missing: None,
            served: Served::default(),
            rail_view: RailView::default(),
            sessions: None,
            workspace: None,
            export_pending: None,
            now: Duration::ZERO,
            doing: Doing::Thinking,
            settled: None,
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
        self.note_error(missing.said());
        self.open_settings(Some(missing.clone()));
        self.missing = Some(missing);
        self
    }

    pub fn rail_guid(&self) -> String {
        self.rail.session()
    }

    /// The entries the thread is holding, oldest first.
    pub fn entries(&self) -> &[Entry] {
        self.thread.entries()
    }

    /// The thread itself: its entries, and the heights they were measured at.
    pub fn thread(&self) -> &Thread {
        &self.thread
    }

    /// How many rows entry `index` takes at `width`: the thread's answer, given
    /// the clock and the store a height depends on — read from the fields
    /// because the thread is borrowed mutably for the same call.
    pub fn measure(&mut self, index: usize, width: u16) -> usize {
        let now = self.now;
        let pictures = self.pictures.as_ref();
        self.thread.measure(index, width, now, pictures)
    }

    /// How many heights have been measured from scratch over the frame's whole
    /// life: a frame reports the difference as its own.
    pub(crate) fn measured(&self) -> usize {
        self.thread.measured()
    }

    /// How many rows have been read to place the thread so far: the frame
    /// reports the difference as the rows a delta cost it.
    pub(crate) fn counted(&self) -> usize {
        self.thread.counted()
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

    /// Whether this frame asks for a tick: a run in flight, and nothing else. A
    /// still frame asks for none — an idle composer owes the clock nothing, and
    /// a thought that has ended in place asks for nothing either.
    pub fn ticks(&self) -> bool {
        self.running.is_some()
    }

    /// Lets the clock on by `delta`, and does nothing else with it. Only a frame
    /// that asked for a tick is told the time, so a still frame's clock never
    /// runs.
    pub fn pulse(&mut self, delta: Duration) {
        self.now += delta;
    }

    /// What this frame is doing, as the one thing a terminal is told: a run in
    /// flight is working on what its last event named, a staged missing key is
    /// the one blockage sindri has, and a still frame is whatever its last run
    /// settled in. The clock is the frame's own, so a report says how long the
    /// work has taken rather than asking the run again.
    pub fn status(&self) -> State {
        if self.running.is_some() {
            return State::Working {
                doing: self.doing,
                turn: self.rail.turns(),
                elapsing: self.now,
            };
        }
        if self.awaiting_key() {
            return State::Blocked;
        }
        self.settled.unwrap_or(State::Idle)
    }

    /// Whether the modal the front door staged is still asking for a key: a run
    /// with no credential cannot answer, so the frame says it waits for a human
    /// rather than that it is thinking. The modal going takes the blockage with
    /// it — a human who has typed the key, or dismissed the ask, is owed the
    /// word that nothing is running.
    fn awaiting_key(&self) -> bool {
        matches!(&self.overlay, Some(Overlay::Settings(settings)) if settings.missing().is_some())
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

    /// The ask the modal or the model screen made, for the loop to carry to a
    /// thread: the row and the screen are fields while it is out, because
    /// neither waits for anything.
    pub fn take_ask(&mut self) -> Option<Serving> {
        self.served.take()
    }

    /// What a thread made of the ask, and whether it drew anything: a listing
    /// for a wire or base URL the launch has left is dropped where it lands,
    /// and the row it would have said something on says what it said before.
    pub fn listed(&mut self, serving: Serving, listed: Result<Vec<String>, String>) -> bool {
        if !self.served.landed(serving, listed) {
            return false;
        }
        // The two places a listing is drawn are the modal's model row and the
        // model screen: with neither open nothing on screen moved, and nothing
        // is laid out again for it. Open, each ranks the names that just landed
        // against the fragment it already holds, so a fragment typed while the
        // ask was out draws its list without waiting for a keystroke.
        match self.overlay.as_mut() {
            Some(Overlay::Settings(settings)) => settings.refresh(&self.served),
            Some(Overlay::Model(model)) => model.refresh(&self.served),
            _ => return false,
        }
        true
    }

    /// The names the wire serves, what they answer for and whether the ask is
    /// still out: what the model row states, what the `/model` screen lists, and
    /// what a completion offers.
    pub fn served(&self) -> &Served {
        &self.served
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

    /// The launch a save asked for, and whether the rail heard it: a refusal is
    /// a line in the thread, and a launch is the rail's own facts rewritten.
    pub fn reloaded(&mut self, reloaded: Result<Launch, String>) -> bool {
        match reloaded {
            Ok(launch) => self.rail.reconfigured(launch),
            Err(error) => self.note_error(format!(
                "settings saved, but the next run cannot start: {error}"
            )),
        }
        true
    }

    /// A walk of the workspace answering, and whether the offer standing in the
    /// composer is the one that asked: a generation the frame is not holding
    /// belongs to a draft that has moved on, and its paths are nothing to draw.
    pub fn paths(&mut self, generation: u64, paths: Vec<String>, capped: bool) -> bool {
        let Some(completion) = self.completion.as_mut() else {
            return false;
        };
        if completion.generation != generation {
            return false;
        }
        completion.items = crate::completion::offered(&paths, &completion.typed);
        completion.capped = capped;
        true
    }

    /// The launch `/new` asked for: either way the frame says something, a
    /// thread cleared or the reason it was not.
    pub fn renewed(&mut self, renewed: Result<Launch, String>) -> bool {
        match renewed {
            Ok(launch) => {
                self.renew = Renew::Idle;
                let session = launch.session.clone();
                self.loading = Loading::Idle;
                self.armed = None;
                self.spilling = Spilling::default();
                self.thread.clear();
                self.queue.clear();
                self.early.clear();
                self.chosen = None;
                self.walking = None;
                self.window = Window::default();
                self.rail = Rail::new(launch);
                // A refused export was a refusal about the thread that asked:
                // a new thread must ask again before it replaces that file.
                self.export_pending = None;
                self.note_info(format!("new session {session}"));
            }
            Err(error) => {
                self.renew = Renew::Idle;
                self.note_error(error);
            }
        }
        true
    }

    pub fn note_info(&mut self, text: impl Into<String>) {
        self.thread.push(Entry::Note(Note::info(text)));
    }

    pub fn note_error(&mut self, text: impl Into<String>) {
        self.thread.push(Entry::Note(Note::error(text)));
    }

    pub(crate) fn note_width(&mut self, width: u16) {
        if self.width != Some(width) {
            self.width = Some(width);
            self.thread.forget();
        }
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
