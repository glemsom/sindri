//! The `/` commands: a draft line the frame answers itself instead of handing it to a
//! run — a conversation started again, a skill loaded, a model named, a screen opened,
//! the launch ended (§12, §5).
//!
//! Reading is one function, because every spelling of one command has to be one value:
//! `/skill:name` and `/skill name` are the same line (§5), and a `/` inside a word is
//! nothing at all. What the value then comes to is `view`'s to decide, and whether the
//! name in it is a name at all is the catalog's — nothing here looks at a skill, a wire
//! or a config, so a command line is read the same way on every frame.
//!
//! A line that names no command is not a refusal: it is a prompt like any other, and the
//! run reads it. The frame has no band to refuse a line in, and a reader who typed
//! `/skillet` asked the model something rather than losing their words (§12, §15).

/// The one line that starts a conversation again.
const NEW: &str = "/new";

/// The command that loads a skill, and the colon `/skill:name` hangs the name on.
const SKILL: &str = "/skill";

/// The command that offers the names the wire serves, and the one that names a model by
/// hand when the wire never offered one.
const MODEL: &str = "/model";

/// The command that opens the screen the settings modal fills in (#49).
const SETTINGS: &str = "/settings";

/// The command that opens the keys in force.
const HELP: &str = "/help";

/// The command that ends the launch.
const QUIT: &str = "/quit";

/// One command line, read off a draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `/new`: the history and the loaded-skill set go, and the run in flight is left
    /// rather than stopped (§13).
    New,
    /// `/skill:name`, or `/skill name`, which is the same line: one skill, by the name a
    /// person may ask for it by (§5).
    Skill(String),
    /// `/model`: the screen of the names the wire serves (§10, §12).
    Model,
    /// `/model <name>`: a model named by hand, which the wire may never have offered.
    ModelNamed(String),
    /// `/settings`: the screen the settings modal fills in (#49).
    Settings,
    /// `/help`: the keys in force, which is the screen `?` opens.
    Help,
    /// `/quit`: the launch ends and the terminal is given back.
    Quit,
}

/// What `line` names, or `None` when it names no command and is a prompt like any
/// other.
///
/// The line is read with the whitespace around it dropped, so a `/quit` typed with a
/// space after it is still the key it names, and the word is the first run of
/// non-whitespace: everything after it is the command's one argument. A name the wire
/// never offered is one argument with a space in it, which is why `/model` takes the rest
/// of the line whole rather than its first word.
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
        QUIT => Some(Command::Quit),
        MODEL => Some(match rest.is_empty() {
            true => Command::Model,
            false => Command::ModelNamed(rest.to_owned()),
        }),
        SKILL => named(rest),
        other => named(other.strip_prefix(&format!("{SKILL}:"))?),
    }
}

/// A load of the name after the command, or nothing at all when the line names none: a
/// `/skill` with no name names no skill, so it is a line for the model.
fn named(name: &str) -> Option<Command> {
    match name.is_empty() {
        true => None,
        false => Some(Command::Skill(name.to_owned())),
    }
}
