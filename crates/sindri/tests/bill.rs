//! The count the band's head row hangs of the draft: the files its words name and the command it
//! opens with, as a reader's own keys and a card's own rows change it.

mod frame; // one paint-and-read for every test

use std::path::Path;

use frame::Frame;

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::view::{Transcript, View};

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

/// A frame standing in `root` with `draft` typed through the keys a reader's fingers send.
fn typing(root: &Path, draft: &str) -> View {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked().working(root.to_path_buf(), None),
        Transcript::none(),
    );
    view.paste(draft);
    view
}

/// The band's head row, as the glyphs it draws: the first row of the composer band the pass handed
/// back, which is the row the draft's count hangs on.
fn head_row(frame: &Frame) -> String {
    let band = frame
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out")
        .composer;
    frame.row(band.y)
}

fn draft(view: &View) -> &str {
    view.composer().text()
}

#[test]
fn a_paste_bills_the_files_it_names_the_moment_it_lands() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "read src/rows.rs");
    let frame = frame::painted(&mut view, 120, 40);
    let head = head_row(&frame);

    assert!(
        head.trim_end().ends_with("1 file"),
        "the head hangs no count of the files the draft the reader pasted names: {head:?}"
    );
}

#[test]
fn a_recalled_prompt_is_billed_as_the_draft_it_becomes() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "read src/rows.rs");
    let Outcome::Send(prompt) = view.key(Key::Enter) else {
        panic!("a draft naming no command is a prompt once it is sent");
    };
    view.enqueue(prompt);

    assert_eq!(
        draft(&view),
        "",
        "the draft a reader sent is the queue's and no longer theirs"
    );
    assert_eq!(
        view.key(Key::Up),
        Outcome::Edited,
        "`↑` recalls the prompt the queue holds"
    );
    let frame = frame::painted(&mut view, 120, 40);
    let head = head_row(&frame);

    assert!(
        head.trim_end().ends_with("1 file"),
        "the head bills the draft a recalled prompt becomes: {head:?}"
    );
}

#[test]
fn a_row_a_card_writes_into_the_draft_is_billed_with_it() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "read ");
    view.key(Key::Char('@'));
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "the chosen path is written into the draft"
    );
    assert_eq!(draft(&view), "read src/rows.rs ");
    let frame = frame::painted(&mut view, 120, 40);
    let head = head_row(&frame);

    assert!(
        head.trim_end().ends_with("1 file"),
        "the head bills the file the picker's row wrote into the draft: {head:?}"
    );

    let mut view = typing(root.path(), "/sk");
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "the row the commands card marks is written into the draft"
    );
    assert_eq!(
        draft(&view),
        "/skill",
        "a row whose line names no command yet is a word of the draft"
    );
    let frame = frame::painted(&mut view, 120, 40);
    let head = head_row(&frame);

    assert!(
        head.chars().all(|glyph| glyph == '\u{2500}'),
        "the head bills nothing of the word the commands card wrote into the draft, so the rule \
         keeps the row: {head:?}"
    );
}

#[test]
fn a_draft_that_bills_nothing_hangs_no_count_on_the_head() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "a draft that names no file of the workspace");
    let frame = frame::painted(&mut view, 120, 40);
    let head = head_row(&frame);

    assert!(
        head.chars().all(|glyph| glyph == '\u{2500}'),
        "a draft of any length bills nothing of itself, so a draft that names no file hangs no \
         count and the rule keeps the row: {head:?}"
    );
}
