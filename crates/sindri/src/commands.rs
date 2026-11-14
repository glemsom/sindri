const NEW: &str = "/new";

const SKILL: &str = "/skill";

const MODEL: &str = "/model";

const SETTINGS: &str = "/settings";

const HELP: &str = "/help";

const STATS: &str = "/stats";

const QUIT: &str = "/quit";

/// A command a reader sends from the draft, and the one line that says what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Said {
    pub name: &'static str,
    pub says: &'static str,
}

/// The commands a launch holds: the three the front door names first, and then the rest.
pub const LIST: [Said; 7] = [
    Said {
        name: MODEL,
        says: "ask the wire what it serves, and name the one to run",
    },
    Said {
        name: SETTINGS,
        says: "the keys the config holds, and the value each is set to",
    },
    Said {
        name: STATS,
        says: "the facts the rail is too narrow to hold",
    },
    Said {
        name: NEW,
        says: "start the conversation again, and forget the last one",
    },
    Said {
        name: SKILL,
        says: "load a skill by name",
    },
    Said {
        name: HELP,
        says: "the keys in force",
    },
    Said {
        name: QUIT,
        says: "end the launch",
    },
];

/// How many of the list the front door names.
pub const NAMED: usize = 3;

/// The commands card: the draft read as a word after a slash, and the rows that word leaves, ranked
/// the way a picker ranks — a name the word starts before a name it holds, and one order for ties.
pub struct Card {
    draft: String,
    word: String,
    tail: String,
    rows: Vec<Said>,
}

impl Card {
    /// The card a draft opens, and `None` unless its first glyph is a slash and a name holds the
    /// word after it.
    pub fn of(draft: &str) -> Option<Card> {
        let draft = match draft.starts_with('/') {
            true => draft.trim(),
            false => return None,
        };
        let word = draft.split_whitespace().next().unwrap_or_default();
        let body = word.strip_prefix('/').unwrap_or_default();
        let asked = body.split(':').next().unwrap_or_default().to_lowercase();
        let mut held: Vec<(u8, Said)> = LIST
            .iter()
            .filter_map(|said| {
                let name = said
                    .name
                    .strip_prefix('/')
                    .unwrap_or_default()
                    .to_lowercase();
                match (name.starts_with(&asked), name.contains(&asked)) {
                    (true, _) => Some((0, *said)),
                    (false, true) => Some((1, *said)),
                    (false, false) => None,
                }
            })
            .collect();
        if held.is_empty() {
            return None;
        }
        held.sort_by(|(left, one), (right, other)| left.cmp(right).then(one.name.cmp(other.name)));
        let tail = match word.find(':') {
            Some(at) => &draft[at..],
            None => &draft[word.len()..],
        };
        Some(Card {
            draft: draft.to_owned(),
            word: word.to_owned(),
            tail: tail.to_owned(),
            rows: held.into_iter().map(|(_, said)| said).collect(),
        })
    }

    /// The rows the word leaves, in the order the card draws them.
    pub fn rows(&self) -> &[Said] {
        &self.rows
    }

    /// The row the draft names: the first, because the card ranks the row the reader means there
    /// and `Enter` takes it.
    pub fn chosen(&self) -> Option<Said> {
        self.rows.first().copied()
    }

    /// The line choosing the first row sends: the draft itself where its word already starts the
    /// name, and the name followed by what the word left otherwise.
    pub fn line(&self) -> String {
        let Some(chosen) = self.chosen() else {
            return self.draft.clone();
        };
        match self.word.starts_with(chosen.name) {
            true => self.draft.clone(),
            false => format!("{}{}", chosen.name, self.tail),
        }
    }
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
mod tests {
    use super::*;

    fn line(draft: &str) -> String {
        Card::of(draft).expect("a card holds this draft").line()
    }

    /// The rows a draft leaves, by name, in the order the card draws them.
    fn names(draft: &str) -> Vec<&'static str> {
        Card::of(draft)
            .map(|card| card.rows().iter().map(|said| said.name).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_bare_slash_holds_every_command_in_one_order() {
        assert_eq!(names("/"), [HELP, MODEL, NEW, QUIT, SETTINGS, SKILL, STATS]);
    }

    #[test]
    fn the_list_narrows_as_the_word_is_typed() {
        assert_eq!(names("/s"), [SETTINGS, SKILL, STATS]);
        assert_eq!(
            names("/st"),
            [STATS],
            "a name that starts with the word is not one that holds it, so `/st` leaves `/stats` \
             alone"
        );
    }

    #[test]
    fn a_name_the_word_starts_outranks_a_name_it_holds() {
        assert_eq!(
            names("/n"),
            [NEW, SETTINGS],
            "`/settings` holds the word and `/new` starts with it, so the row that starts goes first"
        );
        assert_eq!(
            names("/N"),
            [NEW, SETTINGS],
            "the word and the name are read in one case, so a reader who types in capitals \
             narrows the list as one who does not"
        );
    }

    #[test]
    fn a_word_no_command_holds_holds_no_card() {
        for draft in ["/skillet", "/usr", "/etc/hosts"] {
            assert!(
                Card::of(draft).is_none(),
                "{draft:?} names no command, so it is a draft the reader meant to send"
            );
        }
    }

    #[test]
    fn the_line_a_chosen_row_is_sent_as() {
        assert_eq!(line("/mod"), "/model");
        assert_eq!(line("/model"), "/model");
        assert_eq!(line("/model gpt-5"), "/model gpt-5");
        assert_eq!(line("/skill:commit"), "/skill:commit");
        assert_eq!(line("/quit "), "/quit");
    }

    #[test]
    fn the_line_a_card_sends_is_a_command_but_for_the_one_that_takes_a_word() {
        for said in LIST.iter().filter(|said| said.name != SKILL) {
            let named = format!("/{}", &said.name[1..2]);
            assert!(
                read(&line(&named)).is_some(),
                "a row of the card is sent as {:?}, and that line is no command: a reader who \
                 chose the row is handed their words back as a prompt",
                line(&named)
            );
        }
        assert_eq!(
            read(&Card::of("/").expect("a bare slash holds the card").line()),
            Some(Command::Help),
            "the bare slash names the first row, and that row is a line the frame answers"
        );
    }

    #[test]
    fn a_draft_whose_first_glyph_is_not_a_slash_holds_no_card() {
        for draft in ["and/or", " /mod", ""] {
            assert!(
                Card::of(draft).is_none(),
                "{draft:?} does not begin with a slash, so nothing about it asks for the commands"
            );
        }
    }

    #[test]
    fn the_seven_names_the_list_holds_are_seven_different_names() {
        let names: Vec<&str> = LIST.iter().map(|said| said.name).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            names.len(),
            "two rows of the list name one command, so one of them can never be chosen: {names:?}"
        );
    }

    #[test]
    fn every_name_but_the_one_that_takes_a_word_reads_back_as_a_command() {
        for said in LIST.iter().filter(|said| said.name != SKILL) {
            assert!(
                read(said.name).is_some(),
                "the list names {:?} and a line of it is no command the frame answers",
                said.name
            );
        }
        assert!(
            read(SKILL).is_none(),
            "a skill takes a word after it, so the bare name is not a command of its own"
        );
    }
}
