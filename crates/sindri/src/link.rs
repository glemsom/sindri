//! The terminal's own hyperlink: a run of cells can be a way to somewhere, and the way is written
//! around that run by the backend that draws the frame, because no cell carries a destination.

use ratatui::layout::Rect;

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

    pub fn of(laid: impl IntoIterator<Item = (u16, Link)>) -> Links {
        Links {
            laid: laid.into_iter().collect(),
        }
    }

    /// Lays the links a row names at the frame's row `y`, from the frame's column `x`: a row is
    /// laid out from its own left edge and drawn where the frame puts it.
    pub fn lay(&mut self, y: u16, x: u16, links: &[Link]) {
        self.laid.extend(links.iter().map(|link| {
            (
                y,
                Link {
                    left: link.left.saturating_add(x),
                    ..link.clone()
                },
            )
        }));
    }

    /// The destination the cell at `row`'s `column` names, and none where it names one.
    pub fn at(&self, row: u16, column: u16) -> Option<&str> {
        self.laid
            .iter()
            .find(|(y, link)| *y == row && link.covers(column))
            .map(|(_, link)| link.destination.as_str())
    }

    /// No cell of `area` names a destination: what is drawn over a frame is its own, and a way
    /// behind it is not a way a reader can take while it is covered.
    pub fn cover(&mut self, area: Rect) {
        let mut kept = Vec::with_capacity(self.laid.len());
        for (y, link) in self.laid.drain(..) {
            if y < area.y || y >= area.bottom() {
                kept.push((y, link));
                continue;
            }
            let end = link.left.saturating_add(link.width);
            if end <= area.x || link.left >= area.right() {
                kept.push((y, link));
                continue;
            }
            if link.left < area.x {
                kept.push((
                    y,
                    Link {
                        width: area.x - link.left,
                        ..link.clone()
                    },
                ));
            }
            if end > area.right() {
                kept.push((
                    y,
                    Link {
                        left: area.right(),
                        width: end - area.right(),
                        ..link
                    },
                ));
            }
        }
        self.laid = kept;
    }

    pub fn is_empty(&self) -> bool {
        self.laid.is_empty()
    }
}

/// Whether a terminal places a hyperlink, read off the names it gives itself: the ones that speak
/// `OSC 8`, and no guess at a terminal that may not, because a destination is never drawn.
pub fn speaks(term: Option<&str>, program: Option<&str>, vte: Option<&str>) -> bool {
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
    term.is_some_and(names_one)
        || program.is_some_and(names_one)
        || vte.is_some_and(|version| !version.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::*;

    #[test]
    fn the_links_of_a_frame_are_read_at_the_cells_they_cover() {
        let mut links = Links::none();
        links.lay(3, 2, &[Link::new(4, 9, "https://example.test/spec")]);

        assert_eq!(
            links.at(3, 6),
            Some("https://example.test/spec"),
            "a cell the run covers names the destination"
        );
        assert_eq!(links.at(3, 5), None, "the column before it names none");
        assert_eq!(
            links.at(3, 15),
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
    fn a_link_that_ends_before_the_next_one_begins_covers_nothing_between_them() {
        let mut links = Links::none();
        links.lay(0, 0, &[Link::new(0, 3, "one"), Link::new(5, 3, "two")]);

        assert_eq!(links.at(0, 2), Some("one"));
        assert_eq!(
            links.at(0, 3),
            None,
            "the columns between two runs name no destination"
        );
        assert_eq!(links.at(0, 7), Some("two"));
    }

    #[test]
    fn an_area_a_surface_is_drawn_over_names_no_destination() {
        let mut links = Links::none();
        links.lay(0, 0, &[Link::new(2, 10, "https://example.test/spec")]);

        links.cover(Rect::new(0, 0, 8, 24));

        assert_eq!(
            links.at(0, 6),
            None,
            "the cells a card covers are the card's, and the way behind it goes with them"
        );
        assert_eq!(
            links.at(0, 10),
            Some("https://example.test/spec"),
            "and the run the area leaves is still a way to where it named"
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
            !speaks(Some("xterm-256color"), None, Some("")),
            "and a version nobody set is not a terminal that said so"
        );
    }
}
