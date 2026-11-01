//! `/settings`: the one config file, edited through the one schema. The frame
//! stages fields in memory and writes nothing until the user saves, `Esc`
//! throwing the whole draft away; a secret the file holds is never handed back,
//! so no field can show one and no frame can leak one. It is also the modal the
//! front door opens by itself when the run has no key, which is why `Missing` —
//! what is absent, in the words the transcript and the composer use — lives
//! here rather than in a second module both would have to know.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_core::config::{Document, KEYS, Key, Provider, toggle_shown};

use crate::overlay::Overlay;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Staying,
    Discard,
    Save,
    Quit,
    CancelRun,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub key: Key,
    pub text: String,
    /// Whether a keystroke has been in this field. A secret the file holds is
    /// never handed back — so an untouched one must not be written as empty.
    pub touched: bool,
}

/// Which rows a run has nothing for. One absent row is a run that cannot send
/// a prompt; both is the machine that has never been configured at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Absent {
    Key,
    Model,
    Both,
}

/// An absence in words: what a run has nothing for, the variables that set it,
/// and what `/settings` does about it — one vocabulary behind three sentences.
struct Lack {
    /// `no key`, `no model`, `no key and no model`.
    what: &'static str,
    /// What the transcript has the reader type: the row, and every variable
    /// that sets it.
    ways: String,
    /// The same in the composer's one row, which leaves the model to `/settings`.
    room: String,
    /// What `/settings` does for it: `saves one`, `names one`, `sets both`.
    acted: &'static str,
}

/// What the front door found absent when it opened the frame itself: the run
/// has no credential, or no model, or neither. It holds no value —
/// the modal says where a key comes from, never what one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Missing {
    /// The wire the run speaks: its variable is where the credential comes
    /// from, and a local server needs none.
    provider: Provider,
    absent: Absent,
}

impl Missing {
    /// What a resolved config could not find, when it found all of it: `None`
    /// is a run that can send a prompt, and there is nothing to say.
    pub fn of(provider: Provider, key: bool, model: bool) -> Option<Missing> {
        let absent = match (key, model) {
            (true, false) => Absent::Key,
            (false, true) => Absent::Model,
            (true, true) => Absent::Both,
            (false, false) => return None,
        };
        Some(Missing { provider, absent })
    }

    /// The credential's two variables, in the wire's own terms: the one fact
    /// every sentence about a missing key has to say, spelled once.
    pub fn key_vars(self) -> String {
        format!("SINDRI_API_KEY or {}", self.provider.api_key_env())
    }

    /// Whether the credential is one of the absent rows: what the modal's key
    /// row says, and what the composer cannot do.
    pub fn wants_key(self) -> bool {
        matches!(self.absent, Absent::Key | Absent::Both)
    }

    /// The row the modal opens on: the key when there is none, because a run
    /// with no credential is the failure a frame answers with a field.
    pub fn row(self) -> Key {
        match self.absent {
            Absent::Model => Key::Model,
            Absent::Key | Absent::Both => Key::ApiKey,
        }
    }

    /// The one description of an absence, and the only `match` on it: a fourth
    /// state is one arm here rather than three found by reading three of them.
    fn lack(self) -> Lack {
        let key_vars = self.key_vars();
        match self.absent {
            Absent::Key => Lack {
                what: "no key",
                ways: format!("one in the settings modal, or set {key_vars}"),
                room: key_vars,
                acted: "saves one",
            },
            Absent::Model => Lack {
                what: "no model",
                ways: "one in the settings modal, or set SINDRI_MODEL".to_string(),
                room: "SINDRI_MODEL".to_string(),
                acted: "names one",
            },
            Absent::Both => Lack {
                what: "no key and no model",
                ways: format!("them in the settings modal, or set {key_vars} and SINDRI_MODEL"),
                room: key_vars,
                acted: "sets both",
            },
        }
    }

