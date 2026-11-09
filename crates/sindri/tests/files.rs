//! The `@` picker's contract with a reader, walked rather than eyeballed: the keys that open and close the
//! screen, the glyphs that are the query and the ones still the prompt's, and the word a selection leaves in
//! the draft. Every workspace is the test's own, so nothing of this repository decides what a picker offers.

use std::path::Path;

use sindri::composer::{Composer, Key, Outcome};
use sindri::keys::Mode;
use sindri::view::View;

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

fn standing(root: &Path) -> View {
    let mut view = View::boot();
    view.set_workspace(root.to_path_buf());
    view
}

fn draft(view: &View) -> &str {
    view.composer().text()
}

/// A frame standing in `root` with `draft` typed through the keys a reader's fingers send.
fn typing(root: &Path, draft: &str) -> View {
    let mut composer = Composer::new();
    composer.paste(draft);
    let mut view = View::typing(composer);
    view.set_workspace(root.to_path_buf());
    view
}

#[test]
fn an_at_opens_the_files_of_the_workspace() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = standing(root.path());

    assert_eq!(
        view.key(Key::Char('@')),
        Outcome::Edited,
        "`@` is a screen opening, so there is a frame to paint"
    );
    assert_eq!(
        view.mode(),
        Mode::Files,
        "and the picker is the mode on top of the stack"
    );
    assert_eq!(view.key(Key::Escape), Outcome::Edited, "Esc closes it");
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "and the draft has the keys again"
    );
}

#[test]
fn an_at_inside_a_word_is_a_glyph_and_not_a_screen() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "why does user@host");
    view.key(Key::Char('@'));

    assert_eq!(
        view.mode(),
        Mode::Composer,
        "an address holds an `@` that is nobody's key: only an `@` that starts a word opens \
         the picker"
    );
    assert_eq!(draft(&view), "why does user@host@");
}

#[test]
fn an_at_after_a_space_walks_the_workspace_where_the_caret_is() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "read ");

    assert_eq!(view.key(Key::Char('@')), Outcome::Edited);
    assert_eq!(
        view.mode(),
        Mode::Files,
        "an `@` that opens a sentence opens the files: the draft is where a path is wanted"
    );
    assert_eq!(
        draft(&view),
        "read ",
        "and the `@` itself is the key, not a glyph"
    );
}

#[test]
fn a_picker_answers_its_own_keys_and_swallows_the_drafts() {
    let root = workspace(&["src/rows.rs"]);
    let mut view = typing(root.path(), "read ");
    view.key(Key::Char('@'));

    for key in [Key::Quit, Key::Cancel] {
        let before = draft(&view).to_owned();
        view.key(key);
        assert_eq!(
            draft(&view),
            before,
            "{key:?} at the picker leaves the draft under it alone"
        );
    }
    assert_eq!(view.mode(), Mode::Files, "and the screen is still on top");
}

#[test]
fn a_glyph_typed_at_the_picker_narrows_it_and_never_reaches_the_draft() {
    let root = workspace(&["rows.rs", "rail.rs"]);
    let mut view = typing(root.path(), "read ");
    view.key(Key::Char('@'));

    for glyph in "rail".chars() {
        assert_eq!(
            view.key(Key::Char(glyph)),
            Outcome::Edited,
            "each glyph of the query is a frame painted without it"
        );
    }

    assert_eq!(view.picker().query(), "rail");
    assert_eq!(
        view.picker().rows(),
        vec!["rail.rs"],
        "the query narrows the rows as it is typed"
    );
    assert_eq!(
        draft(&view),
        "read ",
        "and not one glyph of it was the draft's"
    );
}

#[test]
fn selecting_a_file_puts_its_path_in_the_draft_and_closes_the_screen() {
    let root = workspace(&["rows.rs", "notes/a file.rs"]);
    let mut view = typing(root.path(), "read ");
    view.key(Key::Char('@'));

    assert_eq!(
        view.key(Key::Down),
        Outcome::Edited,
        "the second row is below the first"
    );
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "Enter puts the chosen path in the draft"
    );

    assert_eq!(
        draft(&view),
        "read 'notes/a file.rs' ",
        "the word a shell reads back as the one path, and the caret is left after it"
    );
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "and the screen is gone: the reader is back in the draft it was written into"
    );
}

#[test]
fn a_path_chosen_in_the_middle_of_a_draft_lands_where_the_caret_is() {
    let root = workspace(&["rows.rs"]);
    let mut view = typing(root.path(), "read then say why");
    // The caret back to just after `read `, where a path goes in front of a word already written.
    for _ in 0.."then say why".len() {
        view.key(Key::Left);
    }

    view.key(Key::Char('@'));
    view.key(Key::Enter);

    assert_eq!(
        draft(&view),
        "read rows.rs then say why",
        "a picked path is inserted at the caret and never appended at the end"
    );
}

#[test]
fn a_query_that_matches_nothing_offers_nothing_and_selects_nothing() {
    let root = workspace(&["rows.rs"]);
    let mut view = typing(root.path(), "");
    view.key(Key::Char('@'));
    for glyph in "zzz".chars() {
        view.key(Key::Char(glyph));
    }

    assert!(view.picker().rows().is_empty());
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Unchanged,
        "there is no row to insert, so Enter changes nothing and the screen stays open to be \
         narrowed again"
    );
    assert_eq!(view.mode(), Mode::Files);
}

#[test]
fn a_frame_told_no_workspace_offers_no_file() {
    let mut view = View::boot();

    assert_eq!(
        view.key(Key::Char('@')),
        Outcome::Unchanged,
        "a frame that was told no directory has no files to offer, so nothing opens"
    );
    assert_eq!(view.mode(), Mode::Composer);
}

#[test]
fn escape_at_the_picker_leaves_the_draft_as_it_was() {
    let root = workspace(&["rows.rs"]);
    let mut view = typing(root.path(), "read ");
    view.key(Key::Char('@'));
    view.key(Key::Char('r'));

    assert_eq!(view.key(Key::Escape), Outcome::Edited);

    assert_eq!(
        draft(&view),
        "read ",
        "nothing of the picker reached the draft"
    );
    assert_eq!(view.mode(), Mode::Composer);
}

#[test]
fn the_workspace_a_frame_walks_is_the_one_it_was_told() {
    let root = workspace(&["only-this.rs"]);
    let mut view = standing(root.path());
    view.key(Key::Char('@'));

    assert_eq!(
        view.picker().rows(),
        vec!["only-this.rs"],
        "the walk is of the workspace the frame was handed, and of nothing else"
    );
}
