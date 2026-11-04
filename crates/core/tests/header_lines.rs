//! One line of an extra-headers table, judged on its own: the sentence a save
//! would refuse it with and the line it is about, asked of the table so a screen
//! can ask it while the line is typed. The sentences are the run's own — the
//! whole-table check is these lines read in order — so a pane can never say
//! something a save would not, and a save can never refuse what the pane drew as
//! legal.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use sindri_core::config::{Document, Key, refusal};
use tempfile::TempDir;

/// A document at a temporary path, so a save here writes nothing a user reads.
fn document() -> (TempDir, Document) {
    let dir = TempDir::new().expect("a temp directory");
    let path = dir.path().join("sindri/config.toml");
    let document = Document::load_at(&path).expect("the document loads");
    (dir, document)
}

/// What a save makes of the same table, which is the sentence every line's
/// answer is held to: one check and one wording, or the pane and the file
/// disagree about what a header is.
fn saved(table: &str) -> String {
    let (_dir, mut document) = document();
    document
        .set(Key::ExtraHeaders, table)
        .expect_err("the table is one a save refuses")
        .message()
        .to_string()
}

/// The line the table is refused for, and the sentence it earns.
fn refused(table: &str) -> (usize, String) {
    refusal(table).expect("the table is one a save refuses")
}

#[test]
fn a_line_no_name_and_value_could_be_is_refused_by_its_place() {
    let table = "x-1 1";
    let (line, said) = refused(table);
    assert_eq!(line, 0, "the only line of the table is the one refused");
    assert_eq!(
        said, "extra_headers: entry 1 is not `Name: value`",
        "a line with no colon is no header, and the sentence names where it is"
    );
    assert_eq!(
        said,
        saved(table),
        "and it is the sentence the same table earns from a save"
    );
}

#[test]
fn a_name_the_wire_owns_is_refused_on_the_line_that_names_it() {
    let table = "x-1: 1\ncontent-type: text/plain";
    let (line, said) = refused(table);
    assert_eq!(line, 1, "the line above it is a header the wire takes");
    assert_eq!(
        said,
        saved(table),
        "the sentence is the run's, word for word"
    );
    assert!(
        said.contains("belongs to the request the wire writes"),
        "and it says why the run will not send it: {said}"
    );
}

#[test]
fn a_name_set_twice_is_refused_at_the_line_that_repeats_it() {
    let table = "x-1: 1\nX-1: 2";
    let (line, said) = refused(table);
    assert_eq!(
        line, 1,
        "the first spelling of a name is the header, so the repeat is refused"
    );
    assert_eq!(said, saved(table), "the same sentence a save states");
    assert!(
        said.contains("`x-1` is set twice"),
        "names are case-insensitive, so the second spelling is the duplicate: {said}"
    );
}

#[test]
fn a_value_the_wire_refuses_names_the_header_and_never_the_value() {
    let table = "x-1: a\u{7}b";
    let (_, said) = refused(table);

    assert_eq!(said, saved(table), "the sentence is the run's own");
    assert!(
        said.contains("x-1"),
        "a refusal names the header it is about: {said}"
    );
    assert!(
        !said.contains('\u{7}'),
        "and never the value, which may be the secret: {said}"
    );
}

#[test]
fn the_line_refused_is_the_line_a_save_states_and_never_one_it_does_not() {
    // A save splits every line before it judges any name or value, so the line
    // with no colon is the sentence for the whole table — and the wire-owned
    // line above it is never the one the save names. The pane asks about the
    // table on every frame and must still say exactly this.
    let table = "content-type: text/plain\nx-2 2";
    let (line, said) = refused(table);
    assert_eq!(
        line, 1,
        "a save never states the line above: the table stops at the line below it"
    );
    assert_eq!(
        said, "extra_headers: entry 2 is not `Name: value`",
        "the line no `Name: value` could be is the one refused"
    );
    assert_eq!(
        said,
        saved(table),
        "and it is the save's own sentence for the table"
    );
}

#[test]
fn a_table_a_save_would_keep_earns_no_sentence() {
    let table = "x-1: 1\n\nx-2: a=1; b=2";
    assert_eq!(
        refusal(table),
        None,
        "every line is one a save keeps, or the blank line it drops"
    );
    assert_eq!(
        refusal(""),
        None,
        "and a table with no line at all is nothing to refuse"
    );
}

#[test]
fn a_blank_line_is_a_place_to_type_and_never_a_refusal() {
    let table = "x-1: 1\n\nx 2: 2";
    let (line, said) = refused(table);
    assert_eq!(
        line, 2,
        "the blank line is the place the next header is typed, and the line under it is the one refused"
    );
    assert_eq!(
        said, "extra_headers: `x 2` is not an HTTP header name",
        "judged by the place it holds among headers: a blank line is no entry"
    );
}

#[test]
fn a_line_with_no_colon_is_counted_among_headers_and_not_among_blank_lines() {
    // The sentence names an entry by its place among the headers a save reads,
    // while the screen marks a line by its place among the text: a blank line
    // between them makes the two numbers differ, and the sentence keeps the
    // entry's.
    let table = "x-1: 1\n\nx-2 2";
    let (line, said) = refused(table);
    assert_eq!(line, 2, "the third line of the table is the one refused");
    assert_eq!(
        said, "extra_headers: entry 2 is not `Name: value`",
        "and it is the second header the save counted, the blank line being none"
    );
}
