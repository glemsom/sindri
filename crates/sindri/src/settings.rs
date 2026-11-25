use sindri_core::config::{self, Config, ConfigError, Environment, Touched};
use sindri_core::redact::MASK;

use crate::composer::Key;
use crate::ink::Ink;
use crate::kept::{Kept, Move, Narrow};
use crate::list::{self, Beside, ScreenRow};
use crate::models::Models;
use sindri_core::provider::served::Listing;
use sindri_core::wire::Wire;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    rows: Vec<Row>,
    narrow: Narrow,
    refusal: Option<String>,
    editor: Option<Editor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Editor {
    Text {
        before: String,
        touched: bool,
    },
    Choices {
        values: Vec<&'static str>,
        narrow: Narrow,
    },
    Model(Models),
}

#[derive(Clone, Debug)]
struct Row {
    key: &'static config::Field,
    text: String,
    touched: bool,
}

impl PartialEq for Row {
    fn eq(&self, other: &Row) -> bool {
        self.key.name == other.key.name && self.text == other.text && self.touched == other.touched
    }
}

impl Eq for Row {}

impl Settings {
    pub fn of(config: &Config) -> Settings {
        Settings {
            rows: Config::SCHEMA
                .iter()
                .map(|key| Row {
                    key,
                    text: key.shown(config),
                    touched: false,
                })
                .collect(),
            narrow: Narrow::default(),
            refusal: None,
            editor: None,
        }
    }

    pub fn refresh(&mut self) -> bool {
        if let Some(Editor::Model(models)) = &mut self.editor {
            *models = Models::new();
            true
        } else {
            false
        }
    }

    pub fn editing(&self) -> bool {
        self.editor.is_some()
    }

    pub fn listing(&mut self, listing: Listing) {
        let current = self
            .rows
            .iter()
            .find(|row| row.key.name == "model")
            .map(|row| row.text.clone());
        if let Some(Editor::Model(models)) = &mut self.editor {
            *models = std::mem::take(models).served_by(Some(listing));
            if let Some(at) = models
                .rows()
                .iter()
                .position(|(id, _)| Some(*id) == current.as_deref())
            {
                for _ in 0..at {
                    models.walked(Move::Next);
                }
            }
        }
    }

    pub fn draft(&self, base: &Config) -> Result<Config, ConfigError> {
        let mut draft = base.clone();
        for row in self.rows.iter().filter(|row| row.touched) {
            let value = if row.text.trim().is_empty() {
                Config::default()
            } else {
                row.key.read(row.text.trim())?
            };
            match row.key.name {
                "provider" => draft.provider = value.provider,
                "model" => draft.model = value.model,
                "route" => draft.route = value.route,
                "base_url" => draft.base_url = value.base_url,
                "api_key" => draft.api_key = value.api_key,
                _ => {}
            }
        }
        Ok(draft)
    }

    fn wire(&self) -> Option<Wire> {
        self.rows
            .iter()
            .find(|row| row.key.name == "provider")
            .and_then(|row| Wire::from_word(&row.text))
    }

    pub fn activate(&mut self) -> bool {
        if let Some(editor) = self.editor.take() {
            let value = match editor {
                Editor::Text { .. } => return false,
                Editor::Choices { values, narrow } => {
                    values.get(narrow.mark()).map(|v| (*v).to_owned())
                }
                Editor::Model(models) => models.chosen().map(str::to_owned),
            };
            if let Some(value) = value {
                self.replace(value);
            }
            return false;
        }
        let Some(row) = self.rows.get(self.current()) else {
            return false;
        };
        self.editor = if row.key.name == "model" {
            Some(Editor::Model(Models::new()))
        } else if let Some(values) = row.key.choices(self.wire()) {
            if values.is_empty() {
                return false;
            }
            let mut narrow = Narrow::default();
            for _ in 0..values.iter().position(|v| *v == row.text).unwrap_or(0) {
                narrow.walked(Move::Next, values.len());
            }
            Some(Editor::Choices { values, narrow })
        } else {
            Some(Editor::Text {
                before: row.text.clone(),
                touched: row.touched,
            })
        };
        matches!(self.editor, Some(Editor::Model(_)))
    }

