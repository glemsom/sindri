//! A thought as a real terminal draws it: its words are rows of the thread, at
//! the tail, in the thought's own ink, while the provider is still sending; no
//! row above them moves; and once the run is over they stay where the reader read
//! them, under a marker row that says so. What a cancel leaves of a thought it
//! cut off, a thought longer than the column, and a terminal too narrow for the
//! rail are here too. Driven through a pseudo-terminal, because the ink is the
//! SGR a real terminal is sent and the tail is a terminal's own scroll, and a
//! drawn buffer is neither.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::process::Child;
use std::time::Duration;

use common::provider::{MockProvider, Reply, frames};
use common::pty::{CANCEL, Pty, expect, spawn, type_line};
use common::screen::{self, Ink};
use common::{Env, TEST_KEY, write_config};
use ratatui::style::Color;
use sindri_tui::theme;

/// The word the marker row carries while the run thinks, and the glyph it hangs
/// off: no other row of the frame says either, so a test that looks for the
/// marker is looking at the marker.
const THINKING: &str = "⌁ thinking";
/// The word the marker row settles on once the thought is over.
const REASONED: &str = "⌁ reasoned";
/// The words a cancelled run's own note carries: a cancel is answered where the
/// reader is looking, because the request in flight is not interrupted.
const CANCELLED: &str = "the draft is kept";
/// What the reader typed: the row those words sit on is a row the thought is
/// never allowed to move.
const PROMPT: &str = "what changed?";
/// The columns the transcript has at the pty's hundred: the terminal's width
/// less the rail and the gap beside it, which is where a row of words stops.
const COLUMN: usize = 66;