    /// The transcript's one line: what is absent and every way to set it —
    /// said once, when the frame opens, and never on a frame.
    pub fn said(self) -> String {
        let Lack { what, ways, .. } = self.lack();
        format!("{what}: type {ways}")
    }

    /// What the composer says where the prompt would be: a run with no key
    /// cannot send one, and the frame says why rather than pretending.
    pub fn composer(self) -> String {
        let Lack { what, room, .. } = self.lack();
        format!("{what} · a prompt cannot be sent · /settings, or set {room}")
    }

    /// A prompt sent anyway: the draft stays where it was, and this is the one
    /// line the transcript gets for the keystroke.
    pub fn refused(self) -> String {
        let Lack { what, acted, .. } = self.lack();
        format!("not sent: this run has {what} — /settings {acted}")
    }

    /// What is still absent once a save has landed: a row the file now carries
    /// is filled in, and the rest is not. A key in the environment is not this
    /// frame's to see, so the file is what a save can answer for.
    pub fn satisfied(self, document: &Document) -> Option<Missing> {
        let key = self.absent != Absent::Model && !document.is_set(Key::ApiKey);
        let model = self.absent != Absent::Key && !document.is_set(Key::Model);
        Missing::of(self.provider, key, model)
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    document: Document,
    fields: Vec<Field>,
    chosen: usize,
    error: Option<String>,
    missing: Option<Missing>,
}

impl Settings {
    pub fn open() -> Result<Settings, String> {
        Document::load()
            .map(Settings::at)
            .map_err(|error| error.message().to_string())
    }

    pub fn at_path(path: &Path) -> Result<Settings, String> {
        Document::load_at(path)
            .map(Settings::at)
            .map_err(|error| error.message().to_string())
    }

    pub fn at(document: Document) -> Settings {
        let fields = KEYS
            .iter()
            .map(|key| Field {
                key: *key,
                text: document.editable(*key),
                touched: false,
            })
            .collect();
        Settings {
            document,
            fields,
            chosen: 0,
            error: None,
            missing: None,
        }
    }

    /// The modal a first run opens by itself: its cursor on the row the run
    /// has nothing for, and, above the rows, the four facts that row needs —
    /// where a value comes from, where it is written, that the file is `0600`,
    /// and that a credential is never shown again.
    pub fn staged(mut self, missing: Missing) -> Settings {
        self.chosen = KEYS
            .iter()
            .position(|key| *key == missing.row())
            .unwrap_or_default();
        self.missing = Some(missing);
        self
    }

    pub fn path(&self) -> &Path {
        self.document.path()
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn chosen(&self) -> usize {
        self.chosen
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Why the modal opened without being asked, when it did.
    pub fn missing(&self) -> Option<Missing> {
        self.missing
    }

    pub fn staged_secret(&self) -> bool {
        self.fields
            .iter()
            .any(|field| field.key.secret() && !field.text.is_empty())
    }

    pub fn key(&mut self, key: KeyEvent) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => Outcome::Discard,
            KeyCode::Enter => Outcome::Save,
            KeyCode::Up | KeyCode::BackTab => {
                self.focus(false);
                Outcome::Staying
            }
            KeyCode::Down | KeyCode::Tab => {
                self.focus(true);
                Outcome::Staying
            }
            KeyCode::Char('q') if ctrl => Outcome::Quit,
            KeyCode::Char('c') if ctrl => Outcome::CancelRun,
            KeyCode::Char('u') if ctrl => {
                self.clear();
                Outcome::Staying
            }
            KeyCode::Backspace => {
                self.backspace();
                Outcome::Staying
            }
            KeyCode::Char(' ') if self.flip() => Outcome::Staying,
            KeyCode::Char(ch) if !ctrl => {
                self.insert(ch);
                Outcome::Staying
            }
            _ => Outcome::Staying,
        }
    }

