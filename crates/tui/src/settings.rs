//! `/settings`: the one config file, edited through the one schema (ADR-0005,
//! ADR-0014). The frame stages fields in memory and writes nothing until the
//! user saves; `Esc` throws the whole draft away. A secret the file holds is
//! never handed back, so no field can show one and no frame can leak one.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_core::config::{Document, KEYS, Key};

use crate::overlay::Overlay;

/// What one keystroke in the modal left the frame to do, if anything: the modal
/// knows its own keys, and the frame knows what saving or leaving means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The keystroke was a field's, and the modal stays.
    Staying,
    /// `Esc`: throw the draft away.
    Discard,
    /// `Enter`: write the file.
    Save,
    /// `Ctrl-Q`: leave the frame, draft and all.
    Quit,
    /// `Ctrl-C`: cancel the run behind the modal, which stays open.
    CancelRun,
}

/// One editable field: the key it stages, and the text as typed.
#[derive(Debug, Clone)]
pub struct Field {
    /// The file's own key.
    pub key: Key,
    /// What the user has typed, or what the file said before that.
    pub text: String,
}

/// The modal's state: the document it will write, and the fields over it.
#[derive(Debug, Clone)]
pub struct Settings {
    document: Document,
    fields: Vec<Field>,
    chosen: usize,
    error: Option<String>,
}

impl Settings {
    /// The config file a run would read.
    pub fn open() -> Result<Settings, String> {
        match Document::load() {
            Ok(document) => Ok(Settings::at(document)),
            Err(error) => Err(error.message().to_string()),
        }
    }

    /// The config file at `path`, for a frame that was pointed at one.
    pub fn at_path(path: &Path) -> Result<Settings, String> {
        Document::load_at(path)
            .map(Settings::at)
            .map_err(|error| error.message().to_string())
    }

    /// The modal over one document, already loaded: a test points one at a
    /// temporary directory rather than a real user's config.
    pub fn at(document: Document) -> Settings {
        let fields = KEYS
            .iter()
            .map(|key| Field {
                key: *key,
                text: document.editable(*key),
            })
            .collect();
        Settings {
            document,
            fields,
            chosen: 0,
            error: None,
        }
    }

    /// Where saving will write, said in full: a user agreeing to write a secret
    /// is told the path, not a placeholder.
    pub fn path(&self) -> &Path {
        self.document.path()
    }

    /// The fields, in the order they are shown.
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Which field has the cursor.
    pub fn chosen(&self) -> usize {
        self.chosen
    }

    /// Why the last save was refused, if it was.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Whether the file already names this key — what a field says instead of a
    /// secret's value.
    pub fn is_set(&self, key: Key) -> bool {
        self.document.is_set(key)
    }

    /// Whether a secret has been typed in this draft: the plaintext warning,
    /// and the only thing that turns writing a key to disk into an act.
    pub fn staged_secret(&self) -> bool {
        self.fields
            .iter()
            .any(|field| field.key.secret() && !field.text.is_empty())
    }

    /// One key. Every character is a character of the field with the cursor;
    /// `Enter` saves, `Esc` discards, and the two ways out of the frame stay
    /// true behind a modal (ADR-0013, ADR-0014).
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
            // Space is the toggle's keystroke, and a space everywhere else.
            KeyCode::Char(' ') if self.flip() => Outcome::Staying,
            KeyCode::Char(ch) if !ctrl => {
                self.insert(ch);
                Outcome::Staying
            }
            _ => Outcome::Staying,
        }
    }

    /// Up and Down, or Tab and Shift-Tab: the field with the cursor.
    pub fn focus(&mut self, down: bool) {
        let last = self.fields.len().saturating_sub(1);
        self.chosen = if down {
            (self.chosen + 1).min(last)
        } else {
            self.chosen.saturating_sub(1)
        };
    }

    /// One character, appended to the field with the cursor.
    pub fn insert(&mut self, ch: char) {
        if let Some(field) = self.fields.get_mut(self.chosen) {
            field.text.push(ch);
            self.error = None;
        }
    }

    /// Backspace: the last character of the field with the cursor.
    pub fn backspace(&mut self) {
        if let Some(field) = self.fields.get_mut(self.chosen) {
            field.text.pop();
            self.error = None;
        }
    }

    /// Ctrl-U: the field with the cursor, emptied. An empty value unsets the
    /// key, so the environment or the default speaks instead.
    pub fn clear(&mut self) {
        if let Some(field) = self.fields.get_mut(self.chosen) {
            field.text.clear();
            self.error = None;
        }
    }

    /// Space on a toggle: the only field where a keystroke is not a character.
    pub fn flip(&mut self) -> bool {
        let Some(field) = self.fields.get_mut(self.chosen) else {
            return false;
        };
        if field.key != Key::Thinking {
            return false;
        }
        // An unset toggle is the run's own default, which is off, so the first
        // space turns it on.
        let on = matches!(field.text.trim(), "true" | "1" | "yes" | "on");
        field.text = if on { "false" } else { "true" }.to_string();
        self.error = None;
        true
    }

    /// Stages every field through the schema and writes the file. A value the
    /// schema refuses leaves the file exactly as it was, and the modal open.
    pub fn save(&mut self) -> Result<(), String> {
        // Staged on a copy: a document that refuses one value must not be left
        // holding the ones before it.
        let mut staged = self.document.clone();
        for field in &self.fields {
            staged
                .set(field.key, &field.text)
                .map_err(|error| format!("{}: {}", field.key.name(), error.message()))?;
        }
        staged.save().map_err(|error| error.message().to_string())?;
        self.document = staged;
        self.error = None;
        Ok(())
    }

    /// Says why nothing was written, in the modal, without closing it.
    pub fn failed(&mut self, message: String) {
        self.error = Some(message);
    }

    /// The document as it stands: what a saved config would say.
    pub fn document(&self) -> &Document {
        &self.document
    }
}

/// The modal's hands: what `/settings` opens, what a key does while it is open,
/// and what saving means to the frame. They live beside the fields they edit.
impl crate::app::App {
    /// What a key does while the settings modal has the screen (ADR-0014). The
    /// modal knows its own keys and says what they meant; writing the file,
    /// leaving, and cancelling the run are the frame's.
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

    /// `Enter` in the settings modal: the file is written now, and the front end
    /// is asked to read it again, so the next prompt runs on it (ADR-0014). A
    /// value the schema refuses keeps the modal open and writes nothing.
    pub(crate) fn save_settings(&mut self) {
        let Some(Overlay::Settings(settings)) = self.overlay.as_mut() else {
            return;
        };
        match settings.save() {
            Ok(()) => {
                let path = settings.path().display().to_string();
                self.overlay = None;
                self.note_info(format!(
                    "settings saved to {path}; the next prompt uses them"
                ));
                self.reload = true;
            }
            Err(message) => settings.failed(message),
        }
    }

    /// `/settings`: the modal over the one config file and the one schema. A
    /// file the schema refuses — an unknown key, a value that is not one — is
    /// said in the transcript, and no modal opens over it (ADR-0005).
    pub(crate) fn open_settings(&mut self) {
        let opened = match &self.config {
            Some(path) => Settings::at_path(path),
            None => Settings::open(),
        };
        match opened {
            Ok(settings) => self.overlay = Some(Overlay::Settings(settings)),
            Err(message) => self.note_error(format!("settings: {message}")),
        }
    }
}
