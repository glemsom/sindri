//! The extra headers row's fold: `Enter` on the row opens the table as a list of
//! `Name: value` lines with `+ add a header` at the end, `a` adds one, `d`
//! removes the one under the cursor, `Enter` hands that line the keys, `Ctrl-U`
//! empties it, and `Esc` shuts the fold and keeps the draft. A keyboard alone
//! builds the table: a paste is one way in and no longer the only one.

use super::*;
use sindri_tui::overlay::Overlay;
use sindri_tui::theme;

/// The text the modal holds for `key`, read off the field rather than off a row
/// a screen may truncate.
fn field(f: &Fixture, key: Key) -> String {
    let Some(Overlay::Settings(settings)) = f.app.overlay() else {
        panic!("the settings modal is open");
    };
    settings
        .fields()
        .iter()
        .find(|field| field.key == key)
        .expect("every key has a field")
        .text
        .clone()
}

/// The modal open with the cursor on the headers row, the table as the file
/// left it: what every case here starts from, so a test says only what it does
/// differently.
fn opened(file: Option<&str>) -> Fixture {
    let mut f = fixture(file);
    open(&mut f.app);
    down_to(&mut f.app, Key::ExtraHeaders);
    f
}

/// The header the fold's cursor marks: the one `d` removes and `Enter` writes.
/// Read off the row the `▸` stands on, because that is what a reader sees.
fn marked(f: &mut Fixture) -> String {
    let rows = screen(&mut f.app, 100, 30);
    let row = rows
        .iter()
        .find(|row| row.contains("▸ "))
        .unwrap_or_else(|| panic!("the fold marks a line: {}", shown(&rows)));
    row.split("▸ ")
        .nth(1)
        .expect("the mark has a line after it")
        .split(" · ")
        .next()
        .expect("a line before the row's note")
        .trim_end_matches([' ', '│'])
        .to_string()
}

/// `Enter`, the one key whose meaning this row climbs.
fn enter(f: &mut Fixture) {
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
}

/// A fold opened on the table the file holds.
fn folded(f: &mut Fixture) {
    enter(f);
}

#[test]
fn enter_on_the_headers_row_folds_the_table_open_under_it() {
    let mut f = opened(Some(
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\nx-2 = \"2\"\n",
    ));
    let shut = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !shut.contains("+ add a header") && !shut.contains("▸"),
        "the table stands shut until the row is asked to open it: {shut}"
    );

    folded(&mut f);
    let open = shown(&screen(&mut f.app, 100, 30));
    assert!(
        f.app.overlay().is_some(),
        "the fold is not the modal closing"
    );
    assert!(
        open.contains("▸ x-1: 1") && open.contains("x-2: 2"),
        "one line per header, the one under the cursor marked: {open}"
    );
    assert!(
        open.contains("+ add a header"),
        "and the offer to add one at the end: {open}"
    );
    assert_eq!(
        written(&f.path),
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\nx-2 = \"2\"\n",
        "and opening a fold writes nothing"
    );
}

#[test]
fn the_fold_opens_on_the_first_header_of_the_table() {
    let mut f = opened(Some(
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\nx-2 = \"2\"\n",
    ));
    folded(&mut f);
    assert_eq!(
        marked(&mut f),
        "x-1: 1",
        "the cursor opens on the table's first header, as the table is read"
    );
}

#[test]
fn esc_shuts_the_fold_and_keeps_the_draft_and_a_second_esc_discards() {
    let mut f = opened(Some("model = \"m\"\n[extra_headers]\nx-1 = \"1\"\n"));
    folded(&mut f);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(f.app.overlay().is_some(), "one Esc shuts the fold alone");
    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-1: 1",
        "and keeps the draft as it was"
    );
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains("+ add a header"),
        "the table is the plain row again: {rows}"
    );

    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(f.app.overlay().is_none(), "the second Esc is the modal's");
    assert_eq!(
        written(&f.path),
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\n",
        "and discards the draft as it always did"
    );
}

#[test]
fn the_enter_after_the_fold_shuts_saves_the_table_the_row_drew() {
    let mut f = opened(Some("model = \"m\"\n[extra_headers]\nx-1 = \"1\"\n"));
    folded(&mut f);
    enter(&mut f);
    type_text(&mut f.app, " two");
    enter(&mut f);
    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-1: 1 two",
        "the line under the cursor took what was typed on it"
    );

    enter(&mut f);
    assert!(
        f.app.overlay().is_none(),
        "a second Enter on the shut row still saves the whole modal"
    );
    assert_eq!(
        written(&f.path),
        "model = \"m\"\n\n[extra_headers]\nx-1 = \"1 two\"\n",
        "the file carries the table the row showed, in the same spelling"
    );
}

