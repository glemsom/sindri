const NEW: &str = "/new";

const SKILL: &str = "/skill";

const MODEL: &str = "/model";

const SETTINGS: &str = "/settings";

const HELP: &str = "/help";

const STATS: &str = "/stats";

const QUIT: &str = "/quit";

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
