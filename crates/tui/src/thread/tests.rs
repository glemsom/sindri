//! The thread's own tests, over a thread of entries alone: which heights a
//! mutation forgets, and which it leaves standing.

use std::time::Duration;

use super::*;
use crate::entry::Note;

/// Three notes, so that a thread has an entry above a change, the entry the
/// change lands in, and an entry below it.
fn thread() -> Thread {
    let mut thread = Thread::default();
    for text in ["first", "second", "third"] {
        thread.push(Entry::Note(Note::info(text)));
    }
    thread
}

/// What the thread says entry `index` is worth at 80 columns, measured and
/// kept.
fn measure(thread: &mut Thread, index: usize) -> usize {
    thread.measure(index, 80, Duration::ZERO, None)
}

/// Every entry measured, so that a forget is the only thing a later measure
/// can cost.
fn measured(thread: &mut Thread) {
    for index in 0..thread.len() {
        measure(thread, index);
    }
}

#[test]
fn a_push_does_not_forget_a_height_above_it() {
    let mut thread = thread();
    let first = measure(&mut thread, 0);
    let counted = thread.measured();

    thread.push(Entry::Note(Note::info("fourth")));

    assert_eq!(
        measure(&mut thread, 0),
        first,
        "the note above the new one is worth what it was worth"
    );
    assert_eq!(
        thread.measured(),
        counted,
        "and it was read back rather than measured again"
    );
}

#[test]
fn an_edit_forgets_the_entry_it_changed_and_every_entry_after_it() {
    let mut thread = thread();
    measured(&mut thread);
    let counted = thread.measured();

    thread.edit(1, |entry| {
        if let Entry::Note(note) = entry {
            note.text.push_str(" and more");
        }
    });

    measure(&mut thread, 0);
    assert_eq!(
        thread.measured(),
        counted,
        "the entry above the change is still what it was measured at"
    );
    measure(&mut thread, 1);
    measure(&mut thread, 2);
    assert_eq!(
        thread.measured(),
        counted + 2,
        "the entry that changed and the one below it are measured again"
    );
}

#[test]
fn an_edit_of_the_newest_entry_forgets_the_newest_entry_alone() {
    let mut thread = thread();
    measured(&mut thread);
    let counted = thread.measured();

    thread.edit_last(|entry| {
        if let Entry::Note(note) = entry {
            note.text.push_str(" and more");
        }
    });

    measure(&mut thread, 0);
    measure(&mut thread, 1);
    assert_eq!(
        thread.measured(),
        counted,
        "the entries above the newest one keep their heights: a streamed fragment moves no row above it"
    );
    measure(&mut thread, 2);
    assert_eq!(
        thread.measured(),
        counted + 1,
        "the newest entry is measured again, because its text changed"
    );
}

#[test]
fn a_keep_that_drops_nothing_forgets_nothing() {
    let mut thread = thread();
    measured(&mut thread);
    let counted = thread.measured();

    thread.keep(|entry| !matches!(entry, Entry::Note(note) if note.text.is_empty()));

    assert_eq!(thread.len(), 3, "all three notes are kept");
    for index in 0..thread.len() {
        measure(&mut thread, index);
    }
    assert_eq!(
        thread.measured(),
        counted,
        "a removal that removed nothing moved no row, so no height is forgotten"
    );
}

#[test]
fn a_keep_that_drops_one_entry_forgets_every_height() {
    let mut thread = thread();
    measured(&mut thread);
    let counted = thread.measured();

    thread.keep(|entry| !matches!(entry, Entry::Note(note) if note.text == "second"));

    assert_eq!(thread.len(), 2, "the note the keep refused has gone");
    for index in 0..thread.len() {
        measure(&mut thread, index);
    }
    assert_eq!(
        thread.measured(),
        counted + 2,
        "a removal moves every row below it, and there is no index to name"
    );
}

#[test]
fn a_clear_forgets_every_height() {
    let mut thread = thread();
    measured(&mut thread);
    let counted = thread.measured();

    thread.clear();

    assert_eq!(thread.len(), 0, "every entry went");
    assert_eq!(thread.last_index(), None, "and there is no newest entry");
    thread.push(Entry::Note(Note::info("first")));
    measure(&mut thread, 0);
    assert_eq!(
        thread.measured(),
        counted + 1,
        "the entry that took the first one's index was measured, not read back"
    );
}

#[test]
fn a_measured_entry_is_not_measured_again_at_the_width_it_was_measured_at() {
    let mut thread = thread();
    let once = measure(&mut thread, 0);
    let counted = thread.measured();

    let again = measure(&mut thread, 0);

    assert_eq!(
        again, once,
        "the same entry at the same width is worth the same"
    );
    assert_eq!(
        thread.measured(),
        counted,
        "and the second frame reads the height rather than measuring it"
    );
    measure(&mut thread, 0);
    assert!(
        thread.counted() > 0,
        "the rows the first measurement read are still counted"
    );
}

#[test]
fn a_forget_measures_every_entry_again() {
    let mut thread = thread();
    measured(&mut thread);
    let counted = thread.measured();

    thread.forget();

    for index in 0..thread.len() {
        measure(&mut thread, index);
    }
    assert_eq!(
        thread.measured(),
        counted + 3,
        "a diagram that landed and a width that changed move rows no index can name, so every height goes"
    );
}

#[test]
fn a_width_that_changed_is_measured_again() {
    let mut thread = thread();
    measure(&mut thread, 0);
    let counted = thread.measured();

    thread.measure(0, 40, Duration::ZERO, None);

    assert_eq!(
        thread.measured(),
        counted + 1,
        "a height is trusted only for the width it was measured at"
    );
}
