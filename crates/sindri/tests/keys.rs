use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::ink::{Ink, Look};
use sindri::keys::{Act, DESCRIPTIONS, Description, Mode};
use sindri::list::ScreenRow;
use sindri::rows::marks;
use sindri::view::ScreenBody;
use sindri::view::{Transcript, View};

fn typed(lines: &[&str]) -> View {
    drafted(
        View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        ),
        lines,
    )
}

fn drafted(mut view: View, lines: &[&str]) -> View {
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            view.key(Key::Newline);
        }
        view.paste(line);
    }
    view
}

fn waiting(draft: &[&str], prompts: &[&str]) -> View {
    let root = tempfile::tempdir().expect("a temp workspace");
    let mut view = View::of(
        Capabilities::default(),
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::none(),
    );
    for prompt in prompts {
        view.paste(prompt);
        match view.key(Key::Enter) {
            Outcome::Send(prompt) => view.enqueue(prompt),
            other => panic!("a draft should have been sent, and {other:?} came instead"),
        }
    }
    drafted(view, draft)
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
fn the_sideways_arrows_walk_the_marked_fence_they_answer() {
    assert_eq!(
        act_of(Mode::Composer, Key::ShiftRight),
        None,
        "`Shift+→` walks the fence under the mark on"
    );
    assert_eq!(
        act_of(Mode::Composer, Key::ShiftLeft),
        None,
        "and `Shift+←` walks it back"
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
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );

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
    let mut view = typed(&["why is the sky"]);

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
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
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
    let bound: Vec<(Mode, Key)> = Mode::ALL
        .iter()
        .flat_map(|mode| mode.table().iter().map(move |binding| (*mode, binding)))
        .filter(|(_, binding)| binding.act == Act::Up)
        .map(|(mode, binding)| (mode, binding.key))
        .collect();

    assert_eq!(
        bound,
        vec![(Mode::Composer, Key::Up)],
        "the draft's `↑` resolves in exactly one place, and the rule is decided there"
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
    let mut view = typed(&["one", "two"]);

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
        View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none()
        )
        .key(Key::Drop),
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
        "`Ctrl-X` resolves in exactly one place, and the queue is what answers it"
    );
}

#[test]
fn a_paste_while_a_screen_is_open_reaches_no_draft() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.key(Key::Char('?'));

    view.paste("a prompt nobody can see");

    assert_eq!(
        view.composer().text(),
        "",
        "a screen has no draft, so a clipboard's lines land nowhere rather than in a \
         draft the reader cannot read"
    );
}

#[test]
fn every_mode_is_described_by_exactly_one_entry() {
    for mode in Mode::ALL {
        let describing: Vec<&Description> = DESCRIPTIONS
            .iter()
            .filter(|described| described.mode == mode)
            .collect();

        assert_eq!(
            describing.len(),
            1,
            "a mode states its name, its card's title, its table and what a printable key \
             falls to in one entry, and {mode:?} has {}: a mode with none has no keys to \
             answer, and one with two has no answer to which describes it",
            describing.len()
        );
    }

    for described in DESCRIPTIONS {
        assert!(
            Mode::ALL.contains(&described.mode),
            "{:?} is described, and no variant of `Mode::ALL` is named {:?}: an entry left \
             over describes a screen nothing can open",
            described.mode,
            described.mode
        );
    }

    assert_eq!(
        DESCRIPTIONS.len(),
        Mode::ALL.len(),
        "one entry per screen and no more: a count that differs is an entry twice or a mode \
         left undescribed"
    );
}

