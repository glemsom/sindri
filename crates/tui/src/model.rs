//! `/model`: the names the wire the run speaks offers, on a screen of their own.
//! It is a screen and not the modal, so it carries no field: every name on it is
//! one the wire named, ranked by the one ranking the model row already states —
//! every name while nothing is typed, and the names a fragment reaches once one
//! is — so nothing here is a second list of what a wire can be asked for. The
//! name `Enter` takes is written to the config file through the document the
//! modal writes, and the frame is told to read that file again, so the next
//! prompt runs on it without a restart. A name the wire never offered is
//! `/model <name>`, the same write without the screen, and `Esc` is the one key
//! that closes it and writes nothing. A wire that cannot answer still opens the
//! screen: the rows the names would fill say why there are none, because an
//! empty screen would say nothing.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_core::config::{Document, Key};

use crate::overlay::Overlay;
use crate::settings::{List, Served, Serving};

/// What one key asked of the frame: a screen is a choice, so the four things it
/// can ask for are to stay, to close, to take a name, and the two the frame owns
/// wherever an overlay has the screen. `Held` is the key it holds nothing for,
/// which is the one answer that leaves the frame as it was.
#[derive(Debug, Clone)]
pub enum Answer {
    Staying,
    Held,
    Close,
    Take(String),
    Quit,
    CancelRun,
}

#[derive(Debug, Clone)]
pub struct Model {
    document: Document,
    /// The model this run asks: the name the cursor opens on, so `Enter` without
    /// a keystroke before it writes back what the run already holds.
    current: String,
    /// The fragment typed on the screen: what the names are ranked from, and the
    /// one row the screen states it on.
    filter: String,
    /// The names, and which of them the cursor is on. The screen stands on every
    /// name the wire offered, and a keystroke narrows it — the model row's own
    /// `List`, so the ranking and the window are the two the modal draws.
    list: List,
    error: Option<String>,
}

impl Model {
    /// The screen over the config file — at the frame's own path where it was
    /// pointed at one — with its cursor on the model this run asks.
    pub fn open(path: Option<&Path>, current: &str) -> Result<Model, String> {
        crate::settings::document(path).map(|document| Model::at(document, current))
    }

    fn at(document: Document, current: &str) -> Model {
        Model {
            document,
            current: current.to_string(),
            filter: String::new(),
            list: List::default(),
            error: None,
        }
    }

    pub fn path(&self) -> &Path {
        self.document.path()
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn list(&self) -> &List {
        &self.list
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The names the wire named, ranked by the fragment typed on the screen. The
    /// cursor lands on the run's own name the first time there are names to land
    /// on — a listing that lands after the screen opened is what fills it — and
    /// every frame after that is the ranking a keystroke moved.
    pub(crate) fn refresh(&mut self, served: &Served) {
        let names = served.names();
        if !self.list.folded() && !names.is_empty() {
            self.list.choice(names, &self.current);
        }
        self.list.refresh(names, &self.filter);
    }

    /// The name written to the config file: the one writer the modal saves
    /// through, so the file `/model` writes is the file `/settings` writes.
    pub fn set(&mut self, name: &str) -> Result<(), String> {
        self.document = crate::settings::write(&self.document, Key::Model, name)?;
        self.error = None;
        Ok(())
    }

    pub fn failed(&mut self, message: String) {
        self.error = Some(message);
    }

    /// One key: a letter is the fragment the names are ranked from, `↑↓` walks
    /// them, `Home` and `End` are the first name and the last of the ones the
    /// screen shows, `Enter` takes the one the cursor is on, and `Esc` closes
    /// the screen with the file exactly as it was.
    pub fn key(&mut self, key: KeyEvent, served: &Served) -> Answer {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => Answer::Close,
            KeyCode::Enter => match self.list.take().map(str::to_owned) {
                Some(name) => Answer::Take(name),
                // A screen holding no name has nothing to write, and a keystroke
                // that writes nothing is not the screen closing.
                None => Answer::Held,
            },
            KeyCode::Up | KeyCode::BackTab => {
                self.list.step(true);
                Answer::Staying
            }
            KeyCode::Down | KeyCode::Tab => {
                self.list.step(false);
                Answer::Staying
            }
            // The two ends of what the screen shows: `Home` is the first name
            // on it and `End` the last, on the names a fragment reaches.
            KeyCode::Home | KeyCode::End => match self.list.end(key.code == KeyCode::Home) {
                true => Answer::Staying,
                false => Answer::Held,
            },
            KeyCode::Char('q') if ctrl => Answer::Quit,
            KeyCode::Char('c') if ctrl => Answer::CancelRun,
            KeyCode::Backspace => {
                self.filter.pop();
                self.refresh(served);
                Answer::Staying
            }
            KeyCode::Char(ch) if !ctrl => {
                self.filter.push(ch);
                self.refresh(served);
                Answer::Staying
            }
            _ => Answer::Held,
        }
    }

    /// A paste into the fragment: the screen's one field is what it filters by,
    /// so a name copied from elsewhere is typed there rather than a key the
    /// screen holds nothing for. The names are ranked again on the frame the
    /// paste lands, as they are after a letter.
    pub fn paste(&mut self, text: &str, served: &Served) -> bool {
        let text = crate::settings::one_line(text);
        if text.is_empty() {
            return false;
        }
        self.filter.push_str(&text);
        self.refresh(served);
        true
    }

    /// A screenful at a time: `PgUp`/`PgDn` walk the cursor by the rows the
    /// screen showed, because the list windows itself around the name it is on.
    pub fn page(&mut self, rows: usize, up: bool) {
        for _ in 0..rows {
            self.list.step(up);
        }
    }
}

impl crate::app::App {
    /// A key while the model screen has the screen, and whether the screen took
    /// it: a key it holds nothing for leaves the frame as it was.
    pub(crate) fn model_key(&mut self, key: KeyEvent) -> bool {
        let Some(Overlay::Model(model)) = self.overlay.as_mut() else {
            return false;
        };
        let answer = model.key(key, &self.served);
        match answer {
            Answer::Staying => true,
            Answer::Held => false,
            Answer::Close => {
                self.overlay = None;
                true
            }
            Answer::Take(name) => self.save_model(&name),
            Answer::Quit => {
                self.quit = true;
                true
            }
            Answer::CancelRun => self.cancel_or_clear(),
        }
    }

