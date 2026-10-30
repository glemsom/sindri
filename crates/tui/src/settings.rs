//! `/settings`: the one config file, edited through the one schema
//! (ADR-0005). The frame stages fields in memory and writes nothing until the
//! user saves; `Esc` throws the whole draft away. A secret the file holds is
//! never handed back, so no field can show one and no frame can leak one.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_core::config::{Document, KEYS, Key, toggle_shown};

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

#[derive(Debug, Clone)]
pub struct Settings {
    document: Document,
    fields: Vec<Field>,
    chosen: usize,
    error: Option<String>,
}

impl Settings {
    pub fn open() -> Result<Settings, String> {
        match Document::load() {
            Ok(document) => Ok(Settings::at(document)),
            Err(error) => Err(error.message().to_string()),
        }
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
        }
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
        let last = self.fields.len().saturating_sub(1);
        self.chosen = if down {
            (self.chosen + 1).min(last)
        } else {
            self.chosen.saturating_sub(1)
        };
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
                self.overlay = None;
                self.note_info(format!(
                    "settings saved to {path}; the next prompt uses them"
                ));
                self.reload = true;
            }
            Err(message) => settings.failed(message),
        }
    }

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
