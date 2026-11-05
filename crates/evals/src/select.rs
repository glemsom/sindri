//! Which eval `--only` names, and the refusal a selector earns when it names
//! none of them: a selector is a word of an eval's name — the number the name
//! opens with, or a word of the sentence after it — and never a prefix of
//! either, because `1` bringing eval 10 along is a narrowed suite that runs two
//! evals and says nothing about the second.

/// What a selector named.
#[derive(Debug, PartialEq, Eq)]
pub enum Named<'a> {
    /// No selector was given, so the whole suite runs.
    Every,
    /// The one eval whose name carries the selector as a word.
    One(&'a str),
}

/// The refusal a selector earns, and the reason nothing ran.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused<'a> {
    /// No eval's name carries the selector.
    None { selector: &'a str },
    /// More than one does, and a narrowed suite that runs both is the bug this
    /// refusal replaces.
    Several {
        selector: &'a str,
        names: Vec<&'a str>,
    },
}

impl Refused<'_> {
    /// The one line the suite prints before it leaves the shell with a usage
    /// error, and the words a test can read without a process: a refusal that
    /// named neither the selector nor the evals it caught would leave a
    /// developer guessing which ones ran together.
    pub fn message(&self) -> String {
        match self {
            Refused::None { selector } => {
                format!("sindri-evals: no eval matches `{selector}`")
            }
            Refused::Several { selector, names } => format!(
                "sindri-evals: `{selector}` names {} evals: {} — name one of them",
                names.len(),
                names
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
        }
    }
}

/// Which of `names` the selector names, or the refusal that stops the suite
/// before it runs anything.
pub fn named<'a>(selector: Option<&'a str>, names: &[&'a str]) -> Result<Named<'a>, Refused<'a>> {
    let Some(selector) = selector.map(str::trim) else {
        return Ok(Named::Every);
    };
    let matched: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| names_the(name, selector))
        .collect();
    match matched.as_slice() {
        [] => Err(Refused::None { selector }),
        [only] => Ok(Named::One(only)),
        _ => Err(Refused::Several {
            selector,
            names: matched,
        }),
    }
}

/// Whether `selector` is a word of `name`: whole, so a number is its own eval
/// and not every eval whose number starts with it.
fn names_the(name: &str, selector: &str) -> bool {
    !selector.is_empty() && name.split_whitespace().any(|word| word == selector)
}
