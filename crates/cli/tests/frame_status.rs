//! The frame's **status**: the reports a frame says into the alternate screen
//! while it owns it. Only a pty can prove this — that the bytes went to the tty
//! at all, in the order a terminal reads them, and that they went there without
//! being drawn into a cell — so every test here drives the real binary on one.
//! A report past the year is not a report: the stream is read, never the screen.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::path::PathBuf;
use std::process::Child;
use std::time::{Duration, Instant};

use common::provider::{MockProvider, Reply};
use common::pty::{ALTERNATE_SCREEN, Pty, SHOW_CURSOR, expect, spawn, type_line};
use common::screen::Status;
use common::{Env, TEST_KEY, write_config};

/// The introducer every report follows: `OSC 7501 ;`, and nothing else in a
/// frame is one.
const REPORT: &str = "7501;";
/// The sequence that leaves the alternate screen: what a frame writes after its
/// clear, never before it.
const RESTORED: &str = "\u{1b}[?1049l";

fn config_for(mock: &MockProvider) -> String {
    format!(
        "provider = \"anthropic\"\nmodel = \"model-one\"\nbase_url = \"{}\"\n",
        mock.base_url()
    )
}

fn frame_on(env: &Env, pty: &Pty, mock: &MockProvider) -> Child {
    write_config(env, &config_for(mock));
    let base_url = mock.base_url();
    spawn(
        env,
        pty,
        &[
            ("SINDRI_API_KEY", TEST_KEY),
            ("SINDRI_BASE_URL", base_url.as_str()),
        ],
    )
}

/// The states the frame's reports carried, in the order the terminal read them.
fn states(pty: &Pty) -> Vec<String> {
    states_of(&pty.status())
}

fn states_of(reports: &[Status]) -> Vec<String> {
    reports
        .iter()
        .filter_map(|report| report.state().map(str::to_string))
        .collect()
}

/// Every report's identity, whole and undecoded: what a test reads when the
/// claim is about a value the harness does not parse.
fn bodies(pty: &Pty) -> Vec<String> {
    pty.status()
        .iter()
        .map(|report| report.body().to_string())
        .collect()
}

#[test]
fn a_frame_that_starts_no_run_tells_its_terminal_nothing() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a still frame's silence is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("nothing was asked")]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    // Long enough for a pulse or two of a frame that thought it had news.
    pty.listening(&mut child, Duration::from_millis(400));
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    assert_eq!(
        states(&pty),
        ["clear"],
        "a frame that started no run says nothing, and takes back the record it \
         never made: {:?}",
        bodies(&pty)
    );
}

#[test]
fn a_run_is_working_then_done_and_every_report_is_said_between_two_draws() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a run's reports are not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("the answer")]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, "what changed?");
    // The whole run's stream, from the first frame after the prompt to the
    // answer: a report is a thing only this stream can show.
    let run = pty.wait_for(&mut child, "the answer");
    // `done` is owed the moment the run ends, and the report inside the second
    // the last one went in is held: a terminal is told once a second, so the
    // frame's clock is what delivers the last word.
    let settled = pty.listening(&mut child, Duration::from_millis(1300));
    let (code, leaving) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let states = states(&pty);
    let run_states: Vec<&String> = states.iter().filter(|state| *state != "clear").collect();
    assert_eq!(
        run_states.first().map(|state| state.as_str()),
        Some("working"),
        "a run begins by saying it is one: {states:?}"
    );
    assert_eq!(
        run_states.last().map(|state| state.as_str()),
        Some("done"),
        "and ends by saying the answer is written: {states:?}"
    );
    assert_eq!(states.last().map(String::as_str), Some("clear"));

    // The report's message is the run's own outcome, coarse words of ours.
    let done = pty
        .status()
        .iter()
        .find(|report| report.state() == Some("done"))
        .expect("a done report")
        .body()
        .to_string();
    assert!(
        done.contains(":msg=") && done.contains(":app=sindri") && done.contains(":title="),
        "a report carries the identity and the words: {done:?}"
    );

    // A draw ends by showing the terminal's cursor, so a report written after
    // one arrives after that. Two reports with no draw between them would be a
    // report asked for by a cell the frame had not drawn yet.
    let mut stream = String::from(ALTERNATE_SCREEN);
    stream.push_str(&run);
    stream.push_str(&settled);
    stream.push_str(&leaving);
    // A draw ends by showing the terminal's cursor, so a report written after
    // one has that behind it. The clear is not one of these: it goes out as the
    // frame leaves, with no draw after it and none owed.
    let (mut at, mut reports) = (0, 0);
    while let Some(found) = stream[at..].find(REPORT) {
        let start = at + found;
        let body = &stream[start + REPORT.len()..];
        if !body.starts_with("state=clear") {
            reports += 1;
            assert!(
                stream[at..start].contains(SHOW_CURSOR),
                "report {reports} has no finished draw before it: it is written \
                 from inside one"
            );
        }
        at = start + REPORT.len();
    }
    assert!(reports >= 2, "the run said working and done: {stream:?}");

    // And it is a channel of its own: a terminal that ignores the sequence
    // draws what it always drew, because none of it lands in a cell.
    let drawn = pty.screen();
    for refused in [REPORT, "state=", "app=", "title=", "msg="] {
        assert!(
            !drawn.contains(refused),
            "a report is never drawn: {refused:?} is in the cells: {drawn:?}"
        );
    }
}

