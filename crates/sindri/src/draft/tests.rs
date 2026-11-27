use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::*;
use crate::band::{self, Gutter};
use crate::palette::Palette;
use crate::readback::inks;

const PALETTE: Palette = Palette::TRUECOLOR;

fn workspace(files: &[&str]) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("a temp workspace");
    for file in files {
        let path = root.path().join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the directories a file sits in");
        }
        std::fs::write(&path, "").expect("a file");
    }
    root
}

fn joined(row: &str, opens: bool, files: &Files) -> Vec<(String, Ink)> {
    let mut joined: Vec<(String, Ink)> = Vec::new();
    for (run, ink) in runs(row, opens, files) {
        match joined.last_mut() {
            Some((last, seen)) if *seen == ink => last.push_str(run),
            _ => joined.push((run.to_owned(), ink)),
        }
    }
    joined
}

#[test]
fn the_runs_of_a_row_tile_it_glyph_for_glyph() {
    let root = workspace(&["src/rows.rs"]);
    let files = Files::walk(root.path());
    for row in [
        "",
        "one",
        "one two",
        "/model gpt-5",
        "a `b c` d",
        "`a`/help",
        "unclosed `tick and the rest of it",
        "/skill:commit",
        "read src/rows.rs and 日本語 too",
        "``",
        "`",
    ] {
        let tiled: String = runs(row, true, &files)
            .into_iter()
            .map(|(run, _)| run)
            .collect();
        assert_eq!(
            tiled, row,
            "{row:?} is drawn as runs that put every glyph of it on the row once and none twice"
        );
    }
}

#[test]
fn a_row_that_opens_with_a_command_the_card_holds_draws_that_word_in_the_accent() {
    let files = Files::default();

    assert_eq!(
        joined("/model gpt-5", true, &files),
        [
            (String::from("/model"), Ink::Accent),
            (String::from(" gpt-5"), Ink::Plain)
        ],
        "the word the draft opens with is the command it would send, and the words after it are its \
         argument"
    );
    assert_eq!(
        joined("/skill:commit", true, &files),
        [(String::from("/skill:commit"), Ink::Plain)],
        "a removed spelling stays ordinary prompt text"
    );
    assert_eq!(
        joined("/mod", true, &files),
        [(String::from("/mod"), Ink::Accent)],
        "a name the card still holds rows for wears the accent as it is typed, so the ink does not \
         flicker between one key and the next"
    );
}

#[test]
fn a_word_the_card_holds_no_row_for_stays_plain() {
    let files = Files::default();

    assert_eq!(
        joined("/skillet is a pan", true, &files),
        [(String::from("/skillet is a pan"), Ink::Plain)],
        "a draft that opens with a word no command holds is a draft the reader meant to send"
    );
}

#[test]
fn only_the_word_the_row_opens_with_can_wear_the_accent() {
    let files = Files::default();

    assert_eq!(
        joined("run /help", true, &files),
        [(String::from("run /help"), Ink::Plain)],
        "a command is only a command where the draft opens with it, so a later word never takes the \
         accent"
    );
    assert_eq!(
        joined("/model /help", true, &files),
        [
            (String::from("/model"), Ink::Accent),
            (String::from(" /help"), Ink::Plain)
        ],
        "and no second word of that row takes it, though the card holds one"
    );
    assert_eq!(
        joined("`/model` next", true, &files),
        [
            (String::from("`/model`"), Ink::Muted),
            (String::from(" next"), Ink::Plain)
        ],
        "a command inside a span is set in backticks and draws no accent at all"
    );
    assert_eq!(
        joined(" /model", true, &files),
        [(String::from(" /model"), Ink::Plain)],
        "a row that opens with a blank opens with no command at all, so the word under it is quiet"
    );
}

#[test]
fn a_row_that_opens_nothing_never_wears_the_accent() {
    let files = Files::default();

    assert_eq!(
        joined("/model", false, &files),
        [(String::from("/model"), Ink::Plain)],
        "a command opens the draft, so a row under the first one is no command however it reads"
    );
}

#[test]
fn a_word_naming_a_file_of_the_workspace_is_drawn_in_the_ok_ink() {
    let root = workspace(&["src/rows.rs", "Cargo.toml"]);
    let files = Files::walk(root.path());

    assert_eq!(
        joined("read src/rows.rs and Cargo.toml", true, &files),
        [
            (String::from("read "), Ink::Plain),
            (String::from("src/rows.rs"), Ink::Ok),
            (String::from(" and "), Ink::Plain),
            (String::from("Cargo.toml"), Ink::Ok)
        ],
        "a word the walk holds is a file the prompt points at, in the ink a file is given"
    );
    assert_eq!(
        joined("/model src/rows.rs", true, &files),
        [
            (String::from("/model"), Ink::Accent),
            (String::from(" "), Ink::Plain),
            (String::from("src/rows.rs"), Ink::Ok)
        ],
        "the word the row opens with keeps the accent, and the file after it takes the ok ink"
    );
}

#[test]
fn a_backticked_span_recedes_into_the_muted_ink_and_a_file_named_in_it_with_it() {
    let root = workspace(&["src/rows.rs"]);
    let files = Files::walk(root.path());

    assert_eq!(
        joined("keep `src/rows.rs` as it is", true, &files),
        [
            (String::from("keep "), Ink::Plain),
            (String::from("`src/rows.rs`"), Ink::Muted),
            (String::from(" as it is"), Ink::Plain)
        ],
        "a span is set between its backticks whole, and a file named inside it is muted with it"
    );
}

#[test]
fn a_backtick_the_row_never_closes_leaves_the_rest_of_the_row_plain() {
    let root = workspace(&["src/rows.rs"]);
    let files = Files::walk(root.path());

    assert_eq!(
        joined("/model ` src/rows.rs and more", true, &files),
        [
            (String::from("/model"), Ink::Accent),
            (String::from(" ` src/rows.rs and more"), Ink::Plain)
        ],
        "a backtick with no backtick after it opens no span, so the rest of the row recedes into no \
         ink at all — not even the ok ink a file it names is drawn in"
    );
}

#[test]
fn the_band_draws_a_row_of_the_draft_in_the_inks_the_word_the_file_and_the_span_earn() {
    let root = workspace(&["src/rows.rs"]);
    let files = Files::walk(root.path());
    let area = Rect::new(0, 0, 26, 1);
    let mut buffer = Buffer::empty(area);

    band::row(
        &mut buffer,
        area,
        Gutter::Lit,
        &runs("/model src/rows.rs `a b`", true, &files),
        &PALETTE,
    );

    assert_eq!(
        inks(&buffer, 0, &PALETTE),
        [
            (Ink::Dim, 1),
            (Ink::Accent, 7),
            (Ink::Plain, 1),
            (Ink::Ok, 11),
            (Ink::Plain, 1),
            (Ink::Muted, 5)
        ],
        "every run of the row is drawn in the ink it carries, and the lit sigil with the word it \
         opens reads back as one stretch of the accent past the dim blank of the gutter"
    );
}
