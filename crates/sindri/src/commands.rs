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

/// A command a reader sends from the draft, and the one line that says what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: &'static str,
    pub says: &'static str,
}

/// The commands a launch holds: the three the splash screen names first, and then the rest.
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

/// The first `SPLASH` of the table are the commands the splash screen names.
pub const SPLASH: usize = 3;

/// The commands card: the words a draft holds, the rows they leave ranked the way a picker ranks — a name
/// the word starts before a name it holds, and one order for ties — and the row the mark stands on.
#[derive(Clone, Debug)]
pub struct Card {
    narrow: Narrow,
}

impl Card {
    /// The card a draft opens, and `None` unless its first glyph is a slash and a name holds the word
    /// after it.
    pub fn of(draft: &str) -> Option<Card> {
        let words = match draft.starts_with('/') {
            true => draft.trim(),
            false => return None,
        };
        match ranked(word(words)).is_empty() {
            true => None,
            false => Some(Card {
                narrow: Narrow::asked(words),
            }),
        }
    }

    /// The rows the word leaves, in the order the card draws them.
    pub fn rows(&self) -> Vec<Row> {
        ranked(word(self.words()))
    }

    /// The words the card was read from, which are the draft's own.
    pub fn words(&self) -> &str {
        self.narrow.query()
    }

    /// The row the mark stands on.
    pub fn selected(&self) -> usize {
        self.narrow.mark()
    }

    /// The rows the card hands its frame: every row the word leaves with the one line that says what it
    /// does, and the row the mark stands on drawn in the accent.
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

    /// What choosing the marked row sends: the draft itself where its word already spells the name, and
    /// the name followed by what the word left otherwise.
    pub fn spelling(&self) -> String {
        self.spelling_of(self.narrow.mark())
            .unwrap_or_else(|| self.words().to_owned())
    }

    /// The draft as the row at `at` spells it: the row's name followed by what the word left.
    pub fn spelling_of(&self, at: usize) -> Option<String> {
        let chosen = *self.rows().get(at)?;
        let words = self.words();
        Some(match word(words).starts_with(chosen.name) {
            true => words.to_owned(),
            false => format!("{}{}", chosen.name, tail(words)),
        })
    }
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

/// The first word of a draft, which is the word the rows are ranked by.
fn word(draft: &str) -> &str {
    draft.split_whitespace().next().unwrap_or_default()
}

/// What a draft leaves after the word: from a `:` in that word, or the rest of the line after it.
fn tail(draft: &str) -> &str {
    let word = word(draft);
    match word.find(':') {
        Some(at) => &word[at..],
        None => &draft[word.len()..],
    }
}

/// The rows the word after the slash leaves, handed to the rank by name.
fn ranked(asked: &str) -> Vec<Row> {
    let Some(body) = asked.strip_prefix('/') else {
        return Vec::new();
    };
    let query = body.split(':').next().unwrap_or_default();
    leaves(by_name().iter().map(|row| (plain(row), *row)), query)
}

/// The name a query is read against: a command is spelled with a slash and named without one, so a
/// word that holds a slash of its own names no command at all.
fn plain(row: &Row) -> &'static str {
    row.name.strip_prefix('/').unwrap_or(row.name)
}

/// The table in the order its names give, which is the order the rows of one rank come in, sorted once.
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