#[test]
fn leaving_the_frame_clears_its_record_before_the_alternate_screen_goes() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the frame's clear is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("the answer")]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, "what changed?");
    expect(&pty, &mut child, "the answer");

    let (code, leaving) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    let cleared = leaving
        .find("state=clear")
        .unwrap_or_else(|| panic!("Esc out of the frame says clear: {leaving:?}"));
    let restored = leaving
        .find(RESTORED)
        .unwrap_or_else(|| panic!("the alternate screen is left: {leaving:?}"));
    assert!(
        cleared < restored,
        "the record is taken away while the frame still owns the screen: a clear \
         written after the restore is a clear in the shell's scrollback"
    );
}

#[test]
fn a_cancelled_run_leaves_the_terminal_idle_and_never_working() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a cancel's report is not tested here");
        return;
    };
    let env = Env::new();
    let fragments: Vec<String> = (0..200).map(|index| format!("word{index} ")).collect();
    let fragments: Vec<&str> = fragments.iter().map(String::as_str).collect();
    let mock = MockProvider::start(vec![Reply::streamed_paced(
        common::provider::frames::text(&fragments),
        Duration::from_millis(25),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, "answer slowly");
    expect(&pty, &mut child, "word0");
    common::signal(child.id(), libc::SIGINT);
    expect(&pty, &mut child, "cancelled");
    // The frame is still, and the report it owes is held by the second the
    // last one went in: give it the second, and no more.
    pty.listening(&mut child, Duration::from_millis(1500));
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let states = states(&pty);
    let settled: Vec<&String> = states
        .iter()
        .filter(|state| !matches!(state.as_str(), "working" | "clear"))
        .collect();
    assert_eq!(
        settled.len(),
        1,
        "an interrupt reports once, and one word: {states:?}"
    );
    assert_eq!(
        settled[0], "idle",
        "a human at the keyboard has just interrupted a run — a terminal holding \
         `working` for a process that has gone is a terminal lying about a machine"
    );
    let interrupt = states
        .iter()
        .rposition(|state| state == "idle")
        .expect("idle");
    assert!(
        !states[interrupt..].contains(&"working".to_string()),
        "and nothing says working after it: {states:?}"
    );
}

#[test]
fn four_frames_carry_four_titles_and_one_app() {
    let name = |index: usize| format!("waiting-room-{index}");
    let mut titles = Vec::new();
    for index in 0..4 {
        let Some(pty) = Pty::new() else {
            eprintln!("no pty on this machine: four labelled records are not tested here");
            return;
        };
        let mut env = Env::new();
        // The workspace's own name is the title, so four frames in four
        // workspaces are four labelled records rather than four identical ones.
        let named = env.workspace.with_file_name(name(index));
        std::fs::rename(&env.workspace, &named).expect("rename the workspace");
        env.workspace = PathBuf::from(&named);
        let mock = MockProvider::start(vec![Reply::answer("unused")]);
        write_config(&env, &config_for(&mock));
        // No key, so the frame opens on the modal that asks for one: the one
        // genuine blockage sindri has, and the report a waiting frame owes.
        let base_url = mock.base_url();
        let mut child = spawn(
            &env,
            &pty,
            &[
                ("SINDRI_MODEL", "claude-sonnet-4-6"),
                ("SINDRI_BASE_URL", base_url.as_str()),
            ],
        );
        pty.wait_for(&mut child, ALTERNATE_SCREEN);
        pty.listening(&mut child, Duration::from_millis(300));
        let (code, _) = pty.quit(&mut child);
        assert_eq!(code, Some(0), "leaving the frame is not a failure");

        let bodies = bodies(&pty);
        let blocked = pty
            .status()
            .iter()
            .find(|report| report.state() == Some("blocked"))
            .unwrap_or_else(|| panic!("a staged missing key is a blockage: {bodies:?}"))
            .body()
            .to_string();
        assert!(
            blocked.contains(":kind=auth"),
            "and it names which blockage: {blocked:?}"
        );
        for body in &bodies {
            assert!(
                body.contains(":app=sindri"),
                "every report is grouped under one application: {body:?}"
            );
        }
        titles.push(
            bodies
                .first()
                .expect("a report")
                .split(":title=")
                .nth(1)
                .expect("a title")
                .split(':')
                .next()
                .expect("a title value")
                .to_string(),
        );
    }
    assert_eq!(
        titles,
        (0..4)
            .map(|index| sindri_core::status::base64(name(index).as_bytes()))
            .collect::<Vec<String>>(),
        "the title is the workspace's own name, base64'd as every value is"
    );
}

#[test]
fn a_report_is_never_said_twice_and_never_twice_inside_a_second() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the report's rate is not tested here");
        return;
    };
    let env = Env::new();
    // A run that takes a couple of seconds: long enough for the frame's clock
    // to be tempted into saying the same thing on every tick of it.
    let fragments: Vec<String> = (0..80).map(|index| format!("word{index} ")).collect();
    let fragments: Vec<&str> = fragments.iter().map(String::as_str).collect();
    let mock = MockProvider::start(vec![Reply::streamed_paced(
        common::provider::frames::text(&fragments),
        Duration::from_millis(25),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    let began = Instant::now();
    type_line(&pty, "answer slowly");
    expect(&pty, &mut child, "word79");
    let took = began.elapsed();
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let bodies = bodies(&pty);
    for pair in bodies.windows(2) {
        assert_ne!(pair[0], pair[1], "a state said twice in a row: {bodies:?}");
    }
    // One working report a second is the ceiling, and a report held is never a
    // report lost: a run of `took` cannot have said more than one a second plus
    // the state it settled in.
    let working = states(&pty)
        .iter()
        .filter(|state| state.as_str() == "working")
        .count();
    let allowance = took.as_secs() as usize + 2;
    assert!(
        working <= allowance,
        "a {took:?} run said working {working} times, more than one a second: {bodies:?}"
    );
    assert!(working >= 1, "the run said it was working: {bodies:?}");
}
