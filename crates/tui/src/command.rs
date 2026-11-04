//! The `/` commands and the one catalogue of them: a name, the
//! one line it is said in, and the command it opens. The parser, the line an
//! unknown name is refused with and `?`'s command row all read it, so a
//! seventh command is added in one place. `/settings` and `/model` are the two
//! that write the user's disk, and they write through the one schema the run
//! reads; `/export` writes one markdown file, where it is told to; `/skill` is
//! the one name that takes an argument from a fixed set, and the only one a
//! colon may end — `/skill:house-style` is `/skill house-style`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    New,
    /// The model this run asks: `/model` is the screen of the names the wire
    /// offers, and `/model <name>` a name the wire never offered, which needs no
    /// screen to be written.
    Model(Option<String>),
    Settings,
    Help,
    Quit,
    /// The thread as markdown, at the path named or the workspace's default.
    Export(Option<String>),
    /// `/skill:name`, or `/skill name` — and the rest of the line, if there is
    /// one, as the prompt the skill is put to work on. `None` is the bare name:
    /// a question about the catalog rather than a load.
    Skill(Option<SkillAsk>),
    Unknown(String),
}

/// A `/skill` line once its name is read: which skill, and the tail of the line
/// when the reader wrote one. A bare name arms the skill and starts no turn; a
/// tail becomes the prompt the run answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillAsk {
    pub name: String,
    pub prompt: Option<String>,
}

impl SkillAsk {
    /// The first word is the name and the rest of the line is the prompt,
    /// spaces and all — a prompt is a sentence, not a token.
    pub fn parse(line: &str) -> SkillAsk {
        match line.split_once(char::is_whitespace) {
            Some((name, prompt)) => SkillAsk {
                name: name.to_string(),
                prompt: Some(prompt.trim().to_string()).filter(|prompt| !prompt.is_empty()),
            },
            None => SkillAsk {
                name: line.to_string(),
                prompt: None,
            },
        }
    }
}

/// The name `/skill` is said under, and the two spellings that end it: the
/// composer switches its offer to skill names on them, and reads them here
/// rather than writing the name a second time.
pub const SKILL: &str = "skill";
pub const SKILL_COLON: &str = "/skill:";
pub const SKILL_SPACE: &str = "/skill ";

/// One `/` command: the name a draft spells after its slash, the line `?` says
/// of it, and the command that name opens.
pub struct Spec {
    pub name: &'static str,
    pub summary: &'static str,
    /// Whether a colon may end the name and begin its argument. `/export:notes`
    /// is not a path and never was: only the one command whose argument is a
    /// name from a fixed set takes the colon spelling.
    inline: bool,
    /// What the name opens, given the rest of the line. Only `/export`,
    /// `/skill` and `/model` read it: a path is the rest of the line, spaces and
    /// all, and so are a prompt and a model's name.
    open: fn(Option<String>) -> Command,
}

/// Every `/` command, in the order the frame says them. This is the one list
/// of names: a seventh command is added here and nowhere else.
pub const COMMANDS: &[Spec] = &[
    Spec {
        name: "new",
        summary: "start a session",
        inline: false,
        open: |_| Command::New,
    },
    Spec {
        name: "export",
        summary: "export this one",
        inline: false,
        open: Command::Export,
    },
    Spec {
        name: SKILL,
        summary: "load a skill",
        inline: true,
        open: |argument| Command::Skill(argument.map(|line| SkillAsk::parse(&line))),
    },
    Spec {
        name: "model",
        summary: "choose the model",
        inline: false,
        open: Command::Model,
    },
    Spec {
        name: "settings",
        summary: "edit the config",
        inline: false,
        open: |_| Command::Settings,
    },
    Spec {
        name: "help",
        summary: "show this",
        inline: false,
        open: |_| Command::Help,
    },
    Spec {
        name: "quit",
        summary: "leave",
        inline: false,
        open: |_| Command::Quit,
    },
];

