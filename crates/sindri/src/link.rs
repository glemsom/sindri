//! The terminal's own hyperlink: a run of cells can be a way to somewhere, and the way is written
//! around that run by the backend that draws the frame, because no cell carries a destination.

use std::path::Path;

/// The oldest VTE that places a hyperlink: 0.50, said as a terminal says it.
const OLDEST_VTE: u32 = 5000;

/// One run of a row's cells that is a way to somewhere: the columns it covers, measured from the
/// left edge of the row it was laid out in, and the destination a cell of it opens.
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

    /// The same run `by` columns further right: a row indented under a gutter moves the runs it
    /// names with the words they cover.
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

/// The links a frame names, laid on the rows it drew: what travels beside the cells, because a
/// cell is a glyph and a look and has no room for a destination.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Links {
    laid: Vec<(u16, Link)>,
}

impl Links {
    pub fn none() -> Links {
        Links::default()
    }

    /// Lays the links a row names at the frame's row `row`, from the frame's column `left`: a row
    /// is laid out from its own left edge and drawn where the frame puts it.
    pub fn lay(&mut self, row: u16, left: u16, links: &[Link]) {
        self.laid
            .extend(links.iter().map(|link| (row, link.shifted(left))));
    }

    /// The destination the cell at `row`'s `column` names, when it names one.
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

/// The words of a tool row that are a way to a file, and where they go: the path as the call wrote
/// it, which is how the row's cells are found, and the destination the run's judgement made of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Way {
    written: String,
    destination: String,
}

impl Way {
    /// The path a call wrote and the file the run judged those words to be: the destination is made
    /// from the file, so a way is never two strings in an order somebody has to remember.
    pub fn of(written: impl Into<String>, path: &Path) -> Way {
        Way {
            written: written.into(),
            destination: destination(path),
        }
    }

    /// The path as the words it was written with.
    pub fn written(&self) -> &str {
        &self.written
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }
}

/// Whether a terminal places a hyperlink, read off the names it gives itself: the ones that speak
/// `OSC 8`, and no guess at a terminal that may not, because a destination is never drawn. A VTE
/// terminal says which version it is, and the version that learned `OSC 8` is the oldest that does.
pub fn speaks(term: Option<&str>, program: Option<&str>, vte: Option<&str>) -> bool {
    named(term, program)
        || vte.is_some_and(|version| {
            version
                .trim()
                .parse::<u32>()
                .is_ok_and(|version| version >= OLDEST_VTE)
        })
}

/// Whether a terminal names itself after one this product can vouch for: the one read every
/// capability is asked from, because what a terminal can do is said by the name it gives itself
/// and never guessed at one that says nothing.
pub fn named(term: Option<&str>, program: Option<&str>) -> bool {
    let names_one = |word: &str| {
        let word = word.to_ascii_lowercase();
        [
            "kitty",
            "ghostty",
            "wezterm",
            "iterm",
            "vscode",
            "alacritty",
            "foot",
            "contour",
            "rio",
        ]
        .iter()
        .any(|name| word.contains(name))
    };
    term.is_some_and(names_one) || program.is_some_and(names_one)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ways `links` name, laid at frame row `row` from its left edge.
    fn ways(row: u16, links: &[Link]) -> Links {
        let mut ways = Links::none();
        ways.lay(row, 0, links);
        ways
    }

    #[test]
    fn the_links_of_a_frame_are_read_at_the_cells_they_cover() {
        let links = ways(3, &[Link::new(4, 9, "https://example.test/spec")]);

        assert_eq!(
            links.at(3, 6),
            Some("https://example.test/spec"),
            "a cell the run covers names the destination"
        );
        assert_eq!(links.at(3, 3), None, "the column before it names none");
        assert_eq!(
            links.at(3, 13),
            None,
            "and the run ends after its last column, so the cell past it is no way to anywhere"
        );
        assert_eq!(
            links.at(2, 6),
            None,
            "a row the run was not laid on names nothing: a destination never crosses a row"
        );
    }

    #[test]
    fn a_way_laid_where_its_row_was_drawn_begins_where_the_row_was_drawn() {
        let mut links = Links::none();
        links.lay(3, 5, &[Link::new(0, 4, "https://example.test/spec")]);

        assert_eq!(
            links.at(3, 5),
            Some("https://example.test/spec"),
            "a row laid out from its own edge is drawn from the frame's column, and its way with it"
        );
        assert_eq!(
            links.at(3, 4),
            None,
            "and no cell before the row is a way there"
        );
        assert_eq!(links.at(3, 9), None, "nor any cell past the run");
    }

    #[test]
    fn a_link_that_ends_before_the_next_one_begins_covers_nothing_between_them() {
        let links = ways(0, &[Link::new(0, 3, "one"), Link::new(5, 3, "two")]);

        assert_eq!(links.at(0, 2), Some("one"));
        assert_eq!(
            links.at(0, 3),
            None,
            "the columns between two runs name no destination"
        );
        assert_eq!(links.at(0, 7), Some("two"));
    }

    #[test]
    fn a_file_is_a_way_to_the_path_it_resolved_to_spelled_as_one_uri_word() {
        assert_eq!(
            destination(Path::new("/workspace/notes.html")),
            "file:///workspace/notes.html"
        );
        assert_eq!(
            destination(Path::new("/workspace/my notes.html")),
            "file:///workspace/my%20notes.html",
            "a space in a path is escaped, or the terminal reads two words where there is one"
        );
        assert_eq!(
            destination(Path::new("/workspace/100% done.md")),
            "file:///workspace/100%25%20done.md",
            "and so is the character an escape begins with"
        );
    }

    #[test]
    fn a_terminal_that_places_no_hyperlink_is_told_nothing() {
        assert!(!speaks(None, None, None), "a terminal that says nothing");
        assert!(!speaks(Some("xterm-256color"), None, None), "an xterm");
        assert!(
            speaks(Some("xterm-kitty"), None, None),
            "a terminal that names itself after one that places hyperlinks"
        );
        assert!(
            speaks(Some("xterm-256color"), Some("WezTerm"), None),
            "and one named by the program that runs it"
        );
        assert!(
            speaks(Some("xterm-256color"), None, Some("6000")),
            "a VTE terminal places a hyperlink and says so by its version"
        );
        assert!(
            !speaks(Some("xterm-256color"), None, Some("4900")),
            "a VTE older than the one that learned a hyperlink places none"
        );
        assert!(
            !speaks(Some("xterm-256color"), None, Some("")),
            "and a version nobody set is not a terminal that said so"
        );
    }
}