    /// `Enter` on the screen: the name the cursor is on, in the file the run
    /// reads, and the frame told to read it again — no restart, and no second
    /// source of truth for what this run asks.
    fn save_model(&mut self, name: &str) -> bool {
        let Some(Overlay::Model(model)) = self.overlay.as_mut() else {
            return false;
        };
        let written: Result<PathBuf, String> = model.set(name).map(|()| model.path().to_path_buf());
        match written {
            Ok(path) => {
                self.overlay = None;
                self.model_written(name, &path);
                true
            }
            Err(message) => {
                model.failed(message);
                true
            }
        }
    }

    /// `/model`: the names the wire this run speaks offers, on a screen of their
    /// own, and the ask that fills it — one ask per wire and address, so a wire
    /// the modal has just asked for is not asked a second time.
    pub(crate) fn open_model(&mut self) {
        match self.model_screen() {
            Ok(mut model) => {
                let launch = self.rail.launch();
                self.served.want(Serving::of(
                    launch.provider.clone(),
                    launch.base_url.clone(),
                ));
                model.refresh(&self.served);
                self.show(Overlay::Model(Box::new(model)));
            }
            Err(message) => self.note_error(format!("model: {message}")),
        }
    }

    /// `/model <name>`: a name the wire never offered, written the way the screen
    /// writes one — the same document, the same writer, the same reload.
    pub(crate) fn set_model(&mut self, name: &str) {
        let written = self
            .model_screen()
            .and_then(|mut model| model.set(name).map(|()| model.path().to_path_buf()));
        match written {
            Ok(path) => {
                self.model_written(name, &path);
            }
            Err(message) => self.note_error(format!("model: {message}")),
        }
    }

    /// The config file's document, opened as the screen that would write it: the
    /// path the frame was pointed at where it was pointed at one, and the cursor
    /// on the model this run asks. The screen and `/model <name>` read the file
    /// through the one function, so neither writes a file the other did not read.
    fn model_screen(&self) -> Result<Model, String> {
        Model::open(self.config.as_deref(), &self.rail.launch().model)
    }

    /// The one sentence a written name earns, and the one ask it makes of the
    /// front end: the file is read again before the next prompt, so what a run
    /// asks is what the file now says.
    fn model_written(&mut self, name: &str, path: &Path) {
        self.note_info(format!(
            "model set to {name} in {} · the next prompt uses it",
            path.display()
        ));
        self.reload = true;
    }
}
