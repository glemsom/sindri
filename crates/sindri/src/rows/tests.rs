use super::*;

fn words(row: &Row) -> String {
    row.glyphs()
}

#[test]
fn a_closed_call_that_keeps_nothing_back_states_no_count_at_all() {
    let row = call_row(
        "bash",
        r#"{"command": "ls"}"#,
        Some(Exit::Code(0)),
        Some(2),
        Some(0),
        80,
        None,
    );

    assert_eq!(
        words(&row),
        "✓ bash ls 2 ms",
        "a call that closed and kept nothing back says no count, where `⋯0` is the row claiming \
         it hides none"
    );

    let count = call_row(
        "bash",
        r#"{"command": "ls"}"#,
        Some(Exit::Code(0)),
        Some(2),
        Some(1),
        80,
        None,
    );
    assert_eq!(
        words(&count),
        "✓ bash ls 2 ms ⋯1",
        "and a call that keeps one row back still states the one it keeps"
    );
}