fn frame_on(env: &Env, pty: &Pty, mock: &MockProvider) -> Child {
    write_config(
        env,
        &format!(
            "provider = \"anthropic\"\nmodel = \"model-one\"\nbase_url = \"{}\"\nthinking = true\n",
            mock.base_url()
        ),
    );
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

/// A reply that thinks first: `fragments` one frame each, `pause` apart, and then
/// the answer. The pace is the test's, because a thought that has to be watched
/// arriving is a thought the provider has to be slow about.
fn thinking(fragments: &[String], answer: &str, pause: Duration) -> Reply {
    let fragments: Vec<&str> = fragments.iter().map(String::as_str).collect();
    Reply::streamed_paced(frames::reasoning_then_text(&fragments, &[answer]), pause)
}

/// Numbered fragments, lettered so a test can say which of them the frame has
/// reached: `count` of them, `w001` first.
fn numbered(count: usize) -> Vec<String> {
    (1..=count).map(|nth| format!("w{nth:03} ")).collect()
}

/// The ink the theme gives a thought, as the three numbers a terminal is sent:
/// the colour a row of a thought is written in, and no other block of the frame.
fn thought_ink() -> Ink {
    let Color::Rgb(red, green, blue) = theme::THOUGHT else {
        panic!("a thought's ink is a colour a terminal is sent as three numbers");
    };
    (red, green, blue)
}

/// The rows of `screen` that say `phrase`, top to bottom.
fn rows_saying(screen: &str, phrase: &str) -> Vec<usize> {
    screen
        .lines()
        .enumerate()
        .filter(|(_, row)| screen::says(row, phrase))
        .map(|(row, _)| row)
        .collect()
}

/// The one row that says `phrase`: a marker, a fragment and the composer's own
/// border are each drawn once, so a screen that says one of them twice is a
/// frame this test cannot read.
fn row_saying(screen: &str, phrase: &str) -> usize {
    let rows = rows_saying(screen, phrase);
    assert_eq!(
        rows.len(),
        1,
        "the screen says {phrase:?} on the rows {rows:?}, not one: {screen:?}"
    );
    rows[0]
}

/// The row of `screen`, and what it says.
fn row(screen: &str, at: usize) -> &str {
    screen.lines().nth(at).unwrap_or_default()
}

/// What the rows `from..=to` of `screen` say in the transcript's own columns:
/// the rail sits beyond them, and a rail's counters move with every token the
/// run spends, which is not a row of the thread moving.
fn thread_rows(screen: &str, from: usize, to: usize) -> Vec<String> {
    screen
        .lines()
        .skip(from)
        .take(to + 1 - from)
        .map(|line| {
            line.chars()
                .take(COLUMN)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// The time a row claims, `1.2s` read as 1.2 and `1m02s` as 62: the frame's own
/// reading of a duration, and the one thing on a marker row that only a spent
/// clock explains. `None` where the row claims no time at all.
fn claimed_seconds(said: &str) -> Option<f64> {
    let number = said.split_whitespace().last()?.strip_suffix('s')?;
    match number.split_once('m') {
        Some((minutes, seconds)) if digits(minutes) && seconds.len() == 2 && digits(seconds) => {
            Some(minutes.parse::<f64>().ok()? * 60.0 + seconds.parse::<f64>().ok()?)
        }
        Some(_) => None,
        None => match number.split_once('.') {
            Some((whole, tenths)) if digits(whole) && tenths.len() == 1 && digits(tenths) => {
                number.parse().ok()
            }
            Some(_) => None,
            None => None,
        },
    }
}

/// Whether `said` is a run of digits and nothing else, which is how the frame
/// writes every number a row claims.
fn digits(said: &str) -> bool {
    !said.is_empty() && said.chars().all(|ch| ch.is_ascii_digit())
}

#[test]
fn a_run_that_thinks_streams_its_words_at_the_tail_in_the_thoughts_ink() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a streaming thought is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![thinking(
        &numbered(10),
        "the answer",
        Duration::from_millis(200),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, PROMPT);
    let drawn = pty.wait_for_whole_words(&mut child, "w003");

    assert!(
        rows_saying(&drawn, "the answer").is_empty(),
        "the provider is still sending, so no answer is on the glass: {drawn:?}"
    );
    let marker = row_saying(&drawn, THINKING);
    let newest = row_saying(&drawn, "w003");
    assert!(
        marker < newest,
        "the marker row stands above the words it marks: {drawn:?}"
    );

    // The ink is the SGR the frame sent, read off the cells a terminal kept: the
    // theme's own colour, which no other block of the frame is written in.
    let inked = pty.inked(thought_ink());
    for phrase in [THINKING, "w003"] {
        assert!(
            inked.iter().any(|(_, said)| screen::says(said, phrase)),
            "the row that says {phrase:?} is drawn in the thought's ink: {inked:?}"
        );
    }
    assert!(
        !inked.iter().any(|(_, said)| screen::says(said, PROMPT)),
        "and the row the reader's own words are on is not: {inked:?}"
    );

    let below = row(&drawn, newest + 1).trim();
    assert!(
        below.is_empty() || below.starts_with('╭'),
        "the thought's newest row is the thread's last row, at the tail: {drawn:?}"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn no_row_of_the_thread_moves_while_the_thought_streams() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a streaming thought is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![thinking(
        &numbered(20),
        "the answer",
        Duration::from_millis(150),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, PROMPT);
    let first = pty.wait_for_whole_words(&mut child, "w004");
    let second = pty.wait_for_whole_words(&mut child, "w008");

    assert!(
        screen::says(&first, "w004") && screen::says(&second, "w008"),
        "four more fragments arrived between the two frames: {second:?}"
    );
    let marker = row_saying(&first, THINKING);
    let again = row_saying(&second, THINKING);
    // The marker row is left out of both readings: it is the thought's clock,
    // and a clock that did not move would be the frame that fell still.
    assert_eq!(
        thread_rows(&first, 0, marker - 1),
        thread_rows(&second, 0, again - 1),
        "the rows above the thought are where they were: {first:?} then {second:?}"
    );
    // The fragments already on the glass are the words that could have been
    // laid out again under the ones arriving: a fragment is appended to the row
    // it lands on, so the words before it keep the rows they had.
    for phrase in ["w001", "w004"] {
        assert_eq!(
            row_saying(&first, phrase),
            row_saying(&second, phrase),
            "the row that says {phrase:?} is the same row in both frames: \
             {first:?} then {second:?}"
        );
    }

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn an_answered_run_keeps_the_thought_under_a_marker_that_says_reasoned() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: an ended thought is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![thinking(
        &numbered(4),
        "the answer",
        Duration::from_millis(100),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, PROMPT);
    expect(&pty, &mut child, "thinking");
    expect(&pty, &mut child, "the answer");
    let drawn = pty.wait_for_whole_words(&mut child, REASONED);

    assert!(
        rows_saying(&drawn, THINKING).is_empty(),
        "no row is left saying the run is still thinking: {drawn:?}"
    );
    let marker = row_saying(&drawn, REASONED);
    assert!(
        claimed_seconds(row(&drawn, marker)).is_some(),
        "the marker row says the thought is over and how long it took: {:?}",
        row(&drawn, marker)
    );
    let first = row_saying(&drawn, "w001");
    assert!(
        marker < first,
        "and the thought's own rows are still under it: {drawn:?}"
    );
    for phrase in ["w001", "w004", "the answer"] {
        assert!(
            !rows_saying(&drawn, phrase).is_empty(),
            "the transcript still says {phrase:?}: {drawn:?}"
        );
    }

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_cancel_mid_thought_leaves_the_thought_where_it_stood() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a cancelled thought is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![thinking(
        &numbered(40),
        "an answer nobody waits for",
        Duration::from_millis(150),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    let asked = std::time::Instant::now();
    type_line(&pty, PROMPT);
    let streaming = pty.wait_for_whole_words(&mut child, "w010");
    let marker = row_saying(&streaming, THINKING);
    let last = row_saying(&streaming, "w010");

    pty.type_key(CANCEL);
    let drawn = pty.wait_for_whole_words(&mut child, CANCELLED);

    assert!(
        screen::says(&drawn, CANCELLED),
        "a cancel leaves its own note: {drawn:?}"
    );
    assert!(
        rows_saying(&drawn, THINKING).is_empty(),
        "no row is left saying the run is still thinking: {drawn:?}"
    );
    assert!(
        !screen::says(&drawn, "an answer nobody waits for"),
        "and the answer nobody waits for was never drawn: {drawn:?}"
    );
    // The note a cancel writes lands under the thought, and the thread is drawn
    // at its tail: what the thought says, and the order it says it in, is what
    // stays. Its rows are not moved into a surface of their own to be read.
    let ended = row_saying(&drawn, REASONED);
    assert_eq!(
        thread_rows(&streaming, marker + 1, last),
        thread_rows(&drawn, ended + 1, row_saying(&drawn, "w010")),
        "the thought's own rows still say what the reader was reading: {drawn:?}"
    );
    assert_eq!(
        thread_rows(&streaming, row_saying(&streaming, PROMPT), marker - 1),
        thread_rows(&drawn, row_saying(&drawn, PROMPT), ended - 1),
        "and the prompt it hangs under still stands above it: {drawn:?}"
    );
    let claimed = claimed_seconds(row(&drawn, ended)).unwrap_or_default();
    let spent = asked.elapsed().as_secs_f64();
    assert!(
        claimed <= spent + 0.5,
        "the marker claims {claimed}s of a run that is {spent:.1}s old, which is \
         no time the run did not spend: {drawn:?}"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_thought_longer_than_the_column_keeps_its_newest_row_on_the_glass() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a long thought is not tested here");
        return;
    };
    let env = Env::new();
    // One burst of four hundred numbered words — more rows than the transcript
    // has — and then a trickle beside it, so the provider is still sending while
    // the frame draws what it already sent.
    let mut fragments = vec![numbered(400).concat()];
    fragments.extend((401..=415).map(|nth| format!("w{nth:03} ")));
    let mock = MockProvider::start(vec![thinking(
        &fragments,
        "the answer",
        Duration::from_millis(400),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, PROMPT);
    let drawn = pty.wait_for_whole_words(&mut child, "w390");

    assert!(
        screen::says(&drawn, "w390") && screen::says(&drawn, "w400"),
        "the newest rows the thought has are on the glass: {drawn:?}"
    );
    assert!(
        rows_saying(&drawn, "w001").is_empty(),
        "and the rows that left the top are scrolled off rather than kept: {drawn:?}"
    );

    // The frame has not fallen still: each further fragment is a frame of its
    // own, and one that says a word the screen did not have is a frame the clock
    // still reaches while the thought arrives. The wait has the harness's own
    // deadline, so a frame that has fallen still fails here, at this name.
    let later = pty.wait_for_whole_words(&mut child, "w410");
    assert!(
        screen::says(&later, "w410"),
        "the frame keeps drawing while the thought arrives: {later:?}"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_terminal_too_narrow_for_the_rail_still_draws_a_thought() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a narrow thought is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![thinking(
        &numbered(10),
        "the answer",
        Duration::from_millis(200),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    // Ninety columns: under `RAIL_MIN_WIDTH`, so the rail is a strip and the
    // words have the whole width.
    pty.resize(&child, 30, 90);
    let _ = pty.wait_for_frame(&mut child);
    type_line(&pty, PROMPT);
    let drawn = pty.wait_for_whole_words(&mut child, "w003");

    let marker = row_saying(&drawn, THINKING);
    assert!(
        marker < row_saying(&drawn, "w003"),
        "the thought is drawn in a column too narrow for the rail: {drawn:?}"
    );
    let inked = pty.inked(thought_ink());
    assert!(
        inked.iter().any(|(_, said)| screen::says(said, "w003")),
        "in the ink the theme gives it: {inked:?}"
    );
    let widest = drawn
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or_default();
    assert_eq!(
        widest, 90,
        "and the frame draws the ninety columns it has and not one more: {drawn:?}"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}