#[test]
fn the_draft_is_the_one_mode_that_wears_no_card() {
    let draft = Mode::Composer.description();

    assert_eq!(
        draft.title, None,
        "a draft is no screen and takes no card, so the entry that describes it names no \
         title for a card's top row: that is how the card is taken away"
    );
    assert!(
        draft.table.iter().any(|binding| binding.act == Act::Close),
        "the draft's `Esc` still closes the commands card, so the empty title is what takes \
         the card away and not a table with no exit in it"
    );

    let carded: Vec<Mode> = Mode::ALL
        .into_iter()
        .filter(|mode| mode.labels().is_some())
        .collect();

    assert_eq!(
        carded,
        vec![
            Mode::Keys,
            Mode::Files,
            Mode::Models,
            Mode::Settings,
            Mode::Stats
        ],
        "the five screens wear a card and the draft wears none, and each card's two labels \
         are its screen's name and the key its own table binds to leaving it"
    );
}

fn keys_body(width: u16) -> Vec<ScreenRow> {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.key(Key::Char('?'));
    let Some(ScreenBody::Rows(rows)) = view.body(Mode::Keys, width) else {
        panic!("the keys screen is a card of rows");
    };
    rows
}

fn said(row: &ScreenRow) -> String {
    row.spans.iter().map(|(said, _)| said.as_str()).collect()
}

fn marks_section(width: u16) -> (Look, Vec<(String, Look, String)>) {
    let body = keys_body(width);
    let heading = body
        .iter()
        .position(|row| said(row).trim() == "marks")
        .unwrap_or_else(|| {
            let said: Vec<String> = body.iter().map(said).collect();
            panic!("no marks heading at {width} columns: {said:#?}")
        });
    let named = body[heading].spans[0].1;

    let mut section: Vec<(String, Look, String)> = Vec::new();
    for row in &body[heading + 1..] {
        let glyph = row.spans[0].0.trim();
        let words: Vec<&str> = row.spans[1..]
            .iter()
            .map(|(said, _)| said.trim())
            .filter(|said| !said.is_empty())
            .collect();
        match section.last_mut() {
            Some(last) if glyph.is_empty() => {
                last.2.push(' ');
                last.2.push_str(&words.join(" "));
            }
            _ => section.push((glyph.to_owned(), row.spans[0].1, words.join(" "))),
        }
    }
    (named, section)
}

