//! A thought as a real terminal paints it: the pane opens below the thread while
//! the run thinks, no row of the thread moves under it, and the pane folds away
//! into one scar when the thought is over — and what a cancel and a `/new` do to
//! a pane that is still arriving. Driven through a pseudo-terminal, because the
//! pane's shape is a fact about a terminal's own height, resize and clock, and a
//! drawn buffer is none of those.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::ops::RangeInclusive;
use std::process::Child;
use std::time::Duration;

use common::provider::{MockProvider, Reply, frames};
use common::pty::{CANCEL, Pty, expect, spawn, type_command, type_line};
use common::screen;
use common::{Env, TEST_KEY, write_config};

/// The word the pane's head carries while a run thinks, and the glyph it hangs
/// off: no other row of the frame says either, so a test looking for the pane
/// is looking at the pane.
const HEAD: &str = "⌁ thinking";
/// The one row a finished thought leaves in the thread.
const SCAR: &str = "⌁ reasoned";
/// The words a cancelled run's own note carries: a cancel is answered where the
/// reader is looking, because the request in flight is not interrupted.
const CANCELLED: &str = "the draft is kept";
/// What the reader typed: the row their own words sit on is the row the pane is
/// never allowed to move.
const PROMPT: &str = "what changed?";
/// The composer's own title, which no other row says: a pane's last row is the
/// row above it.
const COMPOSER: &str = "prompt";

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
/// the answer. The pace is the test's, because a pane that has to be watched
/// arriving is a thought the provider has to be slow about.
fn thinking(fragments: &[String], answer: &str, pause: Duration) -> Reply {
    let fragments: Vec<&str> = fragments.iter().map(String::as_str).collect();
    Reply::streamed_paced(frames::reasoning_then_text(&fragments, &[answer]), pause)
}

