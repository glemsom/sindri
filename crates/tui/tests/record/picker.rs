//! `/sessions`: the picker over the records, newest first, and the reader it
//! opens. Reading is not resuming — nothing in this path writes.

use super::*;

#[test]
fn the_picker_lists_three_records_newest_first_with_what_each_one_did() {
    let mut f = Fixture::three_records();
    command(&mut f.app, "/sessions");

    let rows = screen(&mut f.app, 110, 24);
    let text = shown(&rows);
    for wanted in [
        "claude-old",
        "claude-middle",
        "claude-mock",
        "how many files?",
        "rewrite the parser",
        "why is the gate red?",
        "1 turn",
        "final",
        "max_turns",
        "running",
    ] {
        assert!(text.contains(wanted), "the picker says {wanted:?}: {text}");
    }
    let older = row_of(&rows, "claude-old");
    let middle = row_of(&rows, "claude-middle");
    let newest = row_of(&rows, "claude-mock");
    assert!(
        newest < middle && middle < older,
        "newest first: {newest} < {middle} < {older}\n{text}"
    );
}

#[test]
fn a_run_that_never_finished_is_listed_as_running_and_reads_back() {
    let mut f = Fixture::three_records();
    command(&mut f.app, "/sessions");
    let rows = screen(&mut f.app, 110, 24);
    let at = row_of(&rows, "claude-mock");
    assert!(
        rows[at].contains("running"),
        "the record says running and the picker says what the record says: {:?}",
        rows[at]
    );

    // It is the first row, so Enter reads it: a run that never finished is
    // evidence, not a hole in the list.
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    let read = shown(&screen(&mut f.app, 110, 24));
    assert!(
        read.contains("why is the gate red?") && read.contains("did not finish"),
        "the reader shows the unfinished record: {read}"
    );
}

#[test]
fn enter_reads_a_record_and_esc_closes_it_and_nothing_is_written() {
    let mut f = Fixture::three_records();
    let before = f.state();
    command(&mut f.app, "/sessions");
    // Down twice: the oldest of the three.
    for _ in 0..2 {
        press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    }
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        matches!(f.app.overlay(), Some(Overlay::Spill(_))),
        "Enter opens the spill-style reader"
    );
    let read = screen(&mut f.app, 110, 24);
    let text = shown(&read);
    assert!(
        text.contains("how many files?") && text.contains("Twelve files."),
        "the transcript is on screen: {text}"
    );
    assert!(
        text.contains("claude-old"),
        "and it says whose transcript it is: {text}"
    );

    // Scrollable, read-only: the rows move, the record does not.
    press(&mut f.app, KeyCode::PageDown, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::PageUp, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(f.app.overlay().is_none(), "Esc closes the reader");
    assert_eq!(f.state(), before, "no path from the picker writes a record");
    assert!(
        f.sessions.read_dir().expect("the sessions dir").count() == 3,
        "and no record appeared"
    );
}

#[test]
fn the_picker_says_so_when_there_is_nothing_recorded() {
    let state = TempDir::new().expect("a state directory");
    let mut app = App::new(launch("4f1c8f2e"), None)
        .with_sessions(state.path().join("sessions"))
        .with_workspace(state.path());
    command(&mut app, "/sessions");
    assert!(app.overlay().is_none(), "an empty list is not an empty box");
    assert!(
        notes(&app).contains("no sessions"),
        "the transcript says why nothing opened: {}",
        notes(&app)
    );
}

#[test]
fn a_list_longer_than_the_box_windows_to_the_row_the_reader_is_on() {
    let state = TempDir::new().expect("a state directory");
    let mut last = String::new();
    for index in 0..8 {
        last = record(
            state.path(),
            &format!("claude-{index}"),
            &format!("prompt {index}"),
            "an answer",
            Some(Stopped::Final),
        );
    }
    let mut app = App::new(launch(&last), None)
        .with_sessions(state.path().join("sessions"))
        .with_workspace(state.path());
    command(&mut app, "/sessions");

    // Eight records do not fit: the window opens on the newest and follows the
    // cursor down, so a record off the first screen can still be read.
    let rows = screen(&mut app, 110, 14);
    assert!(
        shown(&rows).contains("prompt 7"),
        "the newest is on screen: {}",
        shown(&rows)
    );
    for _ in 0..7 {
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    let rows = screen(&mut app, 110, 14);
    assert!(
        shown(&rows).contains("prompt 0"),
        "the oldest, chosen last, is on screen: {}",
        shown(&rows)
    );
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        shown(&screen(&mut app, 110, 14)).contains("prompt 0"),
        "and Enter reads the row that was chosen"
    );
}
