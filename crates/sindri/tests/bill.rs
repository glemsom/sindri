mod frame;

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

fn typing(root: &Path, draft: &str) -> View {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked().working(root.to_path_buf(), None),
        Transcript::none(),
    );
    view.paste(draft);
    view
}

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

    for text in ["/skill:deploy", "/skill deploy"] {
        let mut view = typing(root.path(), text);
        let frame = frame::painted(&mut view, 120, 40);
        assert!(head_row(&frame).trim().is_empty());
        assert_eq!(draft(&view), text);
        assert!(matches!(view.key(Key::Enter), Outcome::Send(_)));
    }
}

#[test]
fn a_draft_that_bills_nothing_hangs_no_count_on_the_head() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "a draft that names no file of the workspace");
    let frame = frame::painted(&mut view, 120, 40);
    let head = head_row(&frame);

    assert!(
        head.chars().all(|glyph| glyph == ' '),
        "a draft of any length bills nothing of itself, so a draft that names no file hangs no \
         count and the edge is left whole: {head:?}"
    );
}
