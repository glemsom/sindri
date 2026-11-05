//! What the screen holds: every name the wire offered, the cursor on the one
//! this run asks, and — once a fragment is typed — the names that fragment
//! reaches, nearest first, which is the ranking the model row already states.
//! A wire that named nothing still opens the screen, and says why it is empty
//! where the names would be.

use super::*;

#[test]
fn model_opens_a_screenful_of_the_names_the_wire_offers() {
    let mut f = offered(
        Some("model = \"claude-sonnet-4-6\"\n"),
        &["gpt-5", "claude-sonnet-4-6", "llama3-8b"],
    );

    let rows = shown(&screen(&mut f.app, 100, 30));
    for name in ["gpt-5", "claude-sonnet-4-6", "llama3-8b"] {
        assert!(rows.contains(name), "the screen states {name:?}: {rows}");
    }
    assert_eq!(
        marked(&mut f),
        "claude-sonnet-4-6",
        "the cursor opens on the model this run asks, not on the first name"
    );
    assert_eq!(
        written(&f.path),
        "model = \"claude-sonnet-4-6\"\n",
        "and opening the screen writes nothing"
    );
}

#[test]
fn a_fragment_typed_on_the_screen_narrows_it_by_the_ranking_the_row_uses() {
    let mut f = offered(
        Some("model = \"qwen3-32b\"\n"),
        &["gpt-5", "big-gpt", "qwen3-32b"],
    );
    type_text(&mut f.app, "gpt");

    let rows = screen(&mut f.app, 100, 30);
    let text = shown(&rows);
    assert!(
        !text.contains("qwen3-32b"),
        "a name the fragment does not reach is not on the screen: {text}"
    );
    assert!(
        row_of(&rows, "gpt-5") < row_of(&rows, "big-gpt"),
        "the name the fragment starts comes before the one it sits inside, \
         which is the ranking the model row states: {text}"
    );
    assert_eq!(
        marked(&mut f),
        "gpt-5",
        "and the cursor moves to the nearest name"
    );
}

#[test]
fn a_wire_with_no_list_still_opens_the_screen_and_says_why_it_is_empty() {
    let mut f = fixture(Some("model = \"gpt-5\"\n"));
    open(&mut f.app);
    assert!(
        f.app.overlay().is_some(),
        "the screen opens before any name has landed"
    );
    let serving = f
        .app
        .take_ask()
        .expect("opening the screen asks the wire for its names");
    let asking = shown(&screen(&mut f.app, 100, 30));
    assert!(
        asking.contains("asking") && asking.contains(serving.wire()),
        "while the ask is out the screen says so, and which wire it asked: {asking}"
    );

    assert!(
        f.app.listed(serving, Err("HTTP 404".to_string())),
        "a wire that refused is news the screen draws"
    );
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        f.app.overlay().is_some(),
        "and the screen is still there to read"
    );
    assert!(
        rows.contains("could not be asked") && rows.contains("/model <name>"),
        "where the names would be, the screen says why there are none, and what \
         sets one anyway: {rows}"
    );
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        f.app.overlay().is_some(),
        "Enter on nothing is no name to write, and not the screen closing"
    );
    assert_eq!(
        written(&f.path),
        "model = \"gpt-5\"\n",
        "the file is as it was"
    );
}

#[test]
fn a_fragment_that_reaches_no_name_says_so_where_the_names_would_be() {
    let mut f = offered(None, &["gpt-5", "qwen3-32b"]);
    type_text(&mut f.app, "zzz");

    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        rows.contains("no name reaches zzz"),
        "the screen says the fragment reaches nothing rather than that the wire \
         named nothing: {rows}"
    );
    assert!(
        !rows.contains('▸'),
        "and there is nothing on it to mark: {rows}"
    );
}

/// `n` names the wire offers, in its own order: enough of them to be taller than
/// any screen the frame draws.
fn many(n: usize) -> Vec<String> {
    (0..n).map(|at| format!("model-{at:02}")).collect()
}

