use super::*;

#[test]
fn the_first_whole_line_is_the_word_and_the_lines_under_it_are_none_of_it() {
    assert_eq!(
        first_line(b"no handler for .html\ntry a browser\n"),
        "no handler for .html",
        "a reader is handed the reason and not a wall of the same complaint"
    );
}

#[test]
fn a_line_written_with_no_newline_is_still_one_line() {
    assert_eq!(
        first_line(b"  no handler for .html  "),
        "no handler for .html",
        "an opener that ends its words without a newline still said them"
    );
}

#[test]
fn a_line_something_else_is_still_writing_past_the_last_newline_is_no_line() {
    assert_eq!(
        first_line(b"first line\nno handler for .ht"),
        "first line",
        "a line still being written when the grace ran out is not a line the opener finished"
    );
}

#[test]
fn a_stream_that_said_nothing_has_no_line() {
    assert_eq!(first_line(b""), "");
    assert_eq!(first_line(b"\n\n"), "");
}
