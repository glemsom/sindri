//! The `/` commands and the one catalogue of them (ADR-0011 §14): a name, the
//! one line it is said in, and the command it opens. The parser, the line an
//! unknown name is refused with and `?`'s command row all read it, so a
//! seventh command is added in one place (issue #53). `/settings` is one of
//! the two that write the user's disk, and it writes through the schema the run
//! reads; `/export` writes one markdown file, where it is told to.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    New,
    Settings,
    Help,
    Quit,
    /// The record picker: what every run left, read-only (ADR-0006).
    Sessions,
    /// The thread as markdown, at the path named or the workspace's default.
    Export(Option<String>),
    Unknown(String),
}

/// One `/` command: the name a draft spells after its slash, the line `?` says
/// of it, and the command that name opens.
pub struct Spec {
    pub name: &'static str,
    pub summary: &'static str,
    /// What the name opens, given the rest of the line. Only `/export` reads
    /// it: a path is the rest of the line, spaces and all.
    open: fn(Option<String>) -> Command,
}

/// Every `/` command, in the order the frame says them. This is the one list
/// of names: a seventh command is added here and nowhere else (issue #53).
pub const COMMANDS: &[Spec] = &[
    Spec {
        name: "new",
        summary: "start a session",
        open: |_| Command::New,
    },
    Spec {
        name: "sessions",
        summary: "read an old one",
        open: |_| Command::Sessions,
    },
    Spec {
        name: "export",
        summary: "export this one",
        open: Command::Export,
    },
    Spec {
        name: "settings",
        summary: "edit the config",
        open: |_| Command::Settings,
    },
    Spec {
        name: "help",
        summary: "show this",
        open: |_| Command::Help,
    },
    Spec {
        name: "quit",
        summary: "leave",
        open: |_| Command::Quit,
    },
];

pub fn parse(draft: &str) -> Option<Command> {
    let draft = draft.trim();
    let rest = draft.strip_prefix('/')?;
    let name = rest.split_whitespace().next().unwrap_or_default();
    if name.is_empty() {
        return None;
    }
    let argument = rest[name.len()..].trim();
    Some(match spec(name) {
        Some(spec) => (spec.open)((!argument.is_empty()).then(|| argument.to_string())),
        None => Command::Unknown(format!("/{name}")),
    })
}

/// The catalogue's entry for a name, if it has that name: what `/` completion
/// offers, and what the parser opens.
pub fn spec(name: &str) -> Option<&'static Spec> {
    COMMANDS.iter().find(|spec| spec.name == name)
}

/// The names as the frame says them, `/` and all — the line an unknown command
/// is refused with, and the keys of `?`'s command row.
pub fn names() -> String {
    joined(COMMANDS.iter().map(|spec| format!("/{}", spec.name)))
}

/// `?`'s command row: the names, and what each of them does, in that order —
/// the two halves built from the one walk, so they cannot pair the wrong way.
pub fn row() -> (String, String) {
    let names = names();
    let summaries = joined(COMMANDS.iter().map(|spec| spec.summary.to_string()));
    (names, summaries)
}

/// What a name the catalogue has no entry for is answered with: that there is
/// no such command, and the ones there are. They are said in the catalogue's
/// order, which is the order `?` says them in: the hand-written line this
/// replaces had drifted to `/sessions, /export, /new, …`, so one order had to
/// win and the frame's is the help row's (issue #53).
pub fn refusal(name: &str) -> String {
    format!("{name}: no such command — {}", names())
}

fn joined(words: impl Iterator<Item = String>) -> String {
    words.collect::<Vec<String>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::{COMMANDS, Command, parse, refusal};

    #[test]
    fn the_three_commands_are_recognised() {
        assert_eq!(parse("/new"), Some(Command::New));
        assert_eq!(parse("  /help  "), Some(Command::Help));
        assert_eq!(parse("/quit"), Some(Command::Quit));
    }

    #[test]
    fn the_record_commands_are_recognised_with_and_without_a_path() {
        assert_eq!(parse("/sessions"), Some(Command::Sessions));
        assert_eq!(parse("/export"), Some(Command::Export(None)));
        assert_eq!(
            parse("  /export  notes/gate two.md  "),
            Some(Command::Export(Some("notes/gate two.md".to_string()))),
            "a path is the rest of the line, spaces and all"
        );
    }

    /// The command each name opens, written here from the enum rather than
    /// read out of the catalogue: a name paired with the wrong command is a
    /// failure, not a tautology (issue #53).
    const OPENS: &[(&str, Command)] = &[
        ("new", Command::New),
        ("sessions", Command::Sessions),
        ("export", Command::Export(None)),
        ("settings", Command::Settings),
        ("help", Command::Help),
        ("quit", Command::Quit),
    ];

    #[test]
    fn every_name_in_the_catalogue_opens_the_command_it_is_paired_with() {
        assert_eq!(
            OPENS.len(),
            COMMANDS.len(),
            "the catalogue is these and no others"
        );
        for (name, opens) in OPENS {
            assert_eq!(
                parse(&format!("/{name}")),
                Some(opens.clone()),
                "/{name} opens {opens:?}"
            );
        }
    }

    #[test]
    fn the_catalogue_names_each_command_once_and_says_what_it_does() {
        let mut names: Vec<&str> = COMMANDS.iter().map(|spec| spec.name).collect();
        let listed = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), listed, "each name is listed once: {names:?}");
        for spec in COMMANDS {
            assert!(!spec.summary.is_empty(), "{} says what it does", spec.name);
            assert!(
                !spec.summary.contains(['.', '\n']),
                "{}'s summary is one line: {:?}",
                spec.name,
                spec.summary
            );
        }
    }

    #[test]
    fn a_refused_command_names_every_command_the_catalogue_has() {
        let line = refusal("/settle");
        assert!(
            line.starts_with("/settle: no such command — "),
            "the refusal says what it is: {line:?}"
        );
        for spec in COMMANDS {
            assert!(
                line.contains(&format!("/{}", spec.name)),
                "the refusal names /{}: {line:?}",
                spec.name
            );
        }
        let listed = line
            .split(" — ")
            .nth(1)
            .expect("the refusal lists the ones there are");
        assert_eq!(
            listed.matches('/').count(),
            COMMANDS.len(),
            "and names nothing the catalogue does not have: {line:?}"
        );
    }

    #[test]
    fn settings_opens_the_config_and_an_unknown_command_says_so() {
        assert_eq!(parse("/settings"), Some(Command::Settings));
        assert_eq!(
            parse("/settle"),
            Some(Command::Unknown("/settle".to_string()))
        );
        assert_eq!(parse("not a command"), None);
        assert_eq!(parse("/"), None);
    }
}
