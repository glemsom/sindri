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

/// The commands a launch holds: the three the front door names first, and then the rest.
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

/// The first `DOOR` of the table are the commands the front door names.
pub const DOOR: usize = 3;

/// The commands card: the draft read as a word after a slash, and the rows that word leaves, ranked
/// the way a picker ranks — a name the word starts before a name it holds, and one order for ties.
pub struct Card {
    draft: String,
    word: String,
    tail: String,
    rows: Vec<Row>,
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
        let mut held: Vec<(u8, Row)> = TABLE
            .iter()
            .filter_map(|row| {
                let name = row
                    .name
                    .strip_prefix('/')
                    .unwrap_or_default()
                    .to_lowercase();
                match (name.starts_with(&asked), name.contains(&asked)) {
                    (true, _) => Some((0, *row)),
                    (false, true) => Some((1, *row)),
                    (false, false) => None,
                }
            })
            .collect();
        if held.is_empty() {
            return None;
        }
        held.sort_by(|(left, one), (right, other)| left.cmp(right).then(one.name.cmp(other.name)));
        let tail = match word.find(':') {
            Some(at) => &word[at..],
            None => &draft[word.len()..],
        };
        Some(Card {
            draft: draft.to_owned(),
            word: word.to_owned(),
            tail: tail.to_owned(),
            rows: held.into_iter().map(|(_, row)| row).collect(),
        })
    }

    /// The rows the word leaves, in the order the card draws them.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The row the draft names: the first, because the card ranks the row the reader means there
    /// and `Enter` takes it.
    pub fn chosen(&self) -> Option<Row> {
        self.rows.first().copied()
    }

    /// What choosing the first row sends: the draft itself where its word already starts the name,
    /// and the name followed by what the word left otherwise.
    pub fn spelling(&self) -> String {
        self.spelling_of(0).unwrap_or_else(|| self.draft.clone())
    }

    /// The draft as the row at `at` spells it: the row's name followed by what the word left.
    pub fn spelling_of(&self, at: usize) -> Option<String> {
        let chosen = self.rows.get(at)?;
        Some(match self.word.starts_with(chosen.name) {
            true => self.draft.clone(),
            false => format!("{}{}", chosen.name, self.tail),
        })
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

    fn spelling(draft: &str) -> String {
        Card::of(draft).expect("a card holds this draft").spelling()
    }

    /// The rows a draft leaves, by name, in the order the card draws them.
    fn names(draft: &str) -> Vec<&'static str> {
        Card::of(draft)
            .map(|card| card.rows().iter().map(|row| row.name).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_bare_slash_holds_every_command_in_one_order() {
        assert_eq!(
            names("/"),
            [HELP, MODEL, NEW, QUIT, SETTINGS, SKILL, STATS],
            "every command starts the empty word, so the list is the table in the one order its \
             names give it"
        );
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
    fn the_word_the_draft_names_is_sent_as_the_command_it_stands_for() {
        assert_eq!(spelling("/mod"), "/model");
        assert_eq!(spelling("/model"), "/model");
        assert_eq!(spelling("/model gpt-5"), "/model gpt-5");
        assert_eq!(spelling("/skill:commit"), "/skill:commit");
        assert_eq!(spelling("/qui"), "/quit");
        assert_eq!(spelling("/quit "), "/quit");
        assert_eq!(
            spelling("/"),
            HELP,
            "a bare slash names the first row of the list, which is the one `Enter` sends"
        );
    }

    #[test]
    fn every_row_of_the_card_spells_the_line_it_sends() {
        let card = Card::of("/n").expect("a card holds this word");

        assert_eq!(
            card.spelling_of(0),
            Some(String::from(NEW)),
            "the first row the card draws is the line `Enter` sends"
        );
        assert_eq!(
            card.spelling_of(1),
            Some(String::from(SETTINGS)),
            "and a row under it spells its own name with what the word left"
        );
        assert_eq!(
            card.spelling_of(2),
            None,
            "a row the card does not hold spells nothing at all"
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
    fn the_seven_names_the_table_holds_are_seven_different_names() {
        let names: Vec<&str> = TABLE.iter().map(|row| row.name).collect();
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
        for row in TABLE.iter().filter(|row| row.name != SKILL) {
            assert!(
                read(row.name).is_some(),
                "the list names {:?} and a line of it is no command the frame answers",
                row.name
            );
        }
        assert!(
            read(SKILL).is_none(),
            "a skill takes a word after it, so the bare name is not a command of its own"
        );
    }
}
