use std::path::Path;

use sindri_core::records::Told;
use sindri_core::redact::SecretSet;
use sindri_core::run::Stop;

use crate::term::pty::Pty;

use super::*;

/// The payload of every OSC sequence the stream was handed, in the order it was handed them:
/// what a terminal at the far end reads, with the bytes around it taken off.
fn sequences(written: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(written).into_owned();
    let mut said = Vec::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find("\x1b]") {
        rest = &rest[at + 2..];
        let Some(end) = rest.find("\x1b\\") else {
            break;
        };
        said.push(rest[..end].to_owned());
        rest = &rest[end + 2..];
    }
    said
}

fn failed() -> Stop {
    Stop::Error(SecretSet::default().redact("the wire said no"))
}

#[test]
fn a_window_names_the_workspace_and_the_state_and_writes_the_title_only_when_the_state_changes() {
    let mut written = Vec::new();
    {
        let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), true);
        window.say(Told::Idle).expect("the state a launch opens in");
        window
            .say(Told::Asking { turn: 1 })
            .expect("the run has begun");
        window
            .say(Told::Asking { turn: 2 })
            .expect("a turn inside the same state");
        window
            .say(Told::Stopped {
                turn: 2,
                stop: Stop::Final,
            })
            .expect("the run answered");
    }

    let titles: Vec<String> = sequences(&written)
        .into_iter()
        .filter(|payload| payload.starts_with("2;"))
        .collect();
    assert_eq!(
        titles,
        [
            "2;sindri \u{2014} idle".to_owned(),
            "2;sindri \u{2014} working".to_owned(),
            "2;sindri \u{2014} done".to_owned(),
        ],
        "the title is the workspace and the state, and a turn inside one state names the same \
             title: a title re-said every turn is a title bar repainted for nothing"
    );
}

#[test]
fn a_streaming_run_carries_a_mark_and_every_stop_clears_it() {
    let mut written = Vec::new();
    {
        let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), true);
        window.say(Told::Idle).expect("before the run begins");
        window
            .say(Told::Asking { turn: 1 })
            .expect("the run streams");
        window
            .say(Told::Stopped {
                turn: 1,
                stop: failed(),
            })
            .expect("the run stopped");
    }

    let marks: Vec<String> = sequences(&written)
        .into_iter()
        .filter(|payload| payload.starts_with("9;4;"))
        .collect();
    assert_eq!(
        marks,
        ["9;4;0".to_owned(), "9;4;3".to_owned(), "9;4;0".to_owned()],
        "the mark is worn while the run streams and taken away by the stop: a mark left on a \
             finished run says a run is working that has ended"
    );
    assert_eq!(marks.last().map(String::as_str), Some("9;4;0"));
}

#[test]
fn a_run_that_ends_unwatched_raises_one_notification_and_one_watched_raises_none() {
    let mut written = Vec::new();
    {
        let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), true);
        window.focused(false);
        window.say(Told::Idle).expect("before the run begins");
        window
            .say(Told::Asking { turn: 1 })
            .expect("the run streams");
        window
            .say(Told::Stopped {
                turn: 1,
                stop: Stop::Final,
            })
            .expect("the run answered while nobody was looking");
        window.focused(true);
        window
            .say(Told::Asking { turn: 2 })
            .expect("the reader is back");
        window
            .say(Told::Stopped {
                turn: 2,
                stop: failed(),
            })
            .expect("the run failed where the reader watched it fail");
    }

    let notifications: Vec<String> = sequences(&written)
        .into_iter()
        .filter(|payload| payload.starts_with("9;") && !payload.starts_with("9;4;"))
        .collect();
    assert_eq!(
        notifications,
        ["9;sindri: answered".to_owned()],
        "one notification for the run nobody saw end, and none for the one they watched"
    );
}

