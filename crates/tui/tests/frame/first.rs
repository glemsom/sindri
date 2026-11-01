//! The first frame: an empty thread is not a blank page. It names the wire,
//! the model, the workspace and the sandbox it was launched with, says the
//! contract once, and offers two prompts that can be typed as written — and
//! it leaves a placeholder in the composer and a short thread on the floor, so
//! the newest row is the one above the composer rather than a screenful of
//! blank rows below a lone answer.

use super::*;

use crossterm::event::{KeyCode, KeyModifiers};
use sindri_tui::command::{self, Command};
use sindri_tui::render::{composer, hello};
use sindri_tui::theme;

/// The transcript column's own characters, above the composer: the rail keeps
/// its 32 columns and a column of canvas separates them.
const COLUMN: usize = 100 - RAIL;

fn body(rows: &[String]) -> Vec<String> {
    rows.iter()
        .take_while(|row| !row.starts_with('╭'))
        .map(|row| row[..COLUMN.min(row.len())].trim_end().to_string())
        .collect()
}

#[test]
fn the_first_frame_names_the_wire_the_model_the_workspace_and_the_sandbox() {
    let mut app = app();
    let rows = screen(&mut app, 100, 24);
    let column = body(&rows);
    let first = column
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("the first frame has a first line: {rows:#?}"));

    for fact in [
        "◆ sindri",
        "anthropic",
        "claude-sonnet-4-6",
        "~/git/sindri",
        "⬢ bwrap",
    ] {
        assert!(
            first.contains(fact),
            "the first line names {fact:?}, from the launch: {first:?}"
        );
    }
}

#[test]
fn the_first_frame_paints_the_posture_in_the_ink_the_rail_does() {
    assert_eq!(
        posture_ink(&mut app(), 100, 24),
        theme::GOOD,
        "a sandboxed run's posture is good on the first frame too"
    );
    assert_eq!(
        posture_ink(&mut App::new(unsafe_launch(), None), 100, 24),
        theme::BAD,
        "and a run that left the sandbox is painted as one that did"
    );
}

#[test]
fn the_first_frame_says_the_contract_once_in_a_dim_sentence() {
    let mut app = app();
    let rows = screen(&mut app, 100, 24);
    let column = body(&rows);
    let said = column.join(" ");
    assert!(
        said.contains(hello::CONTRACT),
        "the contract is one sentence, wrapped where the column ends: {rows:#?}"
    );

    let row = column
        .iter()
        .position(|row| row.contains("one tool"))
        .unwrap_or_else(|| panic!("the contract is said once: {rows:#?}"));
    let buf = buffer(&mut app, 100, 24);
    let at = column[row]
        .find("one tool")
        .expect("the sentence begins there");
    assert_eq!(
        buf[(at as u16, row as u16)].fg,
        theme::FAINT,
        "the contract is dim: {rows:#?}"
    );
}

#[test]
fn the_first_frame_offers_two_things_to_type_and_help() {
    let mut app = app();
    let rows = screen(&mut app, 100, 24);
    let column = body(&rows);

    for example in hello::EXAMPLES {
        assert!(
            column.iter().any(|row| row.contains(example)),
            "the first frame offers {example:?}: {rows:#?}"
        );
    }
    assert!(
        column.iter().any(|row| row.contains("/help")),
        "/help is offered with them: {rows:#?}"
    );
}

#[test]
fn every_example_the_first_frame_offers_is_accepted_as_written() {
    for example in hello::EXAMPLES {
        match command::parse(example) {
            // Not a command, so it is a prompt: the frame sends exactly it.
            None => {
                let mut app = app();
                type_text(&mut app, example);
                press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
                let job = app
                    .start()
                    .unwrap_or_else(|| panic!("{example:?} starts a run"));
                assert_eq!(job.prompt, example, "an example is a real prompt");
            }
            // A command the frame knows, never one it would answer with
            // "no such command".
            Some(named) => assert!(
                !matches!(named, Command::Unknown(_)),
                "{example:?} is a command the frame runs: {named:?}"
            ),
        }
    }
    assert_eq!(
        command::parse(hello::HELP),
        Some(Command::Help),
        "/help is the command it says it is"
    );
}

