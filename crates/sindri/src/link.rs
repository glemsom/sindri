use std::path::Path;

/// The oldest VTE that places a hyperlink: 0.50, said as a terminal says it.
const OLDEST_VTE: u32 = 5000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    left: u16,
    width: u16,
    destination: String,
}

impl Link {
    pub fn new(left: u16, width: u16, destination: impl Into<String>) -> Link {
        Link {
            left,
            width,
            destination: destination.into(),
        }
    }

    pub(crate) fn shifted(&self, by: u16) -> Link {
        Link {
            left: self.left.saturating_add(by),
            ..self.clone()
        }
    }

    fn covers(&self, column: u16) -> bool {
        self.left <= column && column < self.left.saturating_add(self.width)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Links {
    laid: Vec<(u16, Link)>,
}

impl Links {
    pub fn none() -> Links {
        Links::default()
    }

    pub fn lay(&mut self, row: u16, left: u16, links: &[Link]) {
        self.laid
            .extend(links.iter().map(|link| (row, link.shifted(left))));
    }

    pub fn at(&self, row: u16, column: u16) -> Option<&str> {
        self.laid
            .iter()
            .find(|(y, link)| *y == row && link.covers(column))
            .map(|(_, link)| link.destination.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.laid.is_empty()
    }
}

/// The destination a judged file is a way to: the terminal's own `file` URL, spelled as a URI so
/// a path with a space in it is one word to the terminal that opens it.
fn destination(path: &Path) -> String {
    let mut url = String::from("file://");
    for glyph in path.display().to_string().chars() {
        match glyph {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' | '/' | ':' | '@' => {
                url.push(glyph);
            }
            other => {
                for byte in other.to_string().bytes() {
                    url.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    url
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Way {
    written: String,
    destination: String,
}

impl Way {
    pub fn of(written: impl Into<String>, path: &Path) -> Way {
        Way {
            written: written.into(),
            destination: destination(path),
        }
    }

    pub fn written(&self) -> &str {
        &self.written
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }
}

/// The terminals that speak `OSC 8`, which are the ones a window and a mark are written to as well.
pub const HYPERLINKS: &[&str] = &[
    "kitty",
    "ghostty",
    "wezterm",
    "iterm",
    "vscode",
    "alacritty",
    "foot",
    "contour",
    "rio",
];

/// Whether a terminal places a hyperlink, read off the names it gives itself: the ones that speak
/// `OSC 8`, and no guess at a terminal that may not, because a destination is never drawn. A VTE
/// terminal says which version it is, and the version that learned `OSC 8` is the oldest that does.
pub fn speaks(term: Option<&str>, program: Option<&str>, vte: Option<&str>) -> bool {
    named(term, program, HYPERLINKS)
        || vte.is_some_and(|version| {
            version
                .trim()
                .parse::<u32>()
                .is_ok_and(|version| version >= OLDEST_VTE)
        })
}

pub fn named(term: Option<&str>, program: Option<&str>, names: &[&str]) -> bool {
    let names_one = |word: &str| {
        let word = word.to_ascii_lowercase();
        names.iter().any(|name| word.contains(name))
    };
    term.is_some_and(names_one) || program.is_some_and(names_one)
}

#[cfg(test)]
mod tests;