#[test]
fn the_terminal_cursor_stands_on_the_line_a_reader_is_writing() {
    let mut f = opened(Some(
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\nx-2 = \"2\"\n",
    ));
    folded(&mut f);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    enter(&mut f);

    let at = common::cursor(&mut f.app, 100, 30);
    let rows = screen(&mut f.app, 100, 30);
    let line = &rows[at.y as usize];
    // The cells before the cursor, as a reader reads them: the box's own
    // borders are glyphs a byte index cannot be trusted with.
    let typed: String = line.chars().take(at.x as usize).collect();
    assert!(
        typed.ends_with("x-2: 2"),
        "the cursor stands past the last character of the line the keys are \
         the line's of: {line}"
    );
}

#[test]
fn a_header_typed_on_the_shut_row_still_saves_with_enter() {
    let mut f = opened(Some("model = \"m\"\n"));
    type_text(&mut f.app, "x-add: 2");

    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-add: 2",
        "the shut row is the field it always was: the fold's letter is its own"
    );
    enter(&mut f);
    assert!(
        f.app.overlay().is_none(),
        "Enter on a row someone has typed in writes it"
    );
    assert!(
        written(&f.path).contains("x-add = \"2\""),
        "and the file holds the header: {}",
        written(&f.path)
    );
}

#[test]
fn a_keyboard_alone_builds_a_table_of_three_headers() {
    let mut f = opened(Some("model = \"m\"\n"));
    folded(&mut f);

    for line in ["x-1: 1", "x-2: 2", "x-3: 3"] {
        press(&mut f.app, KeyCode::Char('a'), KeyModifiers::NONE);
        type_text(&mut f.app, line);
        enter(&mut f);
    }
    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-1: 1\nx-2: 2\nx-3: 3",
        "add, fill, add, fill, add, fill: no paste anywhere in it"
    );

    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);
    enter(&mut f);
    let file = written(&f.path);
    assert!(
        file.contains("x-1 = \"1\"")
            && file.contains("x-2 = \"2\"")
            && file.contains("x-3 = \"3\""),
        "all three are in the file the save wrote: {file}"
    );
}

#[test]
fn a_header_added_at_the_fold_lands_at_the_end_of_the_table() {
    let mut f = opened(Some(
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\nx-2 = \"2\"\n",
    ));
    folded(&mut f);
    press(&mut f.app, KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(
        marked(&mut f),
        "",
        "the fold's cursor is on a blank line at the end, ready for a header"
    );

    type_text(&mut f.app, "x-3: 3");
    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-1: 1\nx-2: 2\nx-3: 3",
        "and what is typed there is the table's next line"
    );
}

#[test]
fn d_removes_the_header_under_the_cursor_and_leaves_the_rest() {
    let mut f = opened(Some(
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\nx-2 = \"2\"\nx-3 = \"3\"\n",
    ));
    folded(&mut f);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Char('d'), KeyModifiers::NONE);

    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-1: 1\nx-3: 3",
        "the header the cursor was on is gone and the two around it are whole"
    );
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains("x-2: 2"),
        "and the row no longer draws it: {rows}"
    );

    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);
    enter(&mut f);
    let file = written(&f.path);
    assert!(
        !file.contains("x-2") && file.contains("x-1 = \"1\"") && file.contains("x-3 = \"3\""),
        "the save writes the table the row showed: {file}"
    );
}

#[test]
fn a_line_edited_in_the_middle_keeps_the_headers_either_side_of_it() {
    let mut f = opened(Some(
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\nx-2 = \"2\"\nx-3 = \"3\"\n",
    ));
    folded(&mut f);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    enter(&mut f);
    type_text(&mut f.app, "9");

    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-1: 1\nx-2: 2\nx-3: 39",
        "the keys are the line's, so the character lands on it and nowhere else"
    );
    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);
    enter(&mut f);
    let file = written(&f.path);
    assert!(
        file.contains("x-1 = \"1\"")
            && file.contains("x-2 = \"2\"")
            && file.contains("x-3 = \"39\""),
        "the neighbours are exactly as they were: {file}"
    );
}

#[test]
fn a_line_being_written_holds_the_letters_the_fold_takes() {
    let mut f = opened(Some("model = \"m\"\n"));
    folded(&mut f);
    press(&mut f.app, KeyCode::Char('a'), KeyModifiers::NONE);
    type_text(&mut f.app, "x-add: dad");

    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-add: dad",
        "a header's value may hold the two letters the fold takes, because a \
         line being written owns its own keys"
    );
}

