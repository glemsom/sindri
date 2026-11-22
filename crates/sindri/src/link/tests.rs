use super::*;

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