    fn replace(&mut self, value: String) {
        let row = self.standing();
        row.text = value;
        row.touched = true;
        self.refusal = None;
        if self.wire() == Some(Wire::Anthropic)
            && let Some(row) = self.rows.iter_mut().find(|row| row.key.name == "route")
            && !row.text.is_empty()
        {
            row.text.clear();
            row.touched = true;
        }
    }

    pub fn cancel_editor(&mut self) -> bool {
        let Some(editor) = self.editor.take() else {
            return false;
        };
        if let Editor::Text { before, touched } = editor {
            self.standing().text = before;
            self.standing().touched = touched;
        }
        true
    }

    pub fn rows(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.rows
            .iter()
            .map(|row| (row.key.name, row.text.as_str()))
    }

    pub fn current(&self) -> usize {
        self.narrow.mark()
    }

    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    pub fn body(&self, width: u16) -> Vec<ScreenRow> {
        match &self.editor {
            Some(Editor::Model(models)) => {
                let mut rows = models.body();
                rows.extend(list::refusal(
                    "Ctrl-F: refresh; Ctrl-U: enter identifier manually; Esc: cancel",
                    width,
                ));
                if let Some(reason) = self.refusal() {
                    rows.extend(list::refusal(reason, width));
                }
                return rows;
            }
            Some(Editor::Choices { values, narrow }) => {
                return list::List {
                    query: None,
                    said: None,
                    items: list::columned(
                        values
                            .iter()
                            .map(|v| (if v.is_empty() { "Automatic" } else { *v }, "")),
                        Ink::Plain,
                        Beside::Row,
                    ),
                    selected: narrow.mark(),
                    marker: list::MARK,
                    chooses: true,
                }
                .rows();
            }
            _ => {}
        }
        let mut shown: Vec<(String, String)> = self
            .rows()
            .map(|(key, value)| {
                let label = if key == "provider" { "Wire" } else { key };
                let value = if key == "route" && self.wire() == Some(Wire::Anthropic) {
                    "Not applicable".to_owned()
                } else if key == "route" && value.is_empty() {
                    let mut endpoint = self
                        .wire()
                        .map(sindri_core::provider::endpoint::Endpoint::own);
                    if let Some(url) = self
                        .rows
                        .iter()
                        .find(|row| row.key.name == "base_url")
                        .filter(|row| !row.text.is_empty())
                    {
                        endpoint = endpoint.map(|endpoint| endpoint.based(&url.text));
                    }
                    let model = self
                        .rows
                        .iter()
                        .find(|row| row.key.name == "model")
                        .map(|row| row.text.as_str())
                        .unwrap_or("");
                    match endpoint.and_then(|endpoint| endpoint.route_of(model)) {
                        Some(route) => format!("Automatic → {route}"),
                        None => "Automatic".to_owned(),
                    }
                } else {
                    value.to_owned()
                };
                (label.to_owned(), value)
            })
            .collect();
        shown.push(("Apply".into(), "changes affect the next run".into()));
        shown.push(("Cancel".into(), "discard changes".into()));
        let keys: Vec<(&str, &str)> = shown
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let mut rows = list::List {
            query: None,
            said: None,
            items: list::columned(keys.into_iter(), Ink::Muted, Beside::Row),
            selected: self.current(),
            marker: list::MARK,
            chooses: true,
        }
        .rows();
        if self
            .rows
            .iter()
            .any(|row| row.touched && matches!(row.key.name, "provider" | "base_url"))
        {
            rows.extend(list::refusal(
                "Endpoint changed: check the model, address and credentials before Apply",
                width,
            ));
        }
        if matches!(self.editor, Some(Editor::Text { .. })) {
            rows.extend(list::refusal(
                "Editing: Enter confirms; Esc restores; Ctrl-U clears",
                width,
            ));
        }
        if let Some(reason) = self.refusal() {
            rows.extend(list::refusal(reason, width));
        }
        rows
    }

