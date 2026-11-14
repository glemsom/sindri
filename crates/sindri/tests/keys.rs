//! The mode stack's contract: one key table per mode with no key bound twice in one of them, a screen that
//! answers only its own keys, and `?` and `↑` resolving where the tables say they do.

use sindri::composer::{Composer, Key, Outcome};
use sindri::keys::{Act, Mode};
use sindri::view::View;

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

/// The act the table of `mode` gives `key`, or nothing where no row of it answers.
fn act_of(mode: Mode, key: Key) -> Option<Act> {
    mode.table()
        .iter()
        .find(|binding| binding.key == key)
        .map(|binding| binding.act)
}

#[test]
fn the_four_keys_that_move_the_transcript_are_one_band_each_way() {
    assert_eq!(act_of(Mode::Composer, Key::PageUp), Some(Act::Back));
    assert_eq!(
        act_of(Mode::Composer, Key::Clear),
        Some(Act::Back),
        "`Ctrl-U` is the same band back as `PgUp`, and on the draft it is no longer a key \
         nobody answers"
    );
    assert_eq!(act_of(Mode::Composer, Key::PageDown), Some(Act::Forward));
    assert_eq!(
        act_of(Mode::Composer, Key::Forward),
        Some(Act::Forward),
        "`Ctrl-D` is the same band on as `PgDn`"
    );
}

#[test]
fn end_returns_the_frame_to_the_tail_and_is_the_carets_own_only_there() {
    assert_eq!(
        act_of(Mode::Composer, Key::End),
        Some(Act::Tail),
        "`End` is the tail of the run, and the end of the line only when the frame is \
         already there"
    );
}

#[test]
fn the_settings_screen_still_empties_a_value_with_a_ctrl_u() {
    assert_eq!(
        act_of(Mode::Settings, Key::Clear),
        Some(Act::Clear),
        "the one key two modes answer is each mode's own act: the transcript moves on the \
         draft and a value is unset at the settings screen"
    );
}

#[test]
fn the_stats_screen_is_the_sixth_mode_and_answers_escape_and_nothing_else() {
    assert_eq!(
        Mode::ALL.len(),
        6,
        "a mode added later is a variant, a table and a row of `ALL`, so the `?` screen lists \
         it with the five that came before"
    );
    assert!(Mode::ALL.contains(&Mode::Stats));
    assert_eq!(
        Mode::Stats.name(),
        "stats",
        "the word the `?` screen heads it with"
    );

    let table = Mode::Stats.table();

    assert_eq!(
        table.len(),
        1,
        "a screen of what the run has already written has nothing to type into and nothing to \
         walk, so it answers one key"
    );
    assert_eq!(table[0].key, Key::Escape);
    assert_eq!(
        table[0].act,
        Act::Close,
        "the key that gives the frame back"
    );
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
    // The screen is opened on an empty draft, because that is the only place `?` is a key: from there a
    // glyph that reached the draft under it would show.
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
fn the_up_arrows_one_arm_is_bound_in_one_row_of_one_table() {
    // The draft's `↑` is `Act::Up` in exactly one row: the queue's recall and the row above the caret are
    // one act. A screen's `↑` is its own act — a list's selection is not a caret — and this rule does
    // not reach it.
    let bound: Vec<(Mode, Key)> = Mode::ALL
        .iter()
        .flat_map(|mode| mode.table().iter().map(move |binding| (*mode, binding)))
        .filter(|(_, binding)| binding.act == Act::Up)
        .map(|(mode, binding)| (mode, binding.key))
        .collect();

    assert_eq!(
        bound,
        vec![(Mode::Composer, Key::Up)],
        "the draft's `↑` resolves in exactly one place, and §12's rule is decided there (§12)"
    );
}

#[test]
fn the_pickers_up_and_down_are_the_lists_and_never_the_drafts() {
    let files = Mode::Files.table();

    assert_eq!(
        files
            .iter()
            .find(|binding| binding.key == Key::Up)
            .map(|binding| binding.act),
        Some(Act::Previous),
        "`↑` at the picker moves the selection, and never the draft's caret under the \
         screen: the keys a mode binds are its own"
    );
    assert_eq!(
        files
            .iter()
            .find(|binding| binding.key == Key::Down)
            .map(|binding| binding.act),
        Some(Act::Next)
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
        "`Ctrl-X` resolves in exactly one place, and the queue is what answers it (§12)"
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
