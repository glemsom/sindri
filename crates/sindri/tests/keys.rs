//! The mode stack's contract, walked rather than eyeballed: one key table per mode
//! with no key bound twice in one of them, a screen that answers only its own keys,
//! and the two keys that carry a rule — `?` that opens the keys and `↑` that the
//! queue shares with the draft — resolving exactly where the appendix says (§12, A7,
//! A9).

use sindri::composer::{Composer, Key, Outcome};
use sindri::keys::{Act, Mode};
use sindri::view::View;

/// A draft holding `text`, typed a line at a time: the composer is built through the
/// same keys a reader's fingers send, so a frame here is one the product can make.
fn typed(lines: &[&str]) -> Composer {
    let mut composer = Composer::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            composer.key(Key::Newline);
        }
        composer.paste(line);
    }
    composer
}

/// A view a run is live in and prompts are waiting in: `prompts` typed and sent while
/// nothing could run them, in the order a reader types them, and then `draft`'s lines
/// typed into the box underneath — so a test starts where the reader is.
fn waiting(draft: &[&str], prompts: &[&str]) -> View {
    let mut view = View::boot();
    for prompt in prompts {
        view.paste(prompt);
        match view.key(Key::Enter) {
            Outcome::Send(prompt) => view.enqueue(prompt),
            other => panic!("a draft should have been sent, and {other:?} came instead"),
        }
    }
    for (index, line) in draft.iter().enumerate() {
        if index > 0 {
            view.key(Key::Newline);
        }
        view.paste(line);
    }
    view
}

#[test]
fn no_mode_binds_one_key_twice() {
    for mode in Mode::ALL {
        let table = mode.table();
        assert!(
            !table.is_empty(),
            "{} is a mode and a mode is a table of the keys it answers",
            mode.name()
        );
        for (at, one) in table.iter().enumerate() {
            for other in &table[at + 1..] {
                assert_ne!(
                    one.key,
                    other.key,
                    "{} binds {:?} twice, so the key would resolve in two rows of one \
                     table: {one:?} and {other:?}",
                    mode.name(),
                    one.key
                );
            }
        }
    }
}

#[test]
fn every_row_of_a_table_says_what_its_key_is_and_does() {
    for mode in Mode::ALL {
        for binding in mode.table() {
            assert!(
                !binding.name.is_empty() && !binding.does.is_empty(),
                "the `?` screen draws every row of {} as a name and what it does, and \
                 {binding:?} has one of them missing",
                mode.name()
            );
        }
    }
}

#[test]
fn question_mark_opens_the_keys_screen_and_escape_closes_it() {
    let mut view = View::boot();

    assert_eq!(view.mode(), Mode::Composer, "a launch opens on the draft");
    assert_eq!(
        view.key(Key::Char('?')),
        Outcome::Edited,
        "`?` is a screen opening, so there is a frame to paint"
    );
    assert_eq!(
        view.mode(),
        Mode::Keys,
        "and the screen is on top of the stack"
    );
    assert_eq!(
        view.key(Key::Escape),
        Outcome::Edited,
        "Esc closes it, and a frame is painted without it"
    );
    assert_eq!(view.mode(), Mode::Composer, "the draft has the keys again");
}

#[test]
fn a_question_mark_with_a_draft_typed_is_a_glyph_and_not_a_screen() {
    let mut view = View::typing(typed(&["why is the sky"]));

    assert_eq!(
        view.key(Key::Char('?')),
        Outcome::Edited,
        "a prompt that asks a question is a prompt that can hold a question mark"
    );
    assert_eq!(view.composer().text(), "why is the sky?");
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "no screen was opened by typing"
    );
}

#[test]
fn a_screen_answers_its_own_keys_and_swallows_the_drafts() {
    // The screen is opened on an empty draft, because that is the only place `?` is a
    // key: from there, a glyph that reached the draft under it would show.
    let mut view = View::boot();
    view.key(Key::Char('?'));

    for key in [
        Key::Char('x'),
        Key::Enter,
        Key::Backspace,
        Key::Quit,
        Key::Drop,
    ] {
        assert_eq!(
            view.key(key),
            Outcome::Unchanged,
            "the screen on top answers {key:?} nothing at all"
        );
    }
    assert_eq!(
        view.mode(),
        Mode::Keys,
        "and it is still on top: Ctrl-Q belongs to the draft, not to the screen"
    );
    assert_eq!(
        view.composer().text(),
        "",
        "not one of those keys reached the draft under the screen"
    );
}