    pub fn touched(&self) -> Touched {
        let mut touched = Touched::new();
        for row in self.rows.iter().filter(|row| row.touched) {
            touched.set(row.key.name, row.text.as_str());
        }
        touched
    }

    pub fn refuse(&mut self, reason: &str) {
        self.refusal = Some(reason.to_owned());
    }

    pub fn clear(&mut self) -> bool {
        if matches!(self.editor, Some(Editor::Model(_))) {
            self.editor = Some(Editor::Text {
                before: self.standing().text.clone(),
                touched: self.standing().touched,
            });
            return true;
        }
        if matches!(self.editor, Some(Editor::Choices { .. })) {
            return false;
        }
        if !self.has_row() {
            return false;
        }
        let row = self.standing();
        if row.text.is_empty() {
            return false;
        }
        row.text.clear();
        row.touched = true;
        self.refusal = None;
        true
    }

    pub fn space(&mut self) -> bool {
        if matches!(self.editor, Some(Editor::Model(_) | Editor::Choices { .. })) {
            return self.key(Key::Char(' '));
        }
        if !self.has_row() {
            return false;
        }
        if !self.standing().key.switch() {
            return self.write(' ');
        }
        let row = self.standing();
        row.text = match row.text.as_str() {
            "true" => String::from("false"),
            _ => String::from("true"),
        };
        row.touched = true;
        self.refusal = None;
        true
    }

    pub fn key(&mut self, key: Key) -> bool {
        if let Some(Editor::Model(models)) = &mut self.editor {
            return models.key(key);
        }
        if matches!(self.editor, Some(Editor::Choices { .. })) {
            return false;
        }
        if !self.has_row() {
            return false;
        }
        match key {
            Key::Char(glyph) if !glyph.is_control() => self.write(glyph),
            Key::Backspace => self.backspace(),
            _ => false,
        }
    }

    fn has_row(&self) -> bool {
        self.rows.get(self.current()).is_some()
    }

    fn standing(&mut self) -> &mut Row {
        let at = self.narrow.mark();
        &mut self.rows[at]
    }

    fn write(&mut self, glyph: char) -> bool {
        self.unmask();
        let row = self.standing();
        row.text.push(glyph);
        row.touched = true;
        self.refusal = None;
        true
    }

    fn backspace(&mut self) -> bool {
        let masked = self.unmask();
        let erased = self.standing().text.pop().is_some() || masked;
        if erased {
            self.standing().touched = true;
            self.refusal = None;
        }
        erased
    }

    fn unmask(&mut self) -> bool {
        let row = self.standing();
        let masked = row.text == MASK;
        if masked {
            row.text.clear();
        }
        masked
    }
}

impl Kept for Settings {
    fn walked(&mut self, way: Move) -> bool {
        if self.rows.is_empty() {
            return false;
        }
        match &mut self.editor {
            Some(Editor::Model(models)) => models.walked(way),
            Some(Editor::Choices { values, narrow }) => narrow.walked(way, values.len()),
            Some(Editor::Text { .. }) => false,
            None => self.narrow.walked(way, self.rows.len() + 2),
        }
    }

    fn typed(&mut self, key: Key) -> bool {
        if self.editor.is_none() {
            return false;
        }
        self.key(key)
    }

    fn pressed(&mut self, at: usize) -> bool {
        match &mut self.editor {
            Some(Editor::Model(models)) => models.pressed(at),
            Some(Editor::Choices { values, narrow }) => narrow.pressed(at, values.len()),
            Some(Editor::Text { .. }) => false,
            None => self.narrow.pressed(at, self.rows.len() + 2),
        }
    }
}

pub fn persist(
    environment: &impl Environment,
    touched: &Touched,
    overrides: Config,
) -> Result<Config, ConfigError> {
    Config::save_touched(environment, touched)?;
    Config::resolve(environment, overrides)
}

#[cfg(test)]
mod tests;