pub fn parse(draft: &str) -> Option<Command> {
    let draft = draft.trim();
    let rest = draft.strip_prefix('/')?;
    let head = rest.split_whitespace().next().unwrap_or_default();
    if head.is_empty() {
        return None;
    }
    // A colon may end the one name that says it can, and what follows it is the
    // first word of the argument: `/skill:house-style do it` is the same line as
    // `/skill house-style do it`.
    let (name, inline) = match head.split_once(':') {
        Some((name, inline)) if spec(name).is_some_and(|spec| spec.inline) => (name, Some(inline)),
        _ => (head, None),
    };
    let rest = rest[head.len()..].trim();
    let argument = match (inline, rest.is_empty()) {
        (Some(inline), true) => inline.to_string(),
        (Some(inline), false) => format!("{inline} {rest}"),
        (None, _) => rest.to_string(),
    };
    Some(match spec(name) {
        Some(spec) => (spec.open)((!argument.is_empty()).then_some(argument)),
        None => Command::Unknown(format!("/{head}")),
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
/// replaces had drifted from it, so one order had to win and the frame's is the
/// help row's.
pub fn refusal(name: &str) -> String {
    format!("{name}: no such command — {}", names())
}

fn joined(words: impl Iterator<Item = String>) -> String {
    words.collect::<Vec<String>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::{COMMANDS, Command, SKILL, SKILL_COLON, SKILL_SPACE, SkillAsk, parse, refusal};

    #[test]
    fn a_command_is_recognised_with_or_without_surrounding_space() {
        assert_eq!(parse("/new"), Some(Command::New));
        assert_eq!(parse("  /help  "), Some(Command::Help));
        assert_eq!(parse("/quit"), Some(Command::Quit));
    }

    #[test]
    fn an_export_is_recognised_with_and_without_a_path() {
        assert_eq!(parse("/export"), Some(Command::Export(None)));
        assert_eq!(
            parse("  /export  notes/gate two.md  "),
            Some(Command::Export(Some("notes/gate two.md".to_string()))),
            "a path is the rest of the line, spaces and all"
        );
    }

    /// The command each name opens, written here from the enum rather than
    /// read out of the catalogue: a name paired with the wrong command is a
    /// failure, not a tautology.
    const OPENS: &[(&str, Command)] = &[
        ("new", Command::New),
        ("export", Command::Export(None)),
        (SKILL, Command::Skill(None)),
        ("model", Command::Model(None)),
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
    fn a_skill_is_named_with_a_colon_or_a_space_and_the_tail_is_its_prompt() {
        let named = SkillAsk {
            name: "house-style".to_string(),
            prompt: None,
        };
        assert_eq!(
            parse("/skill:house-style"),
            Some(Command::Skill(Some(named.clone()))),
            "the colon spelling"
        );
        assert_eq!(
            parse("  /skill house-style  "),
            Some(Command::Skill(Some(named.clone()))),
            "the space spelling"
        );
        let run = SkillAsk {
            name: "house-style".to_string(),
            prompt: Some("tidy the notes/ dir".to_string()),
        };
        assert_eq!(
            parse("/skill:house-style tidy the notes/ dir"),
            Some(Command::Skill(Some(run.clone()))),
            "a tail after the colon spelling is the prompt, spaces and all"
        );
        assert_eq!(
            parse("/skill house-style tidy the notes/ dir"),
            Some(Command::Skill(Some(run))),
            "and a tail after the space spelling is the same prompt"
        );
        assert_eq!(
            parse("/skill"),
            Some(Command::Skill(None)),
            "a bare `/skill` asks which skills there are"
        );
    }

    #[test]
    fn a_colon_ends_the_one_name_that_says_it_may() {
        assert_eq!(
            parse("/export:notes/two.md"),
            Some(Command::Unknown("/export:notes/two.md".to_string())),
            "a path is not a name a colon may end"
        );
        assert_eq!(
            parse("/skill:"),
            Some(Command::Skill(None)),
            "an empty name is no name"
        );
    }

    #[test]
    fn the_skill_spellings_are_the_name_the_catalogue_has() {
        assert_eq!(format!("/{SKILL}:"), SKILL_COLON, "the colon spelling");
        assert_eq!(format!("/{SKILL} "), SKILL_SPACE, "the space spelling");
        assert!(
            super::spec(SKILL).is_some(),
            "the completion switches on a name the catalogue has"
        );
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
