//! The draft's own keys, driven the way a reader presses them: the character
//! `Delete` takes and the row it joins, the cursor `Ctrl-Left` and `Ctrl-Right`
//! move by a word, the word `Ctrl-W` takes back — and, where the draft has
//! nothing to give, the same keys saying the frame has nothing new to draw.

use super::*;
use crossterm::event::KeyEvent;

/// One key as a terminal sends it, answered with whether it left the frame
/// something new to draw.
fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> bool {
    app.key(KeyEvent::new(code, modifiers))
}

#[test]
fn the_delete_key_takes_the_character_the_cursor_stands_on() {
    let mut app = answered(Fake::taking());
    type_text(&mut app, "two");
    key(&mut app, KeyCode::Left, KeyModifiers::NONE);

    assert!(
        key(&mut app, KeyCode::Delete, KeyModifiers::NONE),
        "a key that took a character says the draft changed"
    );
    assert_eq!(app.composer().text(), "tw");
    assert_eq!(
        app.composer().cursor(),
        (2, 0),
        "the cursor holds what followed the character it took"
    );
}

#[test]
fn the_delete_key_at_the_end_of_a_row_joins_the_row_below() {
    let mut app = answered(Fake::taking());
    type_text(&mut app, "one");
    key(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    type_text(&mut app, "two");
    key(&mut app, KeyCode::Up, KeyModifiers::NONE);

    assert!(key(&mut app, KeyCode::Delete, KeyModifiers::NONE));
    assert_eq!(
        app.composer().text(),
        "onetwo",
        "one row where there were two"
    );
    assert_eq!(
        app.composer().cursor(),
        (3, 0),
        "the join is behind the cursor"
    );
}

#[test]
fn the_delete_key_at_the_end_of_the_last_row_has_nothing_to_take() {
    let mut app = answered(Fake::taking());
    type_text(&mut app, "one");

    assert!(
        !key(&mut app, KeyCode::Delete, KeyModifiers::NONE),
        "the frame has nothing new to draw"
    );
    assert_eq!(
        app.composer().text(),
        "one",
        "and the draft is where it was"
    );
}

#[test]
fn ctrl_left_and_ctrl_right_move_the_cursor_a_word_at_a_time() {
    let mut app = answered(Fake::taking());
    type_text(&mut app, "the quick brown");

    assert!(key(&mut app, KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(
        app.composer().cursor(),
        (10, 0),
        "the start of the word the cursor ended"
    );
    assert!(key(&mut app, KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(
        app.composer().cursor(),
        (4, 0),
        "the start of the one before it"
    );
    assert!(key(&mut app, KeyCode::Right, KeyModifiers::CONTROL));
    assert_eq!(
        app.composer().cursor(),
        (10, 0),
        "and back again, a word at a time"
    );
    assert_eq!(
        app.composer().text(),
        "the quick brown",
        "a move takes nothing"
    );
}

#[test]
fn ctrl_w_takes_the_word_before_the_cursor_back() {
    let mut app = answered(Fake::taking());
    type_text(&mut app, "edit the middle word");

    assert!(key(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(
        app.composer().text(),
        "edit the middle ",
        "the word and the space before it are gone"
    );
    assert_eq!(
        app.composer().cursor(),
        (16, 0),
        "the cursor is where it began"
    );
}

#[test]
fn ctrl_w_at_the_head_of_a_row_takes_the_word_the_row_above_ends_with() {
    let mut app = answered(Fake::taking());
    type_text(&mut app, "one two");
    key(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    type_text(&mut app, "three");
    key(&mut app, KeyCode::Left, KeyModifiers::CONTROL);

    assert!(key(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL));
    assert_eq!(
        app.composer().text(),
        "one three",
        "the word goes with the break it stood behind, as the row below joins onto it"
    );
}

#[test]
fn the_editing_keys_say_so_when_a_blank_draft_leaves_nothing_to_take() {
    let mut app = answered(Fake::taking());
    for (code, modifiers) in [
        (KeyCode::Delete, KeyModifiers::NONE),
        (KeyCode::Left, KeyModifiers::CONTROL),
        (KeyCode::Right, KeyModifiers::CONTROL),
        (KeyCode::Char('w'), KeyModifiers::CONTROL),
    ] {
        assert!(
            !key(&mut app, code, modifiers),
            "{code:?} changed nothing, so the frame has nothing new to draw"
        );
    }
    assert_eq!(app.composer().text(), "");
}
