use std::borrow::Cow;
use std::sync::OnceLock;

use sindri_core::run::Loaded;
use sindri_core::skills::Catalog;

use crate::composer::Key;
use crate::ink::{Ink, Look};
use crate::kept::{CAP, Kept, Move, Narrow, leaves};
use crate::list::{self, Beside, ScreenRow};

const NEW: &str = "/new";

const MODEL: &str = "/model";

const SETTINGS: &str = "/settings";

const HELP: &str = "/help";

const STATS: &str = "/stats";

const QUIT: &str = "/quit";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: Cow<'static, str>,
    pub says: Cow<'static, str>,
}

pub const TABLE: [Row; 6] = [
    Row {
        name: Cow::Borrowed(MODEL),
        says: Cow::Borrowed("ask the wire what it serves, and name the one to run"),
    },
    Row {
        name: Cow::Borrowed(SETTINGS),
        says: Cow::Borrowed("the keys the config holds, and the value each is set to"),
    },
    Row {
        name: Cow::Borrowed(STATS),
        says: Cow::Borrowed("the facts the rail is too narrow to hold"),
    },
    Row {
        name: Cow::Borrowed(NEW),
        says: Cow::Borrowed("start the conversation again, and forget the last one"),
    },
    Row {
        name: Cow::Borrowed(HELP),
        says: Cow::Borrowed("the keys in force"),
    },
    Row {
        name: Cow::Borrowed(QUIT),
        says: Cow::Borrowed("end the launch"),
    },
];

pub const SPLASH: usize = 3;

#[derive(Clone, Debug)]
pub struct Card {
    narrow: Narrow,
    skills: Vec<Row>,
    loaded: Loaded,
}

impl Card {
    pub fn of(draft: &str) -> Option<Card> {
        let words = draft;
        match draft.starts_with('/') && recognised(word(words)) {
            true => Some(Card {
                narrow: Narrow::asked(words),
                skills: Vec::new(),
                loaded: Loaded::default(),
            }),
            false => None,
        }
    }

    pub fn with_catalog(draft: &str, catalog: &Catalog) -> Option<Card> {
        if !draft.starts_with('/') {
            return None;
        }
        let mut card = Card {
            narrow: Narrow::asked(draft),
            skills: Vec::new(),
            loaded: Loaded::default(),
        };
        card.skills = catalog
            .invocable_by_human()
            .into_iter()
            .map(|skill| Row {
                name: Cow::Owned(format!("/{}", skill.name)),
                says: Cow::Owned(skill.description),
            })
            .collect();
        if card.ranked().is_empty() {
            return None;
        }
        Some(card)
    }

    pub fn loading(&mut self, loaded: &Loaded) {
        self.loaded = loaded.clone();
    }

    pub fn rows(&self) -> Vec<Row> {
        self.ranked()
            .into_iter()
            .take(CAP)
            .map(|mut row| {
                if row
                    .name
                    .strip_prefix('/')
                    .is_some_and(|name| self.loaded.contains(name))
                {
                    row.says = Cow::Owned(format!("loaded · {}", row.says));
                }
                row
            })
            .collect()
    }

    fn ranked(&self) -> Vec<Row> {
        let mut rows = by_name().to_vec();
        rows.extend(
            self.skills
                .iter()
                .filter(|row| !TABLE.iter().any(|builtin| builtin.name == row.name))
                .cloned(),
        );
        rows.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        leaves(
            rows.iter().map(|row| (plain(row), row.clone())),
            word(self.words()).trim_start_matches('/'),
        )
    }

    pub fn words(&self) -> &str {
        self.narrow.query()
    }

    pub fn selected(&self) -> usize {
        self.narrow.mark()
    }

    pub fn body(&self) -> Vec<ScreenRow> {
        let held = self.rows();
        let mut rows = list::List {
            query: None,
            said: None,
            items: list::columned(
                held.iter()
                    .map(|row| (row.name.as_ref(), row.says.as_ref())),
                Ink::Plain,
                Beside::Own(Ink::Dim),
            ),
            selected: self.narrow.mark(),
            marker: list::MARK,
            chooses: true,
        }
        .rows();
        let matches = self.ranked().len();
        if matches > CAP {
            rows.push(list::whole(
                &format!("1–{} of {matches}", held.len()),
                Look::plain(Ink::Dim),
            ));
        }
        rows
    }

    pub fn spelling(&self) -> String {
        self.spelling_of(self.narrow.mark())
            .unwrap_or_else(|| self.words().to_owned())
    }

    pub fn spelling_of(&self, at: usize) -> Option<String> {
        let rows = self.rows();
        let chosen = rows.get(at)?;
        let words = self.words();
        Some(format!("{}{}", chosen.name, &words[word(words).len()..]))
    }
}

pub fn recognised(word: &str) -> bool {
    !ranked(word).is_empty()
}

impl Kept for Card {
    fn walked(&mut self, way: Move) -> bool {
        self.narrow.walked(way, self.rows().len())
    }

    fn typed(&mut self, key: Key) -> bool {
        let skills = &self.skills;
        self.narrow.typed(key, |words| {
            Card {
                narrow: Narrow::asked(words),
                skills: skills.clone(),
                loaded: Loaded::default(),
            }
            .rows()
            .len()
        })
    }

    fn pressed(&mut self, at: usize) -> bool {
        self.narrow.pressed(at, self.rows().len())
    }
}

fn word(draft: &str) -> &str {
    draft.split_whitespace().next().unwrap_or_default()
}

fn ranked(asked: &str) -> Vec<Row> {
    let Some(body) = asked.strip_prefix('/') else {
        return Vec::new();
    };
    let query = body;
    leaves(by_name().iter().map(|row| (plain(row), row.clone())), query)
}

fn plain(row: &Row) -> &str {
    row.name.strip_prefix('/').unwrap_or(row.name.as_ref())
}

fn by_name() -> &'static [Row] {
    static BY_NAME: OnceLock<Vec<Row>> = OnceLock::new();
    BY_NAME.get_or_init(|| {
        let mut rows = TABLE.to_vec();
        rows.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        rows
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    New,
    InvokeSkill(String, String),
    Model,
    ModelNamed(String),
    Settings,
    Help,
    Stats,
    Quit,
}

/// A served name may hold a space, so `/model` takes the rest of the line whole.
pub fn read(line: &str) -> Option<Command> {
    let line = line.trim();
    let (word, rest) = match line.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim()),
        None => (line, ""),
    };
    match word {
        NEW => Some(Command::New),
        SETTINGS => Some(Command::Settings),
        HELP => Some(Command::Help),
        STATS => Some(Command::Stats),
        QUIT => Some(Command::Quit),
        MODEL => Some(match rest.is_empty() {
            true => Command::Model,
            false => Command::ModelNamed(rest.to_owned()),
        }),
        _ => None,
    }
}

pub fn read_with_catalog(line: &str, catalog: &Catalog) -> Option<Command> {
    if let Some(command) = read(line) {
        return Some(command);
    }
    let line = line.trim();
    let first = word(line);
    let name = first.strip_prefix('/')?;
    catalog.get(name).filter(|skill| skill.human_invocable())?;
    Some(Command::InvokeSkill(
        name.to_owned(),
        line[first.len()..].trim().to_owned(),
    ))
}

#[cfg(test)]
mod tests;
