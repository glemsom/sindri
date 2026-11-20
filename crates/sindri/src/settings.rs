use sindri_core::config::{self, Config, ConfigError, Environment, Touched};
use sindri_core::redact::MASK;

use crate::composer::Key;
use crate::ink::Ink;
use crate::kept::{Kept, Move, Narrow};
use crate::list::{self, Beside, ScreenRow};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    rows: Vec<Row>,
    narrow: Narrow,
    refusal: Option<String>,
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
        }
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

    /// The rows the screen's card holds: the config's keys with the values the visit carries, the row
    /// the reader is on marked, and under them the refusal that turned the visit back. A value too
    /// wide for the frame is cut at its right edge rather than dropped, as the picker's paths are.
    pub fn body(&self, width: u16) -> Vec<ScreenRow> {
        let keys: Vec<(&str, &str)> = self.rows().collect();
        let mut rows = list::List {
            query: None,
            said: None,
            items: list::columned(keys.into_iter(), Ink::Muted, Beside::Row),
            selected: self.current(),
            marker: list::MARK,
            chooses: false,
        }
        .rows();
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

    /// A backspace on a row holding the mask unsets it whole: the secret is not something
    /// this screen may hold even for one keystroke.
    fn backspace(&mut self) -> bool {
        let masked = self.unmask();
        let erased = self.standing().text.pop().is_some() || masked;
        if erased {
            self.standing().touched = true;
            self.refusal = None;
        }
        erased
    }

    /// The mask is what the file holds and not what a reader typed: the first glyph replaces
    /// it, and the row holds no value a save may write until one does.
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
        self.narrow.walked(way, self.rows.len())
    }

    fn typed(&mut self, key: Key) -> bool {
        self.key(key)
    }

    /// A visit's rows hang no choice, so no press is ever read as one.
    fn pressed(&mut self, _at: usize) -> bool {
        false
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
