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

#[test]
fn the_facts_a_tally_states_are_joined_by_the_gap_the_rail_states_its_own_with() {
    let tally = Tally {
        calls: 2,
        failed: 1,
        files: BTreeSet::from([String::from("rows.rs")]),
        elapsed_ms: 2_400,
        spent: Usage {
            input: Some(48_200),
            output: Some(3_100),
            ..Usage::default()
        },
    };

    let words = tally.words(120);
    assert_eq!(
        words,
        [
            "2 calls", "1 failed", "1 file", "2.4 s", "48.2k in", "3.1k out"
        ]
        .join(ledger::GAP),
        "the separator between two facts of a tally is the rail's one gap, so one figure wears one \
         spelling across the frame"
    );
    assert_eq!(
        tally.bill(),
        Some(["48.2k in", "3.1k out"].join(ledger::GAP)),
        "and the bill's two figures are parted by the same gap"
    );
}
