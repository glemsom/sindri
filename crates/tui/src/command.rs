//! The `/` commands the composer can send (ADR-0011 §8, §14, §12). `/settings`
//! is the one that writes the user's disk, and it writes through the schema the
//! run reads (ADR-0014).

/// A command the composer sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Start over: a fresh session GUID and an empty transcript.
    New,
    /// Edit the one config file, through the one schema.
    Settings,
    /// The list of every binding.
    Help,
    /// Leave the frame.
    Quit,
    /// Something the frame does not know.
    Unknown(String),
}

/// The command a draft is, when it is one: a `/` on the first row and nothing
/// before it. A prompt that starts with a slash mid-sentence is a prompt.
pub fn parse(draft: &str) -> Option<Command> {
    let draft = draft.trim();
    let rest = draft.strip_prefix('/')?;
    let name = rest.split_whitespace().next().unwrap_or_default();
    Some(match name {
        "new" => Command::New,
        "settings" => Command::Settings,
        "help" => Command::Help,
        "quit" => Command::Quit,
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