#[test]
fn the_keys_screen_names_every_mark_under_its_key_table_in_the_ink_it_is_drawn_in() {
    let (heading, named) = marks_section(68);
    assert_eq!(
        heading,
        Look::plain(Ink::Dim),
        "the marks sit under the key table as a section of their own, headed in the dim ink the \
         `keys` and `composer` sections are headed in"
    );

    let wanted: Vec<(String, Look, String)> = marks()
        .iter()
        .map(|mark| {
            (
                mark.glyph().to_owned(),
                Look::plain(mark.ink()),
                mark.says().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        named, wanted,
        "one row per mark the band draws, in the order the drawing declares them: the glyph, the \
         ink it is drawn in, and the words that say what it means"
    );
}

#[test]
fn a_narrow_card_wraps_a_marks_meaning_and_never_cuts_it_in_half() {
    let (_, roomy) = marks_section(68);
    let (_, narrow) = marks_section(36);
    assert_eq!(
        narrow, roomy,
        "the same marks, the same inks and the same words at a width that cannot hold them on one \
         row: a meaning wraps and is never cut"
    );

    let body = keys_body(36);
    let said: Vec<String> = body.iter().map(said).collect();
    let heading = said
        .iter()
        .position(|row| row.trim() == "marks")
        .expect("a marks heading");
    assert!(
        said.len() - heading - 1 > marks().len(),
        "at 36 columns a meaning runs over more than one row: {said:#?}"
    );
    for (glyph, _, said) in &narrow {
        assert!(
            !glyph.is_empty() && !said.is_empty(),
            "a mark a terminal that spells no colour is read by carries its glyph and its words, \
             and never the ink alone"
        );
    }
}

fn recalled(prompts: &[&str]) -> View {
    let root = tempfile::tempdir().expect("a temp state home");
    let environment: std::collections::BTreeMap<String, String> = [(
        String::from("XDG_STATE_HOME"),
        root.path().to_string_lossy().into_owned(),
    )]
    .into_iter()
    .collect();
    let mut recall = sindri_core::recall::Recall::load(&environment);
    for prompt in prompts {
        recall.send(prompt);
    }
    View::of(
        Capabilities::default(),
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::none(),
    )
    .recalling(recall)
}

fn queued(mut view: View, prompt: &str) -> View {
    view.paste(prompt);
    match view.key(Key::Enter) {
        Outcome::Send(prompt) => view.enqueue(prompt),
        other => panic!("a draft should have been sent, and {other:?} came instead"),
    }
    view
}

#[test]
fn up_on_an_empty_draft_hands_back_the_prompt_sent_last() {
    let mut view = recalled(&["the first thing", "the second thing"]);

    assert_eq!(
        view.key(Key::Up),
        Outcome::Edited,
        "a recall is a draft to paint"
    );
    assert_eq!(
        view.composer().text(),
        "the second thing",
        "the prompt asked last is the whole draft again"
    );
    assert_eq!(
        view.composer().cursor(),
        "the second thing".len(),
        "with the caret at its end, ready to be typed on"
    );
}

#[test]
fn up_again_walks_back_one_prompt_at_a_time() {
    let mut view = recalled(&["the first thing", "the second thing", "the third thing"]);
    view.key(Key::Up);
    view.key(Key::Up);

    assert_eq!(
        view.composer().text(),
        "the second thing",
        "the second `↑` is the prompt before the one on screen"
    );
    view.key(Key::Up);
    assert_eq!(
        view.composer().text(),
        "the first thing",
        "and the third is the oldest there is"
    );
    assert_eq!(
        view.key(Key::Up),
        Outcome::Unchanged,
        "past the oldest the walk stops and the frame stays as it is"
    );
    assert_eq!(view.composer().text(), "the first thing");
}

#[test]
fn down_walks_forward_and_past_the_newest_gives_the_draft_back() {
    let mut view = recalled(&["the first thing", "the second thing"]);
    view.paste("half a thought");
    view.key(Key::Left);
    view.key(Key::Left);
    let (text, cursor) = (view.composer().text().to_owned(), view.composer().cursor());

    view.key(Key::Up);
    view.key(Key::Up);
    assert_eq!(view.composer().text(), "the first thing");

    assert_eq!(view.key(Key::Down), Outcome::Edited, "`↓` is the walk on");
    assert_eq!(view.composer().text(), "the second thing");
    assert_eq!(
        view.key(Key::Down),
        Outcome::Edited,
        "and past the newest it is the draft the walk interrupted"
    );
    assert_eq!(
        view.composer().text(),
        text,
        "the words exactly as they were"
    );
    assert_eq!(
        view.composer().cursor(),
        cursor,
        "and the caret standing where it stood"
    );
    assert_eq!(
        view.key(Key::Down),
        Outcome::Unchanged,
        "past the draft it interrupted there is nothing further to walk"
    );
}

#[test]
fn a_recalled_prompt_is_the_whole_draft_and_never_spliced_into_it() {
    let mut view = recalled(&["the whole prompt"]);
    view.paste("typed, with the caret");
    for _ in 0..4 {
        view.key(Key::Left);
    }

    view.key(Key::Up);
    assert_eq!(
        view.composer().text(),
        "the whole prompt",
        "the text the caret stood in the middle of does not survive the recall"
    );
}

#[test]
fn a_recalled_multi_line_prompt_arrives_whole_with_the_caret_at_its_end() {
    let mut view = recalled(&["first line\nsecond line\nthird line"]);

    view.key(Key::Up);
    assert_eq!(
        view.composer().text(),
        "first line\nsecond line\nthird line",
        "the lines the reader wrote are handed back as lines"
    );
    assert_eq!(
        view.composer().cursor(),
        "first line\nsecond line\nthird line".len(),
        "and the caret waits at the tail of the last one"
    );
}

#[test]
fn an_edit_to_a_recalled_prompt_keeps_the_walk_where_it_was() {
    let mut view = recalled(&["the first thing", "the second thing", "the third thing"]);
    view.key(Key::Up);
    view.key(Key::Up);
    view.paste("!");

    assert_eq!(
        view.composer().text(),
        "the second thing!",
        "a recalled prompt is a draft like any other"
    );
    view.key(Key::Up);
    assert_eq!(
        view.composer().text(),
        "the first thing",
        "and the next `↑` takes the prompt before the one the reader was editing"
    );
}

#[test]
fn down_inside_a_multi_line_draft_moves_the_caret_and_never_the_walk() {
    let mut view = recalled(&["one\ntwo"]);
    assert_eq!(
        view.key(Key::Up),
        Outcome::Edited,
        "the recall is the whole draft, two lines of it"
    );
    assert_eq!(
        view.composer().caret(),
        (1, 3),
        "the caret waits on row two"
    );

    view.key(Key::Up);
    assert_eq!(
        view.composer().caret(),
        (0, 3),
        "off the first line `↑` is the draft's, so the caret rose a row"
    );
    assert_eq!(
        view.key(Key::Down),
        Outcome::Edited,
        "and off the last line `↓` is the draft's too"
    );
    assert_eq!(
        view.composer().caret(),
        (1, 3),
        "so the caret came back down"
    );
    assert_eq!(
        view.composer().text(),
        "one\ntwo",
        "neither press moved the walk off the prompt on screen"
    );
}

#[test]
fn the_queue_hands_back_its_prompt_before_the_recall_does() {
    let mut view = queued(recalled(&["the first thing"]), "the queued prompt");

    assert_eq!(
        view.key(Key::Up),
        Outcome::Edited,
        "the queue's prompt is the first thing `↑` takes back"
    );
    assert_eq!(view.composer().text(), "the queued prompt");
    view.key(Key::Up);
    assert_eq!(
        view.composer().text(),
        "the queued prompt",
        "and the recall is what answers once the queue holds nothing"
    );
}

#[test]
fn sending_a_prompt_ends_the_walk_and_keeps_it_in_the_recall() {
    let mut view = recalled(&["the first thing", "the second thing"]);
    view.key(Key::Up);
    view.key(Key::Up);
    assert_eq!(view.composer().text(), "the first thing");

    assert!(
        matches!(view.key(Key::Enter), Outcome::Send(prompt) if prompt.text() == "the first thing"),
        "a recalled prompt is a draft Enter sends"
    );
    view.key(Key::Up);
    assert_eq!(
        view.composer().text(),
        "the first thing",
        "the prompt just sent is the newest of the recall, so the next `↑` starts there"
    );
    view.key(Key::Up);
    assert_eq!(
        view.composer().text(),
        "the second thing",
        "and walks back from it"
    );
}

#[test]
fn ctrl_c_ends_the_walk_and_leaves_the_recall_alone() {
    let mut view = recalled(&["the first thing", "the second thing"]);
    view.key(Key::Up);
    view.key(Key::Up);
    assert_eq!(view.composer().text(), "the first thing");

    view.key(Key::Cancel);
    assert_eq!(
        view.composer().text(),
        "",
        "clearing the draft is not walking"
    );
    view.key(Key::Up);
    assert_eq!(
        view.composer().text(),
        "the second thing",
        "the next `↑` starts at the newest again, and the recall holds it still"
    );
}

#[test]
fn a_launch_with_no_recall_still_hands_back_what_it_sent() {
    let mut view = recalled(&[]);
    assert_eq!(
        view.key(Key::Up),
        Outcome::Unchanged,
        "a first run has nothing to hand back, so it is not a flicker"
    );

    view.paste("the only thing I sent");
    assert!(matches!(view.key(Key::Enter), Outcome::Send(_)));
    assert_eq!(
        view.key(Key::Up),
        Outcome::Edited,
        "a launch with nowhere to write still recalls what it sent"
    );
    assert_eq!(view.composer().text(), "the only thing I sent");
}