#[test]
fn the_placeholder_is_drawn_only_while_the_composer_is_empty() {
    let mut app = app();
    let placeholder = composer::PLACEHOLDER;
    let rows = screen(&mut app, 100, 24);
    assert!(
        any(&rows, placeholder),
        "an empty composer says what can be typed: {rows:#?}"
    );

    type_text(&mut app, "w");
    let rows = screen(&mut app, 100, 24);
    assert!(
        !any(&rows, placeholder),
        "the first keystroke takes it away: {rows:#?}"
    );

    // A draft is a draft, even when it is only a space: the placeholder never
    // sits in front of what is being typed.
    type_text(&mut app, " ");
    let rows = screen(&mut app, 100, 24);
    assert!(
        !any(&rows, placeholder),
        "a draft of a space is still a draft: {rows:#?}"
    );
}

#[test]
fn a_one_row_answer_sits_on_the_row_above_the_composer() {
    let mut app = app();
    app.event(Event::TextDelta {
        turn: 1,
        text: "Two files changed.".to_string(),
    });
    let rows = screen(&mut app, 100, 24);
    let composer = rows
        .iter()
        .position(|row| row.starts_with('╭'))
        .unwrap_or_else(|| panic!("the composer is drawn: {rows:#?}"));

    assert!(
        rows[composer - 1].contains("Two files changed."),
        "the newest row is the one just above the composer, not the top of a \
         blank body: {rows:#?}"
    );
}

#[test]
fn the_first_frame_claims_nothing_the_launch_did_not_say() {
    let mut launch = launch();
    launch.branch = None;
    launch.session = "00000000-1111-2222-3333-444444444444".to_string();
    launch.temp = "/nowhere/sindri/tmp".to_string();
    let mut app = App::new(launch, None);
    let rows = screen(&mut app, 100, 24);
    let column = body(&rows).join(" ");

    for invented in [
        "00000000-1111",
        "/nowhere/sindri/tmp",
        "tokens",
        "branch",
        "1 in",
    ] {
        assert!(
            !column.contains(invented),
            "the first frame claims no {invented:?} the run has not established: {rows:#?}"
        );
    }

    // The rest of the empty frame is the rail, and it says only what a launch
    // and no run establish: nothing spent, no cache to be a share of, and the
    // branch as the dash of one that is not there.
    let rails = rails(&rows, 100);
    for (label, said) in [("tokens", "0 in · 0 out"), ("cache", "—"), ("branch", "—")] {
        assert_eq!(
            value(&rails, label),
            said,
            "the empty frame's figures are its own zeros, never invented: {label:?} in {rows:#?}"
        );
    }
}

#[test]
fn a_narrow_first_frame_still_names_the_wire_the_model_and_the_sandbox() {
    let mut app = app();
    let rows = screen(&mut app, 60, 24);
    let first = rows
        .iter()
        .find(|row| row.starts_with('◆'))
        .unwrap_or_else(|| panic!("the first frame has a first line: {rows:#?}"));

    for fact in ["◆ sindri", "anthropic", "claude-sonnet-4-6", "⬢ bwrap"] {
        assert!(
            first.contains(fact),
            "at sixty columns the first line still names {fact:?}: {first:?}"
        );
    }
    assert!(
        first.contains('…'),
        "the directory is the fact that gives way, shortened and not cut: {first:?}"
    );
}

#[test]
fn a_narrow_first_frame_offers_its_two_examples_and_help_without_cutting_one() {
    let mut app = app();
    let rows = screen(&mut app, 60, 24);
    for offer in hello::EXAMPLES.into_iter().chain([hello::HELP]) {
        let row = rows
            .iter()
            .find(|row| row.contains(offer))
            .unwrap_or_else(|| {
                panic!("at sixty columns the first frame offers {offer:?}: {rows:#?}")
            });
        assert!(
            !row.contains('…'),
            "the offer is whole rather than cut: {row:?} in {rows:#?}"
        );
    }
}
