//! Where the frame stands: the directory it reads at the door, and what the
//! frame does when that read fails — one line in the transcript, and no path
//! offered that would have been built from nothing.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app;
use crate::entry::Entry;
use crate::msg::Msg;
use crate::run::{recorded, walk};

/// Every line the frame has said to the reader, in order.
fn said(app: &crate::app::App) -> Vec<String> {
    app.entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Note(note) => Some(note.text.clone()),
            _ => None,
        })
        .collect()
}

/// The frame standing in a directory the process can no longer name, as it is
/// when the directory is removed under it: `getcwd` answers that there is no
/// such file, and the frame is opened with nothing to resolve against.
fn in_a_removed_directory(read: impl FnOnce() -> crate::app::App) -> crate::app::App {
    let standing = tempfile::tempdir().expect("a directory to stand in");
    let removed = standing.path().to_path_buf();
    let before = std::env::current_dir().expect("the directory the test was started in");
    std::env::set_current_dir(&removed).expect("the frame stands in it");
    std::fs::remove_dir(&removed).expect("the directory is removed under the process");
    let app = read();
    // Put back before anything is asserted: a test that fails here is a test
    // that still leaves the rest of them a directory to stand in.
    std::env::set_current_dir(&before).expect("the test's own directory");
    app
}

#[test]
fn a_frame_whose_directory_was_removed_under_it_says_so_once() {
    let app = in_a_removed_directory(|| recorded(app()));

    let about: Vec<String> = said(&app)
        .into_iter()
        .filter(|line| line.contains("the directory this frame stands in"))
        .collect();
    assert_eq!(
        about.len(),
        1,
        "one line, and it is the one about the directory"
    );
    assert!(
        about[0].ends_with("No such file or directory (os error 2)"),
        "and it names why the read failed: {about:?}"
    );
}

#[test]
fn a_walk_with_no_directory_to_root_it_in_offers_nothing() {
    let mut app = in_a_removed_directory(|| recorded(app()));
    assert!(
        app.workspace_dir().is_none(),
        "what the frame could not read stays absent, never the empty path"
    );

    for ch in "@ma".chars() {
        app.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let mut walking = false;
    walk(&mut app, &tx, &mut walking, None);
    assert!(!walking, "nothing is walked when there is nowhere to walk");

    let Ok(Msg::Paths(generation, paths, capped)) = rx.try_recv() else {
        panic!("the ask is answered, so the composer waits for no walk");
    };
    assert_eq!(Some(generation), app.completion().map(|c| c.generation));
    assert!(paths.is_empty(), "and it offers nothing: {paths:?}");
    assert!(!capped, "an empty walk is not a capped one");
}
