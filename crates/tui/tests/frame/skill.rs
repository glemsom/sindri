//! A skill in the frame: the rail's chip beside the posture, the one row a load
//! takes in the thread, and the body that row opens rather than pours.

use super::*;
use crossterm::event::{KeyCode, KeyModifiers};
use sindri_core::provider::Asked;
use sindri_tui::theme;

/// The body one activation hands back: the wrapper, the directory with the
/// clause a path in the body resolves from, and the words, as the `skill` tool
/// renders them.
const BODY: &str = "outcome: loaded\nskill: house-style\ndir: /w/.agents/skills/house-style — a path written in the body resolves from here\n<skill_content name=\"house-style\">\nAnswer in short sentences.\n</skill_content>\n";

/// A load as the loop reports one: the call that asked for it, and the result
/// the conversation now carries.
fn loaded(app: &mut App, turn: u32, name: &str) {
    app.event(Event::ToolCall {
        turn,
        asked: Asked::Skill {
            name: name.to_string(),
        },
    });
    app.event(Event::SkillLoaded {
        turn,
        name: name.to_string(),
        outcome: Some("loaded".to_string()),
        body: BODY.to_string(),
    });
}

#[test]
fn a_load_is_one_row_that_says_the_skill_is_in_force() {
    let mut app = app();
    loaded(&mut app, 1, "house-style");
    app.finished(ended(Stop::Final, 1, Usage::default()));
    let rows = screen(&mut app, 100, 30);

    let at = row_of(&rows, "skill house-style");
    assert!(
        rows[at].contains('✓') && rows[at].contains("loaded"),
        "a load has no exit to wait for and runs nothing: its row says what it \
         made true: {rows:#?}"
    );
    assert!(
        !any(&rows, "Answer in short sentences."),
        "the body is never poured into the thread: {rows:#?}"
    );
}

#[test]
fn the_body_is_behind_the_row_and_enter_opens_it() {
    let mut app = app();
    app.event(Event::TextDelta {
        turn: 1,
        text: "the answer".to_string(),
    });
    loaded(&mut app, 1, "house-style");
    app.finished(ended(Stop::Final, 1, Usage::default()));
    let collapsed = screen(&mut app, 100, 30);
    assert!(
        row(&collapsed, "skill house-style").contains("lines"),
        "the row says what is behind it: {collapsed:#?}"
    );

    // An empty composer opens the newest entry a key can open, which is the
    // load: the body is one key away, never one message long.
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let opened = screen(&mut app, 100, 30);
    assert!(
        any(&opened, "Answer in short sentences."),
        "Enter opens the body: {opened:#?}"
    );
}

#[test]
fn the_rail_carries_a_skills_chip_beside_the_sandbox_posture() {
    let mut app = app();
    loaded(&mut app, 1, "house-style");
    loaded(&mut app, 2, "release-checks");
    let rows = screen(&mut app, 100, 30);

    assert!(
        any(&rows, "⬢ bwrap · 1 writable"),
        "the posture is where it always was: {rows:#?}"
    );
    let chip = row_of(&rows, "◆");
    assert!(
        rows[chip].contains("house-style · release-checks"),
        "the skills chip names what is in force, in name order: {rows:#?}"
    );
    assert_eq!(
        chip + 1,
        row_of(&rows, "⬢ bwrap"),
        "and it hangs off the edge above the posture: {rows:#?}"
    );
}

#[test]
fn a_new_session_takes_the_skills_chip_and_the_rows_with_the_thread() {
    let mut app = app();
    loaded(&mut app, 1, "house-style");
    assert!(any(&screen(&mut app, 100, 30), "◆"));

    app.renewed(Ok(launch()));

    let rows = screen(&mut app, 100, 30);
    assert!(
        !any(&rows, "◆"),
        "a thread that was told nothing says nothing is in force: {rows:#?}"
    );
    assert!(
        !any(&rows, "skill house-style"),
        "and the load's row went with it: {rows:#?}"
    );
}

#[test]
fn the_skills_chip_is_one_run_of_the_accents_ink_on_the_raised_tier() {
    let mut app = app();
    loaded(&mut app, 1, "house-style");
    let cells = frame_cells(&mut app, 100, 30);
    let (glyph, row) = (0..30)
        .find_map(|y| {
            (68..100)
                .find(|x| cells[(*x, y)].symbol() == "◆")
                .map(|x| (x, y))
        })
        .expect("the rail wears the skills' glyph");
    // From the glyph to the last cell the raised run covers, as the posture's
    // chip is read: the pad before it is the row's, and the chip is its own.
    let chip: Vec<ratatui::style::Color> = (glyph..100)
        .take_while(|x| cells[(*x, row)].bg == theme::RAISED)
        .map(|x| cells[(x, row)].fg)
        .collect();
    assert!(
        chip.len() > 1 && chip.windows(2).all(|pair| pair[0] == pair[1]),
        "the chip is one raised run of one ink: {chip:?}"
    );
}

#[test]
fn a_skills_chip_wider_than_the_rail_is_shortened_and_ends_in_an_ellipsis() {
    let mut app = app();
    for (turn, name) in SHIPPED.iter().enumerate() {
        loaded(&mut app, turn as u32 + 1, name);
    }
    let rows = screen(&mut app, 100, 30);
    let chip = row_of(&rows, "◆");
    let rail = rail_of(&rows[chip]);

    // The chip's raised run reaches the rail's last column — at a hundred
    // columns, the frame's own last: a value drawn whole would run out of the
    // frame mid-name with nothing said about it.
    let cells = frame_cells(&mut app, 100, 30);
    let last = 100 - 1;
    assert_eq!(
        cells[(last, chip as u16)].bg,
        theme::RAISED,
        "the chip's tier runs to the rail's last column: {rail:?}"
    );

    // What is in force is longer than the rail can draw, so the value is cut in
    // cells and says so: the head of the fact, and an ellipsis where the rest
    // went.
    let drawn = rail.trim();
    assert!(
        drawn.ends_with('…'),
        "a chip too wide for the rail says more is in force: {rail:?}"
    );
    let said = "◆ edit-files · search-and-read-files";
    assert!(
        said.starts_with(drawn.trim_end_matches('…')),
        "the chip draws the head of what is in force and no word it invented: \
         {rail:?}"
    );
}