#[test]
fn a_list_taller_than_the_screen_scrolls_and_the_keys_stay_on_it() {
    let names = many(40);
    let mut f = offered(None, &names.iter().map(String::as_str).collect::<Vec<_>>());
    for _ in 0..30 {
        press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    }

    let text = shown(&screen(&mut f.app, 100, 20));
    assert!(
        text.contains("Enter set") && text.contains("Esc close"),
        "the keys are never the row a long list pushes off: {text}"
    );
    assert!(
        text.contains("model-30"),
        "the name the cursor stepped to is the name that is drawn: {text}"
    );
    assert!(
        !text.contains("model-00"),
        "and the list scrolled rather than grew: {text}"
    );
}

#[test]
fn a_page_key_walks_the_screen_by_the_names_it_showed() {
    let names = many(40);
    let mut f = offered(None, &names.iter().map(String::as_str).collect::<Vec<_>>());
    // Drawn first, because what a page is worth is what the screen showed.
    let drawn = screen(&mut f.app, 100, 20);
    let shown_names = drawn.iter().filter(|row| row.contains("model-")).count();
    assert!(
        shown_names > 1,
        "the screen holds a screenful of names: {drawn:#?}"
    );

    press(&mut f.app, KeyCode::PageDown, KeyModifiers::NONE);
    assert_eq!(
        marked(&mut f),
        format!("model-{shown_names:02}"),
        "a page is as many names as the screen showed"
    );
    press(&mut f.app, KeyCode::PageUp, KeyModifiers::NONE);
    assert_eq!(
        marked(&mut f),
        "model-00",
        "and a page back is the same distance the other way"
    );
}

#[test]
fn home_and_end_are_the_first_and_the_last_name_the_screen_shows() {
    let names = many(40);
    let mut f = offered(None, &names.iter().map(String::as_str).collect::<Vec<_>>());

    press(&mut f.app, KeyCode::End, KeyModifiers::NONE);
    assert_eq!(
        marked(&mut f),
        "model-39",
        "End is the last name the wire named, not a page towards it"
    );
    press(&mut f.app, KeyCode::Home, KeyModifiers::NONE);
    assert_eq!(
        marked(&mut f),
        "model-00",
        "and Home is the first, whatever the screen was showing"
    );
}

#[test]
fn a_fragment_narrows_what_the_two_ends_mean() {
    let mut f = offered(None, &["gpt-5", "qwen3-32b", "big-gpt"]);
    type_text(&mut f.app, "gpt");

    press(&mut f.app, KeyCode::End, KeyModifiers::NONE);
    assert_eq!(
        marked(&mut f),
        "big-gpt",
        "End is the last name the fragment reaches, not the last name the wire named"
    );
    press(&mut f.app, KeyCode::Home, KeyModifiers::NONE);
    assert_eq!(
        marked(&mut f),
        "gpt-5",
        "and Home is the first name it reaches"
    );
}

#[test]
fn the_key_line_names_the_two_end_keys_beside_the_page_keys() {
    let mut f = offered(None, &["gpt-5", "qwen3-32b"]);
    let text = shown(&screen(&mut f.app, 100, 30));
    assert!(
        text.contains("Home/End") && text.contains("PgUp/PgDn"),
        "the keys are one line, and the ends stand beside the pages: {text}"
    );
}

#[test]
fn an_end_key_with_no_name_on_the_screen_leaves_the_frame_as_it_was() {
    let mut f = fixture(Some("model = \"gpt-5\"\n"));
    open(&mut f.app);
    f.app
        .take_ask()
        .expect("opening the screen asks the wire for its names");
    for code in [KeyCode::Home, KeyCode::End] {
        assert!(
            !f.app.key(KeyEvent::new(code, KeyModifiers::NONE)),
            "{code:?} has no name to stand on, so the frame has nothing new to draw"
        );
    }
    assert!(
        f.app.overlay().is_some(),
        "and the screen is still there to read"
    );
}
