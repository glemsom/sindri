use std::sync::OnceLock;

use crate::composer::Key;
use crate::ink::Ink;
use crate::kept::{Kept, Move, Narrow, leaves};
use crate::list::{self, Beside, ScreenRow};

const NEW: &str = "/new";

const SKILL: &str = "/skill";

const MODEL: &str = "/model";

const SETTINGS: &str = "/settings";

const HELP: &str = "/help";

const STATS: &str = "/stats";

const QUIT: &str = "/quit";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: &'static str,
    pub says: &'static str,
}

pub const TABLE: [Row; 7] = [
    Row {
        name: MODEL,
        says: "ask the wire what it serves, and name the one to run",
    },
    Row {
        name: SETTINGS,
        says: "the keys the config holds, and the value each is set to",
    },
    Row {
        name: STATS,
        says: "the facts the rail is too narrow to hold",
    },
    Row {
        name: NEW,
        says: "start the conversation again, and forget the last one",
    },
    Row {
        name: SKILL,
        says: "load a skill by name",
    },
    Row {
        name: HELP,
        says: "the keys in force",
    },
    Row {
        name: QUIT,
        says: "end the launch",
    },
];

pub const SPLASH: usize = 3;

#[derive(Clone, Debug)]
pub struct Card {
    narrow: Narrow,
}

impl Card {
    pub fn of(draft: &str) -> Option<Card> {
        let words = draft.trim();
        match draft.starts_with('/') && recognised(word(words)) {
            true => Some(Card {
                narrow: Narrow::asked(words),
            }),
            false => None,
        }
    }

    pub fn rows(&self) -> Vec<Row> {
        ranked(word(self.words()))
    }

    pub fn words(&self) -> &str {
        self.narrow.query()
    }

    pub fn selected(&self) -> usize {
        self.narrow.mark()
    }

    pub fn body(&self) -> Vec<ScreenRow> {
        list::List {
            query: None,
            said: None,
            items: list::columned(
                self.rows().into_iter().map(|row| (row.name, row.says)),
                Ink::Plain,
                Beside::Own(Ink::Dim),
            ),
            selected: self.narrow.mark(),
            marker: list::MARK,
            chooses: true,
        }
        .rows()
    }

    pub fn spelling(&self) -> String {
        self.spelling_of(self.narrow.mark())
            .unwrap_or_else(|| self.words().to_owned())
    }

    pub fn spelling_of(&self, at: usize) -> Option<String> {
        let chosen = *self.rows().get(at)?;
        let words = self.words();
        Some(match word(words).starts_with(chosen.name) {
            true => words.to_owned(),
            false => format!("{}{}", chosen.name, tail(words)),
        })
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
        self.narrow.typed(key, |words| ranked(word(words)).len())
    }

    fn pressed(&mut self, at: usize) -> bool {
        self.narrow.pressed(at, self.rows().len())
    }
}

fn word(draft: &str) -> &str {
    draft.split_whitespace().next().unwrap_or_default()
}

fn tail(draft: &str) -> &str {
    let word = word(draft);
    match word.find(':') {
        Some(at) => &word[at..],
        None => &draft[word.len()..],
    }
}

fn ranked(asked: &str) -> Vec<Row> {
    let Some(body) = asked.strip_prefix('/') else {
        return Vec::new();
    };
    let query = body.split(':').next().unwrap_or_default();
    leaves(by_name().iter().map(|row| (plain(row), *row)), query)
}

fn plain(row: &Row) -> &'static str {
    row.name.strip_prefix('/').unwrap_or(row.name)
}

fn by_name() -> &'static [Row] {
    static BY_NAME: OnceLock<Vec<Row>> = OnceLock::new();
    BY_NAME.get_or_init(|| {
        let mut rows = TABLE.to_vec();
        rows.sort_unstable_by_key(|row| row.name);
        rows
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    New,
    Skill(String),
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
        SKILL => named(rest),
        other => named(other.strip_prefix(&format!("{SKILL}:"))?),
    }
}

fn named(name: &str) -> Option<Command> {
    match name.is_empty() {
        true => None,
        false => Some(Command::Skill(name.to_owned())),
    }
}

#[cfg(test)]
mod tests;
