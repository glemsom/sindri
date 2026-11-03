//! An `open` call in the frame: the row prints the target it handed over, and it
//! is over the moment it was announced, because there is no exit the human could
//! watch and no output this process ever saw.

use super::*;
use sindri_core::provider::Target;

/// The one call the model makes to the third tool.
fn opened(app: &mut App, turn: u32, target: &str) {
    app.event(Event::ToolCall {
        turn,
        asked: Asked::Open {
            target: Target::Url(target.to_string()),
        },
    });
}

#[test]
fn an_open_call_prints_its_target_and_closes_on_the_announcement() {
    let mut app = app();
    opened(&mut app, 1, "https://example.com/report.html");
    app.event(Event::TextDelta {
        turn: 1,
        text: "I put the report in front of you.".to_string(),
    });
    app.finished(ended(Stop::Final, 1, Usage::default()));
    let rows = screen(&mut app, 100, 30);

    let at = row_of(&rows, "https://example.com/report.html");
    assert!(
        rows[at].contains("announced"),
        "a call that only hands a target over has no exit to watch, so its row is \
         already over and says it was announced: {rows:#?}"
    );
    assert!(
        !rows[at].contains("running"),
        "and it is never left counting as though something were still on its way: {rows:#?}"
    );
}

#[test]
fn an_open_call_claims_no_exit_no_clock_and_no_output() {
    let mut app = app();
    opened(&mut app, 1, "https://example.com/report.html");
    let rows = screen(&mut app, 100, 30);

    let at = row_of(&rows, "https://example.com/report.html");
    assert!(
        !rows[at].contains("exit"),
        "nothing exited in this process, so the row never claims an exit status: {rows:#?}"
    );
    assert!(
        !rows[at].contains("0.0s"),
        "and no clock ran here, so the row does not show the zero it measured: {rows:#?}"
    );
    assert!(
        !any(&rows, "lines"),
        "an open call has no output behind it to open: {rows:#?}"
    );
}
