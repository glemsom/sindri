//! A call's command is one row of the frame: every run of whitespace in it is
//! one space, so a heredoc or two commands on two lines move no other row, and
//! a command the column cannot hold ends in `…` rather than running past the
//! frame's edge. The row flattens what it draws and leaves the command alone,
//! so the entry — and the record and every export made from it — still holds
//! the line breaks the model wrote.

use super::*;
use sindri_core::exec::{Outcome, ToolResult};
use sindri_core::provider::Asked;
use sindri_tui::entry::{Call, Entry};

/// The command of a call that folds the file it was given, as the model writes
/// one and as this tree's own habits write bash: three lines, one command, a tab
/// before the file and a run of spaces inside the body.
const HEREDOC: &str = "cat <<'EOF' >\tnotes.md\n\n# Notes,  quietly\nEOF";

/// A command longer than the transcript column at a hundred cells.
const LONG: &str = "grep -rn abcdefghijabcdefghijabcdefghijabcdefghijabcdefghijabcdefghij .";

/// A call's own row, as the frame drew it: the one row that opens with the
/// folded marker.
fn call_row(rows: &[String]) -> &String {
    rows.iter()
        .find(|row| row.contains("⎿"))
        .unwrap_or_else(|| panic!("a call has a row of its own: {rows:#?}"))
}

/// The command any call of a turn was announced with.
fn commands(app: &App) -> Vec<String> {
    app.thread()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Call(Call { command, .. }) => command.clone(),
            _ => None,
        })
        .collect()
}

#[test]
fn a_command_of_several_lines_is_drawn_on_one_row_of_the_transcript() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: HEREDOC.to_string(),
        },
    });
    app.output(Side::Stdout, "wrote notes.md");
    app.over(Some(ran(0)));

    let rows = screen(&mut app, 120, 24);
    assert!(
        any(&rows, "cat <<'EOF' > notes.md # Notes, quietly EOF"),
        "the whole command is on one row, every run of whitespace one space: {rows:#?}"
    );
    assert_eq!(
        rows.iter().filter(|row| row.contains("EOF")).count(),
        1,
        "and the command is one row of the frame, not one row per line of it: {rows:#?}"
    );
    assert!(
        any(&rows, "exit 0"),
        "the row still says what the call ended on, after the command it draws: {rows:#?}"
    );
}

#[test]
fn a_command_the_column_cannot_hold_ends_in_an_ellipsis_inside_the_column() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: LONG.to_string(),
        },
    });
    app.over(Some(ran(0)));

    let rows = screen(&mut app, 100, 24);
    let row = call_row(&rows);
    // The transcript column's cells, and the canvas between it and the rail:
    // the command fills the column it was given and takes no cell past it.
    let shown: String = row.chars().take(MARGIN).collect();
    assert!(
        shown.ends_with('…'),
        "a cut command says it is an excerpt, in the column's last cell: {row:?}"
    );
    assert!(
        row.chars()
            .skip(MARGIN)
            .take(render::GAP_COLUMNS as usize)
            .all(|ch| ch == ' '),
        "and no cell of it is drawn past the column, in the canvas before the rail: {row:?}"
    );
}

#[test]
fn a_refused_calls_command_is_cut_at_the_column_and_not_at_the_status_field() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: LONG.to_string(),
        },
    });
    app.over(Some(ToolResult {
        outcome: Outcome::SandboxError {
            detail: "bwrap could not start".to_string(),
        },
        stdout: String::new(),
        stderr: String::new(),
        truncated: false,
        spill: Vec::new(),
        duration_ms: 1,
    }));

    let rows = screen(&mut app, 100, 24);
    let row = call_row(&rows);
    assert!(
        row.contains("sandbox: bwrap could not start"),
        "the reason the sandbox gave is on the row whole: {row:?}"
    );
    assert!(
        row.chars().take(MARGIN).collect::<String>().ends_with('…'),
        "the command behind a reason longer than its field is still cut at the column: {row:?}"
    );
}

#[test]
fn a_running_call_and_the_same_call_collapsed_put_their_commands_on_one_column() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: "cargo test --workspace".to_string(),
        },
    });
    app.output(Side::Stdout, "running 12 tests");
    let running = before_command(&screen(&mut app, 100, 24));

    app.over(Some(ran(0)));
    let collapsed = before_command(&screen(&mut app, 100, 24));

    assert_eq!(
        running, collapsed,
        "the status and the duration keep their columns, so a call that is running and \
         the same call collapsed line their commands up"
    );
}

#[test]
fn the_command_a_call_was_announced_with_keeps_its_line_breaks() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        asked: Asked::Bash {
            command: HEREDOC.to_string(),
        },
    });

    let _ = screen(&mut app, 120, 24);
    assert_eq!(
        commands(&app),
        vec![HEREDOC.to_string()],
        "the row flattens what it draws and never the command it was given"
    );
}

/// The cells the frame puts before the command on a call's row: the marker, the
/// status and the duration. Counted in characters, which are cells here because
/// every glyph the row draws before its command is one cell wide.
fn before_command(rows: &[String]) -> usize {
    let row = call_row(rows);
    let at = row
        .find("cargo test")
        .unwrap_or_else(|| panic!("the call's row holds its command: {row:?}"));
    row[..at].chars().count()
}
