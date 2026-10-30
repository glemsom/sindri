//! The `/` commands the composer can send (ADR-0011). `/settings` is one of the
//! two that write the user's disk, and it writes through the schema the run
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

pub fn parse(draft: &str) -> Option<Command> {
    let draft = draft.trim();
    let rest = draft.strip_prefix('/')?;
    let name = rest.split_whitespace().next().unwrap_or_default();
    // `/export` takes the rest of the line as one path: a file name may hold a
    // space, and splitting on it would export somewhere the user did not name.
    let argument = rest[name.len()..].trim();
    Some(match name {
        "new" => Command::New,
        "settings" => Command::Settings,
        "help" => Command::Help,
        "quit" => Command::Quit,
        "sessions" => Command::Sessions,
        "export" => Command::Export((!argument.is_empty()).then(|| argument.to_string())),
        "" => return None,
        other => Command::Unknown(format!("/{other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::{Command, parse};

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