#[test]
fn the_up_arrow_is_bound_in_one_mode_and_one_row() {
    let bound: Vec<(Mode, Act)> = Mode::ALL
        .iter()
        .flat_map(|mode| mode.table().iter().map(move |binding| (*mode, binding)))
        .filter(|(_, binding)| binding.key == Key::Up)
        .map(|(mode, binding)| (mode, binding.act))
        .collect();

    assert_eq!(
        bound,
        vec![(Mode::Composer, Act::Up)],
        "`↑` resolves in exactly one place: the queue's recall and the draft's row \
         above are one arm, and A9 is decided there (A9)"
    );
}

#[test]
fn up_on_the_first_line_is_the_queues_and_off_it_moves_the_caret() {
    let mut view = View::typing(typed(&["one", "two"]));

    assert_eq!(
        view.key(Key::Up),
        Outcome::Edited,
        "off the first line the arrow is the draft's"
    );
    assert_eq!(
        view.composer().caret(),
        (0, 3),
        "so the caret rose a row, keeping its column"
    );

    assert_eq!(
        view.key(Key::Up),
        Outcome::Unchanged,
        "on the first line it is the queue's, and with no queue there is nothing to \
         recall and nothing to repaint"
    );
    assert_eq!(view.composer().caret(), (0, 3), "and the caret stayed put");
}

#[test]
fn up_on_the_first_line_recalls_the_newest_queued_prompt() {
    let mut view = waiting(&[], &["first", "second"]);

    assert_eq!(
        view.key(Key::Up),
        Outcome::Edited,
        "the recall is a frame to paint, because the draft and the count both changed"
    );
    assert_eq!(
        view.composer().text(),
        "second",
        "the prompt typed last is the draft again"
    );
    assert_eq!(view.queue().len(), 1, "and it has left the queue");
}

#[test]
fn up_off_the_first_line_moves_the_caret_and_leaves_the_queue_alone() {
    let mut view = waiting(&["half a thought", "and a second line"], &["first"]);

    assert_eq!(
        view.composer().caret(),
        (1, 17),
        "the caret waits on row two"
    );
    assert_eq!(
        view.key(Key::Up),
        Outcome::Edited,
        "off the first line the arrow is the draft's"
    );
    assert_eq!(
        view.composer().caret(),
        (0, 14),
        "so the caret rose a row, keeping its column"
    );
    assert_eq!(
        view.queue().len(),
        1,
        "and the queue is not what answered the key"
    );
}

#[test]
fn ctrl_x_drops_the_newest_queued_prompt() {
    let mut view = waiting(&[], &["first", "second", "third"]);

    assert_eq!(view.key(Key::Drop), Outcome::Edited, "the count changed");
    assert_eq!(
        view.queue().len(),
        2,
        "the prompt typed last is the one dropped"
    );

    assert_eq!(view.key(Key::Up), Outcome::Edited);
    assert_eq!(
        view.composer().text(),
        "second",
        "the third is gone and the order of the rest is untouched"
    );
}

#[test]
fn ctrl_x_with_nothing_queued_is_not_a_repaint() {
    assert_eq!(
        View::boot().key(Key::Drop),
        Outcome::Unchanged,
        "there is no newest queued prompt to drop, so the frame is the one it was"
    );
}

#[test]
fn ctrl_x_is_bound_in_one_mode_and_one_row() {
    let bound: Vec<(Mode, Act)> = Mode::ALL
        .iter()
        .flat_map(|mode| mode.table().iter().map(move |binding| (*mode, binding)))
        .filter(|(_, binding)| binding.key == Key::Drop)
        .map(|(mode, binding)| (mode, binding.act))
        .collect();

    assert_eq!(
        bound,
        vec![(Mode::Composer, Act::Drop)],
        "`Ctrl-X` resolves in exactly one place, and the queue is what answers it (A9)"
    );
}

#[test]
fn a_paste_while_a_screen_is_open_reaches_no_draft() {
    let mut view = View::boot();
    view.key(Key::Char('?'));

    view.paste("a prompt nobody can see");

    assert_eq!(
        view.composer().text(),
        "",
        "a screen has no draft, so a clipboard's lines land nowhere rather than in a \
         draft the reader cannot read"
    );
}