/// Numbered fragments, lettered so a test can say which of them the pane has
/// reached: `count` of them, `w001` first.
fn numbered(count: usize) -> Vec<String> {
    (1..=count).map(|nth| format!("w{nth:03} ")).collect()
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

/// The one row that says `phrase`: the pane's head, the scar and the composer's
/// own border are each drawn once, so a screen saying one of them twice is a
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

/// The rows the pane takes on `screen`: its head, down to the last row above the
/// composer. Its floor never moves, so this is the pane's own count and not the
/// thread's.
fn pane(screen: &str) -> RangeInclusive<usize> {
    let head = row_saying(screen, HEAD);
    let composer = row_saying(screen, COMPOSER);
    assert!(
        composer > head + 1,
        "the pane stands between its head at {head} and the composer at {composer}: {screen:?}"
    );
    head..=(composer - 1)
}

/// The row of `screen`, and what it says.
fn row(screen: &str, at: usize) -> &str {
    screen.lines().nth(at).unwrap_or_default()
}

/// The time the pane's head says, `0.8s` or `1m02s`: the pane's own clock, and
/// the one thing on the glass that moves with no fragment behind it.
fn head_clock(screen: &str) -> String {
    let head = rows_saying(screen, HEAD);
    head.first()
        .map(|at| {
            row(screen, *at)
                .split_whitespace()
                .last()
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default()
}

#[test]
fn a_run_that_thinks_draws_a_pane_whose_head_says_thinking() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the pane's opening is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![thinking(
        &numbered(10),
        "the answer",
        Duration::from_millis(150),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, PROMPT);
    let drawn = pty.wait_for_whole_words(&mut child, "thinking");

    let pane = pane(&drawn);
    assert_eq!(
        row_saying(&drawn, HEAD),
        *pane.start(),
        "the head is the pane's first row: {drawn:?}"
    );
    assert!(
        screen::says(row(&drawn, *pane.start()), HEAD),
        "and says what the run is doing: {:?}",
        row(&drawn, *pane.start())
    );
    for at in pane {
        let said = row(&drawn, at).trim_start();
        assert!(
            said.starts_with('│'),
            "the pane's row {at} hangs off the gutter a turn's answer does, said: {said:?}"
        );
    }

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn no_row_of_the_thread_moves_while_the_thought_streams() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the pane's rows are not tested here");
        return;
    };
    let env = Env::new();
    // A thought of two rows at the transcript's sixty-two columns, arriving in
    // fragments the provider paces: the pane is standing at its height while
    // the words are still arriving, which is when a row could move.
    let mock = MockProvider::start(vec![thinking(
        &numbered(20),
        "the answer",
        Duration::from_millis(150),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, PROMPT);
    let first = pty.wait_for_whole_words(&mut child, "w008");
    assert!(
        screen::says(&first, "w008"),
        "the eighth fragment is on the glass: {first:?}"
    );
    let second = pty.wait_for_frame(&mut child);

    let before = pane(&first);
    let after = pane(&second);
    assert!(
        screen::says(&second, "thinking"),
        "the pane is still on the glass in the second frame: {second:?}"
    );
    assert_eq!(
        before, after,
        "the pane takes the same rows of the transcript in both frames"
    );
    assert_eq!(
        row_saying(&first, PROMPT),
        row_saying(&second, PROMPT),
        "and the row the reader's own words sit on is the same row in both: \
         {first:?} then {second:?}"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_finished_thought_folds_the_pane_into_one_reasoned_row() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the pane's fold is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![thinking(
        &numbered(10),
        "the answer",
        Duration::from_millis(100),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, PROMPT);
    expect(&pty, &mut child, "thinking");
    // The run is not slower for the pane: the answer arrives, and it is the
    // answer and not the thought that the reader is waiting for.
    expect(&pty, &mut child, "the answer");
    let drawn = pty.wait_for_whole_words(&mut child, "reasoned");

    assert!(
        screen::says(&drawn, "the answer"),
        "the answer is in the thread: {drawn:?}"
    );
    assert!(
        rows_saying(&drawn, HEAD).is_empty(),
        "the pane is gone: {drawn:?}"
    );
    let reasoned = rows_saying(&drawn, SCAR);
    assert_eq!(
        reasoned.len(),
        1,
        "one `⌁ reasoned` row stands where the pane was, not {}: {drawn:?}",
        reasoned.len()
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_cancel_mid_thought_folds_the_pane_and_writes_no_scar() {
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
    type_line(&pty, PROMPT);
    expect(&pty, &mut child, "thinking");
    pty.type_key(CANCEL);
    let drawn = pty.wait_for_whole_words(&mut child, CANCELLED);

    assert!(
        screen::says(&drawn, CANCELLED),
        "a cancel leaves its own note: {drawn:?}"
    );
    assert!(
        rows_saying(&drawn, HEAD).is_empty(),
        "the pane goes with the thought: {drawn:?}"
    );
    assert!(
        rows_saying(&drawn, SCAR).is_empty(),
        "and no scar is written for a thought the turn never finished: {drawn:?}"
    );
    // The turn never finished, so it is no part of the document: the record
    // holds no assistant line and no word of what the model was saying.
    let transcript = std::fs::read_to_string(common::the_session(&env).join("transcript.jsonl"))
        .expect("the run's transcript");
    assert!(
        !transcript.contains("\"assistant\"") && !transcript.contains("w001"),
        "a cancelled turn is not written into the record: {transcript:?}"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_new_session_mid_thought_takes_the_pane_with_the_thread() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: `/new`'s pane is not tested here");
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
    type_line(&pty, PROMPT);
    expect(&pty, &mut child, "thinking");
    type_command(&pty, "/new");
    let drawn = pty.wait_for_whole_words(&mut child, "new session");

    assert!(
        screen::says(&drawn, "new session"),
        "`/new` opens its session: {drawn:?}"
    );
    assert!(
        rows_saying(&drawn, HEAD).is_empty() && rows_saying(&drawn, SCAR).is_empty(),
        "the first frame after it says nothing of the thought it superseded: {drawn:?}"
    );
    assert!(
        rows_saying(&drawn, PROMPT).is_empty(),
        "and nothing of the thread it belonged to: {drawn:?}"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_terminal_too_narrow_for_the_rail_still_draws_the_pane() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a narrow pane is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![thinking(
        &numbered(10),
        "the answer",
        Duration::from_millis(150),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    // Ninety columns: under `RAIL_MIN_WIDTH`, so the rail is a strip and the
    // words have the whole width.
    pty.resize(&child, 30, 90);
    let _ = pty.wait_for_frame(&mut child);
    type_line(&pty, PROMPT);
    let drawn = pty.wait_for_whole_words(&mut child, "thinking");

    let pane = pane(&drawn);
    assert_eq!(
        row_saying(&drawn, HEAD),
        *pane.start(),
        "the pane draws in a column too narrow for the rail: {drawn:?}"
    );
    let widest = drawn
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or_default();
    assert_eq!(
        widest, 90,
        "the frame draws the ninety columns it has and not one more: {drawn:?}"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_thought_longer_than_the_pane_keeps_its_newest_row_and_the_frame_keeps_ticking() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: a long thought is not tested here");
        return;
    };
    let env = Env::new();
    // One burst of six hundred characters — ten rows of the transcript's
    // columns, so more than the pane's eight — and then a trickle beside it, so
    // the provider is still thinking while the pane draws what it sent. `w085`
    // is five hundred characters in: the pane has cut the head of the thought
    // and is standing on its tail.
    let mut fragments = vec![numbered(100).concat()];
    fragments.extend(std::iter::repeat_n("and then ".to_string(), 15));
    let mock = MockProvider::start(vec![thinking(
        &fragments,
        "the answer",
        Duration::from_millis(500),
    )]);
    let mut child = frame_on(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, PROMPT);
    expect(&pty, &mut child, "thinking");
    let mut drawn = pty.wait_for_whole_words(&mut child, "w085");
    // The unfurl is the clock's and not the words': the burst is on the glass
    // before the pane has stood to its most, so the frames are read until it
    // has.
    for _ in 0..8 {
        let pane = pane(&drawn);
        if *pane.end() - *pane.start() + 1 == 8 {
            break;
        }
        drawn = pty.wait_for_frame(&mut child);
    }

    let pane = pane(&drawn);
    let rows = *pane.end() - *pane.start() + 1;
    assert_eq!(
        rows, 8,
        "a thought past what the pane holds stands at its most: {drawn:?}"
    );
    assert!(
        !row(&drawn, *pane.end()).trim().is_empty(),
        "the newest words are on the pane's last row: {drawn:?}"
    );
    assert!(
        rows_saying(&drawn, "w001").is_empty(),
        "and the rows that leave the top are cut rather than kept: {drawn:?}"
    );

    // The pane has not fallen still: its head is drawn every tick, so a head
    // that says a later time is a frame the clock still reaches. The loop is
    // bounded because a frame that has fallen still fails here, at its own
    // name, rather than at the harness's deadline.
    let was = head_clock(&drawn);
    let mut later = String::new();
    for _ in 0..8 {
        let next = pty.wait_for_frame(&mut child);
        later = head_clock(&next);
        if later != was {
            break;
        }
    }
    assert_ne!(
        later, was,
        "the frame keeps ticking while a pane is on the glass: {drawn:?}"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}