#[test]
fn a_telling_with_no_window_says_the_status_and_carries_no_title_or_mark() {
    let mut written = Vec::new();
    {
        let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), false);
        window.focused(false);
        window.say(Told::Idle).expect("before the run begins");
        window
            .say(Told::Asking { turn: 1 })
            .expect("the run streams");
        window
            .say(Told::Stopped {
                turn: 1,
                stop: Stop::Final,
            })
            .expect("the run stopped");
    }

    let said = sequences(&written);
    assert!(!said.is_empty(), "the status is still told: {said:?}");
    assert!(
        said.iter().all(|payload| payload.starts_with("7501;")),
        "a stream that took no screen carries the status document and no window: {said:?}"
    );
}

#[test]
fn a_telling_that_is_off_says_nothing_at_all() {
    let mut written = Vec::new();
    let mut window = Window::new(&mut written, false, Path::new("/work/sindri"), true);

    window.say(Told::Idle).expect("a window that is off");
    window.focused(false);
    window
        .say(Told::Stopped {
            turn: 1,
            stop: Stop::Final,
        })
        .expect("a stop nobody asked to hear");
    assert!(written.is_empty(), "nothing was written: {written:?}");
}

#[test]
fn a_telling_says_one_state_once() {
    let mut written = Vec::new();
    {
        let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), false);
        window
            .say(Told::Asking { turn: 1 })
            .expect("the first report");
        window
            .say(Told::Asking { turn: 2 })
            .expect("a turn inside the second");
        window
            .say(Told::Asking { turn: 1 })
            .expect("the state already told");
    }
    let text = String::from_utf8(written).expect("a sequence is UTF-8");

    assert_eq!(
        text.matches("\x1b]7501;").count(),
        1,
        "one state, said once: {text:?}"
    );
    assert!(text.contains("\"turn\":1"), "{text:?}");
}

#[test]
fn a_telling_turned_off_gives_up_the_mark_it_had_set() {
    let mut written = Vec::new();
    {
        let mut window = Window::new(&mut written, true, Path::new("/work/sindri"), true);
        window
            .say(Told::Asking { turn: 1 })
            .expect("the run streams");
        window
            .switch(false)
            .expect("the reader turned the telling off");
    }

    let marks: Vec<String> = sequences(&written)
        .into_iter()
        .filter(|payload| payload.starts_with("9;4;"))
        .collect();
    assert_eq!(
        marks,
        ["9;4;3".to_owned(), "9;4;0".to_owned()],
        "a telling that has gone quiet can never say the stop, so the mark goes with it"
    );
}

#[test]
fn a_pseudo_terminal_reads_the_title_the_mark_and_one_notification_off_the_wire() {
    let pty = Pty::open();
    {
        let mut window = Window::new(
            pty.slave.try_clone().expect("a handle on the terminal"),
            true,
            Path::new("/work/sindri"),
            true,
        );
        window.focused(false);
        window
            .say(Told::Asking { turn: 1 })
            .expect("the run streams");
        window
            .say(Told::Stopped {
                turn: 1,
                stop: Stop::Final,
            })
            .expect("the run answered");
    }

    let drawn = pty.drawn();
    assert!(
        drawn.contains("\x1b]2;sindri \u{2014} working\x1b\\"),
        "the terminal was handed the title the state began in: {drawn:?}"
    );
    assert!(
        drawn.contains("\x1b]9;4;3\x1b\\"),
        "and the mark the run wore while it streamed: {drawn:?}"
    );
    assert!(
        drawn.contains("\x1b]9;4;0\x1b\\"),
        "and the state that took the mark away: {drawn:?}"
    );
    assert_eq!(
        drawn.matches("\x1b]9;sindri: answered\x1b\\").count(),
        1,
        "one notification, off the wire a terminal reads: {drawn:?}"
    );
}

#[test]
fn a_terminal_that_names_none_of_these_sequences_is_handed_no_window() {
    assert!(!speaks(None, None), "a terminal that says nothing");
    assert!(!speaks(Some("xterm-256color"), None), "an xterm");
    assert!(
        speaks(Some("xterm-kitty"), None),
        "a terminal named after one that takes them"
    );
    assert!(
        speaks(Some("xterm-256color"), Some("WezTerm")),
        "and one named by the program that runs it"
    );
}
