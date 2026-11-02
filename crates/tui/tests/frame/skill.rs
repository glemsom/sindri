//! A loaded skill in the frame: its call is a row like any other's, and a row
//! that runs no command is over the moment it is announced rather than counting
//! up until something else ends.

use super::*;
use sindri_core::provider::Asked;

#[test]
fn a_skill_call_is_a_finished_row_that_names_what_it_loaded() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Skill {
            name: "house-style".to_string(),
        },
    });
    app.finished(Finished {
        stopped: Stop::Final,
        turns: 1,
        ..Finished::default()
    });
    let rows = screen(&mut app, 100, 30);

    let row = row_of(&rows, "skill house-style");
    assert!(
        rows[row].contains('✓'),
        "a load has no exit to wait for, so its row is over: {rows:#?}"
    );
}