    pub fn focus(&mut self, down: bool) {
        self.chosen = crate::overlay::step(self.chosen, 1, self.fields.len(), !down);
    }

    /// A screenful at a time: what `PgUp`/`PgDn` and the wheel move the cursor
    /// by in a modal whose rows do not all fit.
    pub fn page(&mut self, rows: usize, up: bool) {
        self.chosen = crate::overlay::step(self.chosen, rows, self.fields.len(), up);
    }

    pub fn insert(&mut self, ch: char) {
        self.edit(|field| field.text.push(ch));
    }

    pub fn backspace(&mut self) {
        self.edit(|field| {
            field.text.pop();
        });
    }

    /// Ctrl-U: the field with the cursor, emptied. An empty value unsets the
    /// key, so the environment or the default speaks instead.
    pub fn clear(&mut self) {
        self.edit(|field| field.text.clear());
    }

    fn edit(&mut self, change: impl FnOnce(&mut Field)) {
        if let Some(field) = self.fields.get_mut(self.chosen) {
            change(field);
            field.touched = true;
            self.error = None;
        }
    }

    pub fn flip(&mut self) -> bool {
        let on = match self.fields.get(self.chosen) {
            Some(field) if field.key == Key::Thinking => toggle_shown(&field.text) == "on",
            _ => return false,
        };
        let text = if on { "false" } else { "true" }.to_string();
        self.edit(|field| field.text = text);
        true
    }

    pub fn save(&mut self) -> Result<(), String> {
        // Staged on a copy: a document that refuses one value must not be
        // left holding the ones before it.
        let mut staged = self.document.clone();
        for field in self.fields.iter().filter(|field| field.touched) {
            staged
                .set(field.key, &field.text)
                .map_err(|error| format!("{}: {}", field.key.name(), error.message()))?;
        }
        staged.save().map_err(|error| error.message().to_string())?;
        self.document = staged;
        self.error = None;
        Ok(())
    }

    pub fn failed(&mut self, message: String) {
        self.error = Some(message);
    }

    pub fn document(&self) -> &Document {
        &self.document
    }
}

impl crate::app::App {
    pub(crate) fn settings_key(&mut self, key: KeyEvent) {
        let Some(Overlay::Settings(settings)) = self.overlay.as_mut() else {
            return;
        };
        match settings.key(key) {
            Outcome::Staying => {}
            Outcome::Discard => self.overlay = None,
            Outcome::Save => self.save_settings(),
            Outcome::Quit => self.quit = true,
            Outcome::CancelRun => self.cancel_or_clear(),
        }
    }

    pub(crate) fn save_settings(&mut self) {
        let Some(Overlay::Settings(settings)) = self.overlay.as_mut() else {
            return;
        };
        match settings.save() {
            Ok(()) => {
                let path = settings.path().display().to_string();
                // The file is read back as what a run would read it as, so a
                // row that was absent and now is not stops being said — and a
                // row that is still absent keeps being said.
                let still = settings
                    .missing()
                    .and_then(|missing| missing.satisfied(settings.document()));
                self.overlay = None;
                self.missing = still;
                self.note_info(format!(
                    "settings saved to {path}; the next prompt uses them"
                ));
                self.reload = true;
            }
            Err(message) => settings.failed(message),
        }
    }

    /// `/settings`, or — with what the front door found absent — the modal it
    /// opens by itself: the same modal, on the row the run has nothing for, and
    /// saying why it is there.
    pub(crate) fn open_settings(&mut self, missing: Option<Missing>) {
        let opened = match &self.config {
            Some(path) => Settings::at_path(path),
            None => Settings::open(),
        };
        match opened {
            Ok(settings) => {
                let settings = match missing {
                    Some(missing) => settings.staged(missing),
                    None => settings,
                };
                self.show(Overlay::Settings(Box::new(settings)));
            }
            Err(message) => self.note_error(format!("settings: {message}")),
        }
    }
}