#[test]
fn ctrl_u_in_the_fold_empties_the_one_line_and_leaves_the_rest() {
    let mut f = opened(Some(
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\nx-2 = \"2\"\nx-3 = \"3\"\n",
    ));
    folded(&mut f);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-1: 1\n\nx-3: 3",
        "the line under the cursor is empty and its neighbours are whole"
    );

    type_text(&mut f.app, "x-2: two");
    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-1: 1\nx-2: two\nx-3: 3",
        "and an emptied line is the one about to be typed"
    );
}

#[test]
fn ctrl_u_on_the_shut_row_leaves_the_key_out_of_the_file() {
    let mut f = opened(Some(
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\nx-2 = \"2\"\n",
    ));
    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    enter(&mut f);

    let file = written(&f.path);
    assert!(
        !file.contains("extra_headers") && !file.contains("x-1"),
        "a key cleared in the row leaves the file: {file}"
    );
}

#[test]
fn a_pasted_block_still_lands_as_one_header_per_line() {
    let mut f = opened(Some("model = \"m\"\n"));
    assert!(f.app.paste("x-1: 1\r\nx-2: 2\r\n"), "the row takes it");
    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-1: 1\nx-2: 2",
        "one line of the clipboard is one header, and its line ending is no \
         header of its own"
    );

    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        rows.contains("2 headers"),
        "the row counts the two headers a save would write: {rows}"
    );
}

#[test]
fn the_row_states_how_many_headers_the_table_holds() {
    let mut f = opened(Some(
        "model = \"m\"\n[extra_headers]\nx-1 = \"1\"\nx-2 = \"2\"\nx-3 = \"3\"\n",
    ));
    let rows = screen(&mut f.app, 100, 30);
    let at = row_of(&rows, "extra_headers");
    let model = row_of(&rows, "provider");
    {
        let cells = common::buffer(&mut f.app, 100, 30);
        for (x, ch) in rows[model].chars().enumerate().take(28) {
            let cell = &cells[(u16::try_from(x).unwrap(), u16::try_from(model).unwrap())];
            eprintln!("MODEL {x} {ch:?} fg={:?} mods={:?}", cell.fg, cell.modifier);
        }
    }
    assert!(
        rows[at].contains("3 headers"),
        "the count is on the row's own line: {}",
        shown(&rows)
    );

    let cells = common::buffer(&mut f.app, 100, 30);
    let x = u16::try_from(rows[at].find("3 headers").expect("the count is there"))
        .expect("a column of the frame");
    let cell = &cells[(x, u16::try_from(at).expect("a row of the frame"))];
    assert_eq!(
        cell.fg,
        theme::FAINT,
        "the count is a faint note and never a value of the file: {}",
        rows[at]
    );
}

#[test]
fn the_fold_draws_a_session_token_as_the_file_writes_it() {
    let mut f = opened(Some(
        "model = \"m\"\n[extra_headers]\nx-session = \"{session}\"\n",
    ));
    folded(&mut f);

    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        rows.contains("x-session: {session}"),
        "the pane draws the file's text and never a resolved one: {rows}"
    );
    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-session: {session}",
        "and a save writes the token back as it was written"
    );
}

#[test]
fn the_hint_line_states_the_keys_the_fold_takes() {
    let mut f = opened(Some("model = \"m\"\n[extra_headers]\nx-1 = \"1\"\n"));
    let shut = row(&screen(&mut f.app, 100, 30), "Ctrl-U");
    assert!(
        shut.contains("Enter save") && shut.contains("Esc discard"),
        "shut, the line is the modal's own: {shut}"
    );

    folded(&mut f);
    let open = row(&screen(&mut f.app, 100, 30), "a add");
    for said in ["↑↓ move the lines", "a add", "d delete", "Enter edit"] {
        assert!(
            open.contains(said),
            "open, the line states the keys the fold takes, {said:?}: {open}"
        );
    }

    enter(&mut f);
    let written = row(&screen(&mut f.app, 100, 30), "Enter done");
    for said in ["Enter done", "Ctrl-U empty the line", "Esc close"] {
        assert!(
            written.contains(said),
            "and while a line is written, {said:?}: {written}"
        );
    }
}

#[test]
fn a_page_off_the_headers_row_shuts_the_fold() {
    let mut f = opened(Some("model = \"m\"\n[extra_headers]\nx-1 = \"1\"\n"));
    folded(&mut f);
    press(&mut f.app, KeyCode::PageUp, KeyModifiers::NONE);

    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains('▸') && !rows.contains("+ add a header"),
        "the table is the headers row's, so a page off it shuts it: {rows}"
    );
    assert_eq!(
        field(&f, Key::ExtraHeaders),
        "x-1: 1",
        "and the draft is kept"
    );
}
