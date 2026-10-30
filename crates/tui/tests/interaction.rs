//! The frame's behaviour, driven the way a keyboard drives it: Ctrl-C cancels
//! a run and keeps the draft, a prompt typed mid-run queues, and a failure is
//! content rather than a toast (ADR-0011).

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sindri_core::cancel::Cancel;
use sindri_tui::driver::{CallOver, Finished, Spilled, Stop, Tokens};
use sindri_tui::render;
use sindri_tui::{App, Card, Launch};

fn app() -> App {
    App::new(
        Launch {
            session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
            temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
            branch: Some("main".to_string()),
            sandbox: "bwrap · 1 writable".to_string(),
            workspace: "~/git/sindri".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            max_turns: 250,
            context_limit: Some(200_000),
        },
        None,
    )
}

fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.key(KeyEvent::new(code, modifiers));
}

fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw(app, frame))
        .expect("the frame draws");
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn any(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|row| row.contains(needle))
}

#[test]
fn ctrl_c_cancels_the_run_and_the_composer_keeps_its_draft() {
    let mut app = app();
    assert!(app.start().is_none(), "nothing to run yet");
    type_text(&mut app, "why does this test fail?");
    assert!(app.start().is_none(), "typing alone starts nothing");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let job = app.start().expect("the draft becomes a run");
    assert_eq!(job.prompt, "why does this test fail?");
    assert!(app.is_running(), "the run is in flight");

    type_text(&mut app, "second thoughts");
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);

    assert!(
        job.cancel.is_triggered(),
        "Ctrl-C raises the one flag the loop and the executor share"
    );
    assert_eq!(
        app.composer().text(),
        "second thoughts",
        "the draft survives the cancel"
    );
    assert!(
        any(&screen(&mut app, 100, 20), "working"),
        "the rail says a run is in flight"
    );
}

#[test]
fn a_cancelled_run_reports_cancelled_and_the_composer_stays_usable() {
    let mut app = app();
    type_text(&mut app, "count the files");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.start().expect("a run starts");
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    app.finished(Finished {
        stopped: Stop::Cancelled,
        turns: 1,
        tokens: Tokens::default(),
        error: None,
        note: None,
    });

    let rows = screen(&mut app, 100, 22);
    assert!(
        any(&rows, "cancelled"),
        "the run reports cancelled: {rows:#?}"
    );
    type_text(&mut app, "try again");
    assert_eq!(
        app.composer().text(),
        "try again",
        "the composer still takes text"
    );
}

#[test]
fn a_cancel_between_prompts_ends_that_run_and_not_the_next() {
    let cancel = Cancel::new();
    let mut app = app().with_cancel(cancel.clone());
    type_text(&mut app, "the last question");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let job = app.start().expect("a run starts");

    // A signal and a key raise the same flag, so raising it anywhere raises
    // it for the run in flight (ADR-0015).
    cancel.trigger();
    assert!(
        job.cancel.is_triggered(),
        "the frame's flag is the run's flag"
    );
    app.finished(Finished {
        stopped: Stop::Cancelled,
        turns: 1,
        ..Finished::default()
    });

    type_text(&mut app, "the next question");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let next = app
        .start()
        .expect("the next prompt runs in the same launch");
    assert!(
        !next.cancel.is_triggered(),
        "the flag is put down between prompts, so a cancel ends one run and never the next"
    );
}

#[test]
fn a_prompt_typed_during_a_run_queues_and_is_sent_when_the_run_ends() {
    let mut app = app();
    type_text(&mut app, "first question");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let job = app.start().expect("the first run starts");

    type_text(&mut app, "second question");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.queued(), 1, "the prompt waits instead of interrupting");
    assert!(
        app.start().is_none(),
        "nothing new starts while the first run is in flight"
    );

    app.finished(Finished {
        stopped: Stop::Final,
        turns: 1,
        tokens: Tokens {
            input: 900,
            output: 120,
            cached: 0,
        },
        error: None,
        note: None,
    });
    let next = app.start().expect("the queue is drained, in order");
    assert_eq!(next.prompt, "second question");
    assert_eq!(app.queued(), 0, "the queue keeps nothing after it is sent");
    assert!(!job.cancel.is_triggered(), "queuing is not cancelling");
}

#[test]
fn typing_does_not_send_and_enter_does() {
    let mut app = app();
    type_text(&mut app, "half a thought");
    assert!(
        app.start().is_none(),
        "typing is not sending: a run begins on Enter, not on the first keystroke"
    );
    assert_eq!(app.composer().text(), "half a thought");

    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let job = app.start().expect("Enter sends the draft");
    assert_eq!(job.prompt, "half a thought");
    assert_eq!(app.composer().text(), "", "the composer empties on send");
}

#[test]
fn shift_enter_breaks_the_line_instead_of_sending() {
    let mut app = app();
    type_text(&mut app, "first line");
    press(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    type_text(&mut app, "second line");
    assert_eq!(app.composer().text(), "first line\nsecond line");
    assert_eq!(app.queued(), 0, "a newline is not a send");
}

#[test]
fn a_killed_command_and_a_timeout_each_say_so_where_they_happened() {
    let mut app = app();
    type_text(&mut app, "wait for it");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.start().expect("a run starts");
    app.event(sindri_core::agent::Event::ToolCall {
        turn: 1,
        command: Some("sleep 600".to_string()),
    });
    app.over(CallOver::Cancelled, Spilled::default());
    let rows = screen(&mut app, 100, 20);
    assert!(
        any(&rows, "✗ cancelled"),
        "the call says it was killed: {rows:#?}"
    );

    let mut app = self::app();
    type_text(&mut app, "build it");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.start().expect("a run starts");
    app.event(sindri_core::agent::Event::ToolCall {
        turn: 1,
        command: Some("cargo build".to_string()),
    });
    app.over(CallOver::TimedOut, Spilled::default());
    let rows = screen(&mut app, 100, 20);
    assert!(
        any(&rows, "✗ timed out"),
        "the call says it timed out: {rows:#?}"
    );
}

#[test]
fn a_broken_sandbox_draws_a_card_naming_the_failing_check() {
    let card = Card {
        title: "sindri cannot start: the sandbox does not work here".to_string(),
        check: "bwrap".to_string(),
        detail: "bwrap cannot create a sandbox here: Operation not permitted — allow unprivileged user namespaces".to_string(),
    };
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw_card(&card, frame))
        .expect("the card draws");
    let buffer = terminal.backend().buffer().clone();
    let shown: String = (0..24)
        .map(|y| {
            (0..100)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<String>>()
        .join("\n");
    assert!(
        shown.contains("bwrap"),
        "the failing check is named: {shown}"
    );
    assert!(
        shown.contains("allow unprivileged user namespaces"),
        "and what to do about it: {shown}"
    );
    assert!(
        shown.contains("sindri cannot start"),
        "before anything else draws: {shown}"
    );
}
